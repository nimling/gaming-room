//! Turns "play this game" into a launch plan, then writes the settings
//! files and starts the game. Planning has no side effects, so the CLI
//! can show a plan (`arcade plan`) and tests can check it.

use crate::config::Config;
use crate::ini::{self, Sections};
use crate::manifest::{ConfigEntry, Manifest, Runtime, Tier};
use crate::nick;
use crate::seat::Seat;
use anyhow::{bail, Context, Result};
use minijinja::{context, Environment, UndefinedBehavior};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "server", rename_all = "lowercase")]
pub enum Mode {
    Play,
    Join(String),
    Host,
}

#[derive(Debug, Clone, Serialize)]
pub struct Step {
    pub title: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum FileWrite {
    Whole { path: PathBuf, content: String },
    Ini { path: PathBuf, set: Sections },
}

impl FileWrite {
    pub fn path(&self) -> &Path {
        match self {
            FileWrite::Whole { path, .. } | FileWrite::Ini { path, .. } => path,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Plan {
    pub game: String,
    pub name: String,
    pub mode: Mode,
    pub tier: Tier,
    pub width: u32,
    pub height: u32,
    pub fps_cap: u32,
    pub program: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub workdir: Option<PathBuf>,
    pub files: Vec<FileWrite>,
    pub steps: Vec<Step>,
}

pub struct Request<'a> {
    pub manifest: &'a Manifest,
    pub seat: &'a Seat,
    pub config: &'a Config,
    pub games_dir: &'a Path,
    pub nickname: &'a str,
    pub mode: Mode,
    /// Use this tier's profile instead of the seat's (tests and `--profile`).
    pub tier: Option<Tier>,
}

/// Template engine for manifests: Jinja syntax, and a typo in a variable
/// name is an error instead of an empty string.
pub fn environment() -> Environment<'static> {
    let mut env = Environment::new();
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    env
}

fn safe_server(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 255
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || ".:-_[]".contains(c))
}

fn expand_home(s: &str, home: &Path) -> PathBuf {
    match s.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None if s == "~" => home.to_path_buf(),
        None => PathBuf::from(s),
    }
}

pub fn build(req: &Request) -> Result<Plan> {
    let m = req.manifest;
    let seat = req.seat;
    nick::validate(req.nickname).context("nickname")?;
    if let Mode::Join(server) = &req.mode {
        if !safe_server(server) {
            bail!("{server:?} is not a server address");
        }
        if m.join.is_none() {
            bail!("{} cannot join LAN games from the launcher", m.name);
        }
    }
    if req.mode == Mode::Host && m.host.is_none() {
        bail!("{} cannot host LAN games from the launcher", m.name);
    }

    let tier = req.tier.unwrap_or(seat.tier);
    let (width, height, fps_cap) = m.resolve(tier, &seat.display)?;
    let server = match &req.mode {
        Mode::Join(s) => s.clone(),
        _ => String::new(),
    };
    let map = m
        .host
        .as_ref()
        .and_then(|h| h.map.clone())
        .unwrap_or_default();
    let ctx = context! {
        nickname => req.nickname.trim(),
        tier => tier.to_string(),
        width, height, fps_cap,
        refresh => seat.display.refresh,
        server, map,
        id => m.id.clone(),
        path => m.path.as_ref().map(|p| p.display().to_string()).unwrap_or_default(),
        home => seat.home.display().to_string(),
    };
    let env = environment();
    let render = |s: &str| -> Result<String> {
        env.render_str(s, &ctx)
            .with_context(|| format!("filling in {s:?}"))
    };

    // Settings files
    let mut files = Vec::new();
    for entry in &m.config {
        match entry {
            ConfigEntry::Template { template, to } => {
                let src = req.games_dir.join(template);
                let text = fs::read_to_string(&src)
                    .with_context(|| format!("reading {}", src.display()))?;
                files.push(FileWrite::Whole {
                    path: expand_home(&render(to)?, &seat.home),
                    content: render(&text)?,
                });
            }
            ConfigEntry::Ini { ini, set } => {
                let mut out = Sections::new();
                for (section, keys) in set {
                    let mut rendered = BTreeMap::new();
                    for (k, v) in keys {
                        rendered.insert(k.clone(), render(v)?);
                    }
                    out.insert(section.clone(), rendered);
                }
                files.push(FileWrite::Ini {
                    path: expand_home(&render(ini)?, &seat.home),
                    set: out,
                });
            }
        }
    }

    // Command: [systemd-inhibit …] [gamemoderun] [gamescope … --] [umu-run] game args
    let mut env_vars = BTreeMap::new();
    for (k, v) in &m.launch.env {
        env_vars.insert(k.clone(), render(v)?);
    }
    let mut cmd: Vec<String> = Vec::new();
    // Steam's own start command returns at once; the game runs inside Steam.
    if req.config.idle_inhibit && m.runtime != Runtime::Steam {
        cmd.extend([
            "systemd-inhibit".into(),
            "--what=idle".into(),
            "--who=Arcade".into(),
            format!("--why={} is running", m.name),
            "--mode=block".into(),
        ]);
    }
    if m.gamemode && m.runtime != Runtime::Steam {
        cmd.push("gamemoderun".into());
    }
    if let Some(g) = &m.gamescope {
        cmd.push("gamescope".into());
        for (flag, value) in [
            ("-w", g.width),
            ("-h", g.height),
            ("-W", seat.display.width),
            ("-H", seat.display.height),
            ("-r", fps_cap),
        ] {
            cmd.push(flag.into());
            cmd.push(value.to_string());
        }
        cmd.push("-f".into());
        if g.integer_scale {
            cmd.push("-S".into());
            cmd.push("integer".into());
        }
        cmd.push("--".into());
    }
    if m.runtime == Runtime::Proton {
        cmd.push("umu-run".into());
        env_vars
            .entry("GAMEID".into())
            .or_insert_with(|| "0".into());
        env_vars.entry("WINEPREFIX".into()).or_insert_with(|| {
            seat.home
                .join(".local/share/arcade/prefixes")
                .join(&m.id)
                .display()
                .to_string()
        });
        let proton = req.config.proton.as_ref().map(|p| p.display().to_string());
        env_vars
            .entry("PROTONPATH".into())
            .or_insert_with(|| proton.unwrap_or_else(|| "GE-Proton".into()));
    }
    for part in m.launch.command.iter().chain(&m.launch.args) {
        cmd.push(render(part)?);
    }
    match &req.mode {
        Mode::Play => {}
        Mode::Join(_) => {
            for a in m.join.iter().flat_map(|j| &j.args) {
                cmd.push(render(a)?);
            }
        }
        Mode::Host => {
            for a in m.host.iter().flat_map(|h| &h.args) {
                cmd.push(render(a)?);
            }
        }
    }
    if cmd.is_empty() {
        bail!("{} has no launch command", m.id);
    }
    let program = cmd.remove(0);
    let workdir = match (&m.path, &m.launch.workdir) {
        (Some(p), Some(w)) => Some(p.join(w)),
        (Some(p), None) => Some(p.clone()),
        (None, w) => w.clone(),
    };

    // Steps the screen shows while the game starts
    let mut steps = vec![Step {
        title: format!("Settings for Tier {tier}"),
        detail: m.tuned_label(width, height, fps_cap),
    }];
    let names: Vec<String> = files
        .iter()
        .filter_map(|f| {
            f.path()
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
        })
        .collect();
    let nick_in_args = m
        .launch
        .args
        .iter()
        .chain(&m.launch.command)
        .any(|a| a.contains("nickname"));
    if !names.is_empty() {
        steps.push(Step {
            title: format!("Your name: {}", req.nickname.trim()),
            detail: format!("Written to {}", names.join(", ")),
        });
    } else if nick_in_args {
        steps.push(Step {
            title: format!("Your name: {}", req.nickname.trim()),
            detail: "Passed to the game at start".into(),
        });
    }
    let start = match (m.runtime, &m.gamescope) {
        (Runtime::Steam, _) => (
            "Opening Steam".to_string(),
            "Games install to /games/steam".to_string(),
        ),
        (_, Some(_)) => (
            "Starting in gamescope".into(),
            "Integer scaling keeps the pixels sharp".into(),
        ),
        (Runtime::Proton, None) => (
            "Starting with Proton".into(),
            "Pinned Proton build, prefix in the clean home".into(),
        ),
        (Runtime::Native, None) => (
            "Starting natively".into(),
            if m.gamemode {
                format!("{}, with gamemode", m.engine)
            } else {
                m.engine.clone()
            },
        ),
    };
    steps.push(Step {
        title: start.0,
        detail: start.1,
    });
    match &req.mode {
        Mode::Join(s) => steps.push(Step {
            title: format!("Joining {s}"),
            detail: "Straight to the server".into(),
        }),
        Mode::Host => steps.push(Step {
            title: "Hosting a LAN game".into(),
            detail: map.clone(),
        }),
        Mode::Play => {}
    }

    Ok(Plan {
        game: m.id.clone(),
        name: m.name.clone(),
        mode: req.mode.clone(),
        tier,
        width,
        height,
        fps_cap,
        program,
        args: cmd,
        env: env_vars,
        workdir,
        files,
        steps,
    })
}

/// Writes the plan's settings files.
pub fn apply(plan: &Plan) -> Result<()> {
    for f in &plan.files {
        match f {
            FileWrite::Whole { path, content } => {
                if let Some(dir) = path.parent() {
                    fs::create_dir_all(dir)
                        .with_context(|| format!("creating {}", dir.display()))?;
                }
                fs::write(path, content).with_context(|| format!("writing {}", path.display()))?;
            }
            FileWrite::Ini { path, set } => ini::merge_file(path, set)?,
        }
    }
    Ok(())
}

/// Starts the game. Does not wait for it.
pub fn spawn(plan: &Plan) -> Result<Child> {
    let mut cmd = Command::new(&plan.program);
    cmd.args(&plan.args).envs(&plan.env);
    if let Some(dir) = &plan.workdir {
        if !dir.is_dir() {
            bail!(
                "{} is not installed on this seat ({} is missing)",
                plan.name,
                dir.display()
            );
        }
        cmd.current_dir(dir);
    }
    // Own process group: wrappers such as openra-ra start the real game as
    // a child, and stopping only the wrapper would leave the game running.
    cmd.process_group(0);
    cmd.spawn()
        .with_context(|| format!("starting {}", plan.program))
}

/// The plan as one shell line, for people reading `arcade plan`.
pub fn shell_line(plan: &Plan) -> String {
    let quote = |s: &str| {
        if !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_./=:@%+,".contains(c))
        {
            s.to_string()
        } else {
            format!("'{}'", s.replace('\'', r"'\''"))
        }
    };
    plan.env
        .iter()
        .map(|(k, v)| format!("{k}={}", quote(v)))
        .chain(std::iter::once(quote(&plan.program)))
        .chain(plan.args.iter().map(|a| quote(a)))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Sends a signal to a game's whole process group. True if any process got it.
/// A system call, not a `kill` process: the launcher checks this every few
/// seconds while a game runs, and must cost the game nothing.
fn signal_group(child: &Child, signal: libc::c_int) -> bool {
    let Ok(pid) = libc::pid_t::try_from(child.id()) else {
        return false;
    };
    // SAFETY: kill(2) only sends a signal. The game was spawned with
    // process_group(0), so -pid is its own group, never the launcher's.
    unsafe { libc::kill(-pid, signal) == 0 }
}

/// True while the game, or anything it started, is still running.
pub fn alive(child: &mut Child) -> bool {
    matches!(child.try_wait(), Ok(None)) || signal_group(child, 0)
}

/// Stops a game and everything it started: SIGTERM to the process group,
/// up to three seconds to close, then SIGKILL for whatever is left.
pub fn stop(child: &mut Child) {
    signal_group(child, libc::SIGTERM);
    for _ in 0..30 {
        if !alive(child) {
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    if alive(child) {
        signal_group(child, libc::SIGKILL);
    }
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seat::Display;

    fn seat(home: &Path) -> Seat {
        Seat {
            name: "Seat 04".into(),
            tier: Tier::B,
            display: Display {
                width: 2560,
                height: 1440,
                refresh: 144,
            },
            gpu: "AMD · 8 GB".into(),
            home: home.to_path_buf(),
        }
    }

    fn manifest(yaml: &str) -> Manifest {
        serde_yaml::from_str(yaml).unwrap()
    }

    const UT: &str = r#"
id: ut99
name: Unreal Tournament
runtime: native
engine: OldUnreal 469e
path: /games/classic/ut99
players: { min: 2, max: 16 }
launch: { command: ["./ut-bin"], workdir: System64 }
join: { args: ["{{ server }}"] }
host: { map: DM-Morpheus, args: ["{{ map }}?Listen"] }
config:
  - ini: "~/.utpg/System/User.ini"
    set: { DefaultPlayer: { Name: "{{ nickname }}" } }
profiles:
  A: { resolution: native, fps_cap: refresh }
  B: { resolution: native, fps_cap: 144 }
  C: { resolution: 1920x1080, fps_cap: 144 }
"#;

    fn req<'a>(m: &'a Manifest, s: &'a Seat, c: &'a Config, mode: Mode) -> Request<'a> {
        Request {
            manifest: m,
            seat: s,
            config: c,
            games_dir: Path::new("."),
            nickname: "Kaptein",
            mode,
            tier: None,
        }
    }

    #[test]
    fn plays_native_game_with_gamemode() {
        let home = tempfile::tempdir().unwrap();
        let (m, s, c) = (manifest(UT), seat(home.path()), Config::default());
        let p = build(&req(&m, &s, &c, Mode::Play)).unwrap();
        assert_eq!(p.program, "gamemoderun");
        assert_eq!(p.args, ["./ut-bin"]);
        assert_eq!(
            p.workdir.as_deref(),
            Some(Path::new("/games/classic/ut99/System64"))
        );
        assert_eq!((p.width, p.height, p.fps_cap), (2560, 1440, 144));
        assert_eq!(p.files[0].path(), home.path().join(".utpg/System/User.ini"));
        assert!(p.steps.iter().any(|st| st.title == "Your name: Kaptein"));
    }

    #[test]
    fn joins_and_hosts() {
        let home = tempfile::tempdir().unwrap();
        let (m, s, c) = (manifest(UT), seat(home.path()), Config::default());
        let p = build(&req(&m, &s, &c, Mode::Join("192.0.2.20:7777".into()))).unwrap();
        assert_eq!(p.args.last().unwrap(), "192.0.2.20:7777");
        let p = build(&req(&m, &s, &c, Mode::Host)).unwrap();
        assert_eq!(p.args.last().unwrap(), "DM-Morpheus?Listen");
    }

    #[test]
    fn refuses_unsafe_server_and_bad_nickname() {
        let home = tempfile::tempdir().unwrap();
        let (m, s, c) = (manifest(UT), seat(home.path()), Config::default());
        assert!(build(&req(&m, &s, &c, Mode::Join("x; rm -rf ~".into()))).is_err());
        let mut r = req(&m, &s, &c, Mode::Play);
        r.nickname = "no spaces";
        assert!(build(&r).is_err());
    }

    #[test]
    fn tier_override_changes_settings() {
        let home = tempfile::tempdir().unwrap();
        let (m, s, c) = (manifest(UT), seat(home.path()), Config::default());
        let mut r = req(&m, &s, &c, Mode::Play);
        r.tier = Some(Tier::C);
        let p = build(&r).unwrap();
        assert_eq!((p.width, p.height), (1920, 1080));
    }

    #[test]
    fn old_game_runs_in_gamescope() {
        let home = tempfile::tempdir().unwrap();
        let yaml = r#"
id: wargus
name: Warcraft II
runtime: native
players: { min: 1, max: 8 }
gamescope: { width: 640, height: 480 }
launch: { command: ["wargus"] }
profiles:
  A: { resolution: 640x480, fps_cap: 60 }
  B: { resolution: 640x480, fps_cap: 60 }
  C: { resolution: 640x480, fps_cap: 60 }
"#;
        let (m, s, c) = (manifest(yaml), seat(home.path()), Config::default());
        let p = build(&req(&m, &s, &c, Mode::Play)).unwrap();
        assert_eq!(
            shell_line(&p),
            "gamemoderun gamescope -w 640 -h 480 -W 2560 -H 1440 -r 60 -f -S integer -- wargus"
        );
    }

    #[test]
    fn seats_hold_an_idle_lock_while_a_game_runs() {
        let home = tempfile::tempdir().unwrap();
        let m = manifest(UT);
        let s = seat(home.path());
        let c: Config = serde_yaml::from_str("idle_inhibit: true").unwrap();
        let p = build(&req(&m, &s, &c, Mode::Play)).unwrap();
        assert_eq!(p.program, "systemd-inhibit");
        assert_eq!(
            p.args[..5],
            [
                "--what=idle",
                "--who=Arcade",
                "--why=Unreal Tournament is running",
                "--mode=block",
                "gamemoderun"
            ]
        );
        assert_eq!(p.args.last().map(String::as_str), Some("./ut-bin"));
    }

    #[test]
    fn windows_game_uses_umu_and_pinned_proton() {
        let home = tempfile::tempdir().unwrap();
        let yaml = r#"
id: wc3
name: Warcraft III
runtime: proton
players: { min: 1, max: 12 }
gamemode: false
launch: { command: ["/games/classic/wc3/Warcraft III.exe"] }
profiles:
  A: { resolution: native, fps_cap: 144 }
  B: { resolution: native, fps_cap: 144 }
  C: { resolution: native, fps_cap: 144 }
"#;
        let m = manifest(yaml);
        let s = seat(home.path());
        let c: Config = serde_yaml::from_str("proton: /opt/proton/GE-Proton-pinned").unwrap();
        let p = build(&req(&m, &s, &c, Mode::Play)).unwrap();
        assert_eq!(p.program, "umu-run");
        assert_eq!(p.env["PROTONPATH"], "/opt/proton/GE-Proton-pinned");
        assert!(p.env["WINEPREFIX"].ends_with(".local/share/arcade/prefixes/wc3"));
        assert!(shell_line(&p).ends_with("umu-run '/games/classic/wc3/Warcraft III.exe'"));
    }

    #[test]
    fn typo_in_template_is_an_error() {
        let home = tempfile::tempdir().unwrap();
        let m = manifest(&UT.replace("{{ nickname }}", "{{ nickame }}"));
        let (s, c) = (seat(home.path()), Config::default());
        assert!(build(&req(&m, &s, &c, Mode::Play)).is_err());
    }

    #[test]
    fn applies_files_and_starts_the_program() {
        let home = tempfile::tempdir().unwrap();
        let out = home.path().join("args.txt");
        let yaml = format!(
            r#"
id: fake
name: Fake game
runtime: native
gamemode: false
players: {{ min: 1, max: 2 }}
launch: {{ command: ["sh", "-c", "echo \"$1\" > {out}", "fake", "{{{{ nickname }}}}"] }}
config:
  - ini: "~/fake/game.ini"
    set: {{ Player: {{ Name: "{{{{ nickname }}}}" }} }}
profiles:
  A: {{ resolution: native, fps_cap: 60 }}
  B: {{ resolution: native, fps_cap: 60 }}
  C: {{ resolution: native, fps_cap: 60 }}
"#,
            out = out.display()
        );
        let (m, s, c) = (manifest(&yaml), seat(home.path()), Config::default());
        let p = build(&req(&m, &s, &c, Mode::Play)).unwrap();
        apply(&p).unwrap();
        let status = spawn(&p).unwrap().wait().unwrap();
        assert!(status.success());
        assert_eq!(fs::read_to_string(&out).unwrap().trim(), "Kaptein");
        assert_eq!(
            fs::read_to_string(home.path().join("fake/game.ini")).unwrap(),
            "[Player]\nName=Kaptein\n"
        );
    }

    #[test]
    fn stop_ends_the_whole_game_not_just_the_wrapper() {
        let home = tempfile::tempdir().unwrap();
        let yaml = r#"
id: wrapper
name: Wrapper game
runtime: native
gamemode: false
players: { min: 1, max: 2 }
launch: { command: ["sh", "-c", "sleep 300 & sleep 300"] }
profiles:
  A: { resolution: native, fps_cap: 60 }
  B: { resolution: native, fps_cap: 60 }
  C: { resolution: native, fps_cap: 60 }
"#;
        let (m, s, c) = (manifest(yaml), seat(home.path()), Config::default());
        let p = build(&req(&m, &s, &c, Mode::Play)).unwrap();
        let mut child = spawn(&p).unwrap();
        thread::sleep(Duration::from_millis(300));
        assert!(alive(&mut child));
        stop(&mut child);
        assert!(!alive(&mut child), "a process of the game survived stop()");
    }
}

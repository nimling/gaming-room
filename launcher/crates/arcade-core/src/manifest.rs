//! Game manifests: one YAML file per game in `games/`. See games/README.md.

use crate::seat::Display;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

/// Hardware tier. A is the strongest, C the weakest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
pub enum Tier {
    A,
    B,
    C,
}

impl Tier {
    pub const ALL: [Tier; 3] = [Tier::A, Tier::B, Tier::C];
}

impl fmt::Display for Tier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Tier::A => "A",
            Tier::B => "B",
            Tier::C => "C",
        })
    }
}

impl FromStr for Tier {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        match s.trim() {
            "A" | "a" => Ok(Tier::A),
            "B" | "b" => Ok(Tier::B),
            "C" | "c" => Ok(Tier::C),
            other => bail!("unknown tier {other:?}; use A, B or C"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Group {
    #[default]
    Classic,
    Steam,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Runtime {
    Native,
    Proton,
    Steam,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub subtitle: String,
    #[serde(default)]
    pub group: Group,
    pub runtime: Runtime,
    #[serde(default)]
    pub engine: String,
    /// Install folder. Leave out for games installed as a system package.
    #[serde(default)]
    pub path: Option<PathBuf>,
    #[serde(default)]
    pub players: Option<Players>,
    pub launch: Launch,
    #[serde(default)]
    pub join: Option<Join>,
    #[serde(default)]
    pub host: Option<Host>,
    #[serde(default)]
    pub config: Vec<ConfigEntry>,
    #[serde(default)]
    pub profiles: BTreeMap<Tier, Profile>,
    #[serde(default)]
    pub gamescope: Option<Gamescope>,
    #[serde(default = "yes")]
    pub gamemode: bool,
    /// Weakest tier the game runs well on. Seats below it do not show it.
    #[serde(default)]
    pub lowest_tier: Option<Tier>,
    #[serde(default)]
    pub art: Option<Art>,
    /// Why this game is good in a LAN room: a short paragraph for the
    /// game details view.
    #[serde(default)]
    pub why: Option<String>,
    /// A few short points for the game details view.
    #[serde(default)]
    pub highlights: Vec<String>,
}

/// Longest `why` text, in characters.
pub const WHY_MAX: usize = 400;
/// Most highlights, and the longest one in characters.
pub const HIGHLIGHTS_MAX: usize = 5;
pub const HIGHLIGHT_MAX: usize = 80;

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Players {
    pub min: u32,
    pub max: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Launch {
    /// Program and fixed arguments. Relative to the work folder if it starts with `./`.
    pub command: Vec<String>,
    #[serde(default)]
    pub args: Vec<String>,
    /// Work folder, relative to `path`.
    #[serde(default)]
    pub workdir: Option<PathBuf>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Join {
    /// Extra arguments to join a server. `{{ server }}` is the address.
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Host {
    pub args: Vec<String>,
    #[serde(default)]
    pub map: Option<String>,
}

/// A settings file the launcher writes before every start.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ConfigEntry {
    /// Render a whole file from a template in the games folder.
    Template { template: String, to: String },
    /// Set keys in an INI file and keep the rest.
    Ini {
        ini: String,
        set: BTreeMap<String, BTreeMap<String, String>>,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    /// `native` or `WIDTHxHEIGHT`.
    pub resolution: String,
    /// `refresh` or a number.
    pub fps_cap: NumOrText,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum NumOrText {
    Num(u32),
    Text(String),
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Gamescope {
    /// Size the game renders at; gamescope scales it to the screen.
    pub width: u32,
    pub height: u32,
    #[serde(default = "yes")]
    pub integer_scale: bool,
}

/// Placeholder art drawn by the launcher until real art is added.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Art {
    pub hue: u16,
    pub glow: u16,
    pub motif: String,
}

pub const MOTIFS: [&str; 6] = ["rings", "grid", "hex", "contour", "streaks", "dots"];

/// What the launcher screen needs to show one game.
#[derive(Debug, Clone, Serialize)]
pub struct Card {
    pub id: String,
    pub name: String,
    pub sub: String,
    pub group: Group,
    pub runtime: String,
    pub engine: String,
    pub players: String,
    pub max: u32,
    pub start: &'static str,
    pub map: String,
    pub art: CardArt,
    pub tuned: String,
    pub installed: bool,
    pub can_join: bool,
    pub can_host: bool,
    /// Empty when the manifest has no `why`.
    pub why: String,
    pub highlights: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CardArt {
    pub a: u16,
    pub b: u16,
    pub motif: String,
}

pub fn parse_size(s: &str) -> Option<(u32, u32)> {
    let (w, h) = s.trim().split_once(['x', '×'])?;
    let (w, h) = (w.trim().parse().ok()?, h.trim().parse().ok()?);
    (w > 0 && h > 0).then_some((w, h))
}

/// Ids are lowercase letters, digits and `-`. They are also file names.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

impl Manifest {
    pub fn load(path: &Path) -> Result<Manifest> {
        let text =
            fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        serde_yaml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }

    /// Loads `<dir>/<id>.yaml`.
    pub fn load_id(dir: &Path, id: &str) -> Result<Manifest> {
        if !valid_id(id) {
            bail!("{id:?} is not a game id");
        }
        let path = dir.join(format!("{id}.yaml"));
        if !path.exists() {
            bail!("no game called {id:?} in {}", dir.display());
        }
        Manifest::load(&path)
    }

    /// Resolution and frame cap for a tier, as (width, height, fps).
    pub fn resolve(&self, tier: Tier, display: &Display) -> Result<(u32, u32, u32)> {
        if self.profiles.is_empty() && self.runtime == Runtime::Steam {
            return Ok((display.width, display.height, display.refresh));
        }
        let p = self
            .profiles
            .get(&tier)
            .with_context(|| format!("{} has no profile for tier {tier}", self.id))?;
        let (w, h) = if p.resolution.trim().eq_ignore_ascii_case("native") {
            (display.width, display.height)
        } else {
            parse_size(&p.resolution)
                .with_context(|| format!("bad resolution {:?}", p.resolution))?
        };
        let fps = match &p.fps_cap {
            NumOrText::Num(n) => *n,
            NumOrText::Text(t) if t.trim().eq_ignore_ascii_case("refresh") => display.refresh,
            NumOrText::Text(t) => t
                .trim()
                .parse()
                .with_context(|| format!("bad fps_cap {t:?}"))?,
        };
        Ok((w, h, fps))
    }

    /// The one-line tuning summary the screen shows.
    pub fn tuned_label(&self, w: u32, h: u32, fps: u32) -> String {
        match (&self.gamescope, self.runtime) {
            (_, Runtime::Steam) => "Each game's own settings".into(),
            (Some(g), _) => format!("{}×{}, integer-scaled in gamescope", g.width, g.height),
            (None, _) => format!("{w}×{h} · {fps} fps cap"),
        }
    }

    pub fn installed(&self) -> bool {
        match &self.path {
            Some(p) => p.exists(),
            None => self.launch.command.first().is_some_and(|c| on_path(c)),
        }
    }

    pub fn fits(&self, tier: Tier) -> bool {
        self.lowest_tier.is_none_or(|lowest| tier <= lowest)
    }

    pub fn card(&self, tier: Tier, display: &Display) -> Card {
        let tuned = match self.resolve(tier, display) {
            Ok((w, h, fps)) => self.tuned_label(w, h, fps),
            Err(e) => format!("{e}"),
        };
        let art = self.art.as_ref().map_or(
            CardArt {
                a: 220,
                b: 200,
                motif: "dots".into(),
            },
            |a| CardArt {
                a: a.hue,
                b: a.glow,
                motif: a.motif.clone(),
            },
        );
        Card {
            id: self.id.clone(),
            name: self.name.clone(),
            sub: self.subtitle.clone(),
            group: self.group,
            runtime: match self.runtime {
                Runtime::Native => "Native",
                Runtime::Proton => "Proton",
                Runtime::Steam => "Steam",
            }
            .into(),
            engine: self.engine.clone(),
            players: match self.players {
                Some(p) if p.min == p.max => p.min.to_string(),
                Some(p) => format!("{}–{}", p.min, p.max),
                None => "Per game".into(),
            },
            max: self.players.map_or(0, |p| p.max),
            start: match (self.runtime, &self.gamescope) {
                (Runtime::Steam, _) => "steam",
                (_, Some(_)) => "gamescope",
                (Runtime::Proton, None) => "proton",
                (Runtime::Native, None) => "native",
            },
            map: self
                .host
                .as_ref()
                .and_then(|h| h.map.clone())
                .unwrap_or_default(),
            art,
            tuned,
            installed: self.installed(),
            can_join: self.join.is_some(),
            can_host: self.host.is_some(),
            why: self
                .why
                .as_deref()
                .map(|w| w.split_whitespace().collect::<Vec<_>>().join(" "))
                .unwrap_or_default(),
            highlights: self.highlights.iter().map(|h| h.trim().into()).collect(),
        }
    }

    /// Problems with this manifest, as sentences. Empty means valid.
    pub fn problems(&self, file: &Path, games_dir: &Path) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();

        if !valid_id(&self.id) {
            out.push(format!(
                "id {:?} must be lowercase letters, digits and -",
                self.id
            ));
        }
        if file.file_stem().and_then(|s| s.to_str()) != Some(self.id.as_str()) {
            out.push(format!("file name must be {}.yaml", self.id));
        }
        if self.name.trim().is_empty() {
            out.push("name is empty".into());
        }
        if let Some(p) = &self.path {
            if !p.is_absolute() {
                out.push(format!("path {} must be absolute", p.display()));
            }
        }
        match self.players {
            Some(p) if p.min == 0 || p.min > p.max => out.push(format!(
                "players: min {} and max {} do not fit",
                p.min, p.max
            )),
            None if self.runtime != Runtime::Steam => out.push("players is missing".into()),
            _ => {}
        }
        if self.launch.command.is_empty() {
            out.push("launch.command is empty".into());
        }
        if self.runtime != Runtime::Steam {
            for t in Tier::ALL {
                if let Err(e) = self.resolve(t, &Display::default()) {
                    out.push(format!("{e:#}"));
                }
            }
        }
        if let Some(j) = &self.join {
            if !j.args.iter().any(|a| a.contains("server")) {
                out.push("join.args must use {{ server }}".into());
            }
        }
        if let Some(g) = &self.gamescope {
            if g.width == 0 || g.height == 0 {
                out.push("gamescope width and height must be above 0".into());
            }
        }
        if let Some(a) = &self.art {
            if !MOTIFS.contains(&a.motif.as_str()) {
                out.push(format!("art.motif must be one of {}", MOTIFS.join(", ")));
            }
        }
        if let Some(w) = &self.why {
            let n = w.trim().chars().count();
            if n == 0 {
                out.push("why is empty; leave it out instead".into());
            } else if n > WHY_MAX {
                out.push(format!("why has {n} characters; keep it to {WHY_MAX}"));
            }
        }
        if self.highlights.len() > HIGHLIGHTS_MAX {
            out.push(format!(
                "highlights has {} points; keep it to {HIGHLIGHTS_MAX}",
                self.highlights.len()
            ));
        }
        for h in &self.highlights {
            let n = h.trim().chars().count();
            if n == 0 {
                out.push("highlights has an empty point".into());
            } else if n > HIGHLIGHT_MAX {
                out.push(format!(
                    "highlight {:?} has {n} characters; keep it to {HIGHLIGHT_MAX}",
                    h.trim()
                ));
            }
        }

        let env = crate::plan::environment();
        let mut templates: Vec<(String, String)> = Vec::new();
        let launch = self
            .launch
            .command
            .iter()
            .chain(&self.launch.args)
            .chain(self.launch.env.values());
        templates.extend(launch.map(|s| ("launch".to_string(), s.clone())));
        let lan = self
            .join
            .iter()
            .flat_map(|j| &j.args)
            .chain(self.host.iter().flat_map(|h| &h.args));
        templates.extend(lan.map(|s| ("join/host".to_string(), s.clone())));
        for entry in &self.config {
            match entry {
                ConfigEntry::Template { template, to } => {
                    templates.push(("config.to".into(), to.clone()));
                    match fs::read_to_string(games_dir.join(template)) {
                        Ok(text) => templates.push((template.clone(), text)),
                        Err(_) => out.push(format!(
                            "template {template} not found in {}",
                            games_dir.display()
                        )),
                    }
                }
                ConfigEntry::Ini { ini, set } => {
                    templates.push(("config.ini".into(), ini.clone()));
                    templates.extend(
                        set.values()
                            .flat_map(|kv| kv.values())
                            .map(|v| ("config.set".to_string(), v.clone())),
                    );
                }
            }
        }
        for (what, source) in &templates {
            if let Err(e) = env.template_from_str(source) {
                out.push(format!("{what}: {e}"));
            }
        }
        out
    }
}

fn on_path(program: &str) -> bool {
    if program.contains('/') {
        return Path::new(program).exists();
    }
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
}

/// Loads every `*.yaml` in `dir`, sorted by file name. Files that fail to
/// load are returned as problems instead of stopping the rest.
pub fn load_all(dir: &Path) -> Result<(Vec<Manifest>, Vec<String>)> {
    let mut games = Vec::new();
    let mut problems = Vec::new();
    for path in yaml_files(dir)? {
        match Manifest::load(&path) {
            Ok(m) => games.push(m),
            Err(e) => problems.push(format!("{e:#}")),
        }
    }
    Ok((games, problems))
}

pub struct Report {
    pub checked: usize,
    pub problems: Vec<String>,
}

/// Loads and checks every manifest in `dir`.
pub fn validate_dir(dir: &Path) -> Result<Report> {
    let files = yaml_files(dir)?;
    let mut problems = Vec::new();
    for path in &files {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        match Manifest::load(path) {
            Ok(m) => problems.extend(
                m.problems(path, dir)
                    .into_iter()
                    .map(|p| format!("{name}: {p}")),
            ),
            Err(e) => problems.push(format!("{name}: {e:#}")),
        }
    }
    Ok(Report {
        checked: files.len(),
        problems,
    })
}

fn yaml_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "yaml" || x == "yml"))
        .collect();
    files.sort();
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    const UT: &str = r#"
id: ut99
name: Unreal Tournament
runtime: native
path: /games/classic/ut99
players: { min: 2, max: 16 }
launch: { command: ["./ut-bin"], workdir: System64 }
join: { args: ["{{ server }}"] }
profiles:
  A: { resolution: native, fps_cap: refresh }
  B: { resolution: native, fps_cap: 144 }
  C: { resolution: 1920x1080, fps_cap: 144 }
"#;

    fn display() -> Display {
        Display {
            width: 2560,
            height: 1440,
            refresh: 165,
        }
    }

    #[test]
    fn resolves_profiles_per_tier() {
        let m: Manifest = serde_yaml::from_str(UT).unwrap();
        assert_eq!(m.resolve(Tier::A, &display()).unwrap(), (2560, 1440, 165));
        assert_eq!(m.resolve(Tier::B, &display()).unwrap(), (2560, 1440, 144));
        assert_eq!(m.resolve(Tier::C, &display()).unwrap(), (1920, 1080, 144));
    }

    #[test]
    fn valid_manifest_has_no_problems() {
        let m: Manifest = serde_yaml::from_str(UT).unwrap();
        assert!(m
            .problems(Path::new("games/ut99.yaml"), Path::new("games"))
            .is_empty());
    }

    #[test]
    fn finds_problems() {
        let m: Manifest =
            serde_yaml::from_str(&UT.replace("C: { resolution: 1920x1080, fps_cap: 144 }", ""))
                .unwrap();
        let p = m.problems(Path::new("games/other.yaml"), Path::new("games"));
        assert!(p.iter().any(|s| s.contains("file name")), "{p:?}");
        assert!(p.iter().any(|s| s.contains("tier C")), "{p:?}");
    }

    #[test]
    fn unknown_keys_are_refused() {
        assert!(serde_yaml::from_str::<Manifest>(&format!("{UT}\nwrapers: []\n")).is_err());
    }

    #[test]
    fn tier_filter() {
        let mut m: Manifest = serde_yaml::from_str(UT).unwrap();
        assert!(m.fits(Tier::C));
        m.lowest_tier = Some(Tier::B);
        assert!(m.fits(Tier::A) && m.fits(Tier::B) && !m.fits(Tier::C));
    }

    #[test]
    fn card_for_the_screen() {
        let m: Manifest = serde_yaml::from_str(UT).unwrap();
        let c = m.card(Tier::C, &display());
        assert_eq!(c.players, "2–16");
        assert_eq!(c.tuned, "1920×1080 · 144 fps cap");
        assert!(c.can_join && !c.can_host);
    }

    #[test]
    fn why_and_highlights_are_optional() {
        let m: Manifest = serde_yaml::from_str(UT).unwrap();
        let c = m.card(Tier::B, &display());
        assert!(c.why.is_empty() && c.highlights.is_empty());
    }

    #[test]
    fn why_and_highlights_reach_the_card() {
        let text = format!(
            "{UT}why: >-\n  Fast frags,\n  instant fun.\nhighlights:\n  - \" InstaGib \"\n  - Bots fill empty slots\n"
        );
        let m: Manifest = serde_yaml::from_str(&text).unwrap();
        assert!(m
            .problems(Path::new("games/ut99.yaml"), Path::new("games"))
            .is_empty());
        let c = m.card(Tier::B, &display());
        assert_eq!(c.why, "Fast frags, instant fun.");
        assert_eq!(c.highlights, ["InstaGib", "Bots fill empty slots"]);
    }

    #[test]
    fn why_and_highlights_are_checked() {
        let mut m: Manifest = serde_yaml::from_str(UT).unwrap();
        m.why = Some("  ".into());
        m.highlights = vec![String::new(), "x".repeat(HIGHLIGHT_MAX + 1)];
        let p = m.problems(Path::new("games/ut99.yaml"), Path::new("games"));
        assert!(p.iter().any(|s| s.contains("why is empty")), "{p:?}");
        assert!(p.iter().any(|s| s.contains("empty point")), "{p:?}");
        assert!(p.iter().any(|s| s.contains("keep it to 80")), "{p:?}");

        m.why = Some("w".repeat(WHY_MAX + 1));
        m.highlights = vec!["ok".into(); HIGHLIGHTS_MAX + 1];
        let p = m.problems(Path::new("games/ut99.yaml"), Path::new("games"));
        assert!(p.iter().any(|s| s.contains("keep it to 400")), "{p:?}");
        assert!(p.iter().any(|s| s.contains("keep it to 5")), "{p:?}");
    }

    #[test]
    fn ids_are_safe_file_names() {
        assert!(valid_id("openra-ra"));
        assert!(!valid_id("../etc"));
        assert!(!valid_id("UT99"));
    }
}

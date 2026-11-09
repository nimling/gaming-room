//! `arcade`: everything the launcher screen does, from the terminal.
//! Claude and CI use it to test games on the canary seat.

use anyhow::{bail, Context, Result};
use arcade_core::manifest::{self, Manifest, Tier};
use arcade_core::plan::{self, Mode, Plan, Request};
use arcade_core::{nick, Config, Display, Seat};
use clap::{Args, Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::thread;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "arcade", version, about = "Start games on a gaming room seat")]
struct Cli {
    /// Folder with the game manifests [default: games_dir in /etc/arcade/arcade.yaml]
    #[arg(long, global = true, env = "ARCADE_GAMES")]
    games: Option<PathBuf>,
    /// Read seat files under this folder instead of / (for tests)
    #[arg(long, global = true, env = "ARCADE_ROOT", default_value = "/")]
    root: PathBuf,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List the games this seat shows
    List {
        /// Also list games that are not installed or need a stronger tier
        #[arg(long)]
        all: bool,
        #[arg(long)]
        json: bool,
    },
    /// Check every manifest; exit code 1 if any has a problem
    Validate,
    /// Show what a launch would do, without doing it
    Plan(Target),
    /// Start a game
    Launch {
        #[command(flatten)]
        target: Target,
        /// Show the plan and stop
        #[arg(long)]
        dry_run: bool,
        /// Take a screenshot after this many seconds, then close the game (test mode)
        #[arg(long, value_name = "SECONDS", value_parser = parse_seconds)]
        screenshot_after: Option<u64>,
        #[arg(long, default_value = "/tmp/arcade-screenshot.png")]
        screenshot: PathBuf,
    },
    /// Show the nickname, or set it
    Nick { name: Option<String> },
    /// Close everything and reset the seat for the next person
    EndSession,
}

#[derive(Args)]
struct Target {
    /// Game id, the manifest's file name without .yaml
    id: String,
    /// Join this server, for example 192.0.2.20:7777
    #[arg(long, conflicts_with = "host")]
    join: Option<String>,
    /// Host a LAN game
    #[arg(long)]
    host: bool,
    /// Use this tier's settings instead of the seat's
    #[arg(long = "profile", value_name = "TIER")]
    tier: Option<Tier>,
    /// Pretend the screen is this size, for example 2560x1440@144
    #[arg(long)]
    display: Option<Display>,
    /// Nickname to use [default: the saved one, or "Player"]
    #[arg(long)]
    nick: Option<String>,
    #[arg(long)]
    json: bool,
}

fn parse_seconds(s: &str) -> Result<u64, String> {
    s.trim_end_matches('s')
        .parse()
        .map_err(|_| format!("{s:?} is not a number of seconds"))
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("arcade: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<ExitCode> {
    let cli = Cli::parse();
    let config = Config::load(&cli.root)?;
    let games_dir = cli
        .games
        .clone()
        .unwrap_or_else(|| config.games_dir.clone());
    let seat = Seat::detect(&cli.root);

    match cli.cmd {
        Cmd::List { all, json } => {
            let (games, problems) = manifest::load_all(&games_dir)?;
            for p in problems {
                eprintln!("skipped: {p}");
            }
            let cards: Vec<_> = games
                .iter()
                .filter(|m| all || (m.installed() && m.fits(seat.tier)))
                .map(|m| m.card(seat.tier, &seat.display))
                .collect();
            if json {
                println!("{}", serde_json::to_string_pretty(&cards)?);
            } else {
                println!("{} · Tier {} · {}", seat.name, seat.tier, seat.display);
                for c in &cards {
                    let mark = if c.installed { "" } else { "  (not installed)" };
                    println!(
                        "  {:<12} {:<26} {:<7} {}{mark}",
                        c.id, c.name, c.runtime, c.tuned
                    );
                }
            }
        }
        Cmd::Validate => {
            let report = manifest::validate_dir(&games_dir)?;
            for p in &report.problems {
                println!("FAIL  {p}");
            }
            println!(
                "{} manifests checked, {} problems",
                report.checked,
                report.problems.len()
            );
            if !report.problems.is_empty() {
                return Ok(ExitCode::from(1));
            }
        }
        Cmd::Plan(t) => {
            let p = make_plan(&t, &games_dir, &seat, &config)?;
            print_plan(&p, t.json)?;
        }
        Cmd::Launch {
            target,
            dry_run,
            screenshot_after,
            screenshot,
        } => {
            let p = make_plan(&target, &games_dir, &seat, &config)?;
            print_plan(&p, target.json)?;
            if dry_run {
                return Ok(ExitCode::SUCCESS);
            }
            plan::apply(&p)?;
            let mut child = plan::spawn(&p)?;
            match screenshot_after {
                Some(secs) => {
                    thread::sleep(Duration::from_secs(secs));
                    if let Some(status) = child.try_wait()? {
                        bail!("{} closed before the screenshot ({status})", p.name);
                    }
                    let shot = take_screenshot(&screenshot);
                    let _ = child.kill();
                    let _ = child.wait();
                    shot?;
                    println!("screenshot: {}", screenshot.display());
                }
                None => {
                    let status = child.wait()?;
                    if !status.success() {
                        bail!("{} exited with {status}", p.name);
                    }
                }
            }
        }
        Cmd::Nick { name } => match name {
            Some(n) => {
                nick::save(&seat.home, &n)?;
                println!("{}", n.trim());
            }
            None => match nick::load(&seat.home) {
                Some(n) => println!("{n}"),
                None => bail!("no nickname set yet"),
            },
        },
        Cmd::EndSession => {
            let status = Command::new("systemctl")
                .args(["start", "--no-block", "seat-reset.service"])
                .status()
                .context("running systemctl")?;
            if !status.success() {
                bail!("systemctl could not start seat-reset.service ({status})");
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn make_plan(t: &Target, games_dir: &Path, seat: &Seat, config: &Config) -> Result<Plan> {
    let m = Manifest::load_id(games_dir, &t.id)?;
    let mut seat = seat.clone();
    if let Some(d) = t.display {
        seat.display = d;
    }
    let nickname = t
        .nick
        .clone()
        .or_else(|| nick::load(&seat.home))
        .unwrap_or_else(|| "Player".into());
    let mode = match (&t.join, t.host) {
        (Some(server), _) => Mode::Join(server.clone()),
        (None, true) => Mode::Host,
        (None, false) => Mode::Play,
    };
    plan::build(&Request {
        manifest: &m,
        seat: &seat,
        config,
        games_dir,
        nickname: &nickname,
        mode,
        tier: t.tier,
    })
}

fn print_plan(p: &Plan, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(p)?);
        return Ok(());
    }
    println!(
        "{} · Tier {} · {}×{} · {} fps cap",
        p.name, p.tier, p.width, p.height, p.fps_cap
    );
    for s in &p.steps {
        println!("  step  {}: {}", s.title, s.detail);
    }
    for f in &p.files {
        println!("  write {}", f.path().display());
    }
    if let Some(dir) = &p.workdir {
        println!("  in    {}", dir.display());
    }
    println!("  run   {}", plan::shell_line(p));
    Ok(())
}

/// Screenshot of the whole screen with KDE's Spectacle (works on Wayland).
fn take_screenshot(path: &Path) -> Result<()> {
    let status = Command::new("spectacle")
        .args(["--background", "--nonotify", "--fullscreen", "--output"])
        .arg(path)
        .status()
        .context("running spectacle")?;
    if !status.success() {
        bail!("spectacle failed ({status})");
    }
    Ok(())
}

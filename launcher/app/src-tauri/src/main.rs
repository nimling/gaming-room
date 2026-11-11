//! The full-screen Arcade launcher. The screen is `../ui/index.html`;
//! this file gives it the seat, the games and the power to start them.
//!
//! Environment for testing off a seat:
//!   ARCADE_ROOT=<dir>   read seat files under <dir> instead of /
//!   ARCADE_GAMES=<dir>  game manifests folder
//!   ARCADE_WINDOWED=1   do not go full screen

use arcade_core::manifest::{self, Card, Manifest};
use arcade_core::plan::{self, Mode, Plan, Request};
use arcade_core::{lan, nick, ptt, Config, Seat};
use serde::Serialize;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use tauri::{Manager, State};

struct Arcade {
    root: PathBuf,
    config: Config,
    games_dir: PathBuf,
    game: Mutex<Option<(String, Child)>>,
}

#[derive(Serialize)]
struct Boot {
    seat_name: String,
    tier: String,
    display: String,
    gpu: String,
    nickname: Option<String>,
    ptt: Option<String>,
    voice: Option<String>,
    hosts: Vec<lan::Entry>,
    games: Vec<Card>,
}

fn text(e: anyhow::Error) -> String {
    format!("{e:#}")
}

#[tauri::command]
fn bootstrap(app: State<'_, Arcade>) -> Result<Boot, String> {
    let seat = Seat::detect(&app.root);
    let (games, problems) = manifest::load_all(&app.games_dir).map_err(text)?;
    for p in problems {
        eprintln!("arcade: skipped {p}");
    }
    let games = games
        .iter()
        .filter(|m| m.installed() && m.fits(seat.tier))
        .map(|m| m.card(seat.tier, &seat.display))
        .collect();
    Ok(Boot {
        seat_name: seat.name.clone(),
        tier: seat.tier.to_string(),
        display: seat.display.to_string(),
        gpu: seat.gpu.clone(),
        nickname: nick::load(&seat.home),
        ptt: ptt::load(&seat.home),
        voice: app.config.voice.as_ref().map(|v| v.channel.clone()),
        hosts: lan::entries(&app.config, &seat.name),
        games,
    })
}

/// Saves the nickname and joins voice under it. `key` is the push-to-talk
/// key the player picked on the same screen; `None` is the default.
#[tauri::command]
fn set_nickname(app: State<'_, Arcade>, name: String, key: Option<String>) -> Result<(), String> {
    let seat = Seat::detect(&app.root);
    nick::save(&seat.home, &name).map_err(text)?;
    if key.as_deref() != ptt::load(&seat.home).as_deref() {
        if let Some(k) = &key {
            ptt::validate(k).map_err(text)?;
        }
        // Mumble writes its settings when it quits, so it must be gone first.
        stop_mumble();
        ptt::save(&seat.home, key.as_deref()).map_err(text)?;
    }
    if let Some(url) = app.config.mumble_url(name.trim()) {
        Command::new("mumble")
            .arg(url)
            .spawn()
            .map_err(|e| format!("could not start Mumble: {e}"))?;
    }
    Ok(())
}

#[tauri::command]
fn launch(
    app: State<'_, Arcade>,
    id: String,
    mode: String,
    server: Option<String>,
) -> Result<Plan, String> {
    stop_game(&app);
    let seat = Seat::detect(&app.root);
    let m = Manifest::load_id(&app.games_dir, &id).map_err(text)?;
    let nickname = nick::load(&seat.home).ok_or("Choose a nickname first")?;
    let mode = match mode.as_str() {
        "join" => Mode::Join(server.ok_or("No server to join")?),
        "host" => Mode::Host,
        _ => Mode::Play,
    };
    let p = plan::build(&Request {
        manifest: &m,
        seat: &seat,
        config: &app.config,
        games_dir: &app.games_dir,
        nickname: &nickname,
        mode,
        tier: None,
    })
    .map_err(text)?;
    plan::apply(&p).map_err(text)?;
    let child = plan::spawn(&p).map_err(text)?;
    *app.game.lock().unwrap_or_else(|e| e.into_inner()) = Some((id, child));
    Ok(p)
}

/// Asks Mumble to quit and waits up to five seconds for it.
fn stop_mumble() {
    let running = || {
        Command::new("pgrep")
            .args(["-x", "mumble"])
            .output()
            .is_ok_and(|o| o.status.success())
    };
    if !running() {
        return;
    }
    let _ = Command::new("pkill").args(["-x", "mumble"]).status();
    for _ in 0..50 {
        if !running() {
            return;
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn stop_game(app: &Arcade) {
    if let Some((_, mut child)) = app.game.lock().unwrap_or_else(|e| e.into_inner()).take() {
        plan::stop(&mut child);
    }
}

#[tauri::command]
fn quit(app: State<'_, Arcade>) {
    stop_game(&app);
}

/// The id of the game that is still running, if any.
#[tauri::command]
fn running(app: State<'_, Arcade>) -> Option<String> {
    let mut game = app.game.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((id, child)) = game.as_mut() {
        if plan::alive(child) {
            return Some(id.clone());
        }
    }
    *game = None;
    None
}

#[tauri::command]
fn end_session(app: State<'_, Arcade>) -> Result<(), String> {
    stop_game(&app);
    let status = Command::new("systemctl")
        .args(["start", "--no-block", "seat-reset.service"])
        .status()
        .map_err(|e| format!("could not run systemctl: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("the seat reset did not start ({status})"))
    }
}

fn main() {
    let root = std::env::var_os("ARCADE_ROOT").map_or_else(|| PathBuf::from("/"), PathBuf::from);
    let config = Config::load(&root).unwrap_or_else(|e| {
        eprintln!("arcade: {e:#}; using default settings");
        Config::default()
    });
    let games_dir =
        std::env::var_os("ARCADE_GAMES").map_or_else(|| config.games_dir.clone(), PathBuf::from);

    tauri::Builder::default()
        .manage(Arcade {
            root,
            config,
            games_dir,
            game: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            set_nickname,
            launch,
            quit,
            running,
            end_session
        ])
        .setup(|app| {
            if std::env::var_os("ARCADE_WINDOWED").is_some() {
                if let Some(w) = app.get_webview_window("main") {
                    w.set_fullscreen(false)?;
                }
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("the Arcade window could not start");
}

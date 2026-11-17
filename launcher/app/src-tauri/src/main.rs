//! The full-screen Arcade launcher. The screen is `../ui/index.html`;
//! this file gives it the seat, the games and the power to start them.
//!
//! Environment for testing off a seat:
//!   ARCADE_ROOT=<dir>   read seat files under <dir> instead of /
//!   ARCADE_GAMES=<dir>  game manifests folder
//!   ARCADE_WINDOWED=1   do not go full screen

use arcade_core::manifest::{self, Card, Manifest};
use arcade_core::music::{self, Control, NowPlaying};
use arcade_core::plan::{self, Mode, Plan, Request};
use arcade_core::presence::{self, Beat, Person};
use arcade_core::{browse, lan, nick, ptt, Config, Seat};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

struct Arcade {
    root: PathBuf,
    config: Config,
    games_dir: PathBuf,
    game: Mutex<Option<Running>>,
}

/// The game this launcher started.
struct Running {
    id: String,
    child: Child,
    /// What this seat tells the People list while the game runs.
    beat: Beat,
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
/// Runs off the main thread: stopping Mumble can take up to 5 s, and a sync
/// command would freeze the window that long.
#[tauri::command(async)]
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

// Async: stopping a running game can take up to 3 s.
#[tauri::command(async)]
fn launch(
    handle: AppHandle,
    app: State<'_, Arcade>,
    id: String,
    mode: String,
    server: Option<String>,
) -> Result<Plan, String> {
    stop_game(&app);
    let seat = Seat::detect(&app.root);
    let m = Manifest::load_id(&app.games_dir, &id).map_err(text)?;
    let nickname = nick::load(&seat.home).ok_or("Choose a nickname first")?;
    let beat = Beat {
        seat: seat.name.clone(),
        nickname: nickname.clone(),
        game: Some(id.clone()),
        mode: Some(
            match mode.as_str() {
                "join" | "host" => mode.as_str(),
                _ => "play",
            }
            .to_string(),
        ),
        // Only an ip:port can be joined from another seat.
        address: server.clone().filter(|_| mode == "join").filter(|s| {
            s.rsplit_once(':')
                .is_some_and(|(_, p)| p.parse::<u16>().is_ok())
        }),
    };
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
    report(&app.config, beat.clone());
    *app.game.lock().unwrap_or_else(|e| e.into_inner()) = Some(Running {
        id: id.clone(),
        child,
        beat,
    });
    watch_game(handle, id);
    Ok(p)
}

/// Sends a heartbeat to the People list off the calling thread.
fn report(config: &Config, beat: Beat) {
    if let Some(base) = config.gameservers.clone() {
        thread::spawn(move || {
            if let Err(e) = presence::report(&base, &beat) {
                eprintln!("arcade: people list: {e:#}");
            }
        });
    }
}

/// How often the game watcher reports while a game runs, in its 2 s ticks.
const BEAT_TICKS: u32 = 5;

/// Tells the screen when the game ends, as a `game-ended` event with its id.
/// The screen then needs no timer of its own while the game runs: the
/// webview stays asleep, and this thread costs one system call every 2 s,
/// plus one small heartbeat to the People list every 10 s.
fn watch_game(handle: AppHandle, id: String) {
    thread::spawn(move || {
        let mut tick = 0;
        loop {
            thread::sleep(Duration::from_secs(2));
            tick += 1;
            let app = handle.state::<Arcade>();
            let mut game = app.game.lock().unwrap_or_else(|e| e.into_inner());
            let beat = match game.as_mut() {
                Some(r) if r.id == id => {
                    if plan::alive(&mut r.child) {
                        let beat = (tick % BEAT_TICKS == 0).then(|| r.beat.clone());
                        drop(game);
                        if let Some(b) = beat {
                            report(&app.config, b);
                        }
                        continue;
                    }
                    game.take().map(|r| r.beat)
                }
                // Quit from the screen or replaced by another game: the screen knows.
                _ => return,
            };
            drop(game);
            if let Some(b) = beat {
                report(&app.config, b.idle());
            }
            let _ = handle.emit("game-ended", &id);
            return;
        }
    });
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

/// Stops the running game; returns what it reported to the People list.
fn stop_game(app: &Arcade) -> Option<Beat> {
    let running = app.game.lock().unwrap_or_else(|e| e.into_inner()).take();
    running.map(|mut r| {
        plan::stop(&mut r.child);
        r.beat
    })
}

// Async: stopping a running game can take up to 3 s.
#[tauri::command(async)]
fn quit(app: State<'_, Arcade>) {
    if let Some(beat) = stop_game(&app) {
        report(&app.config, beat.idle());
    }
}

/// The id of the game that is still running, if any.
#[tauri::command]
fn running(app: State<'_, Arcade>) -> Option<String> {
    let mut game = app.game.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(r) = game.as_mut() {
        if plan::alive(&mut r.child) {
            return Some(r.id.clone());
        }
    }
    *game = None;
    None
}

#[derive(Serialize)]
struct Lan {
    /// Games running on the LAN, per game id.
    games: BTreeMap<String, Vec<browse::Game>>,
    /// Everyone in a launcher on the gaming VLAN, with the server to join.
    people: Vec<Person>,
}

/// What this seat reports now: the running game, or the player in the
/// launcher. `None` before a nickname is chosen.
fn current_beat(app: &Arcade) -> Option<Beat> {
    let game = app.game.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(r) = game.as_ref() {
        return Some(r.beat.clone());
    }
    drop(game);
    let seat = Seat::detect(&app.root);
    let nickname = nick::load(&seat.home)?;
    Some(Beat {
        seat: seat.name,
        nickname,
        game: None,
        mode: None,
        address: None,
    })
}

/// "On the LAN" and the People list. The screen asks every few seconds
/// while it is awake, so nothing polls while a game or another window has
/// the screen; the same request is this seat's heartbeat.
// Async: one scan listens for half a second.
#[tauri::command(async)]
fn lan_games(app: State<'_, Arcade>) -> Lan {
    let config = &app.config;
    let beat = current_beat(&app);
    let (games, entries) = thread::scope(|s| {
        let people = s.spawn(|| {
            let base = config.gameservers.as_deref()?;
            let list = match &beat {
                Some(b) => presence::report(base, b),
                None => presence::list(base),
            };
            list.map_err(|e| eprintln!("arcade: people list: {e:#}"))
                .ok()
        });
        let games = browse::scan(config);
        (games, people.join().ok().flatten().unwrap_or_default())
    });
    let people = presence::resolve(&entries, &games, browse::own_ip(config));
    Lan { games, people }
}

#[tauri::command]
fn music_open() -> Result<(), String> {
    music::open().map_err(text)
}

/// What Spotify plays; `None` when it is not open.
// Async: playerctl waits on D-Bus.
#[tauri::command(async)]
fn music_status() -> Option<NowPlaying> {
    music::now_playing()
}

// Async: playerctl waits on D-Bus.
#[tauri::command(async)]
fn music_control(action: String) -> Result<(), String> {
    music::control(Control::parse(&action).map_err(text)?).map_err(text)
}

// Async: stopping a running game can take up to 3 s.
#[tauri::command(async)]
fn end_session(app: State<'_, Arcade>) -> Result<(), String> {
    stop_game(&app);
    // Off the People list at once; the reset does not wait for it.
    if let Some(base) = app.config.gameservers.clone() {
        thread::spawn(move || presence::leave(&base));
    }
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
            lan_games,
            end_session,
            music_open,
            music_status,
            music_control
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

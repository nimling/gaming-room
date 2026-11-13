//! Spotify for the player. The app is the official client from Flathub; the
//! player signs in with their own account and the seat reset signs them out.
//! The launcher's mini player reads and steers it over MPRIS with `playerctl`.

use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::process::{Command, Stdio};

/// The Flatpak app id of the Spotify client.
pub const APP_ID: &str = "com.spotify.Client";

/// Spotify's MPRIS player name, for `playerctl --player`.
const PLAYER: &str = "spotify";

/// Separates the fields in playerctl's output. Song titles never hold it.
const SEP: char = '\u{1f}';

/// What Spotify is doing now, for the status bar and the music flyout.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NowPlaying {
    /// "Playing", "Paused" or "Stopped"
    pub status: String,
    pub title: String,
    pub artist: String,
}

/// The buttons in the mini player.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Control {
    PlayPause,
    Next,
    Previous,
}

impl Control {
    pub fn parse(s: &str) -> Result<Self> {
        Ok(match s {
            "play-pause" => Self::PlayPause,
            "next" => Self::Next,
            "previous" => Self::Previous,
            _ => bail!("unknown music control {s:?}"),
        })
    }

    fn verb(self) -> &'static str {
        match self {
            Self::PlayPause => "play-pause",
            Self::Next => "next",
            Self::Previous => "previous",
        }
    }
}

/// Starts Spotify, or brings the open window forward.
pub fn open() -> Result<()> {
    Command::new("flatpak")
        .args(["run", APP_ID])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("could not start Spotify")?;
    Ok(())
}

/// What Spotify plays, or `None` when it is not running.
pub fn now_playing() -> Option<NowPlaying> {
    let format = ["{{status}}", "{{xesam:title}}", "{{xesam:artist}}"].join(&SEP.to_string());
    let out = Command::new("playerctl")
        .args(["--player", PLAYER, "metadata", "--format", &format])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    parse(&String::from_utf8_lossy(&out.stdout))
}

/// Presses a mini player button.
pub fn control(c: Control) -> Result<()> {
    let status = Command::new("playerctl")
        .args(["--player", PLAYER, c.verb()])
        .stderr(Stdio::null())
        .status()
        .context("could not run playerctl")?;
    if !status.success() {
        bail!("Spotify is not open");
    }
    Ok(())
}

fn parse(out: &str) -> Option<NowPlaying> {
    let mut f = out.trim_end_matches('\n').split(SEP);
    let status = f.next().filter(|s| !s.is_empty())?.to_string();
    Some(NowPlaying {
        status,
        title: f.next().unwrap_or_default().to_string(),
        artist: f.next().unwrap_or_default().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_playing_song() {
        let np = parse("Playing\u{1f}Kids\u{1f}MGMT\n").unwrap();
        assert_eq!(np.status, "Playing");
        assert_eq!(np.title, "Kids");
        assert_eq!(np.artist, "MGMT");
    }

    #[test]
    fn keeps_tabs_and_dots_in_titles() {
        let np = parse("Paused\u{1f}Song 2\t(Remastered) · Live\u{1f}Blur").unwrap();
        assert_eq!(np.title, "Song 2\t(Remastered) · Live");
    }

    #[test]
    fn spotify_open_with_nothing_queued() {
        let np = parse("Stopped\u{1f}\u{1f}\n").unwrap();
        assert_eq!(np.status, "Stopped");
        assert!(np.title.is_empty() && np.artist.is_empty());
    }

    #[test]
    fn no_output_is_no_player() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("\n"), None);
    }

    #[test]
    fn controls() {
        assert_eq!(Control::parse("next").unwrap(), Control::Next);
        assert_eq!(Control::parse("play-pause").unwrap().verb(), "play-pause");
        assert!(Control::parse("volume").is_err());
    }
}

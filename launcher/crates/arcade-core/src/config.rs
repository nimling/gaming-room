//! Seat-wide launcher settings from /etc/arcade/arcade.yaml (written by Ansible).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    /// Folder with the game manifests and their templates.
    pub games_dir: PathBuf,
    /// Pinned Proton build for Windows games (PROTONPATH for umu-run).
    pub proton: Option<PathBuf>,
    /// Mumble server to join after the nickname step.
    pub voice: Option<Voice>,
    /// Every seat and the server on the gaming VLAN, for the IP addresses
    /// screen. Ansible fills it from the private inventory.
    pub hosts: Vec<Host>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Host {
    pub name: String,
    pub address: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Voice {
    pub server: String,
    #[serde(default = "lobby")]
    pub channel: String,
}

fn lobby() -> String {
    "Lobby".into()
}

impl Default for Config {
    fn default() -> Self {
        Config {
            games_dir: PathBuf::from("/usr/local/share/arcade/games"),
            proton: None,
            voice: None,
            hosts: Vec::new(),
        }
    }
}

impl Config {
    /// Reads `<root>/etc/arcade/arcade.yaml`. A missing file gives the defaults.
    pub fn load(root: &Path) -> Result<Config> {
        let path = root.join("etc/arcade/arcade.yaml");
        if !path.exists() {
            return Ok(Config::default());
        }
        let text =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        serde_yaml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }

    /// `mumble://` link that joins the voice channel under this nickname.
    pub fn mumble_url(&self, nickname: &str) -> Option<String> {
        self.voice.as_ref().map(|v| {
            format!(
                "mumble://{nickname}@{}/{}?version=1.2.0",
                v.server, v.channel
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let c = Config::load(dir.path()).unwrap();
        assert!(c.voice.is_none());
        assert_eq!(c.games_dir, PathBuf::from("/usr/local/share/arcade/games"));
    }

    #[test]
    fn mumble_url_uses_nickname_and_channel() {
        let c: Config = serde_yaml::from_str("voice: { server: gaming-server }").unwrap();
        assert_eq!(
            c.mumble_url("Kaptein").unwrap(),
            "mumble://Kaptein@gaming-server/Lobby?version=1.2.0"
        );
    }
}

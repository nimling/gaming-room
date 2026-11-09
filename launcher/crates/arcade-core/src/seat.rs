//! Facts about this seat, read from files the Ansible roles write.

use crate::manifest::Tier;
use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Display {
    pub width: u32,
    pub height: u32,
    pub refresh: u32,
}

impl Default for Display {
    fn default() -> Self {
        Display {
            width: 1920,
            height: 1080,
            refresh: 60,
        }
    }
}

impl fmt::Display for Display {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}×{} · {} Hz", self.width, self.height, self.refresh)
    }
}

/// Parses `2560x1440@144` or `2560x1440` (60 Hz).
impl FromStr for Display {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        let s = s.trim();
        let (size, refresh) = match s.split_once('@') {
            Some((size, hz)) => (
                size,
                hz.trim().parse().context("refresh rate is not a number")?,
            ),
            None => (s, 60),
        };
        let Some((width, height)) = crate::manifest::parse_size(size) else {
            bail!("display {s:?} is not like 2560x1440@144");
        };
        if refresh == 0 {
            bail!("refresh rate must be above 0");
        }
        Ok(Display {
            width,
            height,
            refresh,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Seat {
    /// What the screen calls this seat, for example "Seat 04".
    pub name: String,
    pub tier: Tier,
    pub display: Display,
    /// Short graphics description, for example "AMD · 8 GB".
    pub gpu: String,
    pub home: PathBuf,
}

impl Seat {
    /// Reads `/etc/seat/tier`, `/etc/seat/display`, `/etc/seat/hardware.json`
    /// and `/etc/hostname` under `root`. Missing files give safe defaults:
    /// tier C (the weakest) and 1920×1080 at 60 Hz.
    pub fn detect(root: &Path) -> Seat {
        let read = |p: &str| {
            fs::read_to_string(root.join(p))
                .ok()
                .map(|s| s.trim().to_string())
        };
        let tier = read("etc/seat/tier")
            .and_then(|t| t.parse().ok())
            .unwrap_or(Tier::C);
        let display = read("etc/seat/display")
            .and_then(|d| d.parse().ok())
            .unwrap_or_default();
        let name = read("etc/hostname")
            .map(|h| seat_name(&h))
            .unwrap_or_else(|| "This seat".into());
        let gpu = read("etc/seat/hardware.json")
            .map(|j| gpu_label(&j))
            .unwrap_or_else(|| "Unknown".into());
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| "/home/player".into());
        Seat {
            name,
            tier,
            display,
            gpu,
            home,
        }
    }
}

fn seat_name(hostname: &str) -> String {
    match hostname.strip_prefix("seat-") {
        Some(n) if !n.is_empty() => format!("Seat {n}"),
        _ => hostname.to_string(),
    }
}

fn gpu_label(json: &str) -> String {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else {
        return "Unknown".into();
    };
    let vendor = match v["gpu_vendor"].as_str().unwrap_or("unknown") {
        "nvidia" => "NVIDIA",
        "amd" => "AMD",
        "intel" => "Intel",
        _ => "Unknown",
    };
    match v["vram_mib"].as_u64() {
        Some(mib) if mib > 0 => format!("{vendor} · {} GB", (mib + 512) / 1024),
        _ => vendor.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_display() {
        assert_eq!(
            "2560x1440@144".parse::<Display>().unwrap(),
            Display {
                width: 2560,
                height: 1440,
                refresh: 144
            }
        );
        assert_eq!("1920x1080".parse::<Display>().unwrap().refresh, 60);
        assert!("big".parse::<Display>().is_err());
    }

    #[test]
    fn detects_from_files() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("etc/seat")).unwrap();
        fs::write(root.path().join("etc/seat/tier"), "B\n").unwrap();
        fs::write(root.path().join("etc/seat/display"), "2560x1440@144\n").unwrap();
        fs::write(
            root.path().join("etc/seat/hardware.json"),
            r#"{"gpu_vendor":"amd","vram_mib":8176}"#,
        )
        .unwrap();
        fs::write(root.path().join("etc/hostname"), "seat-04\n").unwrap();
        let s = Seat::detect(root.path());
        assert_eq!(s.tier, Tier::B);
        assert_eq!(s.display.refresh, 144);
        assert_eq!(s.name, "Seat 04");
        assert_eq!(s.gpu, "AMD · 8 GB");
    }

    #[test]
    fn missing_files_give_safe_defaults() {
        let root = tempfile::tempdir().unwrap();
        let s = Seat::detect(root.path());
        assert_eq!(s.tier, Tier::C);
        assert_eq!(s.display, Display::default());
    }
}

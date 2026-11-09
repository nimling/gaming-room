//! The player's nickname. It lives in the player's home, so the seat
//! reset at End session forgets it.

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// 2 to 16 characters: ASCII letters, digits, `-` or `_`. Old games and
/// config files break on anything else.
pub fn validate(name: &str) -> Result<()> {
    let n = name.trim();
    if n.len() < 2 || n.len() > 16 {
        bail!("Use 2 to 16 characters.");
    }
    if !n
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        bail!("Use only letters A-Z, numbers, - or _.");
    }
    Ok(())
}

pub fn path(home: &Path) -> PathBuf {
    home.join(".config/arcade/nickname")
}

pub fn load(home: &Path) -> Option<String> {
    let text = fs::read_to_string(path(home)).ok()?;
    let n = text.trim().to_string();
    validate(&n).ok().map(|_| n)
}

pub fn save(home: &Path, name: &str) -> Result<()> {
    validate(name)?;
    let p = path(home);
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    fs::write(&p, format!("{}\n", name.trim())).with_context(|| format!("writing {}", p.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_plain_names() {
        for n in ["Lyn", "Kaptein_99", "no-scope", "AB"] {
            assert!(validate(n).is_ok(), "{n}");
        }
    }

    #[test]
    fn refuses_bad_names() {
        for n in ["", "A", "seventeen-chars-x", "Bjørn", "a b", "x;rm"] {
            assert!(validate(n).is_err(), "{n}");
        }
    }

    #[test]
    fn saves_and_loads() {
        let home = tempfile::tempdir().unwrap();
        assert!(load(home.path()).is_none());
        save(home.path(), "Pixel").unwrap();
        assert_eq!(load(home.path()).as_deref(), Some("Pixel"));
    }
}

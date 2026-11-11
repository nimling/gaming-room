//! The player's push-to-talk key. The golden home gives Mumble the mouse
//! side button and V; on the name screen the player can pick one key or
//! button instead. Both files live in the player's home, so the seat
//! reset at End session brings back the default.

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

/// Keys and buttons a player can pick, by their browser `KeyboardEvent.code`
/// (or our own name for a mouse button), with the Linux event code Mumble
/// reads from evdev. Enter, Esc, Space, Tab and the arrows are left out:
/// the launcher needs them.
const KEYS: &[(&str, u16)] = &[
    ("MouseBack", 275),
    ("MouseForward", 276),
    ("MouseMiddle", 274),
    ("KeyQ", 16),
    ("KeyW", 17),
    ("KeyE", 18),
    ("KeyR", 19),
    ("KeyT", 20),
    ("KeyY", 21),
    ("KeyU", 22),
    ("KeyI", 23),
    ("KeyO", 24),
    ("KeyP", 25),
    ("KeyA", 30),
    ("KeyS", 31),
    ("KeyD", 32),
    ("KeyF", 33),
    ("KeyG", 34),
    ("KeyH", 35),
    ("KeyJ", 36),
    ("KeyK", 37),
    ("KeyL", 38),
    ("KeyZ", 44),
    ("KeyX", 45),
    ("KeyC", 46),
    ("KeyV", 47),
    ("KeyB", 48),
    ("KeyN", 49),
    ("KeyM", 50),
    ("Digit1", 2),
    ("Digit2", 3),
    ("Digit3", 4),
    ("Digit4", 5),
    ("Digit5", 6),
    ("Digit6", 7),
    ("Digit7", 8),
    ("Digit8", 9),
    ("Digit9", 10),
    ("Digit0", 11),
    ("F1", 59),
    ("F2", 60),
    ("F3", 61),
    ("F4", 62),
    ("F5", 63),
    ("F6", 64),
    ("F7", 65),
    ("F8", 66),
    ("F9", 67),
    ("F10", 68),
    ("F11", 87),
    ("F12", 88),
    ("Backquote", 41),
    ("CapsLock", 58),
    ("ShiftLeft", 42),
    ("ShiftRight", 54),
    ("ControlLeft", 29),
    ("ControlRight", 97),
    ("AltLeft", 56),
    ("AltRight", 100),
];

/// The golden home's default: mouse side button and V.
const DEFAULT: &[u16] = &[275, 47];

/// Mumble's data for the push-to-talk action (a serialised QVariant).
const PTT_DATA: &str = "AAAAAAE=";

fn event_code(key: &str) -> Result<u16> {
    match KEYS.iter().find(|(k, _)| *k == key) {
        Some((_, c)) => Ok(*c),
        None => bail!("{key} cannot be used for push to talk."),
    }
}

pub fn validate(key: &str) -> Result<()> {
    event_code(key).map(|_| ())
}

/// A Linux event code as Mumble stores a button: the code + 8, as a
/// serialised QVariant int (type 2, not null, big-endian), in base64.
fn mumble_button(code: u16) -> String {
    let v = u32::from(code) + 8;
    let mut b = vec![0, 0, 0, 2, 0];
    b.extend_from_slice(&v.to_be_bytes());
    base64(&b)
}

fn base64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in bytes.chunks(3) {
        let n = c
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | (u32::from(b) << (16 - 8 * i)));
        for i in 0..4 {
            out.push(if i <= c.len() {
                A[((n >> (18 - 6 * i)) & 63) as usize] as char
            } else {
                '='
            });
        }
    }
    out
}

fn shortcuts(codes: &[u16]) -> Value {
    let defined: Vec<Value> = codes
        .iter()
        .map(|&c| json!({ "index": 1, "suppress": false, "buttons": [mumble_button(c)], "data": PTT_DATA }))
        .collect();
    json!({ "enable_evdev": true, "defined": defined })
}

pub fn path(home: &Path) -> PathBuf {
    home.join(".config/arcade/ptt")
}

pub fn mumble_settings(home: &Path) -> PathBuf {
    home.join(".config/Mumble/Mumble/mumble_settings.json")
}

/// The player's own key, or `None` for the default.
pub fn load(home: &Path) -> Option<String> {
    let text = fs::read_to_string(path(home)).ok()?;
    let k = text.trim().to_string();
    validate(&k).ok().map(|_| k)
}

/// Saves the key (`None` brings back the default) and writes it into
/// Mumble's settings. Mumble rewrites that file when it quits, so call
/// this while Mumble is not running.
pub fn save(home: &Path, key: Option<&str>) -> Result<()> {
    let codes = match key {
        Some(k) => vec![event_code(k)?],
        None => DEFAULT.to_vec(),
    };
    let p = mumble_settings(home);
    let mut settings: Value = match fs::read_to_string(&p) {
        Ok(text) => {
            serde_json::from_str(&text).with_context(|| format!("reading {}", p.display()))?
        }
        Err(_) => json!({}),
    };
    let Some(obj) = settings.as_object_mut() else {
        bail!("{} is not a JSON object", p.display());
    };
    obj.insert("shortcuts".into(), shortcuts(&codes));
    write(&p, &(serde_json::to_string_pretty(&settings)? + "\n"))?;

    let own = path(home);
    match key {
        Some(k) => write(&own, &format!("{k}\n")),
        None => match fs::remove_file(&own) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                Err(e).with_context(|| format!("removing {}", own.display()))
            }
            _ => Ok(()),
        },
    }
}

fn write(p: &Path, text: &str) -> Result<()> {
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    fs::write(p, text).with_context(|| format!("writing {}", p.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_like_the_golden_settings() {
        // The values in ansible/roles/launcher/files/mumble_settings.json.
        assert_eq!(mumble_button(275), "AAAAAgAAAAEb");
        assert_eq!(mumble_button(47), "AAAAAgAAAAA3");
    }

    #[test]
    fn refuses_keys_the_launcher_needs() {
        for k in ["Enter", "Escape", "Space", "Tab", "ArrowUp", "", "KeyV; rm"] {
            assert!(validate(k).is_err(), "{k}");
        }
        assert!(validate("MouseForward").is_ok());
    }

    #[test]
    fn writes_mumble_and_keeps_other_settings() {
        let home = tempfile::tempdir().unwrap();
        let h = home.path();
        write(&mumble_settings(h), r#"{"audio":{"transmit_mode":"PTT"}}"#).unwrap();

        save(h, Some("KeyB")).unwrap();
        assert_eq!(load(h).as_deref(), Some("KeyB"));
        let s: Value =
            serde_json::from_str(&fs::read_to_string(mumble_settings(h)).unwrap()).unwrap();
        assert_eq!(s["audio"]["transmit_mode"], "PTT");
        assert_eq!(s["shortcuts"]["defined"].as_array().unwrap().len(), 1);
        assert_eq!(
            s["shortcuts"]["defined"][0]["buttons"][0],
            mumble_button(48)
        );

        save(h, None).unwrap();
        assert!(load(h).is_none());
        let s: Value =
            serde_json::from_str(&fs::read_to_string(mumble_settings(h)).unwrap()).unwrap();
        assert_eq!(s["shortcuts"]["defined"].as_array().unwrap().len(), 2);
    }
}

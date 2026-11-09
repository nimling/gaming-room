//! Sets keys in an INI file and keeps everything else as it was.
//! Old games (Unreal, many DOS ports) keep settings in INI files;
//! replacing the whole file would lose the player's other settings.

use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub type Sections = BTreeMap<String, BTreeMap<String, String>>;

/// Returns `text` with every key in `set` written. Section and key names
/// match without case, as the games read them. Existing keys are replaced
/// in place; new keys go at the end of their section; new sections go at
/// the end of the file.
pub fn merge(text: &str, set: &Sections) -> String {
    let nl = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();

    for (section, keys) in set {
        let header = format!("[{section}]");
        match lines
            .iter()
            .position(|l| l.trim().eq_ignore_ascii_case(&header))
        {
            None => {
                if lines.last().is_some_and(|l| !l.trim().is_empty()) {
                    lines.push(String::new());
                }
                lines.push(header);
                lines.extend(keys.iter().map(|(k, v)| format!("{k}={v}")));
            }
            Some(start) => {
                let end = lines[start + 1..]
                    .iter()
                    .position(|l| l.trim_start().starts_with('['))
                    .map_or(lines.len(), |i| start + 1 + i);
                let mut insert_at = end;
                while insert_at > start + 1 && lines[insert_at - 1].trim().is_empty() {
                    insert_at -= 1;
                }
                let mut new_keys = Vec::new();
                for (k, v) in keys {
                    let found = (start + 1..end).find(|&i| {
                        lines[i]
                            .split_once('=')
                            .is_some_and(|(key, _)| key.trim().eq_ignore_ascii_case(k))
                    });
                    match found {
                        Some(i) => lines[i] = format!("{k}={v}"),
                        None => new_keys.push(format!("{k}={v}")),
                    }
                }
                for (j, line) in new_keys.into_iter().enumerate() {
                    lines.insert(insert_at + j, line);
                }
            }
        }
    }

    let mut out = lines.join(nl);
    out.push_str(nl);
    out
}

/// Merges `set` into the file at `path`, creating it and its folder if needed.
pub fn merge_file(path: &Path, set: &Sections) -> Result<()> {
    let old = if path.exists() {
        fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?
    } else {
        String::new()
    };
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    fs::write(path, merge(&old, set)).with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(section: &str, pairs: &[(&str, &str)]) -> Sections {
        let mut s = Sections::new();
        s.insert(
            section.into(),
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        );
        s
    }

    #[test]
    fn replaces_existing_key_and_keeps_the_rest() {
        let text = "[DefaultPlayer]\nName=Player\nTeam=255\n\n[Other]\nA=1\n";
        let out = merge(text, &set("DefaultPlayer", &[("Name", "Kaptein")]));
        assert_eq!(
            out,
            "[DefaultPlayer]\nName=Kaptein\nTeam=255\n\n[Other]\nA=1\n"
        );
    }

    #[test]
    fn matches_without_case() {
        let out = merge(
            "[defaultplayer]\nname=Player\n",
            &set("DefaultPlayer", &[("Name", "Lyn")]),
        );
        assert_eq!(out, "[defaultplayer]\nName=Lyn\n");
    }

    #[test]
    fn adds_missing_key_at_end_of_section() {
        let text = "[A]\nX=1\n\n[B]\nY=2\n";
        let out = merge(text, &set("A", &[("Z", "3")]));
        assert_eq!(out, "[A]\nX=1\nZ=3\n\n[B]\nY=2\n");
    }

    #[test]
    fn adds_missing_section_at_end() {
        let out = merge("[A]\nX=1\n", &set("B", &[("Y", "2")]));
        assert_eq!(out, "[A]\nX=1\n\n[B]\nY=2\n");
    }

    #[test]
    fn keeps_windows_line_endings() {
        let out = merge("[A]\r\nX=1\r\n", &set("A", &[("X", "2")]));
        assert_eq!(out, "[A]\r\nX=2\r\n");
    }

    #[test]
    fn creates_file_and_folder() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("System/User.ini");
        merge_file(&path, &set("DefaultPlayer", &[("Name", "Mora")])).unwrap();
        assert_eq!(
            fs::read_to_string(path).unwrap(),
            "[DefaultPlayer]\nName=Mora\n"
        );
    }
}

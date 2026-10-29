//! The matugen palette behind the "matugen" theme: a JSON object of colour
//! roles (`"primary": "#ffb4a8"`, ...). Ryoku writes one to
//! `~/.cache/ryoku/colors.json` on every wallpaper change; elsewhere a matugen
//! template (packaging/matugen) writes `~/.config/<app>/colors.json`. The file
//! is polled, and every window is told when it changes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, Emitter};

/// How often the palette file is checked for a change.
const POLL: Duration = Duration::from_secs(2);
/// Largest palette file read.
const MAX_BYTES: u64 = 64 * 1024;

pub type Palette = BTreeMap<String, String>;

/// Where the palette is read from: the user's own path, Ryoku's, else ours.
pub fn palette_path(custom: &str) -> Option<PathBuf> {
    let custom = custom.trim();
    if !custom.is_empty() {
        return Some(expand_home(custom));
    }
    let ryoku = dirs::cache_dir()?.join("ryoku/colors.json");
    if ryoku.exists() {
        return Some(ryoku);
    }
    Some(dirs::config_dir()?.join(crate::settings::APP_DIR).join("colors.json"))
}

fn expand_home(p: &str) -> PathBuf {
    match (p.strip_prefix("~/"), dirs::home_dir()) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => PathBuf::from(p),
    }
}

/// The `#rrggbb` roles of a palette JSON object; anything else is dropped.
fn parse(json: &str) -> Option<Palette> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let obj = v.as_object()?;
    let pal: Palette = obj
        .iter()
        .filter_map(|(k, v)| {
            let s = v.as_str()?.trim();
            let hex = s.strip_prefix('#')?;
            (hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit())).then(|| (k.clone(), s.to_ascii_lowercase()))
        })
        .collect();
    (!pal.is_empty()).then_some(pal)
}

pub fn read_palette(path: &Path) -> Option<Palette> {
    if std::fs::metadata(path).ok()?.len() > MAX_BYTES {
        return None;
    }
    parse(&std::fs::read_to_string(path).ok()?)
}

/// Poll the palette file; on a change, send `theme://palette` to every window.
pub fn watch(app: AppHandle, shared: crate::host::Shared) {
    let spawned = std::thread::Builder::new().name("palette".into()).spawn(move || {
        let mut last: Option<(PathBuf, SystemTime)> = None;
        loop {
            std::thread::sleep(POLL);
            let custom = shared.lock().map(|s| s.settings.palette_path.clone()).unwrap_or_default();
            let Some(path) = palette_path(&custom) else { continue };
            let Ok(modified) = std::fs::metadata(&path).and_then(|m| m.modified()) else { continue };
            let now = Some((path.clone(), modified));
            if now == last {
                continue;
            }
            // The first look only records the state: windows read the palette when they load.
            let first = last.is_none();
            last = now;
            if first {
                continue;
            }
            if let Some(pal) = read_palette(&path) {
                let _ = app.emit("theme://palette", &pal);
            }
        }
    });
    if let Err(e) = spawned {
        log::error!("palette watcher thread: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_hex_roles_and_drops_the_rest() {
        let p = parse(r##"{"primary": "#FFB4A8", "onPrimary": "#561e16", "name": "x", "bad": "#12345", "n": 3}"##).unwrap();
        assert_eq!(p.get("primary").map(String::as_str), Some("#ffb4a8"));
        assert_eq!(p.get("onPrimary").map(String::as_str), Some("#561e16"));
        assert_eq!(p.len(), 2);
        assert!(parse("[]").is_none());
        assert!(parse(r#"{"a": "red"}"#).is_none());
        assert!(parse("not json").is_none());
    }

    #[test]
    fn a_custom_path_wins_and_expands_home() {
        let p = palette_path("~/x/colors.json").unwrap();
        assert!(p.ends_with("x/colors.json") && !p.starts_with("~"));
        assert_eq!(palette_path("/tmp/p.json").unwrap(), PathBuf::from("/tmp/p.json"));
    }
}

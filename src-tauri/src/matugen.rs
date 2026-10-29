//! First-time set-up for the "Matugen" theme.
//!
//! On Ryoku there is nothing to do: Ryoku writes its wallpaper palette to
//! `~/.cache/ryoku/colors.json` and the theme reads it (Ryoku's matugen config is
//! never touched). Elsewhere one button installs Oumu's template into
//! `~/.config/matugen/templates/` and registers it in `~/.config/matugen/config.toml`
//! (backed up first, appended to, never rewritten), so every `matugen` run also
//! writes `~/.config/oumu/colors.json`.

use serde::Serialize;
use std::path::{Path, PathBuf};

/// The template matugen renders into the palette file (Material roles as hex).
const TEMPLATE: &str = include_str!("../../packaging/matugen/colors.json");
const TEMPLATE_NAME: &str = "oumu-colors.json";
const SECTION: &str = "[templates.oumu]";
/// What to run once after set-up (any wallpaper image works; `matugen color hex "#6750a4"` too).
const FIRST_RUN: &str = "matugen image /path/to/your/wallpaper.jpg";

#[derive(Debug, Serialize, PartialEq)]
pub struct Status {
    /// Ryoku provides the palette: nothing to set up.
    pub ryoku: bool,
    /// `matugen` is on PATH.
    pub installed: bool,
    /// Oumu's template is in matugen's config.
    pub registered: bool,
    /// The palette file exists (matugen has run since set-up, or Ryoku wrote it).
    pub palette_ready: bool,
    pub palette_path: String,
    pub config_path: String,
    /// The command to run once, when the palette is not there yet.
    pub next_command: String,
}

fn matugen_dir() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("matugen"))
}

fn ryoku_palette() -> Option<PathBuf> {
    Some(dirs::cache_dir()?.join("ryoku/colors.json"))
}

fn on_path(cmd: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(cmd).is_file()))
}

/// Oumu's section for matugen's config.toml.
fn section(template: &Path, palette: &Path) -> String {
    format!(
        "\n# Added by Oumu (theme \"Matugen\"): renders Oumu's palette file on every matugen run.\n{SECTION}\ninput_path = \"{}\"\noutput_path = \"{}\"\n",
        template.display(),
        palette.display()
    )
}

/// `config` with Oumu's section appended, or `None` when it is already there.
fn with_section(config: &str, template: &Path, palette: &Path) -> Option<String> {
    if config.lines().any(|l| l.trim() == SECTION) {
        return None;
    }
    let mut out = config.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&section(template, palette));
    Some(out)
}

pub fn status(custom_palette: &str) -> Status {
    let ryoku = custom_palette.trim().is_empty() && ryoku_palette().is_some_and(|p| p.exists());
    let palette = crate::theme::palette_path(custom_palette).unwrap_or_default();
    let config = matugen_dir().map(|d| d.join("config.toml")).unwrap_or_default();
    let registered = std::fs::read_to_string(&config).is_ok_and(|c| c.lines().any(|l| l.trim() == SECTION));
    let palette_ready = crate::theme::read_palette(&palette).is_some();
    Status {
        ryoku,
        installed: on_path("matugen"),
        registered,
        palette_ready,
        palette_path: palette.display().to_string(),
        config_path: config.display().to_string(),
        next_command: if palette_ready || ryoku { String::new() } else { FIRST_RUN.into() },
    }
}

/// Install the template and register it. Never on Ryoku, which has its own palette.
pub fn setup(custom_palette: &str) -> Result<Status, String> {
    let st = status(custom_palette);
    if st.ryoku {
        return Err("Ryoku already provides the wallpaper palette; there is nothing to set up.".into());
    }
    if !st.installed {
        return Err("matugen is not installed. On Arch: sudo pacman -S matugen".into());
    }
    let dir = matugen_dir().ok_or("no config folder")?;
    let templates = dir.join("templates");
    std::fs::create_dir_all(&templates).map_err(|e| format!("could not create {}: {e}", templates.display()))?;
    let template = templates.join(TEMPLATE_NAME);
    std::fs::write(&template, TEMPLATE).map_err(|e| format!("could not write {}: {e}", template.display()))?;
    let palette = PathBuf::from(&st.palette_path);
    if let Some(parent) = palette.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let config = dir.join("config.toml");
    let current = std::fs::read_to_string(&config).unwrap_or_default();
    if let Some(next) = with_section(&current, &template, &palette) {
        if config.exists() {
            let backup = dir.join("config.toml.before-oumu");
            if !backup.exists() {
                std::fs::copy(&config, &backup).map_err(|e| format!("could not back up {}: {e}", config.display()))?;
            }
        }
        std::fs::write(&config, next).map_err(|e| format!("could not update {}: {e}", config.display()))?;
        log::info!("matugen: registered Oumu's template in {}", config.display());
    }
    Ok(status(custom_palette))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_once_and_keeps_the_existing_config() {
        let t = Path::new("/h/.config/matugen/templates/oumu-colors.json");
        let p = Path::new("/h/.config/oumu/colors.json");
        let cfg = "[config]\n\n[templates.kitty]\ninput_path = \"a\"\noutput_path = \"b\"";
        let next = with_section(cfg, t, p).unwrap();
        assert!(next.starts_with(cfg), "the user's config is kept as it was");
        assert!(next.contains("[templates.oumu]\ninput_path = \"/h/.config/matugen/templates/oumu-colors.json\"\noutput_path = \"/h/.config/oumu/colors.json\"\n"));
        assert_eq!(with_section(&next, t, p), None, "a second run changes nothing");
        assert!(with_section("", t, p).unwrap().contains(SECTION), "a missing config gets just the section");
    }

    #[test]
    fn the_template_covers_the_roles_the_theme_reads() {
        for role in ["surfaceContainerLow", "onSurface", "primary", "onPrimary", "outline", "error", "tertiary"] {
            assert!(TEMPLATE.contains(&format!("\"{role}\"")), "{role}");
        }
    }
}

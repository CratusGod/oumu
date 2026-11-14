//! "Start Oumu at login": an XDG autostart entry (~/.config/autostart), which Hyprland
//! sessions started through uwsm, KDE and GNOME all run.

use std::path::PathBuf;

const FILE: &str = "dev.cratusgod.oumu.desktop";

fn path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("autostart").join(FILE))
}

pub fn enabled() -> bool {
    path().is_some_and(|p| p.is_file())
}

/// Add or remove the entry; returns the state after.
pub fn set(on: bool) -> Result<bool, String> {
    let p = path().ok_or("no config directory")?;
    if on {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let exe = exe.display().to_string();
        let exec = if exe.chars().all(|c| c.is_ascii_alphanumeric() || "/-_.+".contains(c)) { exe } else { format!("\"{}\"", exe.replace('\\', "\\\\").replace('"', "\\\"").replace('$', "\\$").replace('`', "\\`")) };
        std::fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
        let body = format!("[Desktop Entry]\nType=Application\nName=Oumu\nComment=Writing assistant\nExec={exec}\nIcon=dev.cratusgod.oumu\nTerminal=false\nX-GNOME-Autostart-enabled=true\n");
        std::fs::write(&p, body).map_err(|e| e.to_string())?;
    } else if p.exists() {
        std::fs::remove_file(&p).map_err(|e| e.to_string())?;
    }
    Ok(enabled())
}

//! Window placement for Oumu's own windows. macOS moves Tauri windows
//! directly; on Wayland a client cannot place its windows, so on Linux they are
//! layer-shell surfaces positioned by margins (see `linux/layer.rs`).

use tauri::AppHandle;

/// Plain Tauri window calls: macOS, and the Linux fallback where layer shell is missing.
pub(crate) mod plain {
    use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager};

    #[cfg_attr(target_os = "linux", allow(dead_code))]
    pub fn adopt(_app: &AppHandle, _label: &str) {}

    /// Click-through wanted for windows not shown yet: tao unwraps the GDK
    /// window when it sets an input shape, and a window never shown has none
    /// (an abort at startup wherever layer shell is missing: GNOME, X11).
    static PENDING_PASSTHROUGH: std::sync::Mutex<Vec<(String, bool)>> = std::sync::Mutex::new(Vec::new());

    pub fn show(app: &AppHandle, label: &str) {
        if let Some(w) = app.get_webview_window(label) {
            let _ = w.show();
            let pending = PENDING_PASSTHROUGH.lock().ok().and_then(|mut p| {
                let i = p.iter().position(|(l, _)| l == label)?;
                Some(p.remove(i).1)
            });
            if let Some(on) = pending {
                let _ = w.set_ignore_cursor_events(on);
            }
        }
    }

    pub fn hide(app: &AppHandle, label: &str) {
        if let Some(w) = app.get_webview_window(label) {
            let _ = w.hide();
        }
    }

    pub fn is_visible(app: &AppHandle, label: &str) -> bool {
        app.get_webview_window(label).map(|w| w.is_visible().unwrap_or(false)).unwrap_or(false)
    }

    pub fn focus(app: &AppHandle, label: &str) {
        if let Some(w) = app.get_webview_window(label) {
            let _ = w.set_focus();
        }
    }

    pub fn set_position(app: &AppHandle, label: &str, x: f64, y: f64) {
        if let Some(w) = app.get_webview_window(label) {
            let _ = w.set_position(tauri::Position::Logical(LogicalPosition::new(x, y)));
        }
    }

    pub fn set_size(app: &AppHandle, label: &str, w: f64, h: f64) {
        if let Some(win) = app.get_webview_window(label) {
            let _ = win.set_size(tauri::Size::Logical(LogicalSize::new(w, h)));
        }
    }

    pub fn position(app: &AppHandle, label: &str) -> Option<(f64, f64)> {
        let w = app.get_webview_window(label)?;
        let s = w.scale_factor().unwrap_or(1.0);
        w.outer_position().ok().map(|p| (p.x as f64 / s, p.y as f64 / s))
    }

    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub fn frame(app: &AppHandle, label: &str) -> Option<(f64, f64, f64, f64)> {
        let w = app.get_webview_window(label)?;
        let s = w.scale_factor().unwrap_or(1.0);
        let (p, z) = (w.outer_position().ok()?, w.outer_size().ok()?);
        Some((p.x as f64 / s, p.y as f64 / s, z.width as f64 / s, z.height as f64 / s))
    }

    pub fn set_passthrough(app: &AppHandle, label: &str, on: bool) {
        let Some(w) = app.get_webview_window(label) else { return };
        if w.is_visible().unwrap_or(false) {
            let _ = w.set_ignore_cursor_events(on);
        } else if let Ok(mut p) = PENDING_PASSTHROUGH.lock() {
            p.retain(|(l, _)| l != label);
            p.push((label.to_string(), on));
        }
    }

    /// Normal windows hold no keyboard grab.
    #[cfg_attr(target_os = "linux", allow(dead_code))]
    pub fn release_keyboard(_app: &AppHandle, _label: &str) {}

    #[cfg_attr(target_os = "linux", allow(dead_code))]
    pub fn animates_slide() -> bool {
        true
    }
}

#[cfg(not(target_os = "linux"))]
use plain as imp;
#[cfg(target_os = "linux")]
use crate::linux::layer as imp;

/// Take over a freshly built window (Linux: re-host it as a layer surface).
pub fn adopt(app: &AppHandle, label: &str) {
    imp::adopt(app, label)
}
pub fn show(app: &AppHandle, label: &str) {
    imp::show(app, label)
}
pub fn hide(app: &AppHandle, label: &str) {
    imp::hide(app, label)
}
pub fn is_visible(app: &AppHandle, label: &str) -> bool {
    imp::is_visible(app, label)
}
pub fn focus(app: &AppHandle, label: &str) {
    imp::focus(app, label)
}
/// Top-left corner in logical points, screen layout coordinates.
pub fn set_position(app: &AppHandle, label: &str, x: f64, y: f64) {
    imp::set_position(app, label, x, y)
}
pub fn set_size(app: &AppHandle, label: &str, w: f64, h: f64) {
    imp::set_size(app, label, w, h)
}
pub fn position(app: &AppHandle, label: &str) -> Option<(f64, f64)> {
    imp::position(app, label)
}
/// `(x, y, w, h)` in logical points, screen layout coordinates.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn frame(app: &AppHandle, label: &str) -> Option<(f64, f64, f64, f64)> {
    imp::frame(app, label)
}
/// Let pointer events fall through to the window below.
pub fn set_passthrough(app: &AppHandle, label: &str, on: bool) {
    imp::set_passthrough(app, label, on)
}
/// Hand the keyboard back after `focus` (Linux layer surfaces grab it).
pub fn release_keyboard(app: &AppHandle, label: &str) {
    imp::release_keyboard(app, label)
}
/// Whether the panel slide is drawn by us (macOS) or by the compositor (Hyprland layer animations).
pub fn animates_slide() -> bool {
    imp::animates_slide()
}

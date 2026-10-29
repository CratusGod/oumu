//! The desktop around Oumu: which window is in front and where, the pointer,
//! bringing a window forward and pasting into it.
//!
//! Hyprland answers all of it over its IPC socket, so there Oumu has everything:
//! underlines in place, hover cards, paste-based Accept, a fullscreen guard.
//! Any other desktop (GNOME, KDE, Sway, an X11 session) is handled through
//! accessibility focus events, GDK and the usual input tools:
//!
//! | need | Hyprland | X11 session | other Wayland |
//! |---|---|---|---|
//! | front app | IPC | focus events | focus events |
//! | window position (underlines) | IPC | AT-SPI screen coordinates | none: suggestions show in the panel |
//! | pointer (hover cards, edge tab) | IPC | GDK | none: the edge tab reacts to its own hover |
//! | paste for Accept | IPC shortcut | `xdotool` | `wtype` (wlroots compositors) |

use super::ax::Rect;
use super::hyprland;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::AppHandle;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Hyprland,
    X11,
    Wayland,
}

/// The desktop this session runs, decided once.
pub fn kind() -> Kind {
    static KIND: OnceLock<Kind> = OnceLock::new();
    *KIND.get_or_init(|| {
        let env = |k: &str| std::env::var_os(k).is_some_and(|v| !v.is_empty());
        // OUMU_DESKTOP=hyprland|x11|wayland overrides detection (testing, odd setups).
        let forced = std::env::var("OUMU_DESKTOP").ok().and_then(|v| parse_kind(&v));
        let k = forced.unwrap_or_else(|| detect(env("HYPRLAND_INSTANCE_SIGNATURE") && hyprland::request("j/version").is_some(), env("WAYLAND_DISPLAY"), env("DISPLAY")));
        log::info!("desktop: {k:?}");
        k
    })
}

fn parse_kind(v: &str) -> Option<Kind> {
    match v.trim().to_ascii_lowercase().as_str() {
        "hyprland" => Some(Kind::Hyprland),
        "x11" => Some(Kind::X11),
        "wayland" => Some(Kind::Wayland),
        _ => None,
    }
}

fn detect(hyprland_answers: bool, wayland: bool, x11: bool) -> Kind {
    if hyprland_answers {
        Kind::Hyprland
    } else if !wayland && x11 {
        Kind::X11
    } else {
        Kind::Wayland
    }
}

/// The window in front.
#[derive(Clone, Debug, Default)]
pub struct Front {
    pub pid: i32,
    /// Wayland app id or X11 class (Hyprland), else the accessible app name.
    pub class: String,
    pub title: String,
    /// Where the window is on screen, when the desktop says so.
    pub rect: Option<Rect>,
    pub fullscreen: bool,
    /// A stable key for this window (Hyprland address, else the app's bus name).
    pub key: String,
}

/// How long one answer about the front window is reused. Underline placement
/// asks once per text box; one host tick (220 ms) should cost one lookup.
const FRONT_TTL: Duration = Duration::from_millis(80);
/// How long the monitor layout is reused (the edge tab asks every pointer read).
const MONITOR_TTL: Duration = Duration::from_secs(2);

/// `value` from `cache` while it is younger than `ttl`, else a fresh `read()`.
fn cached<T: Clone>(cache: &Mutex<Option<(Instant, T)>>, ttl: Duration, read: impl FnOnce() -> T) -> T {
    if let Ok(g) = cache.lock() {
        if let Some((at, v)) = g.as_ref() {
            if at.elapsed() < ttl {
                return v.clone();
            }
        }
    }
    let v = read();
    if let Ok(mut g) = cache.lock() {
        *g = Some((Instant::now(), v.clone()));
    }
    v
}

static FRONT: Mutex<Option<(Instant, Option<Front>)>> = Mutex::new(None);
/// Bumped by `forget_front`, so a lookup that started before it is not cached.
static FRONT_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn front() -> Option<Front> {
    use std::sync::atomic::Ordering::SeqCst;
    if let Ok(g) = FRONT.lock() {
        if let Some((at, v)) = g.as_ref() {
            if at.elapsed() < FRONT_TTL {
                return v.clone();
            }
        }
    }
    let gen = FRONT_GEN.load(SeqCst);
    let v = read_front();
    if let Ok(mut g) = FRONT.lock() {
        if FRONT_GEN.load(SeqCst) == gen {
            *g = Some((Instant::now(), v.clone()));
        }
    }
    v
}

/// Forget the cached front window (after Oumu itself moves the focus).
fn forget_front() {
    FRONT_GEN.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    if let Ok(mut g) = FRONT.lock() {
        *g = None;
    }
}

fn read_front() -> Option<Front> {
    if kind() == Kind::Hyprland {
        let w = hyprland::active_window()?;
        return Some(Front {
            pid: w.pid,
            class: w.class,
            title: w.title,
            rect: Some(Rect { x: w.at[0], y: w.at[1], w: w.size[0], h: w.size[1] }),
            fullscreen: w.fullscreen != 0,
            key: w.address,
        });
    }
    // Elsewhere the app that last took accessibility focus is the one in front.
    let f = super::ax::focus_owner()?;
    let rect = if kind() == Kind::X11 { f.screen_rect } else { None };
    Some(Front { pid: f.pid, class: f.app.clone(), title: f.app, rect, fullscreen: false, key: f.bus })
}

/// A monitor's logical box `(x, y, w, h)`.
type Monitor = (f64, f64, f64, f64);

/// The primary monitor's logical box, when the desktop says which it is.
pub fn primary_monitor() -> Option<(f64, f64, f64, f64)> {
    static MONITOR: Mutex<Option<(Instant, Option<Monitor>)>> = Mutex::new(None);
    match kind() {
        Kind::Hyprland => cached(&MONITOR, MONITOR_TTL, hyprland::primary_monitor),
        _ => None,
    }
}

static APP: OnceLock<AppHandle> = OnceLock::new();

/// Give the module the app, for GDK calls on the main thread.
pub fn init(app: AppHandle) {
    let _ = APP.set(app);
}

/// How long a pointer read may wait for the GTK main thread.
const POINTER_WAIT: Duration = Duration::from_millis(100);

/// The pointer in global layout coordinates.
pub fn pointer() -> Option<(f64, f64)> {
    match kind() {
        Kind::Hyprland => hyprland::cursor_pos(),
        // X11 hands GDK the global pointer; Wayland only shows it over our own windows.
        Kind::X11 => {
            let app = APP.get()?;
            let (tx, rx) = std::sync::mpsc::channel();
            app.run_on_main_thread(move || {
                use gtk::prelude::*;
                let pos = gtk::gdk::Display::default()
                    .and_then(|d| d.default_seat())
                    .and_then(|s| s.pointer())
                    .map(|p| {
                        let (_, x, y) = p.position();
                        (x as f64, y as f64)
                    });
                let _ = tx.send(pos);
            })
            .ok()?;
            rx.recv_timeout(POINTER_WAIT).ok().flatten()
        }
        Kind::Wayland => None,
    }
}

/// Bring the app `pid` to the front; `true` when it already is or now is.
pub fn focus_pid(pid: i32) -> bool {
    // Asked right before a paste: never trust a cached answer here.
    if read_front().is_some_and(|f| f.pid == pid) {
        forget_front();
        return true;
    }
    let moved = kind() == Kind::Hyprland && hyprland::focus_pid(pid);
    forget_front();
    moved
}

/// Press Ctrl+V in the window in front (`key`: the Hyprland address).
pub fn paste(key: &str) -> bool {
    match kind() {
        Kind::Hyprland => hyprland::send_shortcut("CTRL", "V", key),
        Kind::X11 => run(&["xdotool", "key", "--clearmodifiers", "ctrl+v"]),
        Kind::Wayland => run(&["wtype", "-M", "ctrl", "v", "-m", "ctrl"]),
    }
}

/// Move the pointer (debug probe only).
pub fn move_pointer(x: f64, y: f64) -> bool {
    kind() == Kind::Hyprland && hyprland::move_cursor(x, y)
}

/// Run an input tool; `false` when it is missing or fails (the caller says why).
fn run(argv: &[&str]) -> bool {
    Command::new(argv[0])
        .args(&argv[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hyprland_wins_then_x11_then_any_wayland() {
        assert_eq!(detect(true, true, true), Kind::Hyprland);
        assert_eq!(detect(false, false, true), Kind::X11);
        // XWayland sets DISPLAY too: a Wayland session stays Wayland.
        assert_eq!(detect(false, true, true), Kind::Wayland);
        assert_eq!(detect(false, true, false), Kind::Wayland);
    }

    #[test]
    fn a_cached_answer_is_reused_until_it_ages_out() {
        let cache = Mutex::new(None);
        let mut reads = 0;
        let mut read = || {
            reads += 1;
            reads
        };
        assert_eq!(cached(&cache, Duration::from_secs(60), &mut read), 1);
        assert_eq!(cached(&cache, Duration::from_secs(60), &mut read), 1, "fresh: no second read");
        assert_eq!(cached(&cache, Duration::ZERO, &mut read), 2, "aged out: read again");
    }

    #[test]
    fn the_override_names_a_desktop() {
        assert_eq!(parse_kind("X11"), Some(Kind::X11));
        assert_eq!(parse_kind(" wayland "), Some(Kind::Wayland));
        assert_eq!(parse_kind("gnome"), None);
    }
}

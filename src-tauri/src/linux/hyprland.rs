//! Hyprland IPC over the compositor's request socket. Wayland gives a client no
//! global coordinates, no pointer position and no "frontmost app": Hyprland does.

use serde::Deserialize;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const IO_TIMEOUT: Duration = Duration::from_millis(400);

/// One of Hyprland's sockets: `.socket.sock` (requests) or `.socket2.sock` (events).
fn socket(name: &str) -> Option<PathBuf> {
    let sig = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp"));
    let p = runtime.join("hypr").join(&sig).join(name);
    if p.exists() {
        return Some(p);
    }
    // Hyprland before 0.40 kept its sockets in /tmp/hypr.
    let legacy = PathBuf::from("/tmp/hypr").join(&sig).join(name);
    legacy.exists().then_some(legacy)
}

fn socket_path() -> Option<PathBuf> {
    socket(".socket.sock")
}

/// Whether an event line (`name>>data`) changes which window is in front or
/// where it is. Hyprland repeats `activewindowv2` for the same window whenever
/// its title changes (a terminal spinner, a browser tab): only a new address
/// counts, tracked in `active`.
fn moves_the_front(line: &str, active: &mut String) -> bool {
    let (name, data) = line.split_once(">>").unwrap_or((line, ""));
    match name {
        "activewindowv2" => {
            if data == active {
                return false;
            }
            *active = data.to_string();
            true
        }
        "closewindow" | "fullscreen" | "movewindowv2" | "workspacev2" | "focusedmonv2" => true,
        _ => false,
    }
}

/// Call `on_change` whenever Hyprland switches, closes, moves or fullscreens
/// the front window. Runs on its own thread; reconnects after a compositor restart.
pub fn watch_front(on_change: fn()) {
    let spawned = std::thread::Builder::new().name("hypr-events".into()).spawn(move || loop {
        if let Some(stream) = socket(".socket2.sock").and_then(|p| UnixStream::connect(p).ok()) {
            let mut active = String::new();
            let mut reader = BufReader::new(stream);
            let mut buf = Vec::new();
            // Bytes, not `lines()`: a window title that is not UTF-8 must not end the stream.
            while reader.read_until(b'\n', &mut buf).is_ok_and(|n| n > 0) {
                if moves_the_front(String::from_utf8_lossy(&buf).trim_end(), &mut active) {
                    on_change();
                }
                buf.clear();
            }
        }
        std::thread::sleep(Duration::from_secs(2));
    });
    if let Err(e) = spawned {
        log::error!("hyprland event thread: {e}");
    }
}

/// Whether a live Hyprland session is reachable.
pub fn available() -> bool {
    socket_path().is_some()
}

/// Send one request (`j/activewindow`, `dispatch …`) and return the reply.
/// Requests sent so far (reported by the debug probe).
pub static REQUESTS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn request(cmd: &str) -> Option<String> {
    REQUESTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut s = UnixStream::connect(socket_path()?).ok()?;
    s.set_read_timeout(Some(IO_TIMEOUT)).ok()?;
    s.set_write_timeout(Some(IO_TIMEOUT)).ok()?;
    s.write_all(cmd.as_bytes()).ok()?;
    let mut out = String::new();
    s.read_to_string(&mut out).ok()?;
    Some(out)
}

fn json<T: for<'de> Deserialize<'de>>(cmd: &str) -> Option<T> {
    serde_json::from_str(&request(cmd)?).ok()
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Window {
    pub address: String,
    pub at: [f64; 2],
    pub size: [f64; 2],
    pub class: String,
    pub title: String,
    pub pid: i32,
    #[serde(default)]
    pub fullscreen: i64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Monitor {
    pub id: i64,
    pub name: String,
    pub x: f64,
    pub y: f64,
    /// Pixels, before scaling.
    pub width: f64,
    pub height: f64,
    pub scale: f64,
    #[serde(default)]
    pub transform: i64,
    #[serde(default)]
    pub focused: bool,
}

impl Monitor {
    /// The monitor's rect in the compositor's logical layout.
    pub fn logical(&self) -> (f64, f64, f64, f64) {
        let s = if self.scale > 0.0 { self.scale } else { 1.0 };
        // Transforms 1, 3, 5 and 7 rotate by 90 degrees.
        let (w, h) = if self.transform % 2 == 1 { (self.height, self.width) } else { (self.width, self.height) };
        (self.x, self.y, w / s, h / s)
    }
}

/// How long an `activewindow` answer is reused: one host tick asks for it once
/// per underline rect, and the window cannot move meaningfully in between.
const ACTIVE_TTL: Duration = Duration::from_millis(120);

/// The focused window. Hyprland answers `{}` when nothing is focused.
pub fn active_window() -> Option<Window> {
    static CACHE: Mutex<Option<(Instant, Option<Window>)>> = Mutex::new(None);
    if let Ok(g) = CACHE.lock() {
        if let Some((t, w)) = g.as_ref() {
            if t.elapsed() < ACTIVE_TTL {
                return w.clone();
            }
        }
    }
    let w = json::<Window>("j/activewindow").filter(|w| !w.address.is_empty());
    if let Ok(mut g) = CACHE.lock() {
        *g = Some((Instant::now(), w.clone()));
    }
    w
}

pub fn monitors() -> Vec<Monitor> {
    json("j/monitors").unwrap_or_default()
}

/// The monitor Oumu docks on: the one at the layout origin, else the lowest
/// id. Never just the focused one, or the panel and tab would follow focus.
fn pick_primary(monitors: &[Monitor]) -> Option<&Monitor> {
    monitors.iter().find(|m| m.x == 0.0 && m.y == 0.0).or_else(|| monitors.iter().min_by_key(|m| m.id))
}

/// How long the primary monitor's box is reused (the host asks every tick).
const PRIMARY_TTL: Duration = Duration::from_secs(2);

/// The primary monitor's logical box `(x, y, w, h)`. Wayland has no primary
/// monitor of its own, so Tauri cannot answer this on Hyprland.
pub fn primary_monitor() -> Option<(f64, f64, f64, f64)> {
    type Cached = Option<(Instant, Option<(f64, f64, f64, f64)>)>;
    static CACHE: Mutex<Cached> = Mutex::new(None);
    if let Ok(g) = CACHE.lock() {
        if let Some((t, v)) = *g {
            if t.elapsed() < PRIMARY_TTL {
                return v;
            }
        }
    }
    let v = pick_primary(&monitors()).map(Monitor::logical);
    if let Ok(mut g) = CACHE.lock() {
        *g = Some((Instant::now(), v));
    }
    v
}

/// Pointer position in the global logical layout.
pub fn cursor_pos() -> Option<(f64, f64)> {
    #[derive(Deserialize)]
    struct P {
        x: f64,
        y: f64,
    }
    json::<P>("j/cursorpos").map(|p| (p.x, p.y))
}

fn ok(reply: Option<String>) -> bool {
    reply.map(|r| r.trim() == "ok").unwrap_or(false)
}

/// Run a dispatcher: the Lua form first (Hyprland 0.55+ with a Lua config
/// rejects the old syntax), then the classic one for older sessions.
fn dispatch(lua: &str, legacy: &str) -> bool {
    ok(request(&format!("dispatch {lua}"))) || ok(request(&format!("dispatch {legacy}")))
}

/// A window selector is only ever built from Hyprland's own hex address or a pid.
fn safe_selector(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == ':')
}

pub fn focus_window(address: &str) -> bool {
    let sel = format!("address:{address}");
    safe_selector(&sel) && dispatch(&format!("hl.dsp.focus({{ window = \"{sel}\" }})"), &format!("focuswindow {sel}"))
}

pub fn focus_pid(pid: i32) -> bool {
    let sel = format!("pid:{pid}");
    dispatch(&format!("hl.dsp.focus({{ window = \"{sel}\" }})"), &format!("focuswindow {sel}"))
}

/// Send a key chord straight to one window (no virtual keyboard, no uinput).
/// `mods` and `key` are fixed strings from this crate (`CTRL`, `V`).
pub fn send_shortcut(mods: &str, key: &str, address: &str) -> bool {
    let sel = format!("address:{address}");
    if !safe_selector(&sel) || !safe_selector(mods) || !safe_selector(key) {
        return false;
    }
    dispatch(
        &format!("hl.dsp.send_shortcut({{ mods = \"{mods}\", key = \"{key}\", window = \"{sel}\" }})"),
        &format!("sendshortcut {mods},{key},{sel}"),
    )
}

/// Move the pointer (debug probe only).
pub fn move_cursor(x: f64, y: f64) -> bool {
    let (x, y) = (x.round() as i64, y.round() as i64);
    dispatch(&format!("hl.dsp.cursor.move({{ x = {x}, y = {y} }})"), &format!("movecursor {x} {y}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_activewindow_and_monitors() {
        let w: Window = serde_json::from_str(r#"{"address":"0x5","at":[16,55],"size":[800,600],"class":"org.gnome.TextEditor","title":"t","pid":42,"fullscreen":0,"extra":1}"#).unwrap();
        assert_eq!(w.pid, 42);
        assert_eq!(w.at, [16.0, 55.0]);
        let m: Vec<Monitor> = serde_json::from_str(r#"[{"id":0,"name":"DP-3","x":1920,"y":0,"width":3840,"height":2160,"scale":2.0,"transform":0,"focused":true}]"#).unwrap();
        assert_eq!(m[0].logical(), (1920.0, 0.0, 1920.0, 1080.0));
        let r: Monitor = serde_json::from_str(r#"{"id":1,"name":"HDMI","x":0,"y":0,"width":1920,"height":1080,"scale":1.0,"transform":1}"#).unwrap();
        assert_eq!(r.logical(), (0.0, 0.0, 1080.0, 1920.0));
    }

    #[test]
    fn primary_is_the_monitor_at_the_origin() {
        let m = |id: i64, x: f64, w: f64| Monitor { id, name: format!("M{id}"), x, y: 0.0, width: w, height: 1080.0, scale: 1.0, transform: 0, focused: id == 1 };
        assert_eq!(pick_primary(&[m(1, 1920.0, 2560.0), m(0, 0.0, 1920.0)]).unwrap().name, "M0");
        // No monitor at the origin: the lowest id, never just the focused one.
        assert_eq!(pick_primary(&[m(1, 100.0, 1920.0), m(2, 2020.0, 1920.0)]).unwrap().name, "M1");
        assert!(pick_primary(&[]).is_none());
    }

    #[test]
    fn selectors_reject_injection() {
        assert!(safe_selector("address:0x55ffc22c9b40"));
        assert!(safe_selector("pid:42"));
        assert!(!safe_selector("address:0x1\" }) hl.dsp.exec_cmd(\"rm"));
        assert!(!safe_selector(""));
    }

    #[test]
    fn window_switches_wake_the_loop_title_changes_do_not() {
        let mut active = String::new();
        assert!(moves_the_front("activewindowv2>>55d0a1b2c3d4", &mut active));
        assert!(!moves_the_front("activewindowv2>>55d0a1b2c3d4", &mut active), "same window, new title");
        assert!(moves_the_front("activewindowv2>>66e1", &mut active), "another window");
        assert!(moves_the_front("closewindow>>55d0a1b2c3d4", &mut active));
        assert!(moves_the_front("workspacev2>>2,2", &mut active));
        assert!(!moves_the_front("windowtitlev2>>55d0a1b2c3d4,New tab", &mut active));
        assert!(!moves_the_front("activewindow>>zen,New tab", &mut active), "the v2 event already covers it");
        assert!(!moves_the_front("", &mut active));
    }

    #[test]
    fn empty_activewindow_is_none() {
        let w: Option<Window> = serde_json::from_str::<Window>("{}").ok();
        assert!(w.is_none() || w.unwrap().address.is_empty());
    }
}

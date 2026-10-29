//! `oumu --translate`: tell the running Oumu to translate the highlighted
//! text and slide its panel out. A keybind runs it (Wayland gives apps no
//! global shortcuts). The request goes over a socket in the user's runtime
//! directory, which only that user can open.
//!
//! A request is `translate`, optionally followed by a newline and the text to
//! translate (DragShift sends the text a user dragged). Without text, the
//! highlighted text is used.

use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

pub const TRANSLATE_FLAG: &str = "--translate";
const CMD_TRANSLATE: &[u8] = b"translate";
const CMD_OPEN: &[u8] = b"open";
const CMD_TOGGLE: &[u8] = b"toggle";

/// What a client (the keybind, DragShift, the browser bridge, a second launch) asks for.
#[derive(Debug, PartialEq)]
enum Request {
    /// Translate the given text, or the highlight when `None`.
    Translate(Option<String>),
    /// Slide the panel out and focus it.
    Open,
    /// Slide the panel out, or tuck it away when it is out.
    Toggle,
}
const IO_TIMEOUT: Duration = Duration::from_millis(500);
/// Longest request read, in bytes.
const MAX_REQUEST: usize = 64 * 1024;

/// A highlight waiting for the Translate view to open and pick it up.
static PENDING: Mutex<Option<String>> = Mutex::new(None);

fn socket_path() -> Option<PathBuf> {
    Some(dirs::runtime_dir()?.join("oumu.sock"))
}

/// Ask a running Oumu to translate; `false` when none is listening.
pub fn send_translate() -> bool {
    send(CMD_TRANSLATE)
}

/// Ask a running Oumu to open its panel; `false` when none is listening.
/// A second launch uses this and exits, so only one Oumu ever runs.
pub fn send_open() -> bool {
    send(CMD_OPEN)
}

fn send(cmd: &[u8]) -> bool {
    let Some(path) = socket_path() else { return false };
    let Ok(mut s) = UnixStream::connect(path) else { return false };
    let _ = s.set_write_timeout(Some(IO_TIMEOUT));
    s.write_all(cmd).is_ok()
}

/// Accept requests from `oumu --translate` for as long as the app runs.
pub fn listen(app: AppHandle) {
    let Some(path) = socket_path() else { return };
    // A socket file left by a crashed Oumu: nothing answers on it, so replace it.
    if path.exists() && UnixStream::connect(&path).is_err() {
        let _ = std::fs::remove_file(&path);
    }
    let listener = match UnixListener::bind(&path) {
        Ok(l) => l,
        Err(e) => {
            log::warn!("oumu --translate will not reach this instance: {e}");
            return;
        }
    };
    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    let spawned = std::thread::Builder::new().name("remote".into()).spawn(move || {
        for conn in listener.incoming() {
            let Ok(mut conn) = conn else { continue };
            let _ = conn.set_read_timeout(Some(IO_TIMEOUT));
            // Up to EOF, or what arrived before the timeout from a client that keeps the socket open.
            let mut buf = Vec::new();
            let _ = (&mut conn).take(MAX_REQUEST as u64).read_to_end(&mut buf);
            match parse_request(&buf) {
                Some(Request::Translate(text)) => translate(&app, text),
                Some(Request::Open) => crate::commands::open_panel_inner(&app, "review", true),
                Some(Request::Toggle) => {
                    if crate::overlay::is_tucked() {
                        crate::commands::open_panel_inner(&app, "review", true);
                    } else {
                        crate::commands::tuck_panel_inner(&app);
                    }
                }
                None => {}
            }
        }
    });
    if let Err(e) = spawned {
        log::error!("remote listener thread: {e}");
    }
}

// ---- browser bridge ---------------------------------------------------------
// The "Translate with Oumu" browser extension reaches Oumu through native
// messaging: the browser starts `oumu` with the host manifest path and the
// extension id (Firefox, Zen) or the extension origin (Chromium), sends one
// length-prefixed JSON message on stdin and reads one reply on stdout.

/// The extension's id (Firefox/Zen); the host manifest allows only this one.
pub const EXTENSION_ID: &str = "oumu@cratusgod";
/// Largest message accepted from the browser, in bytes.
const MAX_NATIVE_MESSAGE: u32 = 1024 * 1024;
/// How long to wait for a Oumu this bridge started to open its socket.
const START_WAIT: Duration = Duration::from_secs(10);
const START_POLL: Duration = Duration::from_millis(200);

/// Started by a browser as a native-messaging host (not by a user).
pub fn launched_by_browser(args: &[String]) -> bool {
    args.iter().skip(1).any(|a| a == EXTENSION_ID || a.starts_with("chrome-extension://"))
}

/// `{"action": "translate", "text": "..."}` framed by a native-endian u32 length.
fn read_native_message(r: &mut impl Read) -> Option<String> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len).ok()?;
    let len = u32::from_ne_bytes(len);
    if len == 0 || len > MAX_NATIVE_MESSAGE {
        return None;
    }
    let mut body = vec![0u8; len as usize];
    r.read_exact(&mut body).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&body).ok()?;
    if v["action"] != "translate" {
        return None;
    }
    let text = v["text"].as_str()?.trim();
    (!text.is_empty()).then(|| text.to_string())
}

fn native_reply(ok: bool, error: &str) -> Vec<u8> {
    let body = serde_json::json!({ "ok": ok, "error": error }).to_string().into_bytes();
    let mut out = (body.len() as u32).to_ne_bytes().to_vec();
    out.extend_from_slice(&body);
    out
}

fn send_text(text: &str) -> bool {
    let Some(path) = socket_path() else { return false };
    let Ok(mut s) = UnixStream::connect(path) else { return false };
    let _ = s.set_write_timeout(Some(IO_TIMEOUT));
    let mut req = CMD_TRANSLATE.to_vec();
    req.push(b'\n');
    req.extend_from_slice(text.as_bytes());
    s.write_all(&req).is_ok()
}

/// Hand `text` to the running Oumu, starting one first when none runs.
fn deliver(text: &str) -> Result<(), String> {
    if send_text(text) {
        return Ok(());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    // Its own process group, so the browser ending this bridge leaves Oumu running.
    std::process::Command::new(exe)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|e| format!("could not start Oumu: {e}"))?;
    let start = std::time::Instant::now();
    while start.elapsed() < START_WAIT {
        std::thread::sleep(START_POLL);
        if send_text(text) {
            return Ok(());
        }
    }
    Err("Oumu did not start in time".into())
}

/// Serve one request from the browser extension; returns the exit code.
pub fn run_native_host() -> i32 {
    let reply = match read_native_message(&mut std::io::stdin().lock()) {
        None => native_reply(false, "no text to translate"),
        Some(text) => match deliver(&text) {
            Ok(()) => native_reply(true, ""),
            Err(e) => native_reply(false, &e),
        },
    };
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(&reply);
    let _ = out.flush();
    0
}

/// The request in `buf`: a command line, then (for translate) the text.
fn parse_request(buf: &[u8]) -> Option<Request> {
    let req = String::from_utf8_lossy(buf);
    let (cmd, body) = req.split_once('\n').unwrap_or((&req, ""));
    match cmd.trim().as_bytes() {
        CMD_TRANSLATE => {
            let body = body.trim();
            Some(Request::Translate((!body.is_empty()).then(|| body.to_string())))
        }
        CMD_OPEN => Some(Request::Open),
        CMD_TOGGLE => Some(Request::Toggle),
        _ => None,
    }
}

/// Read the highlight, open the panel on Translate and hand the text over.
pub fn translate_highlight(app: &AppHandle) {
    translate(app, None);
}

/// Open the panel on Translate with `text`, or with the highlight when none is given.
fn translate(app: &AppHandle, text: Option<String>) {
    let text = text.or_else(super::selection::highlighted_text).unwrap_or_default();
    if let Ok(mut p) = PENDING.lock() {
        *p = Some(text);
    }
    // Focused, so Esc tucks it straight away.
    crate::commands::open_panel_inner(app, "translate", true);
    // The Translate view takes the text from PENDING: when it mounts, and on
    // this event if it is already open. One taker, so it never runs twice.
    let _ = app.emit_to("panel", "panel://translate-highlight", serde_json::json!({}));
}

/// The highlight a `--translate` request left for the Translate view, once.
pub fn take_pending() -> Option<String> {
    PENDING.lock().ok()?.take()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_messages_are_length_prefixed_json() {
        let body = br#"{"action":"translate","text":"Xin ch\u00e0o"}"#;
        let mut framed = (body.len() as u32).to_ne_bytes().to_vec();
        framed.extend_from_slice(body);
        assert_eq!(read_native_message(&mut framed.as_slice()).as_deref(), Some("Xin chào"));
        // Truncated, oversized, wrong action or no text: nothing.
        assert_eq!(read_native_message(&mut &framed[..6]), None);
        let mut huge = (u32::MAX).to_ne_bytes().to_vec();
        huge.extend_from_slice(b"{}");
        assert_eq!(read_native_message(&mut huge.as_slice()), None);
        let other = br#"{"action":"delete","text":"x"}"#;
        let mut f2 = (other.len() as u32).to_ne_bytes().to_vec();
        f2.extend_from_slice(other);
        assert_eq!(read_native_message(&mut f2.as_slice()), None);
    }

    #[test]
    fn replies_are_framed_the_same_way() {
        let r = native_reply(true, "");
        let n = u32::from_ne_bytes(r[..4].try_into().unwrap()) as usize;
        assert_eq!(n, r.len() - 4);
        assert_eq!(serde_json::from_slice::<serde_json::Value>(&r[4..]).unwrap()["ok"], true);
    }

    #[test]
    fn browsers_are_recognised_by_their_launch_arguments() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(launched_by_browser(&a(&["/usr/bin/oumu", "/usr/lib/mozilla/native-messaging-hosts/com.cratusgod.oumu.json", "oumu@cratusgod"])));
        assert!(launched_by_browser(&a(&["/usr/bin/oumu", "chrome-extension://abcdefghijklmnop/"])));
        assert!(!launched_by_browser(&a(&["/usr/bin/oumu"])));
        assert!(!launched_by_browser(&a(&["/usr/bin/oumu", "--translate"])));
    }

    #[test]
    fn requests_carry_their_text_or_use_the_highlight() {
        assert_eq!(parse_request(b"translate"), Some(Request::Translate(None)));
        assert_eq!(parse_request(b"translate\n"), Some(Request::Translate(None)));
        assert_eq!(parse_request("translate\nHôm nay trời đẹp\n".as_bytes()), Some(Request::Translate(Some("Hôm nay trời đẹp".to_string()))));
        assert_eq!(parse_request(b"translate\n   \n"), Some(Request::Translate(None)));
        assert_eq!(parse_request(b""), None);
        assert_eq!(parse_request(b"rm -rf /"), None);
    }

    #[test]
    fn panel_requests_open_and_toggle() {
        assert_eq!(parse_request(b"open"), Some(Request::Open));
        assert_eq!(parse_request(b"toggle\n"), Some(Request::Toggle));
        assert_eq!(parse_request(b"opener"), None);
    }
}

//! `oumu --translate` on Windows: tell the running Oumu to translate some text
//! and slide its panel out. Utsuri's Translate slice uses it (the way DragShift
//! does on Linux), and so can a keybind or a shortcut. A second launch asks the
//! running Oumu to open its panel and exits, so only one Oumu runs.
//!
//! The request goes over a named pipe that carries the user's name; the pipe
//! takes no remote clients. A request is `translate`, optionally followed by a
//! newline and the text (without text, the text selected in the app in front is
//! used), or `open` / `toggle` — the same protocol as the Linux socket.

use std::io::Write;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use windows::core::HSTRING;
use windows::Win32::Foundation::{CloseHandle, ERROR_PIPE_CONNECTED, HANDLE};
use windows::Win32::Storage::FileSystem::{ReadFile, PIPE_ACCESS_INBOUND};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
    PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};

pub const TRANSLATE_FLAG: &str = "--translate";
const CMD_TRANSLATE: &[u8] = b"translate";
const CMD_OPEN: &[u8] = b"open";
const CMD_TOGGLE: &[u8] = b"toggle";
/// Longest request read, in bytes.
const MAX_REQUEST: usize = 64 * 1024;

#[derive(Debug, PartialEq)]
enum Request {
    /// Translate the given text, or the selection in the app in front when `None`.
    Translate(Option<String>),
    /// Slide the panel out and focus it.
    Open,
    /// Slide the panel out, or tuck it away when it is out.
    Toggle,
}

/// Text waiting for the Translate view to open and pick it up.
static PENDING: Mutex<Option<String>> = Mutex::new(None);

/// `\\.\pipe\oumu-<user>`: one Oumu per user on a shared PC. `OUMU_PIPE` (tests)
/// names another pipe, so a test Oumu runs beside the user's own.
pub fn pipe_name() -> String {
    if let Ok(p) = std::env::var("OUMU_PIPE") {
        if p.starts_with(r"\\.\pipe\") {
            return p;
        }
    }
    let user: String = std::env::var("USERNAME")
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .collect();
    format!(r"\\.\pipe\oumu-{}", if user.is_empty() { "user" } else { &user })
}

/// Ask a running Oumu to translate the selection; `false` when none is listening.
pub fn send_translate() -> bool {
    send(CMD_TRANSLATE)
}

/// Ask a running Oumu to open its panel; `false` when none is listening.
pub fn send_open() -> bool {
    send(CMD_OPEN)
}

fn send(req: &[u8]) -> bool {
    // A pipe opens like a file; nobody listening = it does not exist.
    match std::fs::OpenOptions::new().write(true).open(pipe_name()) {
        Ok(mut f) => f.write_all(req).is_ok(),
        Err(_) => false,
    }
}

/// Accept requests for as long as the app runs.
pub fn listen(app: AppHandle) {
    let spawned = std::thread::Builder::new().name("remote".into()).spawn(move || {
        let name = HSTRING::from(pipe_name());
        loop {
            let pipe = unsafe {
                CreateNamedPipeW(
                    &name,
                    PIPE_ACCESS_INBOUND,
                    PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                    PIPE_UNLIMITED_INSTANCES,
                    0,
                    MAX_REQUEST as u32,
                    0,
                    None,
                )
            };
            if pipe.is_invalid() {
                log::warn!("oumu --translate will not reach this instance (no pipe)");
                std::thread::sleep(Duration::from_secs(5));
                continue;
            }
            let connected = unsafe { ConnectNamedPipe(pipe, None) };
            if connected.is_ok() || windows::core::Error::from_win32().code() == ERROR_PIPE_CONNECTED.to_hresult() {
                let buf = read_all(pipe);
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
            unsafe {
                let _ = DisconnectNamedPipe(pipe);
                let _ = CloseHandle(pipe);
            }
        }
    });
    if let Err(e) = spawned {
        log::error!("remote listener thread: {e}");
    }
}

/// Everything the client writes, up to `MAX_REQUEST`, until it closes the pipe.
fn read_all(pipe: HANDLE) -> Vec<u8> {
    let mut out = Vec::new();
    let mut chunk = [0u8; 8192];
    while out.len() < MAX_REQUEST {
        let mut n = 0u32;
        if unsafe { ReadFile(pipe, Some(&mut chunk), Some(&mut n), None) }.is_err() || n == 0 {
            break;
        }
        out.extend_from_slice(&chunk[..n as usize]);
    }
    out.truncate(MAX_REQUEST);
    out
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

/// The text selected in the app in front, through UI Automation.
pub fn highlighted_text() -> Option<String> {
    let el = crate::ax::AXElement::system_wide().focused_element()?;
    let text = el.string("AXValue")?;
    let (start, len) = el.range("AXSelectedTextRange")?;
    let sel = crate::textmap::slice_utf16(&text, start, start + len);
    (!sel.trim().is_empty()).then_some(sel)
}

/// Read the selection, open the panel on Translate and hand the text over.
pub fn translate_highlight(app: &AppHandle) {
    translate(app, None);
}

/// Open the panel on Translate with `text`, or with the selection when none is given.
fn translate(app: &AppHandle, text: Option<String>) {
    let text = text.or_else(highlighted_text).unwrap_or_default();
    if let Ok(mut p) = PENDING.lock() {
        *p = Some(text);
    }
    crate::commands::open_panel_inner(app, "translate", true);
    // The Translate view takes the text from PENDING when it mounts, and on this
    // event when it is already open: one taker, so it never runs twice.
    let _ = app.emit_to("panel", "panel://translate-highlight", serde_json::json!({}));
}

/// The text a `--translate` request left for the Translate view, once.
pub fn take_pending() -> Option<String> {
    PENDING.lock().ok()?.take()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_carry_their_text_or_use_the_selection() {
        assert_eq!(parse_request(b"translate"), Some(Request::Translate(None)));
        assert_eq!(parse_request("translate\nHôm nay trời đẹp\n".as_bytes()), Some(Request::Translate(Some("Hôm nay trời đẹp".into()))));
        assert_eq!(parse_request(b"translate\n  \n"), Some(Request::Translate(None)));
        assert_eq!(parse_request(b"open"), Some(Request::Open));
        assert_eq!(parse_request(b"toggle\n"), Some(Request::Toggle));
        assert_eq!(parse_request(b""), None);
        assert_eq!(parse_request(b"del C:\\"), None);
    }

    #[test]
    fn the_pipe_carries_the_user_name_safely() {
        let n = pipe_name();
        assert!(n.starts_with(r"\\.\pipe\oumu-"));
        assert!(!n[r"\\.\pipe\".len()..].contains(['\\', '/', ' ']));
    }
}

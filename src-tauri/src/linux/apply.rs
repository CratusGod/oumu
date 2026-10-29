//! The Linux replacement ladder: AT-SPI EditableText, then a clipboard paste
//! sent to the window by Hyprland, each verified by reading the field back.

use super::desktop;
use crate::ax::{self, u16_to_char, AXElement};
use crate::textmap::{slice_utf16, splice_utf16};
use anyhow::{anyhow, Result};
use serde::Serialize;
use std::io::Write;
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::Duration;

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Method {
    Ax,
    Paste,
}

#[derive(Clone, Debug, Serialize)]
pub struct ApplyResult {
    pub method: Method,
    pub new_text: String,
}

fn verified(el: &AXElement, loc: usize, replacement: &str) -> Option<String> {
    let now = el.string("AXValue")?;
    let got = slice_utf16(&now, loc, loc + replacement.encode_utf16().count());
    (got == replacement).then_some(now)
}

/// Replace `len` UTF-16 units at `loc` with `replacement` in the host field.
pub fn replace_range(el: &AXElement, pid: i32, loc: usize, len: usize, replacement: &str, before: &str) -> Result<ApplyResult> {
    // Guard: the field must still contain what we think it does.
    let live = el.string("AXValue").ok_or_else(|| anyhow!("The text field is no longer available."))?;
    if live != before {
        let want = slice_utf16(before, loc, loc + len);
        if slice_utf16(&live, loc, loc + len) != want {
            return Err(anyhow!("The text changed since this suggestion was made. Try again."));
        }
    }
    let expected = splice_utf16(&live, loc, len, replacement);
    let new_end = loc + replacement.encode_utf16().count();

    // Rung 1: EditableText delete + insert (one undo step in GTK and Qt).
    let (cs, ce) = (u16_to_char(&live, loc), u16_to_char(&live, loc + len));
    if el.edits_in_place() && el.replace_chars(cs, ce, replacement).is_ok() {
        let mut now = None;
        for _ in 0..EDIT_POLLS {
            sleep(Duration::from_millis(70));
            now = el.string("AXValue");
            if now.as_deref() == Some(expected.as_str()) {
                let _ = el.set_range("AXSelectedTextRange", new_end, 0);
                return Ok(ApplyResult { method: Method::Ax, new_text: expected });
            }
        }
        // A half-applied edit (delete went through, insert did not) must not be pasted over.
        if now.as_deref() != Some(live.as_str()) {
            return Err(anyhow!("This app only partly accepted the replacement. Undo (Ctrl+Z) and edit it directly."));
        }
    }

    // Rung 2: select the range, paste through the clipboard into that window.
    if el.set_range("AXSelectedTextRange", loc, len).is_ok() {
        // Browsers apply a selection asynchronously (renderer process): wait for it.
        let mut sel = None;
        for _ in 0..SELECT_POLLS {
            sel = el.range("AXSelectedTextRange");
            if sel == Some((loc, len)) {
                break;
            }
            sleep(Duration::from_millis(50));
        }
        let sel_ok = sel == Some((loc, len));
        log::debug!("apply: paste rung, selection {sel:?} for ({loc}, {len})");
        if sel_ok && ax::activate_app(pid) {
            sleep(Duration::from_millis(120));
            let pasted = paste_text(pid, replacement);
            if let Err(e) = &pasted {
                log::debug!("apply: paste failed: {e}");
            }
            if let Ok(clip) = pasted {
                // Keep our text on the clipboard until the app has read it: a slow
                // app (Electron, Firefox) reading after the restore would paste the
                // user's old clipboard into the document instead.
                let mut done = None;
                for _ in 0..PASTE_POLLS {
                    sleep(Duration::from_millis(80));
                    if let Some(new_text) = verified(el, loc, replacement) {
                        done = Some(new_text);
                        break;
                    }
                }
                let done = done.or_else(|| el.string("AXValue").filter(|now| *now == expected));
                clip.restore(replacement);
                if let Some(new_text) = done {
                    return Ok(ApplyResult { method: Method::Paste, new_text });
                }
            }
        }
    }
    Err(anyhow!("This app did not accept the replacement. Try editing it directly."))
}

/// The clipboard's preferred type and its bytes, so a paste can put it back
/// exactly (an image stays an image). `None` when the clipboard is empty.
fn wl_save() -> Option<(String, Vec<u8>)> {
    let types = Command::new("wl-paste").arg("--list-types").output().ok()?;
    if !types.status.success() {
        return None;
    }
    let list = String::from_utf8_lossy(&types.stdout);
    let pick = list
        .lines()
        .map(str::trim)
        .find(|t| *t == "text/plain;charset=utf-8")
        .or_else(|| list.lines().map(str::trim).find(|t| t.contains('/')))?
        .to_string();
    let out = Command::new("wl-paste").args(["--no-newline", "--type", &pick]).output().ok()?;
    out.status.success().then_some((pick, out.stdout))
}

/// `sensitive` marks the content for clipboard managers (cliphist, Klipper) to skip.
fn wl_copy(mime: &str, bytes: &[u8], sensitive: bool) -> Result<()> {
    let mut args = vec!["--type", mime];
    if sensitive {
        args.push("--sensitive");
    }
    let mut child = Command::new("wl-copy")
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| anyhow!("wl-copy is missing (install wl-clipboard): {e}"))?;
    child.stdin.take().ok_or_else(|| anyhow!("wl-copy stdin"))?.write_all(bytes)?;
    let status = child.wait()?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("wl-copy failed"))
    }
}

/// Read-back polls after an in-place edit (70 ms each).
const EDIT_POLLS: u32 = 5;

/// Selection read-back polls before a paste (50 ms each).
const SELECT_POLLS: u32 = 10;

/// Read-back polls after a paste (80 ms each, about 1.5 s in all).
const PASTE_POLLS: u32 = 18;

/// What was on the clipboard before a paste.
struct Clip {
    saved: Option<(String, Vec<u8>)>,
}

impl Clip {
    /// Put the user's clipboard back, unless they copied something else meanwhile.
    /// Marked sensitive so clipboard history does not record it a second time.
    fn restore(self, ours: &str) {
        let still_ours = wl_save().map(|(_, b)| b == ours.as_bytes()).unwrap_or(false);
        if !still_ours {
            return;
        }
        match self.saved {
            Some((mime, prev)) => {
                let _ = wl_copy(&mime, &prev, true);
            }
            None => {
                let _ = Command::new("wl-copy").arg("--clear").status();
            }
        }
    }
}

/// Put `text` on the clipboard (marked sensitive, so clipboard history skips
/// it) and send Ctrl+V to the app's window. The caller restores the clipboard.
fn paste_text(pid: i32, text: &str) -> Result<Clip> {
    let win = desktop::front().filter(|w| w.pid == pid).ok_or_else(|| anyhow!("the app's window is not in front"))?;
    let clip = Clip { saved: wl_save() };
    wl_copy("text/plain;charset=utf-8", text.as_bytes(), true)?;
    sleep(Duration::from_millis(60));
    if desktop::paste(&win.key) {
        Ok(clip)
    } else {
        clip.restore(text);
        Err(anyhow!("Hyprland did not send the paste shortcut"))
    }
}

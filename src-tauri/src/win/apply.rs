//! The Windows replacement ladder, each rung verified by reading the field back:
//! select the range through UI Automation and paste over it (the app records one
//! undo step), then, for a plain field that has no text ranges, ValuePattern's
//! SetValue with the whole new text.

use crate::ax::{self, AXElement};
use crate::textmap::{slice_utf16, splice_utf16};
use anyhow::{anyhow, Result};
use serde::Serialize;
use std::thread::sleep;
use std::time::Duration;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    VIRTUAL_KEY, VK_CONTROL, VK_LSHIFT, VK_RSHIFT, VK_SHIFT, VK_V,
};

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

/// Selection read-back polls before a paste (50 ms each).
const SELECT_POLLS: u32 = 10;

/// Read-back polls after a paste or SetValue (80 ms each, about 1.5 s in all).
const READ_POLLS: u32 = 18;

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

    // Rung 1: select the range, paste through the clipboard into that window.
    if el.set_range("AXSelectedTextRange", loc, len).is_ok() {
        let mut sel = None;
        for _ in 0..SELECT_POLLS {
            sel = el.range("AXSelectedTextRange");
            if sel == Some((loc, len)) {
                break;
            }
            sleep(Duration::from_millis(50));
        }
        log::debug!("apply: paste rung, selection {sel:?} for ({loc}, {len})");
        if sel == Some((loc, len)) && ax::activate_app(pid) {
            sleep(Duration::from_millis(120));
            match Clip::save().and_then(|clip| clip.paste(replacement).map(|_| clip)) {
                Ok(clip) => {
                    let mut done = None;
                    for _ in 0..READ_POLLS {
                        sleep(Duration::from_millis(80));
                        if let Some(new_text) = verified(el, loc, replacement) {
                            done = Some(new_text);
                            break;
                        }
                    }
                    clip.restore(replacement);
                    if let Some(new_text) = done {
                        let _ = el.set_range("AXSelectedTextRange", new_end, 0);
                        return Ok(ApplyResult { method: Method::Paste, new_text });
                    }
                    if el.string("AXValue").as_deref() != Some(live.as_str()) {
                        return Err(anyhow!("This app only partly accepted the replacement. Undo (Ctrl+Z) and edit it directly."));
                    }
                }
                Err(e) => log::debug!("apply: paste failed: {e}"),
            }
        }
    }

    // Rung 2: a plain field (no text ranges): set the whole value.
    if el.is_settable("AXValue") && el.set_string("AXValue", &expected).is_ok() {
        for _ in 0..READ_POLLS {
            if el.string("AXValue").as_deref() == Some(expected.as_str()) {
                return Ok(ApplyResult { method: Method::Ax, new_text: expected });
            }
            sleep(Duration::from_millis(80));
        }
    }
    Err(anyhow!("This app did not accept the replacement. Try editing it directly."))
}

/// What was on the clipboard before a paste (text or an image), to put back.
enum Saved {
    Text(String),
    Image(arboard::ImageData<'static>),
    Nothing,
}

struct Clip {
    saved: Saved,
}

impl Clip {
    fn save() -> Result<Clip> {
        let mut cb = arboard::Clipboard::new().map_err(|e| anyhow!("clipboard: {e}"))?;
        let saved = match cb.get_text() {
            Ok(t) => Saved::Text(t),
            Err(_) => match cb.get_image() {
                Ok(i) => Saved::Image(i.to_owned_img()),
                Err(_) => Saved::Nothing,
            },
        };
        Ok(Clip { saved })
    }

    /// Put `text` on the clipboard and press Ctrl+V in the app in front.
    fn paste(&self, text: &str) -> Result<()> {
        let mut cb = arboard::Clipboard::new().map_err(|e| anyhow!("clipboard: {e}"))?;
        cb.set_text(text.to_string()).map_err(|e| anyhow!("clipboard: {e}"))?;
        sleep(Duration::from_millis(40));
        send_ctrl_v()
    }

    /// Put the user's clipboard back, unless they copied something else meanwhile.
    fn restore(self, ours: &str) {
        let Ok(mut cb) = arboard::Clipboard::new() else { return };
        if cb.get_text().ok().as_deref() != Some(ours) {
            return;
        }
        let _ = match self.saved {
            Saved::Text(t) => cb.set_text(t),
            Saved::Image(i) => cb.set_image(i),
            Saved::Nothing => cb.clear(),
        };
    }
}

fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) }, time: 0, dwExtraInfo: 0 },
        },
    }
}

/// Ctrl+V. A Shift the user is holding would turn it into a different shortcut
/// in some apps, so it is released for the paste.
fn send_ctrl_v() -> Result<()> {
    let mut inputs = Vec::new();
    for vk in [VK_SHIFT, VK_LSHIFT, VK_RSHIFT] {
        if unsafe { GetAsyncKeyState(vk.0 as i32) } as u16 & 0x8000 != 0 {
            inputs.push(key(vk, true));
        }
    }
    inputs.extend([key(VK_CONTROL, false), key(VK_V, false), key(VK_V, true), key(VK_CONTROL, true)]);
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize == inputs.len() {
        Ok(())
    } else {
        Err(anyhow!("Windows blocked the paste keystroke"))
    }
}

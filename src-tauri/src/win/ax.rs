//! The Windows side of `crate::ax`: the focused text field through UI Automation
//! (the accessibility API every Win32, WinUI, WPF, Chromium, Firefox and Electron
//! app speaks).
//!
//! The surface mirrors the macOS and Linux modules (same names, same AX attribute
//! strings, UTF-16 offsets), so `host.rs` runs unchanged. Text and offsets both
//! come from the TextPattern, whose GetText is UTF-16 already.
//!
//! UI Automation needs no permission on Windows, so the app is always "trusted".
//! Coordinates are physical screen pixels, which match Tauri's logical ones at
//! 100% display scaling.

use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::OnceLock;
use windows::core::{Interface, BSTR};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, SAFEARRAY};
use windows::Win32::System::Ole::{SafeArrayAccessData, SafeArrayDestroy, SafeArrayGetLBound, SafeArrayGetUBound, SafeArrayUnaccessData};
use windows::Win32::System::ProcessStatus::GetModuleBaseNameW;
use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ};
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetCursorPos, GetForegroundWindow, GetSystemMetrics, GetWindowTextW, GetWindowThreadProcessId,
    IsWindowVisible, SetCursorPos, SetForegroundWindow, ShowWindow, SM_CYSCREEN, SW_RESTORE,
};

/// UI Automation calls made so far (the debug probe reports them).
pub static CALLS: AtomicU64 = AtomicU64::new(0);

/// A screen rectangle in pixels, origin at the top-left of the primary monitor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn intersect(&self, o: &Rect) -> Option<Rect> {
        let x1 = self.x.max(o.x);
        let y1 = self.y.max(o.y);
        let x2 = (self.x + self.w).min(o.x + o.w);
        let y2 = (self.y + self.h).min(o.y + o.h);
        if x2 > x1 && y2 > y1 {
            Some(Rect { x: x1, y: y1, w: x2 - x1, h: y2 - y1 })
        } else {
            None
        }
    }
    pub fn contains(&self, px: f64, py: f64) -> bool {
        px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h
    }
    pub fn is_empty(&self) -> bool {
        self.w <= 0.0 || self.h <= 0.0
    }
    fn from_rect(r: RECT) -> Rect {
        Rect { x: r.left as f64, y: r.top as f64, w: (r.right - r.left) as f64, h: (r.bottom - r.top) as f64 }
    }
}

// ---------------------------------------------------------------------------
// The automation object

/// UI Automation's client object, created in the multithreaded apartment so the
/// elements it hands out may be used from any thread that joined it.
struct Uia(IUIAutomation);
// SAFETY: created and used only in the MTA (every caller runs `com()` first),
// where UI Automation's client interfaces are free-threaded.
unsafe impl Send for Uia {}
unsafe impl Sync for Uia {}

thread_local! {
    static COM: () = unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    };
}

fn com() {
    COM.with(|_| {});
}

fn uia() -> Option<&'static IUIAutomation> {
    com();
    static UIA: OnceLock<Option<Uia>> = OnceLock::new();
    UIA.get_or_init(|| unsafe {
        match CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER) {
            Ok(a) => Some(Uia(a)),
            Err(e) => {
                log::error!("UI Automation is unavailable: {e}");
                None
            }
        }
    })
    .as_ref()
    .map(|u| &u.0)
}

fn count() {
    CALLS.fetch_add(1, Relaxed);
}

// ---------------------------------------------------------------------------
// Trust: UI Automation needs no permission on Windows

pub fn is_trusted() -> bool {
    uia().is_some()
}

pub fn prompt_trust() -> bool {
    is_trusted()
}

/// Keyboard focus is inside one of Oumu's own windows.
pub fn focus_is_ours() -> bool {
    let fg = unsafe { GetForegroundWindow() };
    window_pid(fg) == std::process::id() as i32
}

// ---------------------------------------------------------------------------
// Elements

/// A UI Automation element, or the whole desktop (`system_wide`).
#[derive(Clone)]
pub struct AXElement {
    el: Option<IUIAutomationElement>,
}

// SAFETY: see `Uia`; the host keeps the focused element across its threads.
unsafe impl Send for AXElement {}
unsafe impl Sync for AXElement {}

impl AXElement {
    pub fn system_wide() -> AXElement {
        AXElement { el: None }
    }

    fn wrap(el: IUIAutomationElement) -> AXElement {
        AXElement { el: Some(el) }
    }

    pub fn set_timeout(&self, _secs: f32) {}

    pub fn pid(&self) -> i32 {
        com();
        count();
        self.el.as_ref().and_then(|e| unsafe { e.CurrentProcessId() }.ok()).unwrap_or(0)
    }

    pub fn same_as(&self, other: &AXElement) -> bool {
        match (&self.el, &other.el, uia()) {
            (Some(a), Some(b), Some(u)) => {
                count();
                unsafe { u.CompareElements(a, b) }.map(|b| b.as_bool()).unwrap_or(false)
            }
            (None, None, _) => true,
            _ => false,
        }
    }

    /// Stable for the life of the element: UI Automation's runtime id.
    pub fn identity_hash(&self) -> u64 {
        let mut h = DefaultHasher::new();
        if let Some(e) = &self.el {
            com();
            count();
            if let Ok(sa) = unsafe { e.GetRuntimeId() } {
                safearray_i32(sa).hash(&mut h);
            }
        }
        h.finish()
    }

    pub fn focused_element(&self) -> Option<AXElement> {
        let u = uia()?;
        count();
        unsafe { u.GetFocusedElement() }.ok().map(AXElement::wrap)
    }

    fn walker() -> Option<IUIAutomationTreeWalker> {
        unsafe { uia()?.ControlViewWalker() }.ok()
    }

    pub fn parent(&self) -> Option<AXElement> {
        let e = self.el.as_ref()?;
        count();
        let p = unsafe { Self::walker()?.GetParentElement(e) }.ok()?;
        // The desktop root is nobody's window or scroll pane.
        let root = unsafe { uia()?.GetRootElement() }.ok()?;
        if unsafe { uia()?.CompareElements(&p, &root) }.map(|b| b.as_bool()).unwrap_or(false) {
            return None;
        }
        Some(AXElement::wrap(p))
    }

    fn control_type(&self) -> Option<UIA_CONTROLTYPE_ID> {
        com();
        count();
        unsafe { self.el.as_ref()?.CurrentControlType() }.ok()
    }

    /// The top-level window holding this element.
    pub fn window(&self) -> Option<AXElement> {
        let mut cur = self.parent()?;
        let mut last_window = None;
        for _ in 0..64 {
            if cur.control_type() == Some(UIA_WindowControlTypeId) {
                last_window = Some(cur.clone());
            }
            match cur.parent() {
                Some(p) => cur = p,
                None => break,
            }
        }
        last_window
    }

    /// The macOS role names the host understands, from the UIA control type.
    pub fn role(&self) -> Option<String> {
        let e = self.el.as_ref()?;
        let ct = self.control_type()?;
        count();
        if unsafe { e.CurrentIsPassword() }.map(|b| b.as_bool()).unwrap_or(false) {
            return Some("AXSecureTextField".into());
        }
        // Compared with ==, not matched: in a `match` these names would bind, not compare.
        // What an element can do beats what it says it is: a Qt text edit that is
        // its own top-level window reports "Window", with an editable TextPattern.
        let editable_text = self.text_pattern().is_some() && !self.read_only();
        let role = if ct == UIA_EditControlTypeId {
            if self.read_only() {
                "AXStaticText" // text nobody can edit (a log, a read-only box) is not checked
            } else if self.text_pattern().is_some() {
                "AXTextArea"
            } else {
                "AXTextField"
            }
        } else if ct == UIA_ComboBoxControlTypeId {
            "AXComboBox"
        } else if editable_text {
            "AXTextArea"
        } else if ct == UIA_DocumentControlTypeId {
            "AXWebArea"
        } else if ct == UIA_WindowControlTypeId {
            "AXWindow"
        } else if ct == UIA_PaneControlTypeId {
            "AXGroup"
        } else {
            "AXUnknown"
        };
        Some(role.into())
    }

    pub fn subrole(&self) -> Option<String> {
        None
    }

    fn text_pattern(&self) -> Option<IUIAutomationTextPattern> {
        let e = self.el.as_ref()?;
        com();
        count();
        let p: IUIAutomationTextPattern = unsafe { e.GetCurrentPatternAs(UIA_TextPatternId) }.ok()?;
        (!p.as_raw().is_null()).then_some(p)
    }

    fn value_pattern(&self) -> Option<IUIAutomationValuePattern> {
        let e = self.el.as_ref()?;
        com();
        count();
        let p: IUIAutomationValuePattern = unsafe { e.GetCurrentPatternAs(UIA_ValuePatternId) }.ok()?;
        (!p.as_raw().is_null()).then_some(p)
    }

    fn read_only(&self) -> bool {
        if let Some(v) = self.value_pattern() {
            count();
            return unsafe { v.CurrentIsReadOnly() }.map(|b| b.as_bool()).unwrap_or(true);
        }
        // No ValuePattern (e.g. a RichEdit document): editable when it takes keyboard focus.
        count();
        !self.el.as_ref().and_then(|e| unsafe { e.CurrentIsKeyboardFocusable() }.ok()).map(|b| b.as_bool()).unwrap_or(false)
    }

    /// "AXValue": the whole text (TextPattern first, so offsets agree with ranges).
    pub fn string(&self, name: &str) -> Option<String> {
        match name {
            "AXValue" => {
                if let Some(tp) = self.text_pattern() {
                    count();
                    let doc = unsafe { tp.DocumentRange() }.ok()?;
                    count();
                    return unsafe { doc.GetText(-1) }.ok().map(|b| b.to_string());
                }
                let v = self.value_pattern()?;
                count();
                unsafe { v.CurrentValue() }.ok().map(|b| b.to_string())
            }
            "AXTitle" => {
                count();
                unsafe { self.el.as_ref()?.CurrentName() }.ok().map(|b| b.to_string())
            }
            _ => None,
        }
    }

    pub fn number(&self, _name: &str) -> Option<i64> {
        None
    }

    pub fn has_attr(&self, name: &str) -> bool {
        match name {
            "AXSelectedTextRange" => self.text_pattern().is_some(),
            "AXValue" => self.text_pattern().is_some() || self.value_pattern().is_some(),
            _ => false,
        }
    }

    pub fn is_settable(&self, name: &str) -> bool {
        match name {
            "AXValue" | "AXSelectedText" => self.has_attr("AXValue") && !self.read_only(),
            "AXSelectedTextRange" => self.text_pattern().is_some(),
            _ => false,
        }
    }

    /// UTF-16 offset of `r`'s start inside the document.
    fn offset_of(doc: &IUIAutomationTextRange, r: &IUIAutomationTextRange) -> Option<usize> {
        count();
        let head = unsafe { doc.Clone() }.ok()?;
        unsafe { head.MoveEndpointByRange(TextPatternRangeEndpoint_End, r, TextPatternRangeEndpoint_Start) }.ok()?;
        count();
        let text = unsafe { head.GetText(-1) }.ok()?;
        Some(text.len())
    }

    /// "AXSelectedTextRange": (start, length) in UTF-16 units.
    pub fn range(&self, name: &str) -> Option<(usize, usize)> {
        if name != "AXSelectedTextRange" {
            return None;
        }
        let tp = self.text_pattern()?;
        count();
        let sel = unsafe { tp.GetSelection() }.ok()?;
        if unsafe { sel.Length() }.ok()? < 1 {
            return None;
        }
        let r = unsafe { sel.GetElement(0) }.ok()?;
        let doc = unsafe { tp.DocumentRange() }.ok()?;
        let start = Self::offset_of(&doc, &r)?;
        count();
        let len = unsafe { r.GetText(-1) }.ok().map(|b| b.len()).unwrap_or(0);
        Some((start, len))
    }

    /// A text range for the UTF-16 range `loc..loc + len`.
    ///
    /// UIA moves by "Character", and apps disagree on what that is: Chromium and
    /// Electron count a code point (an emoji is one), RichEdit a UTF-16 unit. So
    /// both counts are tried and the range whose text is the wanted text wins;
    /// a guess that lands one letter off is never used.
    fn text_range(&self, loc: usize, len: usize) -> Option<IUIAutomationTextRange> {
        let tp = self.text_pattern()?;
        count();
        let doc = unsafe { tp.DocumentRange() }.ok()?;
        count();
        let text = unsafe { doc.GetText(-1) }.ok()?.to_string();
        let before = crate::textmap::slice_utf16(&text, 0, loc);
        let want = crate::textmap::slice_utf16(&text, loc, loc + len);
        let by_code_point = (before.chars().count(), want.chars().count());
        let mut tries = vec![by_code_point];
        if (loc, len) != by_code_point {
            tries.push((loc, len));
        }
        for (start, n) in tries {
            let Some(r) = Self::range_by_units(&doc, start, n) else { continue };
            count();
            let got = if n > 0 {
                unsafe { r.GetText(-1) }.ok().map(|b| b.to_string())
            } else {
                // An empty range has no text: check what lies before it instead.
                Self::offset_of(&doc, &r).map(|o| crate::textmap::slice_utf16(&text, 0, o))
            };
            if got.as_deref() == Some(if n > 0 { want.as_str() } else { before.as_str() }) {
                return Some(r);
            }
        }
        log::debug!("no UIA range matches {loc}+{len}");
        None
    }

    fn range_by_units(doc: &IUIAutomationTextRange, start: usize, n: usize) -> Option<IUIAutomationTextRange> {
        unsafe {
            let r = doc.Clone().ok()?;
            r.MoveEndpointByRange(TextPatternRangeEndpoint_End, &r, TextPatternRangeEndpoint_Start).ok()?;
            if start > 0 && r.Move(TextUnit_Character, start as i32).ok()? != start as i32 {
                return None;
            }
            if n > 0 {
                r.MoveEndpointByUnit(TextPatternRangeEndpoint_End, TextUnit_Character, n as i32).ok()?;
            }
            Some(r)
        }
    }

    pub fn set_range(&self, name: &str, loc: usize, len: usize) -> Result<(), i32> {
        if name != "AXSelectedTextRange" {
            return Err(-1);
        }
        let r = self.text_range(loc, len).ok_or(-2)?;
        count();
        unsafe { r.Select() }.map_err(|e| e.code().0)
    }

    /// "AXValue": replace the whole text (ValuePattern). The host's apply path
    /// prefers a selection + paste, which keeps the app's undo history.
    pub fn set_string(&self, name: &str, value: &str) -> Result<(), i32> {
        if name != "AXValue" {
            return Err(-1);
        }
        let v = self.value_pattern().ok_or(-2)?;
        count();
        unsafe { v.SetValue(&BSTR::from(value)) }.map_err(|e| e.code().0)
    }

    pub fn frame(&self) -> Option<Rect> {
        let e = self.el.as_ref()?;
        com();
        count();
        let r = Rect::from_rect(unsafe { e.CurrentBoundingRectangle() }.ok()?);
        (!r.is_empty()).then_some(r)
    }

    /// One rect per visual line of `loc..loc + len`.
    pub fn rects_for_range(&self, loc: usize, len: usize) -> Vec<Rect> {
        // Qt's QPlainTextEdit reports every rect at its block's own height and ignores
        // scrolling, so no rect it gives can be placed on screen: no underlines there
        // (the panel and the button's count still have every mistake).
        com();
        count();
        if self.el.as_ref().and_then(|e| unsafe { e.CurrentClassName() }.ok()).is_some_and(|c| c == "QPlainTextEdit") {
            return Vec::new();
        }
        let rects = self.range_rects(loc, len);
        // Qt's QPlainTextEdit answers every line's rect at the first line's height
        // (block-relative). A word after a line break drawn on line one would be
        // a wrong underline: draw none; the panel still lists the mistake.
        if let (Some(r), Some(text)) = (rects.first(), self.string("AXValue")) {
            let before = crate::textmap::slice_utf16(&text, 0, loc);
            if before.contains(['\n', '\r', '\u{2029}']) {
                if let Some(first) = self.range_rects(0, 1).first() {
                    if (r.y - first.y).abs() < 1.0 {
                        log::debug!("rects for {loc}+{len} sit on line one after a line break: not trusted");
                        return Vec::new();
                    }
                }
            }
        }
        rects
    }

    fn range_rects(&self, loc: usize, len: usize) -> Vec<Rect> {
        let Some(r) = self.text_range(loc, len) else { return Vec::new() };
        count();
        let Ok(sa) = (unsafe { r.GetBoundingRectangles() }) else { return Vec::new() };
        safearray_f64(sa)
            .chunks_exact(4)
            .map(|c| Rect { x: c[0], y: c[1], w: c[2], h: c[3] })
            .filter(|r| !r.is_empty())
            .collect()
    }

    pub fn bounds_for_range(&self, loc: usize, len: usize) -> Option<Rect> {
        let rects = self.rects_for_range(loc, len.max(1));
        let first = *rects.first()?;
        Some(rects.iter().skip(1).fold(first, |a, b| {
            let (x, y) = (a.x.min(b.x), a.y.min(b.y));
            Rect { x, y, w: (a.x + a.w).max(b.x + b.w) - x, h: (a.y + a.h).max(b.y + b.h) - y }
        }))
    }

    /// The text that is on screen, as a UTF-16 range (None: treat all as visible).
    pub fn visible_text_range(&self, _vis: &Rect) -> Option<(usize, usize)> {
        let tp = self.text_pattern()?;
        count();
        let ranges = unsafe { tp.GetVisibleRanges() }.ok()?;
        let n = unsafe { ranges.Length() }.ok()?;
        if n < 1 {
            return None;
        }
        let doc = unsafe { tp.DocumentRange() }.ok()?;
        let first = unsafe { ranges.GetElement(0) }.ok()?;
        let last = unsafe { ranges.GetElement(n - 1) }.ok()?;
        let start = Self::offset_of(&doc, &first)?;
        let end = Self::offset_of(&doc, &last)? + unsafe { last.GetText(-1) }.ok()?.len();
        Some((start, end.saturating_sub(start)))
    }

    pub fn line_for_index(&self, _idx: usize) -> Option<i64> {
        None
    }

    pub fn range_for_line(&self, _line: i64) -> Option<(usize, usize)> {
        None
    }

    /// The nearest ancestor that scrolls (underlines are clipped to it).
    pub fn enclosing_scroll_area(&self) -> Option<AXElement> {
        let mut cur = self.parent();
        for _ in 0..32 {
            let c = cur?;
            count();
            let scrolls = c
                .el
                .as_ref()
                .and_then(|e| unsafe { e.GetCurrentPatternAs::<IUIAutomationScrollPattern>(UIA_ScrollPatternId) }.ok())
                .is_some_and(|p| !p.as_raw().is_null());
            if scrolls {
                return Some(c);
            }
            cur = c.parent();
        }
        None
    }
}

// ---------------------------------------------------------------------------
// SAFEARRAY helpers

fn safearray_vec<T: Copy>(sa: *mut SAFEARRAY) -> Vec<T> {
    if sa.is_null() {
        return Vec::new();
    }
    unsafe {
        let mut out = Vec::new();
        if let (Ok(lo), Ok(hi)) = (SafeArrayGetLBound(sa, 1), SafeArrayGetUBound(sa, 1)) {
            let mut data: *mut core::ffi::c_void = std::ptr::null_mut();
            if hi >= lo && SafeArrayAccessData(sa, &mut data).is_ok() {
                let n = (hi - lo + 1) as usize;
                out.extend_from_slice(std::slice::from_raw_parts(data as *const T, n));
                let _ = SafeArrayUnaccessData(sa);
            }
        }
        let _ = SafeArrayDestroy(sa);
        out
    }
}

fn safearray_i32(sa: *mut SAFEARRAY) -> Vec<i32> {
    safearray_vec(sa)
}

fn safearray_f64(sa: *mut SAFEARRAY) -> Vec<f64> {
    safearray_vec(sa)
}

// ---------------------------------------------------------------------------
// Windows and the pointer

fn window_pid(hwnd: HWND) -> i32 {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid as i32
}

fn exe_name(pid: i32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, false, pid as u32).ok()?;
        let mut buf = [0u16; 260];
        let n = GetModuleBaseNameW(h, None, &mut buf);
        let _ = windows::Win32::Foundation::CloseHandle(h);
        (n > 0).then(|| String::from_utf16_lossy(&buf[..n as usize]).to_lowercase())
    }
}

/// (pid, "bundle id" = lower-case exe name, display name = window title's app part).
pub fn frontmost_app() -> Option<(i32, String, String)> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() {
        return None;
    }
    let pid = window_pid(hwnd);
    if pid == std::process::id() as i32 {
        return Some((pid, crate::host::OUR_BUNDLE.to_string(), "Oumu".into()));
    }
    let exe = exe_name(pid).unwrap_or_default();
    let mut buf = [0u16; 512];
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
    let title = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
    // "notes.txt - Notepad" -> "Notepad"; fall back to the exe's name.
    let name = title
        .rsplit(" - ")
        .next()
        .filter(|s| !s.trim().is_empty() && title.contains(" - "))
        .map(str::to_string)
        .unwrap_or_else(|| exe.trim_end_matches(".exe").to_string());
    Some((pid, exe, name))
}

/// Bring the app's main window to the front (before a paste).
pub fn activate_app(pid: i32) -> bool {
    let fg = unsafe { GetForegroundWindow() };
    if window_pid(fg) == pid {
        return true;
    }
    struct Find {
        pid: i32,
        hwnd: Option<HWND>,
    }
    unsafe extern "system" fn each(hwnd: HWND, lp: LPARAM) -> windows::core::BOOL {
        let f = &mut *(lp.0 as *mut Find);
        if IsWindowVisible(hwnd).as_bool() && window_pid(hwnd) == f.pid {
            f.hwnd = Some(hwnd);
            return false.into();
        }
        true.into()
    }
    let mut f = Find { pid, hwnd: None };
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&mut f as *mut Find as isize));
    }
    let Some(hwnd) = f.hwnd else { return false };
    unsafe {
        let _ = ShowWindow(hwnd, SW_RESTORE);
        SetForegroundWindow(hwnd).as_bool()
    }
}

pub fn mouse_location_top_left() -> Option<(f64, f64)> {
    let mut p = POINT::default();
    unsafe { GetCursorPos(&mut p) }.ok()?;
    Some((p.x as f64, p.y as f64))
}

pub fn primary_screen_height() -> Option<f64> {
    let h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    (h > 0).then_some(h as f64)
}

pub fn post_mouse_move(x: f64, y: f64) -> bool {
    unsafe { SetCursorPos(x.round() as i32, y.round() as i32) }.is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_intersection() {
        let a = Rect { x: 0.0, y: 0.0, w: 10.0, h: 10.0 };
        let b = Rect { x: 5.0, y: 5.0, w: 10.0, h: 10.0 };
        assert_eq!(a.intersect(&b), Some(Rect { x: 5.0, y: 5.0, w: 5.0, h: 5.0 }));
        assert!(a.intersect(&Rect { x: 20.0, y: 0.0, w: 1.0, h: 1.0 }).is_none());
    }

    #[test]
    fn automation_is_available_and_sees_a_focus() {
        assert!(is_trusted());
        // Something always has focus on a desktop session (at worst the shell).
        assert!(AXElement::system_wide().focused_element().is_some());
    }

    #[test]
    fn the_foreground_app_has_a_name() {
        if let Some((pid, exe, _)) = frontmost_app() {
            assert!(pid > 0 && (exe.ends_with(".exe") || exe == crate::host::OUR_BUNDLE || exe.is_empty()));
        }
    }
}

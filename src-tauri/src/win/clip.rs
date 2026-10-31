//! The Windows clipboard around a paste: save everything on it (files copied in
//! Explorer, rich text, images: every format held in memory), put Oumu's text on
//! it marked private, and put the user's things back afterwards.
//!
//! Oumu's text carries the formats Windows reads as "keep out of clipboard history
//! and cloud sync" (the Linux backend's --sensitive), so Win+V never lists it.

use anyhow::{anyhow, Result};
use std::thread::sleep;
use std::time::Duration;
use windows::core::w;
use windows::Win32::Foundation::{HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData, OpenClipboard, RegisterClipboardFormatW,
    SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};

const CF_UNICODETEXT: u32 = 13;
/// Formats held as GDI handles or that Windows synthesizes from others: not
/// copyable as memory, and recreated from what is saved.
const NOT_MEMORY: &[u32] = &[2 /* BITMAP */, 3 /* METAFILEPICT */, 9 /* PALETTE */, 14 /* ENHMETAFILE */,
    0x80 /* OWNERDISPLAY */, 0x82 /* DSPBITMAP */, 0x83 /* DSPMETAFILEPICT */, 0x8E /* DSPENHMETAFILE */];

/// Everything that was on the clipboard, format by format.
pub struct Saved(Vec<(u32, Vec<u8>)>);

/// Open the clipboard; another app may hold it for a moment.
fn open() -> Result<Guard> {
    for _ in 0..20 {
        if unsafe { OpenClipboard(None) }.is_ok() {
            return Ok(Guard);
        }
        sleep(Duration::from_millis(25));
    }
    Err(anyhow!("the clipboard is busy"))
}

struct Guard;
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = unsafe { CloseClipboard() };
    }
}

fn read_global(h: HANDLE) -> Option<Vec<u8>> {
    let g = HGLOBAL(h.0);
    unsafe {
        let n = GlobalSize(g);
        let p = GlobalLock(g) as *const u8;
        if p.is_null() {
            return None;
        }
        let bytes = std::slice::from_raw_parts(p, n).to_vec();
        let _ = GlobalUnlock(g);
        Some(bytes)
    }
}

fn put(fmt: u32, bytes: &[u8]) -> Result<()> {
    unsafe {
        let g = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1))?;
        let p = GlobalLock(g) as *mut u8;
        if p.is_null() {
            return Err(anyhow!("out of memory"));
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
        let _ = GlobalUnlock(g);
        // On success the clipboard owns the memory.
        SetClipboardData(fmt, Some(HANDLE(g.0)))?;
    }
    Ok(())
}

/// Save every in-memory format now on the clipboard.
pub fn save() -> Result<Saved> {
    let _g = open()?;
    let mut out = Vec::new();
    let mut fmt = 0u32;
    loop {
        fmt = unsafe { EnumClipboardFormats(fmt) };
        if fmt == 0 {
            break;
        }
        if NOT_MEMORY.contains(&fmt) {
            continue;
        }
        if let Ok(h) = unsafe { GetClipboardData(fmt) } {
            if let Some(bytes) = read_global(h) {
                out.push((fmt, bytes));
            }
        }
    }
    Ok(Saved(out))
}

/// Put `text` on the clipboard, kept out of clipboard history and cloud sync.
pub fn set_private_text(text: &str) -> Result<()> {
    let _g = open()?;
    unsafe { EmptyClipboard() }?;
    let mut units: Vec<u16> = text.encode_utf16().collect();
    units.push(0);
    put(CF_UNICODETEXT, unsafe { std::slice::from_raw_parts(units.as_ptr() as *const u8, units.len() * 2) })?;
    let zero = 0u32.to_le_bytes();
    unsafe {
        put(RegisterClipboardFormatW(w!("ExcludeClipboardContentFromMonitorProcessing")), &[])?;
        put(RegisterClipboardFormatW(w!("CanIncludeInClipboardHistory")), &zero)?;
        put(RegisterClipboardFormatW(w!("CanUploadToCloudClipboard")), &zero)?;
    }
    Ok(())
}

/// The clipboard's text, if it has any.
pub fn text() -> Option<String> {
    let _g = open().ok()?;
    let h = unsafe { GetClipboardData(CF_UNICODETEXT) }.ok()?;
    let bytes = read_global(h)?;
    let units: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
    Some(String::from_utf16_lossy(&units[..end]))
}

impl Saved {
    /// Put the user's clipboard back, unless they copied something else meanwhile.
    pub fn restore(self, ours: &str) {
        if text().as_deref() != Some(ours) {
            return;
        }
        let Ok(_g) = open() else { return };
        if unsafe { EmptyClipboard() }.is_err() {
            return;
        }
        for (fmt, bytes) in &self.0 {
            let _ = put(*fmt, bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One test touches the real clipboard (and puts the user's content back).
    #[test]
    fn saves_restores_and_marks_private() {
        let before = save().expect("save");
        // something unusual on it: text plus a private format of our own
        {
            let _g = open().unwrap();
            unsafe { EmptyClipboard() }.unwrap();
            let mut units: Vec<u16> = "user text 😀".encode_utf16().collect();
            units.push(0);
            put(CF_UNICODETEXT, unsafe { std::slice::from_raw_parts(units.as_ptr() as *const u8, units.len() * 2) }).unwrap();
            put(unsafe { RegisterClipboardFormatW(w!("OumuTestFormat")) }, b"keep me").unwrap();
        }
        let saved = save().unwrap();
        set_private_text("sentence").unwrap();
        assert_eq!(text().as_deref(), Some("sentence"));
        {
            let _g = open().unwrap();
            let h = unsafe { GetClipboardData(RegisterClipboardFormatW(w!("CanIncludeInClipboardHistory"))) }.unwrap();
            assert_eq!(read_global(h).unwrap()[..4], 0u32.to_le_bytes());
        }
        saved.restore("sentence");
        assert_eq!(text().as_deref(), Some("user text 😀"));
        {
            let _g = open().unwrap();
            let h = unsafe { GetClipboardData(RegisterClipboardFormatW(w!("OumuTestFormat"))) }.unwrap();
            assert_eq!(&read_global(h).unwrap()[..7], b"keep me");
        }
        // put back whatever the user had before the test
        let _ = text().map(|t| before.restore(&t));
    }
}

//! The underline strips: a plain Win32 layered window, always click-through, drawn on the
//! CPU with per-pixel alpha (UpdateLayeredWindow). Underlines are only coloured bars, and
//! a layered window is the one kind of window that both shows partial transparency and
//! lets every click through. (The GPU windows can't be layered: their DirectComposition
//! content disappears.)

use windows62::Win32::Foundation::*;
use windows62::Win32::Graphics::Gdi::*;
use windows62::Win32::System::LibraryLoader::GetModuleHandleW;
use windows62::Win32::UI::WindowsAndMessaging::*;
use windows62::core::w;

pub struct Strips {
    hwnd: HWND,
    frame: (i32, i32, i32, i32),
    visible: bool,
    last: Vec<(i32, i32, i32, i32, u32)>,
}

unsafe extern "system" fn proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        _ => unsafe { DefWindowProcW(h, m, wp, lp) },
    }
}

impl Strips {
    pub fn new() -> Option<Strips> {
        unsafe {
            let inst = GetModuleHandleW(None).ok()?;
            let class = w!("OumuUnderlineStrips");
            let wc = WNDCLASSW { lpfnWndProc: Some(proc), hInstance: inst.into(), lpszClassName: class, ..Default::default() };
            RegisterClassW(&wc);
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class,
                w!("Oumu underlines"),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(inst.into()),
                None,
            )
            .ok()?;
            Some(Strips { hwnd, frame: (0, 0, 1, 1), visible: false, last: vec![] })
        }
    }

    /// Where the layer is (physical px) and whether it shows.
    pub fn place(&mut self, x: i32, y: i32, w: i32, h: i32, visible: bool) {
        let f = (x, y, w.max(1), h.max(1));
        if f != self.frame {
            self.frame = f;
            self.last.clear(); // redraw at the new size
        }
        if visible != self.visible {
            self.visible = visible;
            unsafe {
                if visible {
                    let _ = SetWindowPos(self.hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW);
                } else {
                    let _ = ShowWindow(self.hwnd, SW_HIDE);
                }
            }
        }
    }

    /// Draw bars: (x, y, w, h) in physical px within the layer, straight RGBA colour.
    pub fn draw(&mut self, bars: &[(i32, i32, i32, i32, [u8; 4])]) {
        let key: Vec<(i32, i32, i32, i32, u32)> = bars.iter().map(|b| (b.0, b.1, b.2, b.3, u32::from_be_bytes(b.4))).collect();
        if key == self.last && !key.is_empty() {
            return;
        }
        self.last = key;
        let (fx, fy, fw, fh) = self.frame;
        unsafe {
            let screen = GetDC(None);
            let mem = CreateCompatibleDC(Some(screen));
            let bi = BITMAPINFO { bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: fw, biHeight: -fh, biPlanes: 1, biBitCount: 32, biCompression: BI_RGB.0, ..Default::default() }, ..Default::default() };
            let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
            let Ok(dib) = CreateDIBSection(Some(mem), &bi, DIB_RGB_COLORS, &mut bits, None, 0) else {
                let _ = DeleteDC(mem);
                ReleaseDC(None, screen);
                return;
            };
            let old = SelectObject(mem, dib.into());
            let px = std::slice::from_raw_parts_mut(bits as *mut u32, (fw * fh) as usize);
            px.fill(0);
            for &(x, y, w, h, c) in bars {
                let [r, g, b, a] = c;
                // premultiplied BGRA
                let pm = |v: u8| (v as u32 * a as u32 / 255) & 0xff;
                let v = (a as u32) << 24 | pm(r) << 16 | pm(g) << 8 | pm(b);
                for yy in y.max(0)..(y + h).min(fh) {
                    for xx in x.max(0)..(x + w).min(fw) {
                        px[(yy * fw + xx) as usize] = v;
                    }
                }
            }
            let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
            let dst = POINT { x: fx, y: fy };
            let size = SIZE { cx: fw, cy: fh };
            let src = POINT { x: 0, y: 0 };
            let _ = UpdateLayeredWindow(self.hwnd, Some(screen), Some(&dst), Some(&size), Some(mem), Some(&src), COLORREF(0), Some(&blend), ULW_ALPHA);
            SelectObject(mem, old);
            let _ = DeleteObject(dib.into());
            let _ = DeleteDC(mem);
            ReleaseDC(None, screen);
        }
    }
}

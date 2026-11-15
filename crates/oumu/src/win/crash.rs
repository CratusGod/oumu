//! A crash writes where it happened to %TEMP%\oumu-crash-<pid>.txt: the faulting address and
//! every return address found on the stack, named from the .pdb when one is beside the exe.
//! Test runs (OUMU_DATA_DIR set, headless) get no "Application Error" box that waits for a click.

use std::sync::atomic::{AtomicU32, Ordering};
use windows::Win32::Foundation::{EXCEPTION_ACCESS_VIOLATION, HMODULE};
use windows::Win32::System::Diagnostics::Debug::*;
use windows::Win32::System::LibraryLoader::{GetModuleFileNameW, GetModuleHandleExW, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT};
use windows::Win32::System::Threading::{GetCurrentProcess, GetCurrentThreadStackLimits};
use windows::core::PCWSTR;

static SEEN: AtomicU32 = AtomicU32::new(0);

/// `quiet`: no crash dialog (headless test runs, where a dialog would wait for a click).
pub fn install(quiet: bool) {
    unsafe {
        if quiet {
            SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX | SEM_NOOPENFILEERRORBOX);
        }
        SymSetOptions(SYMOPT_DEFERRED_LOADS | SYMOPT_UNDNAME);
        let _ = SymInitializeW(GetCurrentProcess(), PCWSTR::null(), true);
        AddVectoredExceptionHandler(1, Some(on_exception));
    }
}

fn module_of(addr: u64) -> Option<(String, u64)> {
    unsafe {
        let mut m = HMODULE::default();
        GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT, PCWSTR(addr as *const u16), &mut m).ok()?;
        let mut buf = [0u16; 260];
        let n = GetModuleFileNameW(Some(m), &mut buf) as usize;
        let name = String::from_utf16_lossy(&buf[..n]);
        let short = name.rsplit('\\').next().unwrap_or(&name).to_string();
        Some((short, addr - m.0 as u64))
    }
}

fn symbol_of(addr: u64) -> Option<String> {
    unsafe {
        #[repr(C)]
        struct Buf {
            info: SYMBOL_INFOW,
            name: [u16; 256],
        }
        let mut b: Buf = std::mem::zeroed();
        b.info.SizeOfStruct = std::mem::size_of::<SYMBOL_INFOW>() as u32;
        b.info.MaxNameLen = 256;
        let mut disp = 0u64;
        SymFromAddrW(GetCurrentProcess(), addr, Some(&mut disp), &mut b.info).ok()?;
        let p = b.info.Name.as_ptr();
        let len = (0..b.info.NameLen as usize).take_while(|i| *p.add(*i) != 0).count();
        Some(format!("{}+{:#x}", String::from_utf16_lossy(std::slice::from_raw_parts(p, len)), disp))
    }
}

unsafe extern "system" fn on_exception(info: *mut EXCEPTION_POINTERS) -> i32 {
    const CONTINUE_SEARCH: i32 = 0;
    unsafe {
        let rec = &*(*info).ExceptionRecord;
        if rec.ExceptionCode != EXCEPTION_ACCESS_VIOLATION || SEEN.fetch_add(1, Ordering::SeqCst) > 2 {
            return CONTINUE_SEARCH;
        }
        let ctx = &*(*info).ContextRecord;
        let thread = std::thread::current();
        let mut out = format!("oumu crash: access violation at {:#x} on thread {:?} (rip {:#x}, rsp {:#x})\n", rec.ExceptionAddress as u64, thread.name().unwrap_or("?"), ctx.Rip, ctx.Rsp);
        // stack scan: values that point into a loaded module are (mostly) return addresses
        let sp = ctx.Rsp as *const u64;
        let (mut lo, mut hi) = (0usize, 0usize);
        GetCurrentThreadStackLimits(&mut lo, &mut hi);
        let words = if (lo as u64..hi as u64).contains(&ctx.Rsp) { ((hi as u64 - ctx.Rsp) / 8).min(2048) as usize } else { 0 };
        let mut shown = 0;
        for i in 0..words {
            let v = std::ptr::read_volatile(sp.add(i));
            if v < 0x10000 {
                continue;
            }
            if let Some((m, off)) = module_of(v) {
                let sym = symbol_of(v).unwrap_or_default();
                out.push_str(&format!("  [{i:4}] {m}+{off:#x} {sym}\n"));
                shown += 1;
                if shown >= 60 {
                    break;
                }
            }
        }
        eprint!("{out}");
        let _ = std::fs::write(std::env::temp_dir().join(format!("oumu-crash-{}.txt", std::process::id())), &out);
        CONTINUE_SEARCH
    }
}

/// OUMU_CRASH_TEST=1: crash on purpose (a null call on a worker thread, like the one seen)
/// so the handler and the no-dialog exit can be checked.
pub fn self_test() {
    if std::env::var_os("OUMU_CRASH_TEST").is_some() {
        std::thread::Builder::new()
            .name("crash-test".into())
            .spawn(|| unsafe {
                let f: extern "C" fn() = std::mem::transmute::<usize, extern "C" fn()>(std::hint::black_box(0usize));
                f();
            })
            .ok()
            .map(|h| h.join());
    }
}

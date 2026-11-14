//! Wake the host loop the moment focus moves, so it can rest between ticks
//! when there is nothing to watch (see `pace.rs`), as on Linux.
//!
//! Plain WinEvents, out of context: Windows posts them to this thread's queue;
//! nothing is loaded into, or asked of, the app that sent them (a game included).

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    GetMessageW, EVENT_OBJECT_FOCUS, EVENT_SYSTEM_FOREGROUND, MSG, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
};

/// Another window came to the front, or the keyboard focus moved inside one.
unsafe extern "system" fn on_event(_hook: HWINEVENTHOOK, _event: u32, _hwnd: HWND, _object: i32, _child: i32, _thread: u32, _time: u32) {
    crate::pace::wake();
}

/// Start listening on a thread of its own (the hooks need its message loop).
pub fn watch() {
    let spawned = std::thread::Builder::new().name("focus-events".into()).spawn(|| unsafe {
        let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
        let front = SetWinEventHook(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND, None, Some(on_event), 0, 0, flags);
        let focus = SetWinEventHook(EVENT_OBJECT_FOCUS, EVENT_OBJECT_FOCUS, None, Some(on_event), 0, 0, flags);
        if front.is_invalid() && focus.is_invalid() {
            log::error!("focus hooks: none could be set; the host loop keeps its quick tick");
            WATCHING.store(false, std::sync::atomic::Ordering::Relaxed);
            return;
        }
        WATCHING.store(true, std::sync::atomic::Ordering::Relaxed);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {}
    });
    if let Err(e) = spawned {
        log::error!("focus-events thread: {e}");
    }
}

/// Focus changes reach the loop: it may rest when idle. Without them it must keep polling.
static WATCHING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn is_watching() -> bool {
    WATCHING.load(std::sync::atomic::Ordering::Relaxed)
}

//! How often the host loop runs. Watching a text field needs a quick tick
//! (typing, caret moves, scrolling); with nothing to watch the loop sleeps
//! longer, and anything that can bring a field into view wakes it at once:
//! an accessibility focus event, Hyprland switching windows, the panel opening.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

/// Tick while a document is in front, a check is pending or the panel is open.
pub const BUSY: Duration = Duration::from_millis(100);
/// Tick with nothing to watch. A focus change does not wait for it.
pub const IDLE: Duration = Duration::from_millis(1000);
/// After a wake the loop keeps the busy tick this long: a toolkit marks its
/// field focused a moment after the compositor switches windows.
pub const SETTLE: Duration = Duration::from_millis(1500);

static LAST_WAKE: Mutex<Option<Instant>> = Mutex::new(None);

static WOKEN: Mutex<bool> = Mutex::new(false);
static BELL: Condvar = Condvar::new();
/// Wakes asked for and sleeps cut short by one (the debug probe reports them).
pub static WAKES: AtomicU64 = AtomicU64::new(0);
pub static WOKEN_EARLY: AtomicU64 = AtomicU64::new(0);

/// Run the host loop now instead of at its next tick.
pub fn wake() {
    WAKES.fetch_add(1, Ordering::Relaxed);
    if let Ok(mut t) = LAST_WAKE.lock() {
        *t = Some(Instant::now());
    }
    if let Ok(mut w) = WOKEN.lock() {
        *w = true;
        BELL.notify_all();
    }
}

/// Sleep for `d`, or less if `wake` is called meanwhile (or was since the last sleep).
pub fn sleep(d: Duration) {
    let Ok(g) = WOKEN.lock() else {
        std::thread::sleep(d);
        return;
    };
    if let Ok((mut g, timeout)) = BELL.wait_timeout_while(g, d, |w| !*w) {
        if !timeout.timed_out() {
            WOKEN_EARLY.fetch_add(1, Ordering::Relaxed);
        }
        *g = false;
    }
}

/// Memory is handed back at most this often (see `settle_memory`).
const TRIM_EVERY: Duration = Duration::from_secs(60);

/// When the loop stops watching, hand freed heap memory back to the system.
/// The allocator keeps what a lint or a page of text needed (about 30 MB after
/// typing in a long field); returning it costs a few milliseconds, so it only
/// happens on the way to idle, never while the user types.
pub fn settle_memory(was_watching: bool, watching: bool) {
    static LAST: Mutex<Option<Instant>> = Mutex::new(None);
    if !was_watching || watching {
        return;
    }
    let Ok(mut last) = LAST.lock() else { return };
    if last.is_some_and(|t| t.elapsed() < TRIM_EVERY) {
        return;
    }
    *last = Some(Instant::now());
    #[cfg(target_os = "linux")]
    // SAFETY: malloc_trim only releases free pages of glibc's heap; it takes no pointers.
    unsafe {
        libc::malloc_trim(0);
    }
}

/// The tick for the loop's current state.
pub fn tick_for(watching: bool) -> Duration {
    let settling = LAST_WAKE.lock().ok().and_then(|t| *t).is_some_and(|t| t.elapsed() < SETTLE);
    if watching || settling { BUSY } else { IDLE }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    // One test: `wake`, `sleep` and `tick_for` share process-wide state.
    #[test]
    fn a_wake_cuts_the_sleep_short_and_keeps_the_tick_busy_for_a_while() {
        let t = Instant::now();
        let waker = std::thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(30));
            wake();
        });
        sleep(Duration::from_secs(5));
        waker.join().unwrap();
        assert!(t.elapsed() < Duration::from_secs(2), "woke after {:?}", t.elapsed());
        // The wake was used up: the next sleep runs its full (short) course.
        let t = Instant::now();
        sleep(Duration::from_millis(40));
        assert!(t.elapsed() >= Duration::from_millis(35));

        assert_eq!(tick_for(false), BUSY, "settling right after the wake");
        assert_eq!(tick_for(true), BUSY);
        *LAST_WAKE.lock().unwrap() = Some(Instant::now() - SETTLE * 2);
        assert_eq!(tick_for(false), IDLE, "settled: nothing to watch");
        assert!(IDLE > BUSY);
    }
}

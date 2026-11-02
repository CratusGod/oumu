//! Starting at sign-in: a log file (a release build has no console), and waiting
//! for the taskbar, which Windows may not have up yet when it runs the Run key.

use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use windows::core::w;
use windows::Win32::UI::WindowsAndMessaging::FindWindowW;

/// Longest wait for the taskbar before going on without it.
const TASKBAR_WAIT: Duration = Duration::from_secs(60);
/// A log over this size starts over.
const LOG_MAX: u64 = 1024 * 1024;

/// `%LOCALAPPDATA%\Oumu\oumu.log`.
pub fn log_path() -> Option<PathBuf> {
    Some(dirs::data_local_dir()?.join("Oumu").join("oumu.log"))
}

/// env_logger into the log file (and nowhere else), RUST_LOG respected.
pub fn init_logging() {
    let mut builder = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"));
    if let Some(path) = log_path() {
        let _ = std::fs::create_dir_all(path.parent().unwrap());
        let append = std::fs::metadata(&path).map(|m| m.len() < LOG_MAX).unwrap_or(false);
        let file = std::fs::OpenOptions::new().create(true).write(true).append(append).truncate(!append).open(&path);
        if let Ok(mut f) = file {
            let _ = writeln!(f, "---- Oumu {} started {}", env!("CARGO_PKG_VERSION"), chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
            builder.target(env_logger::Target::Pipe(Box::new(f)));
        }
    }
    let _ = builder.try_init();
}

/// The taskbar (Explorer's Shell_TrayWnd) exists.
pub fn taskbar_up() -> bool {
    unsafe { FindWindowW(w!("Shell_TrayWnd"), None) }.is_ok_and(|h| !h.0.is_null())
}

/// Wait (up to a minute) for the taskbar, so the tray icon has a place to go.
pub fn wait_for_taskbar() {
    if taskbar_up() {
        return;
    }
    let start = Instant::now();
    log::info!("waiting for the taskbar");
    while !taskbar_up() && start.elapsed() < TASKBAR_WAIT {
        std::thread::sleep(Duration::from_millis(500));
    }
    // A moment more: Explorer takes tray icons a little after its window appears.
    std::thread::sleep(Duration::from_secs(2));
    log::info!("taskbar {} after {:.1}s", if taskbar_up() { "up" } else { "still missing" }, start.elapsed().as_secs_f32());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_taskbar_is_found_and_the_log_lives_in_local_appdata() {
        assert!(taskbar_up(), "a desktop session has a taskbar");
        let p = log_path().unwrap();
        assert!(p.ends_with(r"Oumu\oumu.log"));
    }
}

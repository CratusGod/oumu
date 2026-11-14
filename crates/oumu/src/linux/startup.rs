//! The log file: `$XDG_STATE_HOME/oumu/oumu.log` (~/.local/state/oumu), or
//! `$OUMU_DATA_DIR/oumu.log` for tests.

use std::io::Write;
use std::path::PathBuf;

/// A log over this size starts over.
const LOG_MAX: u64 = 1024 * 1024;

pub fn log_path() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("OUMU_DATA_DIR") {
        return Some(PathBuf::from(d).join("oumu.log"));
    }
    Some(dirs::state_dir().or_else(dirs::data_local_dir)?.join("oumu").join("oumu.log"))
}

/// env_logger into the log file, RUST_LOG respected; stderr too when it is a terminal.
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

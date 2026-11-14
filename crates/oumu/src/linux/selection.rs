//! Highlighted text from any app: Wayland's primary selection, the text a
//! middle click would paste. Every app that lets you highlight sets it, even
//! where the text cannot be edited (a web page, a PDF, a terminal).

use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

/// How long the app owning the selection may take to hand it over.
const READ_TIMEOUT: Duration = Duration::from_millis(1500);
/// Longest highlight read, in bytes (features clip their input further).
const MAX_BYTES: usize = 64 * 1024;

/// The highlighted text, trimmed; `None` when nothing (or no text) is highlighted.
pub fn highlighted_text() -> Option<String> {
    let mut child = Command::new("wl-paste")
        .args(["--primary", "--no-newline", "--type", "text"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;
    // Read on a thread: a selection owner that never answers must not hang the caller.
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.take(MAX_BYTES as u64).read_to_end(&mut buf);
        let _ = tx.send(buf);
    });
    let got = rx.recv_timeout(READ_TIMEOUT).ok();
    let _ = child.kill();
    let _ = child.wait();
    clean(&String::from_utf8_lossy(&got?))
}

fn clean(raw: &str) -> Option<String> {
    let t = raw.trim();
    (!t.is_empty()).then(|| t.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_highlights_are_nothing() {
        assert_eq!(clean("  \n"), None);
        assert_eq!(clean("  Hôm nay trời đẹp \n").as_deref(), Some("Hôm nay trời đẹp"));
    }
}

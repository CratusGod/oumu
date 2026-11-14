//! Output paths: results go next to the original and never overwrite anything.
//! Ported from utsuri/util.py (unique_path, sibling) and convert.py (atomic_save).

use std::io;
use std::path::{Path, PathBuf};

/// `path`, or `name (1).ext`, `name (2).ext`... if it already exists.
pub fn unique_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    (1..)
        .map(|i| path.with_file_name(format!("{stem} ({i}){ext}")))
        .find(|c| !c.exists())
        .expect("an unbounded range always finds a free name")
}

/// Output path for `src` in `dir` (normally the source's own folder).
/// `sibling("a.jpg", "Cropped", None)` -> `a Cropped.jpg`; `ext` replaces the extension.
pub fn sibling(src: &Path, dir: Option<&Path>, label: &str, ext: Option<&str>) -> PathBuf {
    let dir = dir.map(Path::to_path_buf).unwrap_or_else(|| src.parent().map(Path::to_path_buf).unwrap_or_default());
    let stem = src.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let stem = if label.is_empty() { stem } else { format!("{stem} {label}").trim().to_string() };
    let ext = match ext {
        Some(e) => e.trim_start_matches('.').to_string(),
        None => src.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default(),
    };
    let name = if ext.is_empty() { stem } else { format!("{stem}.{ext}") };
    unique_path(&dir.join(name))
}

/// Write through a hidden temp file, then rename: Explorer never shows a half-written file.
pub fn atomic_write(out: &Path, write: impl FnOnce(&Path) -> io::Result<()>) -> io::Result<()> {
    let name = out.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = out.with_file_name(format!(".~{name}.part"));
    let res = write(&tmp).and_then(|()| std::fs::rename(&tmp, out));
    if tmp.exists() {
        let _ = std::fs::remove_file(&tmp);
    }
    res
}

/// "2.4 MB", "610.0 KB", "12 B": the sizes shown in notifications.
pub fn human_size(n: u64) -> String {
    let mut v = n as f64;
    for unit in ["B", "KB", "MB", "GB"] {
        if v < 1024.0 || unit == "GB" {
            return if unit == "B" { format!("{v:.0} {unit}") } else { format!("{v:.1} {unit}") };
        }
        v /= 1024.0;
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cratus-paths-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn sibling_names_and_never_overwrites() {
        let d = tmpdir("sib");
        let src = d.join("a.jpg");
        std::fs::write(&src, b"x").unwrap();
        assert_eq!(sibling(&src, None, "Cropped", None), d.join("a Cropped.jpg"));
        assert_eq!(sibling(&src, None, "", Some("png")), d.join("a.png"));
        // same name taken -> numbered
        assert_eq!(sibling(&src, None, "", None), d.join("a (1).jpg"));
        std::fs::write(d.join("a (1).jpg"), b"x").unwrap();
        assert_eq!(sibling(&src, None, "", None), d.join("a (2).jpg"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn atomic_write_leaves_no_part_file_on_error() {
        let d = tmpdir("atomic");
        let out = d.join("o.bin");
        let err = atomic_write(&out, |tmp| {
            std::fs::write(tmp, b"half")?;
            Err(io::Error::other("boom"))
        });
        assert!(err.is_err());
        assert!(!out.exists());
        assert_eq!(std::fs::read_dir(&d).unwrap().count(), 0);
        atomic_write(&out, |tmp| std::fs::write(tmp, b"ok")).unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), b"ok");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn sizes() {
        assert_eq!(human_size(12), "12 B");
        assert_eq!(human_size(2_516_582), "2.4 MB");
    }
}

/// Where an app keeps its settings: %APPDATA%\<app> on Windows, $XDG_CONFIG_HOME/<app>
/// (~/.config/<app>) elsewhere.
pub fn config_dir(app: &str) -> PathBuf {
    platform_dir("APPDATA", "XDG_CONFIG_HOME", ".config").join(app)
}

/// Where an app keeps its logs and small state: %LOCALAPPDATA%\<app> on Windows,
/// $XDG_STATE_HOME/<app> (~/.local/state/<app>) elsewhere.
pub fn state_dir(app: &str) -> PathBuf {
    platform_dir("LOCALAPPDATA", "XDG_STATE_HOME", ".local/state").join(app)
}

fn platform_dir(win: &str, xdg: &str, fallback: &str) -> PathBuf {
    let var = if cfg!(windows) { win } else { xdg };
    if let Some(d) = std::env::var_os(var).map(PathBuf::from).filter(|p| p.is_absolute()) {
        return d;
    }
    if !cfg!(windows) {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(fallback);
        }
    }
    std::env::temp_dir()
}

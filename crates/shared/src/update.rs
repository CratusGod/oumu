//! In-app updates from GitHub releases: is there a newer version, and fetch its setup.
//!
//! The newest release of github.com/<owner>/<repo> is read from the GitHub API. A release
//! offers an update when its tag (`v2.0.1`) is newer than the running version and it carries
//! the app's setup (`Utsuri-Setup.exe`). The download must match the asset's SHA-256, taken
//! from the release (GitHub's `digest`, or a `<setup>.sha256` file beside it); without one it
//! is refused. Installing it is the app's part (crates/install on Windows).
//!
//! The network goes through curl (it ships with Windows 10 and later and every Linux), so the
//! apps carry no TLS stack of their own for this. Tests point `feed` at a local JSON file
//! (`file://`), whose asset URLs may be `file://` too.

use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    /// Without the leading "v".
    pub version: String,
    /// The setup's download URL.
    pub url: String,
    pub name: String,
    /// Lowercase hex.
    pub sha256: String,
    pub size: u64,
    /// The release page, for "what's new".
    pub page: String,
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<ApiAsset>,
}

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    digest: Option<String>,
}

/// The API address of a repo's newest release.
pub fn feed_url(owner: &str, repo: &str) -> String {
    format!("https://api.github.com/repos/{owner}/{repo}/releases/latest")
}

/// "2.0.1" → [2, 0, 1]. A leading "v" and anything after a '-' or '+' are ignored.
fn parts(v: &str) -> Option<Vec<u64>> {
    let v = v.trim().trim_start_matches(['v', 'V']);
    let v = v.split(['-', '+']).next()?;
    let p: Option<Vec<u64>> = v.split('.').map(|x| x.parse().ok()).collect();
    p.filter(|p| !p.is_empty())
}

/// True when `other` is a newer version than `current`.
pub fn is_newer(current: &str, other: &str) -> bool {
    match (parts(current), parts(other)) {
        (Some(mut a), Some(mut b)) => {
            let n = a.len().max(b.len());
            a.resize(n, 0);
            b.resize(n, 0);
            b > a
        }
        _ => false,
    }
}

fn curl() -> Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // the system's own curl, not whatever a PATH entry offers
        let sys = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let mut c = Command::new(sys.join(r"System32\curl.exe"));
        c.creation_flags(0x0800_0000); // no console window
        c
    }
    #[cfg(not(windows))]
    {
        Command::new("curl")
    }
}

fn local_path(url: &str) -> Option<PathBuf> {
    let rest = url.strip_prefix("file://")?;
    // file:///C:/x and file:///home/x
    let rest = if rest.len() > 2 && rest.starts_with('/') && rest.as_bytes()[2] == b':' { &rest[1..] } else { rest };
    Some(PathBuf::from(rest))
}

/// GET a small text (the release list).
fn fetch_text(url: &str, agent: &str) -> Result<String, String> {
    if let Some(p) = local_path(url) {
        return std::fs::read_to_string(&p).map_err(|e| format!("couldn't read {}: {e}", p.display()));
    }
    let out = curl()
        .args(["-fsSL", "--max-time", "20", "--proto", "=https", "-H", "Accept: application/vnd.github+json", "-A", agent, url])
        .output()
        .map_err(|e| format!("couldn't start curl: {e}"))?;
    if !out.status.success() {
        return Err(net_error(&out.stderr));
    }
    String::from_utf8(out.stdout).map_err(|_| "the reply wasn't text".into())
}

fn net_error(stderr: &[u8]) -> String {
    let e = String::from_utf8_lossy(stderr);
    let e = e.trim().trim_start_matches("curl: ").to_string();
    if e.contains("404") {
        "no release was found".into()
    } else if e.contains("403") || e.contains("429") {
        "GitHub is limiting requests: try again in an hour".into()
    } else if e.contains("resolve host") || e.contains("connect") || e.contains("timed out") {
        "no internet connection".into()
    } else if e.is_empty() {
        "the download failed".into()
    } else {
        e
    }
}

/// Read the release list and pick an update, if there is one.
/// `setup` is the asset to look for (e.g. "Utsuri-Setup.exe").
pub fn parse(json: &str, current: &str, setup: &str) -> Result<Option<Release>, String> {
    let r: ApiRelease = serde_json::from_str(json).map_err(|e| format!("the release list didn't read: {e}"))?;
    if r.draft || r.prerelease || !is_newer(current, &r.tag_name) {
        return Ok(None);
    }
    let version = r.tag_name.trim_start_matches(['v', 'V']).to_string();
    let Some(a) = r.assets.iter().find(|a| a.name.eq_ignore_ascii_case(setup)) else {
        // a release without the setup (yet: it is uploaded after the release is made)
        return Ok(None);
    };
    let sha256 = a.digest.as_deref().and_then(|d| d.strip_prefix("sha256:")).map(str::to_ascii_lowercase).unwrap_or_default();
    let sha_file = r.assets.iter().find(|x| x.name.eq_ignore_ascii_case(&format!("{setup}.sha256"))).map(|x| x.browser_download_url.clone());
    let mut rel = Release { version, url: a.browser_download_url.clone(), name: a.name.clone(), sha256, size: a.size, page: r.html_url };
    if rel.sha256.is_empty() {
        // stored in the URL for `check` to read; never left in a returned release
        rel.sha256 = sha_file.map(|u| format!("@{u}")).unwrap_or_default();
    }
    Ok(Some(rel))
}

fn is_hex64(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Ask `feed` (see `feed_url`) for an update to `current`.
pub fn check(feed: &str, current: &str, setup: &str, agent: &str) -> Result<Option<Release>, String> {
    let json = fetch_text(feed, agent)?;
    let Some(mut rel) = parse(&json, current, setup)? else { return Ok(None) };
    if let Some(u) = rel.sha256.strip_prefix('@').map(str::to_string) {
        let t = fetch_text(&u, agent)?;
        rel.sha256 = t.split_whitespace().next().unwrap_or_default().to_ascii_lowercase();
    }
    if !is_hex64(&rel.sha256) {
        return Err(format!("version {} has no checksum, so it can't be installed safely", rel.version));
    }
    Ok(Some(rel))
}

/// SHA-256 of a file, lowercase hex.
pub fn sha256_of(path: &Path) -> std::io::Result<String> {
    use sha2::Digest;
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut h = sha2::Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Download the release's setup into `dir` and check it. Returns the file.
pub fn download(rel: &Release, dir: &Path, agent: &str) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("couldn't create {}: {e}", dir.display()))?;
    let name: String = rel.name.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')).collect();
    let name = if name.is_empty() || name.starts_with('.') { "setup.exe".to_string() } else { name };
    let fin = dir.join(&name);
    let part = dir.join(format!("{name}.part"));
    let _ = std::fs::remove_file(&part);
    if let Some(src) = local_path(&rel.url) {
        std::fs::copy(&src, &part).map_err(|e| format!("couldn't copy {}: {e}", src.display()))?;
    } else {
        let out = curl()
            .args(["-fsSL", "--max-time", "900", "--proto", "=https", "--proto-redir", "=https", "-A", agent, "-o"])
            .arg(&part)
            .arg(&rel.url)
            .output()
            .map_err(|e| format!("couldn't start curl: {e}"))?;
        if !out.status.success() {
            let _ = std::fs::remove_file(&part);
            return Err(net_error(&out.stderr));
        }
    }
    let got = sha256_of(&part).map_err(|e| format!("couldn't read the download: {e}"))?;
    if got != rel.sha256 {
        let _ = std::fs::remove_file(&part);
        return Err("the download was damaged (its checksum didn't match)".into());
    }
    let _ = std::fs::remove_file(&fin);
    std::fs::rename(&part, &fin).map_err(|e| format!("couldn't save the download: {e}"))?;
    Ok(fin)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert!(is_newer("2.0.0", "v2.0.1"));
        assert!(is_newer("2.0.9", "2.1.0"));
        assert!(is_newer("0.2", "0.2.1"));
        assert!(is_newer("1.9.9", "10.0.0"));
        assert!(!is_newer("2.0.1", "v2.0.1"));
        assert!(!is_newer("2.0.1", "2.0.0"));
        assert!(!is_newer("2.0.0", "latest"));
    }

    fn json(tag: &str, digest: &str) -> String {
        format!(
            r#"{{"tag_name":"{tag}","html_url":"https://example.org/r","draft":false,"prerelease":false,
               "assets":[{{"name":"App-Setup.exe","browser_download_url":"https://example.org/App-Setup.exe","size":5,"digest":"{digest}"}},
                         {{"name":"App-Setup.exe.sha256","browser_download_url":"https://example.org/App-Setup.exe.sha256","size":64}}]}}"#
        )
    }

    #[test]
    fn picks_the_setup() {
        let d = "sha256:".to_string() + &"AB".repeat(32);
        let r = parse(&json("v1.2.0", &d), "1.1.0", "app-setup.exe").unwrap().unwrap();
        assert_eq!(r.version, "1.2.0");
        assert_eq!(r.url, "https://example.org/App-Setup.exe");
        assert_eq!(r.sha256, "ab".repeat(32));
        assert_eq!(parse(&json("v1.1.0", &d), "1.1.0", "App-Setup.exe").unwrap(), None);
        assert_eq!(parse(&json("v1.2.0", &d), "1.1.0", "Other.exe").unwrap(), None);
        // no digest: the .sha256 file is fetched later
        let r = parse(&json("v1.2.0", ""), "1.1.0", "App-Setup.exe").unwrap().unwrap();
        assert_eq!(r.sha256, "@https://example.org/App-Setup.exe.sha256");
        assert!(parse("{", "1", "x").is_err());
    }

    #[test]
    fn downloads_and_checks_local_files() {
        let dir = std::env::temp_dir().join(format!("shared-update-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("src.exe");
        std::fs::write(&src, b"hello").unwrap();
        let sum = sha256_of(&src).unwrap();
        assert_eq!(sum, "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
        let url = format!("file:///{}", src.display().to_string().replace('\\', "/").trim_start_matches('/'));
        let feed = dir.join("feed.json");
        std::fs::write(&feed, format!(r#"{{"tag_name":"v9.0.0","assets":[{{"name":"App-Setup.exe","browser_download_url":"{url}","digest":"sha256:{sum}"}}]}}"#)).unwrap();
        let feed_url = format!("file:///{}", feed.display().to_string().replace('\\', "/").trim_start_matches('/'));
        let rel = check(&feed_url, "1.0.0", "App-Setup.exe", "test").unwrap().unwrap();
        let got = download(&rel, &dir.join("dl"), "test").unwrap();
        assert_eq!(std::fs::read(&got).unwrap(), b"hello");
        // a wrong checksum is refused and leaves nothing behind
        let bad = Release { sha256: "0".repeat(64), ..rel };
        assert!(download(&bad, &dir.join("dl2"), "test").is_err());
        assert!(!dir.join("dl2").join("App-Setup.exe").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

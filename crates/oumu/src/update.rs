//! Updates from GitHub (github.com/CratusGod/oumu releases): a look a little after start and
//! twice a day after (Settings can turn that off), or now from the tray or Settings. A newer
//! version shows a banner in the panel and an item in the tray; Update downloads its setup,
//! checks its SHA-256, and runs it quietly over this install, which starts Oumu again.
//!
//! Tests: OUMU_UPDATE_FEED (a release list URL, file:// works), OUMU_FAKE_VERSION (the
//! version this copy claims), OUMU_UPDATE_AFTER_MS (first automatic look), and
//! OUMU_UPDATE_YES=1 (install a found update without asking).

use shared::update::Release;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

pub const SETUP: &str = "Oumu-Setup.exe";
const EVERY: Duration = Duration::from_secs(12 * 3600);

#[derive(Clone, Debug, Default, PartialEq)]
pub enum Status {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Found(Release),
    Downloading(String),
    Failed(String),
}

#[derive(Default)]
struct State {
    status: Status,
    /// the newest version found (kept while a later step fails)
    found: Option<Release>,
    /// "Later" on the banner: hidden until a newer version turns up
    later: Option<String>,
    front: Option<rune::AppHandle<crate::ui::Front>>,
    panel: Option<rune::AppHandle<crate::ui::panel::PanelS>>,
}

fn state() -> &'static Mutex<State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(Default::default)
}

pub fn current() -> String {
    std::env::var("OUMU_FAKE_VERSION").unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string())
}

fn feed() -> String {
    std::env::var("OUMU_UPDATE_FEED").unwrap_or_else(|_| shared::update::feed_url("CratusGod", "oumu"))
}

fn agent() -> String {
    format!("Oumu/{} (update check)", env!("CARGO_PKG_VERSION"))
}

pub fn status() -> Status {
    state().lock().unwrap().status.clone()
}

/// The version found, if any.
pub fn found() -> Option<String> {
    state().lock().unwrap().found.as_ref().map(|r| r.version.clone())
}

/// The version the panel's banner offers (none after "Later" on it).
pub fn banner() -> Option<String> {
    let s = state().lock().unwrap();
    if matches!(s.status, Status::Downloading(_)) {
        return None;
    }
    s.found.as_ref().map(|r| r.version.clone()).filter(|v| s.later.as_ref() != Some(v))
}

pub fn later() {
    let mut s = state().lock().unwrap();
    s.later = s.found.as_ref().map(|r| r.version.clone());
    drop(s);
    notify();
}

/// The UI to tell when something changes.
pub fn set_handles(front: rune::AppHandle<crate::ui::Front>, panel: rune::AppHandle<crate::ui::panel::PanelS>) {
    let mut s = state().lock().unwrap();
    s.front = Some(front);
    s.panel = Some(panel);
}

fn set(st: Status) {
    let mut s = state().lock().unwrap();
    if let Status::Found(r) = &st {
        s.found = Some(r.clone());
    }
    if st == Status::UpToDate {
        s.found = None;
    }
    s.status = st;
    drop(s);
    notify();
}

fn notify() {
    let (front, panel) = {
        let s = state().lock().unwrap();
        (s.front.clone(), s.panel.clone())
    };
    if let Some(p) = panel {
        p.update(|_, _| {});
    }
    if let Some(f) = front {
        f.update(|f, _| f.refresh_tray());
    }
}

/// Can this copy replace itself? (an installed copy, not a build folder)
pub fn installed() -> bool {
    #[cfg(windows)]
    {
        install::installed_here(&install::OUMU).is_some()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Automatic looks: only for an installed copy, never in a test run (unless a test asks).
pub fn start_schedule(shared: crate::host::Shared) {
    let asked = std::env::var("OUMU_UPDATE_AFTER_MS").ok().and_then(|v| v.parse::<u64>().ok());
    if asked.is_none() && (std::env::var_os("OUMU_DATA_DIR").is_some() || !installed()) {
        log::info!("update: no automatic checks (not an installed copy, or a test run)");
        return;
    }
    let first = Duration::from_millis(asked.unwrap_or(30_000));
    std::thread::Builder::new()
        .name("oumu-update".into())
        .spawn(move || {
            std::thread::sleep(first);
            loop {
                let on = shared.lock().map(|s| s.settings.check_updates).unwrap_or(true);
                if on {
                    check(false);
                }
                std::thread::sleep(EVERY);
            }
        })
        .expect("update thread");
}

/// Look for a newer version (on a thread of its own).
pub fn check(manual: bool) {
    {
        let mut s = state().lock().unwrap();
        if matches!(s.status, Status::Checking | Status::Downloading(_)) {
            return;
        }
        s.status = Status::Checking;
    }
    if manual {
        notify();
    }
    std::thread::spawn(move || match shared::update::check(&feed(), &current(), SETUP, &agent()) {
        Ok(Some(rel)) => {
            log::info!("update: {} is available ({})", rel.version, rel.url);
            set(Status::Found(rel));
            if std::env::var_os("OUMU_UPDATE_YES").is_some() {
                install();
            }
        }
        Ok(None) => {
            log::info!("update: {} is the newest", current());
            set(Status::UpToDate);
        }
        Err(e) => {
            log::warn!("update check: {e}");
            // an automatic look that failed says nothing; the next one may work
            let keep = state().lock().unwrap().found.clone();
            set(if manual { Status::Failed(format!("Couldn't check for updates: {e}")) } else { keep.map(Status::Found).unwrap_or_default() });
        }
    });
}

/// Download the found version, check it, and run its setup over this install.
pub fn install() {
    let Some(rel) = state().lock().unwrap().found.clone() else {
        check(true);
        return;
    };
    if matches!(status(), Status::Downloading(_)) {
        return;
    }
    if !installed() {
        set(Status::Failed("This copy runs from a build folder, not an install, so it can't update itself. Get the new version from the releases page.".into()));
        return;
    }
    log::info!("update: downloading {}", rel.version);
    set(Status::Downloading(rel.version.clone()));
    std::thread::spawn(move || {
        let dir = std::env::temp_dir().join("Oumu-update");
        let r = shared::update::download(&rel, &dir, &agent()).and_then(|setup| {
            log::info!("update: downloaded and checked {}", setup.display());
            #[cfg(windows)]
            return install::start_update(&install::OUMU, &setup).map_err(|e| format!("{e:#}"));
            #[cfg(not(windows))]
            return Err(format!("run {} to update", setup.display()));
        });
        match r {
            Ok(()) => {
                log::info!("update: setup started; quitting so it can replace Oumu");
                let front = state().lock().unwrap().front.clone();
                match front {
                    Some(f) => f.update(|f, tx| f.quit(tx)),
                    None => std::process::exit(0),
                }
            }
            Err(e) => {
                log::warn!("update: {e}");
                set(Status::Failed(format!("Couldn't update Oumu: {e}")));
                // the found version stays on offer
                let mut s = state().lock().unwrap();
                s.found = Some(rel);
            }
        }
    });
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn installer_version_is_the_apps() {
        assert_eq!(install::OUMU.version, env!("CARGO_PKG_VERSION"), "bump crates/install AppInfo with Cargo.toml");
    }
}

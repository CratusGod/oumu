//! Oumu: a local-first writing assistant. Harper checks grammar offline, the desktop's
//! accessibility API (UI Automation on Windows, AT-SPI on Linux) reads and fixes the text
//! field in front, and an AI key adds rewrite,
//! translate, tone and more. Windows build: the original Tauri logic runs unchanged on
//! `oumu-rt` (a Tauri-shaped runtime), with a Rune front end in `ui`.

pub mod aiphrases;
pub mod casual;
pub mod commands;
pub mod debug_server;
pub mod engine;
pub mod features;
pub mod foreign;
pub mod history;
pub mod host;
pub mod llm;
pub mod matugen;
pub mod overlay;
pub mod pace;
pub mod score;
pub mod settings;
pub mod style_rules;
pub mod textmap;
pub mod theme;
pub mod ui;
pub mod update;
#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(windows)]
pub mod win;
pub mod wm;

#[cfg(target_os = "linux")]
pub use linux::{apply, ax, remote};
#[cfg(windows)]
pub use win::{apply, ax, remote};

use std::sync::{Arc, Mutex};

pub fn run() {
    // Settings → Apps → Uninstall (written by Oumu-Setup.exe)
    let args: Vec<String> = std::env::args().skip(1).collect();
    #[cfg(windows)]
    if args.iter().any(|a| a == "--uninstall") {
        let o = install::Opts::from_args(&args);
        if let Err(e) = install::uninstall(&install::OUMU, &o, || Ok(())) {
            if !o.silent {
                install::tell("Uninstall Oumu", &format!("Couldn't remove Oumu:\n\n{e:#}"), true);
            }
            std::process::exit(1);
        }
        return;
    }
    // a crash writes where it happened (temp folder); test runs get no crash box
    #[cfg(windows)]
    {
        let quiet = ui::headless::wanted() || std::env::var_os("OUMU_DATA_DIR").is_some();
        win::crash::install(quiet);
        win::crash::self_test();
    }
    // headless renders, scripts and trees (Rune's tooling): every surface as a window
    if ui::headless::wanted() {
        ui::headless::main();
        return;
    }
    // the browser extension's native-messaging host is this same binary
    #[cfg(target_os = "linux")]
    if linux::remote::launched_by_browser(&args) {
        std::process::exit(linux::remote::run_native_host());
    }
    // a release build has no console, so the log goes to a file
    #[cfg(windows)]
    {
        win::startup::init_logging();
        // started at sign-in (the Run key): the taskbar may not exist yet
        win::startup::wait_for_taskbar();
    }
    #[cfg(target_os = "linux")]
    linux::startup::init_logging();
    // `oumu --translate` (a keybind; Utsuri's Translate slice): hand the request to the
    // running Oumu, or start one that translates once it is up
    let translate_on_start = std::env::args().any(|a| a == remote::TRANSLATE_FLAG);
    if translate_on_start && remote::send_translate() {
        return;
    }
    // one Oumu at a time: a second launch opens the running one's panel and exits
    if !translate_on_start && remote::send_open() {
        return;
    }

    let mut state = host::HostState::default();
    state.settings = settings::load();
    state.spot_cache = features::spotcheck::load_cache();
    state.history = history::load();
    let shared: host::Shared = Arc::new(Mutex::new(state));
    let dock_side = shared.lock().unwrap().settings.dock_side.clone();

    let app = tauri::AppHandle::new();
    app.manage(shared.clone());
    ui::run(app, shared, dock_side, translate_on_start);
}

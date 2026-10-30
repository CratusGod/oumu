//! Oumu: a local-first Grammarly-style writing assistant for macOS and Linux.
//! Native Tauri host + Harper engine + Accessibility overlays + LLM features.

pub mod aiphrases;
#[cfg(target_os = "macos")]
pub mod apply;
#[cfg(target_os = "macos")]
pub mod ax;
#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "linux")]
pub use linux::{apply, ax};
pub mod commands;
pub mod debug_server;
pub mod engine;
pub mod features;
pub mod history;
pub mod host;
pub mod llm;
pub mod matugen;
pub mod overlay;
pub mod pace;
pub mod score;
pub mod settings;
pub mod textmap;
pub mod theme;
pub mod wm;

use std::sync::{Arc, Mutex};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::Manager;

pub fn run() {
    // Started by the browser extension as its native-messaging host: answer
    // the one message and exit, without opening any window.
    #[cfg(target_os = "linux")]
    if linux::remote::launched_by_browser(&std::env::args().collect::<Vec<_>>()) {
        std::process::exit(linux::remote::run_native_host());
    }
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).try_init();
    #[cfg(target_os = "linux")]
    linux_env();
    // `oumu --translate` (a keybind): hand the request to the running Oumu,
    // or start one that translates once it is up.
    #[cfg(target_os = "linux")]
    let translate_on_start = std::env::args().any(|a| a == linux::remote::TRANSLATE_FLAG);
    #[cfg(target_os = "linux")]
    if translate_on_start && linux::remote::send_translate() {
        return;
    }
    // One Oumu at a time: a second launch (launcher, autostart) opens
    // the running one's panel and exits.
    #[cfg(target_os = "linux")]
    if !translate_on_start && linux::remote::send_open() {
        return;
    }

    let mut state = host::HostState::default();
    state.settings = settings::load();
    state.history = history::load();
    let shared: host::Shared = Arc::new(Mutex::new(state));
    let dock_side = shared.lock().unwrap().settings.dock_side.clone();

    tauri::Builder::default()
        .manage(shared.clone())
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            let handle = app.handle().clone();
            overlay::create_windows(&handle, &dock_side)?;
            // Start tucked behind the edge tab, like Grammarly. First run (no
            // Accessibility yet) starts open so the permission gate is visible.
            // Linux: the bus is switched back on a moment after launch once the
            // user agreed before, so that counts as trusted here.
            #[cfg(target_os = "linux")]
            let trusted = ax::is_trusted() || ax::consent_saved();
            #[cfg(not(target_os = "linux"))]
            let trusted = ax::is_trusted();
            overlay::set_tucked(&handle, &dock_side, trusted, false, false);
            #[cfg(target_os = "linux")]
            {
                linux::remote::listen(handle.clone());
                if translate_on_start {
                    let h = handle.clone();
                    std::thread::spawn(move || {
                        // Let the panel's page load before it is asked to translate.
                        std::thread::sleep(std::time::Duration::from_millis(1500));
                        linux::remote::translate_highlight(&h);
                    });
                }
            }

            let open = MenuItem::with_id(app, "open", "Open Oumu", true, None::<&str>)?;
            let perms = MenuItem::with_id(app, "perms", "Accessibility permission…", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit Oumu", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &perms, &quit])?;
            let mut tray = TrayIconBuilder::with_id("main").menu(&menu).show_menu_on_left_click(true).tooltip("Oumu");
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.on_menu_event(|app, event| match event.id.as_ref() {
                "open" => commands::open_panel_inner(app, "review", true),
                "perms" => {
                    // Linux: a D-Bus round trip and `gsettings`; keep it off the GTK main thread.
                    #[cfg(target_os = "linux")]
                    std::thread::spawn(|| {
                        ax::prompt_trust();
                    });
                    #[cfg(not(target_os = "linux"))]
                    ax::prompt_trust();
                }
                "quit" => app.exit(0),
                _ => {}
            })
            .build(app)?;

            host::spawn(handle.clone(), shared.clone());
            debug_server::spawn(handle.clone(), shared.clone());
            theme::watch(handle.clone(), shared.clone());
            #[cfg(target_os = "linux")]
            linux::desktop::init(handle.clone());
            // macOS shows its own consent dialog here; on Linux "trust" is a
            // desktop-wide switch, flipped only from the panel's button. The bus
            // drops it again when Oumu quits, so once the user has turned it on
            // (the saved setting says so) each launch turns it back on.
            #[cfg(target_os = "macos")]
            if !ax::is_trusted() {
                ax::prompt_trust();
            }
            #[cfg(target_os = "linux")]
            std::thread::spawn(|| {
                if !ax::is_trusted() && ax::consent_saved() {
                    ax::prompt_trust();
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "panel" {
                    api.prevent_close();
                    commands::tuck_panel_inner(window.app_handle());
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::get_palette,
            commands::matugen_status,
            commands::matugen_setup,
            commands::tab_hover,
            commands::highlighted_text,
            commands::take_pending_translation,
            commands::accept_alert,
            commands::dismiss_alert,
            commands::learn_word,
            commands::mute_rule,
            commands::replace_range,
            commands::run_feature,
            commands::chat_send,
            commands::get_settings,
            commands::set_settings,
            commands::get_history,
            commands::open_panel,
            commands::hide_panel,
            commands::tuck_panel,
            commands::permission_prompt,
            commands::permission_status,
            commands::set_card_rects,
            commands::open_url,
            commands::quit_app,
            commands::ui_ready,
            commands::ui_log,
            commands::probe_key,
            commands::probe_searxng,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Oumu");
}


/// WebKitGTK's DMA-BUF renderer draws blank or flickering webviews on NVIDIA
/// and on some Wayland setups; Oumu's windows are small, so the shared-memory
/// path costs nothing. Must run before GTK starts. An explicit setting wins.
#[cfg(target_os = "linux")]
fn linux_env() {
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
}

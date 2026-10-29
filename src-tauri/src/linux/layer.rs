//! Oumu's windows as Wayland layer-shell surfaces.
//!
//! A Wayland client cannot move its own windows or keep them above others, so
//! the panel, edge tab, Oumu button and underline layer are re-hosted in
//! gtk-layer-shell windows: anchored to the top-left of an output and placed
//! by margins. Tauri keeps owning the webview (IPC, events); only its GTK
//! container changes. Where layer shell is missing (GNOME, an X11 session) the
//! windows stay ordinary Tauri windows.
//!
//! GTK objects live on the main thread: callers on other threads queue work
//! there, and the last requested geometry is mirrored in `GEO` so reads never
//! wait for the main loop.

use crate::wm::plain;
use gtk::prelude::*;
use gtk_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Manager};

#[derive(Clone, Copy, Debug, Default)]
struct Geo {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    visible: bool,
    passthrough: bool,
}

thread_local! {
    static WINDOWS: RefCell<HashMap<String, gtk::ApplicationWindow>> = RefCell::new(HashMap::new());
    /// Output index each surface was last put on.
    static MONITOR: RefCell<HashMap<String, i32>> = RefCell::new(HashMap::new());
}

fn geo() -> &'static Mutex<HashMap<String, Geo>> {
    static GEO: OnceLock<Mutex<HashMap<String, Geo>>> = OnceLock::new();
    GEO.get_or_init(|| Mutex::new(HashMap::new()))
}

fn adopted(label: &str) -> bool {
    geo().lock().map(|g| g.contains_key(label)).unwrap_or(false)
}

fn update(label: &str, f: impl FnOnce(&mut Geo)) -> Option<Geo> {
    let mut g = geo().lock().ok()?;
    let e = g.get_mut(label)?;
    f(e);
    Some(*e)
}

/// Run `f` on the main thread with the layer window for `label`.
fn on_main(app: &AppHandle, label: &str, f: impl FnOnce(&gtk::ApplicationWindow) + Send + 'static) {
    let label = label.to_string();
    let _ = app.run_on_main_thread(move || {
        WINDOWS.with(|m| {
            if let Some(w) = m.borrow().get(&label) {
                f(w);
            }
        })
    });
}

/// The panel takes keyboard focus when clicked (chat, settings fields); the
/// overlays never do.
fn keyboard_mode(label: &str) -> KeyboardMode {
    if label == "panel" {
        KeyboardMode::OnDemand
    } else {
        KeyboardMode::None
    }
}

fn layer_for(label: &str) -> Layer {
    if label == "panel" {
        Layer::Top
    } else {
        Layer::Overlay
    }
}

/// Re-host the Tauri window `label` in a layer-shell surface. Main thread only
/// (call it from `setup`, right after the window is built).
pub fn adopt(app: &AppHandle, label: &str) {
    if !gtk_layer_shell::is_supported() {
        if label != "panel" && unmanage_on_x11(app, label) {
            return;
        }
        log::warn!("layer shell is not available; {label} stays a normal window (overlays will not follow text)");
        return;
    }
    if let Err(e) = try_adopt(app, label) {
        log::warn!("layer shell for {label} failed ({e}); keeping a normal window");
    }
}

/// On X11 an overlay (tab, button, underlines) is override-redirect: the window
/// manager neither focuses nor places it, so showing one never takes the
/// keyboard from the app being typed in. `false` off X11 (nothing changed).
fn unmanage_on_x11(app: &AppHandle, label: &str) -> bool {
    let Some(gw) = app.get_webview_window(label).and_then(|w| w.gtk_window().ok()) else { return false };
    // A tooltip-type window is never given focus, also by XWayland compositors.
    gw.set_type_hint(gtk::gdk::WindowTypeHint::Tooltip);
    gw.set_accept_focus(false);
    gw.realize();
    let Some(gdk_window) = gw.window() else { return false };
    if gdk_window.type_().name() != "GdkX11Window" {
        return false;
    }
    gdk_window.set_override_redirect(true);
    log::info!("{label}: X11 overlay window, not managed by the window manager");
    true
}

fn try_adopt(app: &AppHandle, label: &str) -> Result<(), String> {
    let win = app.get_webview_window(label).ok_or("no such window")?;
    let scale = win.scale_factor().unwrap_or(1.0);
    let size = win.inner_size().map_err(|e| e.to_string())?;
    let (w, h) = (size.width as f64 / scale, size.height as f64 / scale);
    let _ = win.hide();
    let old = win.gtk_window().map_err(|e| format!("gtk_window: {e}"))?;
    let gapp = old.application().ok_or("gtk window has no application")?;
    let vbox = win.default_vbox().map_err(|e| format!("default_vbox: {e}"))?;
    old.remove(&vbox);

    let lw = gtk::ApplicationWindow::new(&gapp);
    lw.set_title(label);
    lw.set_app_paintable(true);
    // Clear the window to transparent before each frame. An app-paintable RGBA
    // window is otherwise never cleared, so wherever the page turned
    // transparent the last frame's pixels stayed: a moving tab left trails.
    lw.connect_draw(|_, cr| {
        cr.set_operator(gtk::cairo::Operator::Source);
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.0);
        let _ = cr.paint();
        cr.set_operator(gtk::cairo::Operator::Over);
        gtk::glib::Propagation::Proceed
    });
    if let Some(visual) = GtkWindowExt::screen(&lw).and_then(|s| s.rgba_visual()) {
        lw.set_visual(Some(&visual));
    }
    lw.add(&vbox);
    lw.init_layer_shell();
    lw.set_namespace(&format!("oumu-{label}"));
    lw.set_layer(layer_for(label));
    lw.set_anchor(Edge::Top, true);
    lw.set_anchor(Edge::Left, true);
    lw.set_anchor(Edge::Right, false);
    lw.set_anchor(Edge::Bottom, false);
    // -1: ignore other surfaces' exclusive zones (bars), so margins count from the output edge.
    lw.set_exclusive_zone(-1);
    lw.set_keyboard_mode(keyboard_mode(label));
    lw.set_size_request(w.round() as i32, h.round() as i32);
    lw.set_default_size(w.round() as i32, h.round() as i32);

    geo().lock().map_err(|e| e.to_string())?.insert(label.to_string(), Geo { w, h, ..Geo::default() });
    WINDOWS.with(|m| m.borrow_mut().insert(label.to_string(), lw));
    log::info!("{label}: layer-shell surface");
    Ok(())
}

/// The WebKit view inside a Tauri window (wry packs it into the window's vbox).
pub fn webkit_view(win: &tauri::WebviewWindow) -> Option<webkit2gtk::WebView> {
    let vbox = win.default_vbox().ok()?;
    vbox.children().into_iter().find_map(|w| w.downcast::<webkit2gtk::WebView>().ok())
}

/// How far left of its monitor a surface may start (negative left margin).
const SLACK_OFF_EDGE: f64 = 64.0;

/// Pick the output that holds the point and the margins inside it.
fn place(lw: &gtk::ApplicationWindow, x: f64, y: f64) {
    let Some(display) = gtk::gdk::Display::default() else { return };
    let mut chosen: Option<(i32, gtk::gdk::Monitor, gtk::gdk::Rectangle)> = None;
    for i in 0..display.n_monitors() {
        if let Some(m) = display.monitor(i) {
            let r = m.geometry();
            // A surface may hang off its monitor's edge (the closed edge tab), so x
            // counts as inside up to one tab's width past the edge.
            let inside = x >= r.x() as f64 - SLACK_OFF_EDGE && x < (r.x() + r.width()) as f64 && y >= r.y() as f64 && y < (r.y() + r.height()) as f64;
            if inside || chosen.is_none() {
                chosen = Some((i, m, r));
                if inside {
                    break;
                }
            }
        }
    }
    let Some((idx, m, r)) = chosen else { return };
    let key = lw.title().map(|t| t.to_string()).unwrap_or_default();
    let changed = MONITOR.with(|mm| mm.borrow_mut().insert(key, idx) != Some(idx));
    if changed {
        lw.set_monitor(&m);
    }
    let left = (x - r.x() as f64).round().max(-SLACK_OFF_EDGE) as i32;
    let top = (y - r.y() as f64).round().max(0.0) as i32;
    lw.set_layer_shell_margin(Edge::Left, left);
    lw.set_layer_shell_margin(Edge::Top, top);
}

fn apply_passthrough(lw: &gtk::ApplicationWindow, on: bool) {
    if on {
        lw.input_shape_combine_region(Some(&gtk::cairo::Region::create()));
    } else {
        lw.input_shape_combine_region(None);
    }
}

pub fn show(app: &AppHandle, label: &str) {
    let Some(g) = update(label, |g| g.visible = true) else { return plain::show(app, label) };
    on_main(app, label, move |lw| {
        place(lw, g.x, g.y);
        lw.show_all();
        apply_passthrough(lw, g.passthrough);
        // A re-mapped surface starts empty and WebKit repaints only what changes
        // next: paint the whole webview again once it has a buffer.
        lw.queue_draw();
        let weak = lw.downgrade();
        gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(REPAINT_AFTER_MS), move || {
            if let Some(lw) = weak.upgrade() {
                repaint(&lw);
            }
        });
    });
}

/// Delay before the second full repaint of a re-mapped surface.
const REPAINT_AFTER_MS: u64 = 120;

/// Redraw every widget in the window, the webview included.
fn repaint(lw: &gtk::ApplicationWindow) {
    lw.queue_draw();
    if let Some(child) = lw.child() {
        child.queue_draw();
        if let Ok(c) = child.downcast::<gtk::Container>() {
            c.forall(|w| w.queue_draw());
        }
    }
}

pub fn hide(app: &AppHandle, label: &str) {
    if update(label, |g| g.visible = false).is_none() {
        return plain::hide(app, label);
    }
    release_keyboard(app, label);
    on_main(app, label, |lw| lw.hide());
}

pub fn is_visible(app: &AppHandle, label: &str) -> bool {
    match geo().lock().ok().and_then(|g| g.get(label).copied()) {
        Some(g) => g.visible,
        None => plain::is_visible(app, label),
    }
}

/// Labels holding an exclusive keyboard grab (see `focus`).
fn grabbed() -> &'static Mutex<std::collections::HashSet<String>> {
    static GRABBED: OnceLock<Mutex<std::collections::HashSet<String>>> = OnceLock::new();
    GRABBED.get_or_init(Default::default)
}

/// Give a layer surface the keyboard now. An on-demand surface only gets it
/// when clicked, and Hyprland hands it back to the window below as soon as
/// an exclusive grab drops to on-demand. So the grab is held, and
/// `release_keyboard` ends it once the pointer has left the surface.
pub fn focus(app: &AppHandle, label: &str) {
    if !adopted(label) {
        return plain::focus(app, label);
    }
    if keyboard_mode(label) == KeyboardMode::None {
        return;
    }
    if let Ok(mut g) = grabbed().lock() {
        g.insert(label.to_string());
    }
    on_main(app, label, |lw| {
        lw.set_keyboard_mode(KeyboardMode::Exclusive);
        lw.present();
        // The page only sees keys (Esc) once the WebKit view inside has GTK focus.
        if let Some(view) = find_webview(lw.upcast_ref()) {
            view.grab_focus();
        }
    });
}

/// End a grab taken by `focus`: the keyboard goes back to the window under
/// the pointer, and the surface takes it again only when clicked.
pub fn release_keyboard(app: &AppHandle, label: &str) {
    let held = grabbed().lock().map(|mut g| g.remove(label)).unwrap_or(false);
    if !held {
        return;
    }
    let mode = keyboard_mode(label);
    on_main(app, label, move |lw| lw.set_keyboard_mode(mode));
}

fn find_webview(w: &gtk::Widget) -> Option<webkit2gtk::WebView> {
    if let Ok(v) = w.clone().downcast::<webkit2gtk::WebView>() {
        return Some(v);
    }
    let c = w.clone().downcast::<gtk::Container>().ok()?;
    c.children().iter().find_map(find_webview)
}

pub fn set_position(app: &AppHandle, label: &str, x: f64, y: f64) {
    let Some(prev) = geo().lock().ok().and_then(|g| g.get(label).copied()) else { return plain::set_position(app, label, x, y) };
    if (prev.x - x).abs() < 0.5 && (prev.y - y).abs() < 0.5 {
        return;
    }
    update(label, |g| {
        g.x = x;
        g.y = y;
    });
    on_main(app, label, move |lw| place(lw, x, y));
}

pub fn set_size(app: &AppHandle, label: &str, w: f64, h: f64) {
    let Some(prev) = geo().lock().ok().and_then(|g| g.get(label).copied()) else { return plain::set_size(app, label, w, h) };
    if (prev.w - w).abs() < 0.5 && (prev.h - h).abs() < 0.5 {
        return;
    }
    update(label, |g| {
        g.w = w;
        g.h = h;
    });
    let (wi, hi) = (w.round().max(1.0) as i32, h.round().max(1.0) as i32);
    on_main(app, label, move |lw| {
        lw.set_size_request(wi, hi);
        lw.resize(wi, hi);
    });
}

pub fn position(app: &AppHandle, label: &str) -> Option<(f64, f64)> {
    match geo().lock().ok().and_then(|g| g.get(label).copied()) {
        Some(g) => Some((g.x, g.y)),
        None => plain::position(app, label),
    }
}

pub fn frame(app: &AppHandle, label: &str) -> Option<(f64, f64, f64, f64)> {
    match geo().lock().ok().and_then(|g| g.get(label).copied()) {
        Some(g) => Some((g.x, g.y, g.w, g.h)),
        None => plain::frame(app, label),
    }
}

pub fn set_passthrough(app: &AppHandle, label: &str, on: bool) {
    let Some(prev) = geo().lock().ok().and_then(|g| g.get(label).copied()) else { return plain::set_passthrough(app, label, on) };
    if prev.passthrough == on {
        return;
    }
    update(label, |g| g.passthrough = on);
    on_main(app, label, move |lw| apply_passthrough(lw, on));
}

/// Hyprland animates layer surfaces opening and closing itself (`layersIn`/`layersOut`);
/// margin-stepping a layer surface would fight that animation.
pub fn animates_slide() -> bool {
    false
}

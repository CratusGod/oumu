//! Window creation and placement: the panel (docked left by default), the
//! floating Oumu button, and the click-through underline layer.

use serde_json::json;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use crate::wm;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

pub const PANEL_W: f64 = 440.0;
pub const PANEL_H: f64 = 740.0;
const MARGIN: f64 = 12.0;
/// Edge tab window: the tab shape is 40 pt wide and nudges out on hover, so the
/// window leaves 8 pt of transparent room for that.
/// The tab window is exactly the tab: it slides as a whole, so nothing moves
/// inside a transparent webview (WebKit leaves trails behind moving content).
pub const TAB_W: f64 = 40.0;
pub const TAB_H: f64 = 96.0;
/// While idle only this much of the tab shows at the screen edge; the rest is off screen.
pub const TAB_STRIP: f64 = 6.0;
const TAB_SLIDE_STEPS: u32 = 10;
const TAB_SLIDE_STEP_MS: u64 = 16;
/// Generation counter so a newer slide of the tab cancels an older one.
static TAB_GEN: AtomicU64 = AtomicU64::new(0);
/// How often the pointer is checked against the tab while it shows.
const TAB_POLL: Duration = Duration::from_millis(60);

/// True while the pointer is over the edge tab (the tab is out).
static TAB_OPEN: AtomicBool = AtomicBool::new(false);
const SLIDE_STEPS: u32 = 16;
const SLIDE_STEP_MS: u64 = 16;

/// True while the panel is slid away behind the edge tab.
static TUCKED: AtomicBool = AtomicBool::new(true);
/// Generation counter so a newer tuck/untuck cancels a slide still in flight.
static ANIM_GEN: AtomicU64 = AtomicU64::new(0);
/// Where the panel was when it tucked, so untuck brings it back there.
static LAST_POS: Mutex<Option<(f64, f64)>> = Mutex::new(None);

pub fn is_tucked() -> bool {
    TUCKED.load(Ordering::SeqCst)
}

pub fn primary_logical(app: &AppHandle) -> (f64, f64, f64, f64) {
    // Wayland has no primary monitor, so Tauri answers None (or not reliably); ask the compositor.
    #[cfg(target_os = "linux")]
    if let Some(b) = crate::linux::desktop::primary_monitor() {
        return b;
    }
    if let Ok(Some(m)) = app.primary_monitor() {
        let s = m.scale_factor();
        let size = m.size();
        let pos = m.position();
        return (pos.x as f64 / s, pos.y as f64 / s, size.width as f64 / s, size.height as f64 / s);
    }
    (0.0, 0.0, 1440.0, 900.0)
}

pub fn panel_rect(app: &AppHandle, side: &str) -> (f64, f64, f64, f64) {
    let (mx, my, mw, mh) = primary_logical(app);
    let h = PANEL_H.min(mh - 80.0).max(520.0);
    let y = my + 40.0;
    let x = if side == "right" { mx + mw - PANEL_W - MARGIN } else { mx + MARGIN };
    (x, y, PANEL_W, h)
}

/// The edge tab sits flush against the screen edge on the dock side, a bit above centre.
/// The tab window's box as its state says: closed it hangs off the edge.
pub fn tab_rect(app: &AppHandle, side: &str) -> (f64, f64, f64, f64) {
    tab_window(side, TAB_OPEN.load(Ordering::SeqCst), primary_logical(app))
}

/// Where the tab window sits: open flush with the edge, closed all but
/// `TAB_STRIP` off screen.
fn tab_window(side: &str, open: bool, (mx, my, mw, mh): (f64, f64, f64, f64)) -> (f64, f64, f64, f64) {
    let hidden = if open { 0.0 } else { TAB_W - TAB_STRIP };
    let x = if side == "right" { mx + mw - TAB_W + hidden } else { mx - hidden };
    let y = my + ((mh - TAB_H) * 0.42).max(60.0);
    (x, y, TAB_W, TAB_H)
}

/// The tab's hit box on a monitor `(x, y, w, h)`: the visible strip when
/// idle, the full tab when open.
fn tab_box(side: &str, open: bool, (mx, my, mw, mh): (f64, f64, f64, f64)) -> (f64, f64, f64, f64) {
    let w = if open { TAB_W } else { TAB_STRIP };
    let x = if side == "right" { mx + mw - w } else { mx };
    let y = my + ((mh - TAB_H) * 0.42).max(60.0);
    (x, y, w, TAB_H)
}

/// Whether the tab should be out for a pointer at `(px, py)`: a closed tab
/// opens when the pointer reaches its sliver, an open one closes once the
/// pointer leaves the full tab.
fn tab_should_open(side: &str, open: bool, mon: (f64, f64, f64, f64), px: f64, py: f64) -> bool {
    let (x, y, w, h) = tab_box(side, open, mon);
    px >= x && px <= x + w && py >= y && py <= y + h
}

/// Slack around a window before the pointer counts as having left it.
const POINTER_SLACK: f64 = 24.0;

/// The pointer is on `(x, y, w, h)` or within `POINTER_SLACK` of it.
pub fn near((x, y, w, h): (f64, f64, f64, f64), px: f64, py: f64) -> bool {
    px >= x - POINTER_SLACK && px <= x + w + POINTER_SLACK && py >= y - POINTER_SLACK && py <= y + h + POINTER_SLACK
}

/// Called with the pointer position every host tick while the tab shows. The
/// host polls the pointer anyway (underline hovers); a 6 px surface gets no
/// reliable pointer-enter events of its own.
pub fn tab_follow_pointer(app: &AppHandle, side: &str, px: f64, py: f64) {
    let open = TAB_OPEN.load(Ordering::SeqCst);
    let want = tab_should_open(side, open, primary_logical(app), px, py);
    if want != open {
        set_tab_open(app, side, want);
    }
}

/// Slide the edge tab out (pointer over it) or back to a sliver.
pub fn set_tab_open(app: &AppHandle, side: &str, open: bool) {
    if TAB_OPEN.swap(open, Ordering::SeqCst) == open {
        return;
    }
    let _ = app.emit_to("tab", "tab://open", json!({"open": open}));
    let gen = TAB_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let (to, y, _, _) = tab_rect(app, side);
    let from = wm::position(app, "tab").map(|p| p.0).unwrap_or(to);
    let app = app.clone();
    std::thread::spawn(move || {
        for i in 1..=TAB_SLIDE_STEPS {
            if TAB_GEN.load(Ordering::SeqCst) != gen {
                return;
            }
            let t = i as f64 / TAB_SLIDE_STEPS as f64;
            let eased = 1.0 - (1.0 - t).powi(3);
            wm::set_position(&app, "tab", from + (to - from) * eased, y);
            std::thread::sleep(Duration::from_millis(TAB_SLIDE_STEP_MS));
        }
    });
}

/// Follow the pointer for the edge tab, faster than the host loop, while the
/// tab shows and the desktop says where the pointer is.
fn follow_pointer_for_tab(app: AppHandle) {
    let spawned = std::thread::Builder::new().name("tab-pointer".into()).spawn(move || loop {
        std::thread::sleep(TAB_POLL);
        if !is_tucked() || !wm::is_visible(&app, "tab") {
            continue;
        }
        let Some((px, py)) = crate::ax::mouse_location_top_left() else { continue };
        let side = app.state::<crate::host::Shared>().lock().map(|s| s.settings.dock_side.clone()).unwrap_or_default();
        tab_follow_pointer(&app, &side, px, py);
    });
    if let Err(e) = spawned {
        log::error!("tab pointer thread: {e}");
    }
}

fn offscreen_x(app: &AppHandle, side: &str) -> f64 {
    let (mx, _, mw, _) = primary_logical(app);
    if side == "right" { mx + mw + 24.0 } else { mx - PANEL_W - 24.0 }
}

fn slide(app: &AppHandle, from: f64, to: f64, y: f64, gen: u64) -> bool {
    for i in 1..=SLIDE_STEPS {
        if ANIM_GEN.load(Ordering::SeqCst) != gen {
            return false;
        }
        let t = i as f64 / SLIDE_STEPS as f64;
        let eased = 1.0 - (1.0 - t).powi(3);
        let x = from + (to - from) * eased;
        wm::set_position(app, "panel", x, y);
        std::thread::sleep(Duration::from_millis(SLIDE_STEP_MS));
    }
    true
}

/// Slide the panel away behind the edge tab (`tucked = true`) or out from the
/// edge (`tucked = false`). Runs on its own thread so the caller never blocks;
/// a newer call cancels an older slide.
pub fn set_tucked(app: &AppHandle, side: &str, tucked: bool, animate: bool, focus: bool) {
    let gen = ANIM_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    TUCKED.store(tucked, Ordering::SeqCst);
    let app = app.clone();
    let side = side.to_string();
    std::thread::spawn(move || {
        let has_panel = app.get_webview_window("panel").is_some();
        let has_tab = app.get_webview_window("tab").is_some();
        let animate = animate && wm::animates_slide();
        if tucked {
            if has_panel {
                if wm::is_visible(&app, "panel") {
                    if let Some((x, y)) = wm::position(&app, "panel") {
                        *LAST_POS.lock().unwrap() = Some((x, y));
                        if animate && !slide(&app, x, offscreen_x(&app, &side), y, gen) {
                            return;
                        }
                    }
                }
                wm::hide(&app, "panel");
            }
            if has_tab {
                // Back as a sliver, whatever state the last hover left it in.
                TAB_OPEN.store(false, Ordering::SeqCst);
                let _ = app.emit_to("tab", "tab://open", json!({"open": false}));
                place_tab(&app, &side);
                wm::show(&app, "tab");
            }
        } else {
            if has_tab {
                wm::hide(&app, "tab");
            }
            if has_panel {
                let (dx, dy, _, _) = panel_rect(&app, &side);
                let (tx, ty) = LAST_POS.lock().unwrap().unwrap_or((dx, dy));
                if animate && !wm::is_visible(&app, "panel") {
                    let from = offscreen_x(&app, &side);
                    wm::set_position(&app, "panel", from, ty);
                    wm::show(&app, "panel");
                    if !slide(&app, from, tx, ty, gen) {
                        return;
                    }
                }
                wm::set_position(&app, "panel", tx, ty);
                if !wm::is_visible(&app, "panel") {
                    wm::show(&app, "panel");
                }
                if focus {
                    wm::focus(&app, "panel");
                }
            }
        }
        let _ = app.emit_to("panel", "panel://tucked", json!({"tucked": tucked}));
    });
}

/// The edge tab is drawn on the overlay layer, above everything, so it would
/// sit on top of a fullscreen game or video. Hide it while a fullscreen window
/// is in front and bring it back after (only if the panel is still tucked).
#[cfg(target_os = "linux")]
pub fn fullscreen_guard(app: &AppHandle, side: &str, fullscreen: bool) {
    static HIDDEN: AtomicBool = AtomicBool::new(false);
    if fullscreen == HIDDEN.load(Ordering::SeqCst) {
        return;
    }
    HIDDEN.store(fullscreen, Ordering::SeqCst);
    if fullscreen {
        wm::hide(app, "tab");
    } else if is_tucked() {
        place_tab(app, side);
        wm::show(app, "tab");
    }
}

/// Snapshot for the debug probe and tests.
pub fn panel_state(app: &AppHandle) -> serde_json::Value {
    let (x, y) = wm::position(app, "panel").unwrap_or((0.0, 0.0));
    let visible = wm::is_visible(app, "panel");
    let tab_visible = wm::is_visible(app, "tab");
    json!({"tucked": is_tucked(), "visible": visible, "x": x, "y": y, "tab_visible": tab_visible, "tab_open": TAB_OPEN.load(Ordering::SeqCst), "tab": wm::frame(app, "tab")})
}

pub fn dock_panel(app: &AppHandle, side: &str) {
    *LAST_POS.lock().unwrap() = None;
    if app.get_webview_window("panel").is_some() {
        let (x, y, pw, ph) = panel_rect(app, side);
        wm::set_size(app, "panel", pw, ph);
        wm::set_position(app, "panel", x, y);
    }
    if app.get_webview_window("tab").is_some() {
        place_tab(app, side);
    }
}

/// Put the tab window at its box (closed or open, no slide).
fn place_tab(app: &AppHandle, side: &str) {
    TAB_GEN.fetch_add(1, Ordering::SeqCst);
    let (x, y, w, h) = tab_rect(app, side);
    wm::set_size(app, "tab", w, h);
    wm::set_position(app, "tab", x, y);
}

/// Linux: run this webview in the panel's WebKit web process instead of a
/// new one (about 65 MB each). The panel must already exist.
#[cfg(target_os = "linux")]
fn shared_process<'a>(app: &AppHandle, b: WebviewWindowBuilder<'a, tauri::Wry, AppHandle>) -> WebviewWindowBuilder<'a, tauri::Wry, AppHandle> {
    match app.get_webview_window("panel").and_then(|p| crate::linux::layer::webkit_view(&p)) {
        Some(view) => b.with_related_view(view),
        None => b,
    }
}

#[cfg(not(target_os = "linux"))]
fn shared_process<'a>(_app: &AppHandle, b: WebviewWindowBuilder<'a, tauri::Wry, AppHandle>) -> WebviewWindowBuilder<'a, tauri::Wry, AppHandle> {
    b
}

pub fn create_windows(app: &AppHandle, side: &str) -> tauri::Result<()> {
    let (x, y, pw, ph) = panel_rect(app, side);
    WebviewWindowBuilder::new(app, "panel", WebviewUrl::App("index.html".into()))
        .title("Oumu")
        .inner_size(pw, ph)
        .min_inner_size(380.0, 520.0)
        .position(x, y)
        .always_on_top(true)
        .resizable(true)
        .theme(Some(tauri::Theme::Dark))
        .visible(false)
        .build()?;

    let (tx, ty, tw, th) = tab_rect(app, side);
    let t = shared_process(app, WebviewWindowBuilder::new(app, "tab", WebviewUrl::App("index.html".into())))
        .title("")
        .inner_size(tw, th)
        .position(tx, ty)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .focused(false)
        .focusable(false)
        .accept_first_mouse(true)
        .visible_on_all_workspaces(true)
        .visible(false)
        .build()?;
    let _ = t;

    let g = shared_process(app, WebviewWindowBuilder::new(app, "oumubutton", WebviewUrl::App("index.html".into())))
        .title("")
        .inner_size(60.0, 60.0)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .focused(false)
        .focusable(false)
        .accept_first_mouse(true)
        .visible_on_all_workspaces(true)
        .visible(false)
        .build()?;
    let _ = g;

    let u = shared_process(app, WebviewWindowBuilder::new(app, "underlines", WebviewUrl::App("index.html".into())))
        .title("")
        .inner_size(10.0, 10.0)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .focused(false)
        .focusable(false)
        .accept_first_mouse(true)
        .visible_on_all_workspaces(true)
        .visible(false)
        .build()?;
    let _ = u;
    for label in ["panel", "tab", "oumubutton", "underlines"] {
        wm::adopt(app, label);
    }
    // Seed the placement the layer-shell backend mirrors.
    #[cfg(target_os = "linux")]
    {
        wm::set_position(app, "panel", x, y);
        wm::set_position(app, "tab", tx, ty);
    }
    wm::set_passthrough(app, "underlines", true);
    follow_pointer_for_tab(app.clone());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_opens_on_the_sliver_and_closes_off_the_open_tab() {
        let mon = (0.0, 0.0, 1920.0, 1080.0);
        let (_, y, _, _) = tab_box("left", false, mon);
        // Closed: only the sliver opens it.
        assert!(tab_should_open("left", false, mon, 2.0, y + 40.0));
        assert!(!tab_should_open("left", false, mon, 20.0, y + 40.0));
        // Open: stays open anywhere over the full tab, closes off it.
        assert!(tab_should_open("left", true, mon, 40.0, y + 40.0));
        assert!(!tab_should_open("left", true, mon, 90.0, y + 40.0));
        assert!(!tab_should_open("left", true, mon, 10.0, y - 30.0));
        // Right edge mirrors it.
        assert!(tab_should_open("right", false, mon, 1918.0, y + 40.0));
        assert!(!tab_should_open("right", false, mon, 1900.0, y + 40.0));
    }

    #[test]
    fn pointer_near_a_window_includes_the_slack() {
        let panel = (12.0, 40.0, 440.0, 740.0);
        assert!(near(panel, 200.0, 300.0));
        assert!(near(panel, 2.0, 400.0), "the edge tab sits within the slack");
        assert!(!near(panel, 600.0, 500.0));
    }

    #[test]
    fn a_closed_tab_hangs_off_the_edge_but_for_its_strip() {
        let mon = (0.0, 0.0, 1920.0, 1080.0);
        assert_eq!(tab_window("left", false, mon).0, -(TAB_W - TAB_STRIP));
        assert_eq!(tab_window("left", true, mon).0, 0.0);
        assert_eq!(tab_window("right", false, mon).0, 1920.0 - TAB_STRIP);
        assert_eq!(tab_window("right", true, mon).0, 1920.0 - TAB_W);
    }

    #[test]
    fn idle_tab_is_a_sliver_flush_with_its_edge() {
        let mon = (0.0, 0.0, 1920.0, 1080.0);
        assert_eq!(tab_box("left", false, mon).0, 0.0);
        assert_eq!(tab_box("left", false, mon).2, TAB_STRIP);
        assert_eq!(tab_box("right", false, mon).0, 1920.0 - TAB_STRIP);
        assert_eq!(tab_box("right", true, mon).0, 1920.0 - TAB_W);
        assert_eq!(tab_box("right", true, mon).1, tab_box("right", false, mon).1);
    }
}

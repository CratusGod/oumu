//! Oumu's front end on Rune: a windowless app (`Front`) with the docked panel (a window),
//! the edge tab, the floating Oumu button and the suggestion card (overlays that never take
//! focus), plus the click-through underline strips (a layered Win32 window). The original
//! logic drives them through `oumu-rt`, as it drove the React UI through Tauri.

pub mod headless;
pub mod icons;
pub mod layer;
pub mod panel;
#[cfg(windows)]
pub mod strips;
#[cfg(not(windows))]
#[path = "strips_rune.rs"]
pub mod strips;
pub mod style;
pub mod theme;

use crate::host::Shared;
use rune::{OverlayHandle, OverlayOptions, OverlayPlacement, OverlaySpec, Tx, WindowHandle};
use serde::Deserialize;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use style::Tok;
use tauri::{AppHandle, UiEvent};

const TICK: Duration = Duration::from_millis(16);

// ---------------------------------------------------------------- the model (TS types)

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Doc {
    pub id: String,
    pub app_name: String,
    pub bundle_id: String,
    pub text: String,
    pub sel_start: usize,
    pub sel_end: usize,
    pub selected_text: String,
    pub truncated: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Alert {
    pub id: String,
    pub category: String,
    pub kind: String,
    pub title: String,
    pub message: String,
    pub start: usize,
    pub end: usize,
    pub original: String,
    pub suggestions: Vec<String>,
    pub rule: String,
    pub rule_dismissals: u32,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Status {
    pub trusted: bool,
    pub frontmost: String,
    pub frontmost_bid: String,
    pub skipped: bool,
    pub skip_built_in: bool,
    pub has_doc: bool,
    pub linting: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Score {
    pub words: u64,
    pub sentences: u64,
    pub characters: u64,
    pub reading_time_sec: u64,
    pub flesch: f64,
    pub overall: Option<f64>,
    pub correctness_count: u64,
    pub clarity_count: u64,
    pub engagement_count: u64,
    pub delivery_count: u64,
    pub correctness_label: String,
    pub clarity_label: String,
    pub engagement_label: String,
    pub delivery_label: String,
    pub correctness_fill: u8,
    pub clarity_fill: u8,
    pub engagement_fill: u8,
    pub delivery_fill: u8,
    pub note: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct ULRect {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub color: String,
    pub category: String,
    pub sentence: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Geometry {
    pub origin_x: f64,
    pub origin_y: f64,
    pub width: f64,
    pub height: f64,
    pub rects: Vec<ULRect>,
    pub hovered: Option<String>,
    pub over_card: bool,
}

/// Everything the windows show, kept current from the logic's events.
#[derive(Default)]
pub struct Model {
    pub doc: Option<Doc>,
    pub alerts: Vec<Alert>,
    pub status: Status,
    pub score: Option<Score>,
    pub geometry: Geometry,
    pub spot_running: bool,
    pub initials: String,
    pub tab_open: bool,
    pub tucked: bool,
}

impl Model {
    pub fn refresh(&mut self, app: &AppHandle) {
        let v = crate::commands::get_state(app.state::<Shared>());
        self.doc = serde_json::from_value(v["doc"].clone()).ok();
        self.alerts = serde_json::from_value(v["alerts"].clone()).unwrap_or_default();
        self.status = serde_json::from_value(v["status"].clone()).unwrap_or_default();
        self.score = serde_json::from_value(v["score"].clone()).ok();
        self.spot_running = v["spot_running"].as_bool().unwrap_or(false);
        self.initials = v["initials"].as_str().unwrap_or("").to_string();
    }

    fn apply(&mut self, ev: &UiEvent, app: &AppHandle) {
        let p = &ev.payload;
        match ev.name.as_str() {
            "host://document" => {
                self.doc = serde_json::from_value(p.clone()).ok();
                // word count, reading time and readability follow the text, not only the alerts
                let score = {
                    let st = app.state::<Shared>();
                    let st = st.lock().unwrap();
                    st.doc.as_ref().map(|d| crate::score::compute(&d.text, &st.alerts))
                };
                self.score = score.and_then(|s| serde_json::to_value(s).ok()).and_then(|v| serde_json::from_value(v).ok());
            }
            "host://alerts" => {
                self.alerts = serde_json::from_value(p.clone()).unwrap_or_default();
                log::info!("alerts: {} {:?}", self.alerts.len(), self.alerts.iter().map(|a| (a.original.as_str(), a.start, a.end, a.category.as_str())).collect::<Vec<_>>());
                // the score follows the alerts
                let v = crate::commands::get_state(app.state::<Shared>());
                self.score = serde_json::from_value(v["score"].clone()).ok();
                self.spot_running = v["spot_running"].as_bool().unwrap_or(false);
            }
            "host://status" => {
                self.status = serde_json::from_value(p.clone()).unwrap_or_default();
                log::info!("status: front={} doc={} skipped={} linting={}", self.status.frontmost, self.status.has_doc, self.status.skipped, self.status.linting);
            }
            "host://geometry" => {
                let g: Geometry = serde_json::from_value(p.clone()).unwrap_or_default();
                if g.rects.len() != self.geometry.rects.len() {
                    log::info!("geometry: {} rects at ({}, {}) {}x{} state={:?}", g.rects.len(), g.origin_x, g.origin_y, g.width, g.height, app.window_state("underlines"));
                }
                self.geometry = g;
            }
            "host://selection" => {
                if let Some(d) = self.doc.as_mut() {
                    d.sel_start = p["start"].as_u64().unwrap_or(0) as usize;
                    d.sel_end = p["end"].as_u64().unwrap_or(0) as usize;
                    d.selected_text = p["text"].as_str().unwrap_or("").to_string();
                }
            }
            "panel://tucked" => self.tucked = p["tucked"].as_bool().unwrap_or(false),
            "tab://open" => self.tab_open = p["open"].as_bool().unwrap_or(false),
            "settings://changed" => self.refresh(app),
            _ => {}
        }
    }
}

// ---------------------------------------------------------------- the app

/// The windowless app: owns the model and places every surface.
pub struct Front {
    app: AppHandle,
    shared: Shared,
    model: Arc<Mutex<Model>>,
    layer: layer::Layer,
    strips: Option<strips::Strips>,
    strips_tried: bool,
    tok: Tok,
    theme_key: String,
    panel: Option<(rune::AppHandle<panel::PanelS>, WindowHandle)>,
    tab: Option<OverlayHandle<layer::SurfS>>,
    button: Option<OverlayHandle<layer::SurfS>>,
    card: Option<OverlayHandle<layer::SurfS>>,
    /// the WinState seq last applied per surface
    applied: [u64; 3],
    focus_applied: u64,
    /// where the panel was last put or reported (physical frame)
    panel_at: Option<((i32, i32), (u32, u32))>,
    card_shown: Option<(f32, f32, f32, f32)>,
    accept_rect: Option<[f32; 4]>,
    e2e_accept: String,
    surf_key: String,
    tray: Option<rune::tray::Tray>,
    /// Linux: where the panel was last asked to be (layout px), when, and the last check.
    #[cfg(target_os = "linux")]
    hypr: HyprPanel,
}

/// The panel's placement through Hyprland (Wayland windows can't place themselves).
#[cfg(target_os = "linux")]
#[derive(Default)]
struct HyprPanel {
    /// the panel maps transparent (its rule); made opaque once it is where the slide starts
    opaque: bool,
    want: Option<(f64, f64, f64, f64)>,
    since: Option<std::time::Instant>,
    checked: Option<std::time::Instant>,
}

/// How often the panel's real place is compared with the wanted one.
#[cfg(target_os = "linux")]
const HYPR_CHECK: Duration = Duration::from_millis(100);
/// Just after a show or move is asked: check every tick, so a panel that mapped off screen
/// starts its slide in at once.
#[cfg(target_os = "linux")]
const HYPR_CHECK_SETTLING: Duration = Duration::from_millis(16);
/// For this long after a move is asked, a different place means "not there yet"; after it,
/// it means the user moved the panel.
#[cfg(target_os = "linux")]
const HYPR_SETTLE: Duration = Duration::from_millis(1000);
/// Hyprland reports positions inside the border: differences this small are the same place.
#[cfg(target_os = "linux")]
const HYPR_SLACK: f64 = 4.0;

/// Linux: everything is in the compositor's layout px (points), so the scale is 1.
#[cfg(not(windows))]
fn primary_monitor() -> tauri::MonitorInfo {
    let (x, y, w, h) = crate::linux::desktop::primary_monitor().unwrap_or((0.0, 0.0, 1920.0, 1080.0));
    tauri::MonitorInfo { x: x as i32, y: y as i32, w: w.max(1.0) as u32, h: h.max(1.0) as u32, scale: 1.0 }
}

/// Physical px per point on the primary display, and the display's frame.
#[cfg(windows)]
fn primary_monitor() -> tauri::MonitorInfo {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromPoint};
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    unsafe {
        let m = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(m, &mut mi);
        let (mut dx, mut dy) = (96u32, 96u32);
        let scale = if GetDpiForMonitor(m, MDT_EFFECTIVE_DPI, &mut dx, &mut dy).is_ok() { dx as f64 / 96.0 } else { 1.0 };
        let r = mi.rcMonitor;
        tauri::MonitorInfo { x: r.left, y: r.top, w: (r.right - r.left).max(1) as u32, h: (r.bottom - r.top).max(1) as u32, scale }
    }
}

fn scale() -> f64 {
    static K: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *K.get_or_init(|| primary_monitor().scale)
}

/// The theme the settings ask for, and a key that changes when it does.
fn current_theme(shared: &Shared) -> (Tok, String) {
    let (id, pal) = {
        let st = shared.lock().unwrap();
        let pal = crate::theme::palette_path(&st.settings.palette_path).and_then(|p| crate::theme::read_palette(&p));
        (st.settings.theme.clone(), pal)
    };
    let key = format!("{id}{pal:?}");
    (Tok::from(&theme::pick(&id, pal.as_ref())), key)
}

impl Front {
    fn new(app: AppHandle, shared: Shared, model: Arc<Mutex<Model>>) -> Front {
        let (tok, theme_key) = current_theme(&shared);
        Front {
            app,
            shared,
            model,
            layer: layer::Layer::default(),
            strips: None,
            strips_tried: false,
            tok,
            theme_key,
            panel: None,
            tab: None,
            button: None,
            card: None,
            applied: [u64::MAX; 3],
            focus_applied: 0,
            panel_at: None,
            card_shown: None,
            accept_rect: None,
            e2e_accept: String::new(),
            surf_key: String::new(),
            tray: None,
            #[cfg(target_os = "linux")]
            hypr: HyprPanel::default(),
        }
    }

    fn wake_panel(&self) {
        if let Some((p, _)) = &self.panel {
            p.update(|_, _| {});
        }
    }

    /// An event from the logic.
    fn on_event(&mut self, ev: UiEvent, tx: &mut Tx) {
        match ev.name.as_str() {
            "__exit" => {
                tx.quit();
                return;
            }
            "__window" => return,
            "theme://palette" | "settings://changed" => self.retheme(),
            _ => {}
        }
        self.model.lock().unwrap().apply(&ev, &self.app);
        let Some((panel, _)) = &self.panel else { return };
        let p = ev.payload.clone();
        match ev.name.as_str() {
            "panel://view" => {
                if let Some(v) = p["view"].as_str().map(String::from) {
                    panel.update(move |s, _| s.set_view(&v));
                }
            }
            "panel://translate-highlight" => panel.update(|s, _| s.translate_pending()),
            "llm://chunk" => panel.update(move |s, _| {
                let (req, delta) = (p["req_id"].as_str().unwrap_or(""), p["delta"].as_str().unwrap_or(""));
                panel::chat_chunk(s, req, delta, p["done"].as_bool().unwrap_or(false), p["error"].as_str().map(String::from));
            }),
            "host://geometry" => {}
            "host://document" => panel.update(|s, _| s.doc_changed()),
            _ => self.wake_panel(),
        }
        self.push_surfaces();
    }

    fn retheme(&mut self) {
        let (tok, key) = current_theme(&self.shared);
        if key == self.theme_key {
            return;
        }
        self.theme_key = key;
        self.tok = tok.clone();
        if let Some((p, _)) = &self.panel {
            let t = tok.clone();
            p.update(move |s, _| {
                s.kit = style::kit(&t);
                s.tok = t;
            });
        }
        self.surf_key.clear();
    }

    /// What the tab, button and card show.
    fn push_surfaces(&mut self) {
        let m = self.model.lock().unwrap();
        let side = self.shared.lock().unwrap().settings.dock_side.clone();
        let card = self.layer.open.as_ref().and_then(|o| {
            let a = m.alerts.iter().find(|a| &a.id == o)?;
            let cat = m.geometry.rects.iter().find(|r| &r.id == o).map_or(a.category.clone(), |r| r.category.clone());
            Some((a.clone(), cat))
        });
        let (alerts, linting, tab_open, error) = (m.alerts.len(), m.status.linting, m.tab_open, self.layer.error.clone());
        drop(m);
        let key = format!("{alerts}|{linting}|{tab_open}|{side}|{card:?}|{error:?}|{}", self.theme_key);
        if key == self.surf_key {
            return;
        }
        self.surf_key = key;
        for ov in [&self.tab, &self.button, &self.card].into_iter().flatten() {
            let (tok, side, card, error) = (self.tok.clone(), side.clone(), card.clone(), error.clone());
            ov.update(move |s, _| {
                if s.tok.bg != tok.bg || s.tok.brand != tok.brand || s.tok.text != tok.text {
                    s.kit = style::kit(&tok);
                }
                s.tok = tok;
                s.alerts = alerts;
                s.linting = linting;
                s.tab_open = tab_open;
                s.side = side;
                if s.card.as_ref().map(|c| &c.0.id) != card.as_ref().map(|c| &c.0.id) {
                    s.accept = None;
                }
                s.card = card;
                s.error = error;
            });
        }
    }

    /// The card's buttons.
    pub fn card_action(&mut self, id: &str, what: &str) {
        let (app, shared) = (self.app.clone(), self.shared.clone());
        let r = match what {
            "accept" => crate::commands::accept_alert_inner(&app, &shared, id, 0).map(|_| ()),
            "dismiss" => crate::commands::dismiss_alert_inner(&app, &shared, id),
            "learn" => crate::commands::learn_word(app.clone(), app.state::<Shared>(), id.to_string()),
            _ => crate::commands::mute_rule(app.clone(), app.state::<Shared>(), id.to_string()),
        };
        log::info!("card {what}: {}", if r.is_ok() { "ok" } else { "failed" });
        match r {
            Ok(()) => {
                self.layer.open = None;
                self.layer.error = None;
            }
            Err(e) => self.layer.error = Some(e),
        }
        self.push_surfaces();
    }

    /// The card's laid-out size (logical px).
    pub fn card_size(&mut self, size: (f32, f32)) {
        let size = (size.0.ceil(), size.1.ceil());
        if self.layer.size != Some(size) {
            self.layer.size = Some(size);
        }
    }

    pub fn card_accept(&mut self, r: [f32; 4]) {
        self.accept_rect = Some(r);
    }

    // ------------------------------------------------------------ the tick

    fn tick(&mut self, tx: &mut Tx) {
        if self.app.exit_requested().is_some() {
            tx.quit();
            return;
        }
        // OUMU_GPU_REPORT=1: log what the GPU device holds whenever it changes (checked every 2 s)
        {
            static LAST: std::sync::Mutex<(Option<std::time::Instant>, String)> = std::sync::Mutex::new((None, String::new()));
            static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
            if *ON.get_or_init(|| std::env::var_os("OUMU_GPU_REPORT").is_some()) {
                let mut l = LAST.lock().unwrap();
                if l.0.is_none_or(|t| t.elapsed() > Duration::from_secs(2)) {
                    l.0 = Some(std::time::Instant::now());
                    if let Some(r) = rune::gpu_allocation_report() {
                        if r != l.1 {
                            log::info!("gpu: {r}");
                            l.1 = r;
                        }
                    }
                }
            }
        }
        if !self.strips_tried {
            self.strips_tried = true;
            self.strips = strips::Strips::new();
            if self.strips.is_none() {
                log::warn!("no underline window");
            }
        }
        self.sync_windows();
        self.sync_layer();
        self.push_surfaces();
    }

    /// Apply what the logic wants to the panel, tab and button.
    fn sync_windows(&mut self) {
        let k = scale();
        // the panel: a real window; the user may drag it
        if let Some((_, win)) = &self.panel {
            if let Some(st) = self.app.window_state("panel") {
                if st.seq != self.applied[0] {
                    self.applied[0] = st.seq;
                    let pos = ((st.x * k).round() as i32, (st.y * k).round() as i32);
                    let size = ((st.w * k).round().max(1.0) as u32, (st.h * k).round().max(1.0) as u32);
                    win.set_position(pos);
                    win.resize((st.w as f32, st.h as f32));
                    // Linux: the rule must be in place before the window maps
                    #[cfg(target_os = "linux")]
                    if st.visible && !crate::linux::hyprland::panel_rule("Oumu", st.x, st.y, st.w, st.h) {
                        log::debug!("hyprland didn't take the panel rule");
                    }
                    win.set_visible(st.visible);
                    self.panel_at = Some((pos, size));
                }
                if st.focus_seq != self.focus_applied {
                    self.focus_applied = st.focus_seq;
                    win.focus();
                }
                #[cfg(windows)]
                if let Some(p) = win.placement() {
                    let now = (p.position, p.content_size);
                    if st.visible && self.panel_at.is_some_and(|a| a != now) {
                        self.panel_at = Some(now);
                        let k = p.scale;
                        self.app.report_frame("panel", p.position.0 as f64 / k, p.position.1 as f64 / k, p.content_size.0 as f64 / k, p.content_size.1 as f64 / k);
                    }
                }
            }
        }
        #[cfg(target_os = "linux")]
        self.place_panel_hyprland();
        for (i, label, ov) in [(1, "tab", &self.tab), (2, "oumubutton", &self.button)] {
            let (Some(ov), Some(st)) = (ov, self.app.window_state(label)) else { continue };
            if st.seq == self.applied[i] {
                continue;
            }
            self.applied[i] = st.seq;
            if st.visible {
                // the tab slides in from its edge (and back out), like the panel
                #[cfg(target_os = "linux")]
                if label == "tab" && !crate::linux::hyprland::layer_slide_rule("Oumu tab", &self.dock_side()) {
                    log::debug!("hyprland didn't take the tab rule");
                }
                ov.set_placement(Some(OverlayPlacement::Rect { x: st.x as f32, y: st.y as f32, width: st.w.max(1.0) as f32, height: st.h.max(1.0) as f32 }));
                ov.show();
            } else {
                ov.hide();
            }
        }
    }

    /// The screen edge the panel docks on ("left" or "right").
    #[cfg(target_os = "linux")]
    fn dock_side(&self) -> String {
        self.shared.lock().map(|s| s.settings.dock_side.clone()).unwrap_or_else(|_| "left".into())
    }

    /// Linux: float, pin and move the panel through Hyprland until it is where the logic
    /// wants it; once it has settled, a different place is the user's drag, reported back.
    #[cfg(target_os = "linux")]
    fn place_panel_hyprland(&mut self) {
        use crate::linux::hyprland;
        let Some(st) = self.app.window_state("panel") else { return };
        if !st.visible {
            self.hypr.want = None;
            self.hypr.opaque = false;
            return;
        }
        let now = std::time::Instant::now();
        let settling_now = self.hypr.since.is_some_and(|t| now - t < HYPR_SETTLE);
        let every = if settling_now { HYPR_CHECK_SETTLING } else { HYPR_CHECK };
        if self.hypr.checked.is_some_and(|t| now - t < every) {
            return;
        }
        self.hypr.checked = Some(now);
        let want = (st.x, st.y, st.w, st.h);
        if self.hypr.want != Some(want) {
            self.hypr.want = Some(want);
            self.hypr.since = Some(now);
        }
        // not mapped yet (just shown): the next check places it
        let Some(w) = hyprland::own_window("Oumu") else { return };
        let at = (w.at[0], w.at[1], w.size[0], w.size[1]);
        let off = (at.0 - want.0).abs().max((at.1 - want.1).abs()).max((at.2 - want.2).abs()).max((at.3 - want.3).abs());
        if off > HYPR_SLACK || !w.floating || !w.pinned {
            let settling = self.hypr.since.is_some_and(|t| now - t < HYPR_SETTLE);
            if settling || !w.floating || !w.pinned {
                if !hyprland::place_own(&w, want.0, want.1, want.2, want.3) {
                    log::debug!("hyprland didn't take the panel placement");
                }
            } else {
                // the user dragged or resized it: that is the panel's place now
                self.hypr.want = Some(at);
                self.app.report_frame("panel", at.0, at.1, at.2, at.3);
            }
        }
        // Hyprland maps a window on screen whatever the rule asks, so the panel maps
        // transparent and shows only now, at the slide's first step (moved in the same turn)
        if !self.hypr.opaque {
            self.hypr.opaque = hyprland::set_opaque(&w);
        }
    }

    /// The underlines (strips window) and the card follow the logic's "underlines" window
    /// and the hovered word.
    fn sync_layer(&mut self) {
        let Some(st) = self.app.window_state("underlines") else { return };
        let k = scale();
        let changed = {
            let m = self.model.lock().unwrap();
            self.layer.update(&m)
        };
        if changed {
            self.push_surfaces();
        }
        let show = st.visible && self.shared.lock().map(|s| s.settings.show_underlines).unwrap_or(true);
        let m = self.model.lock().unwrap();
        if let Some(s) = self.strips.as_mut() {
            s.place((st.x * k).round() as i32, (st.y * k).round() as i32, (st.w * k).round() as i32, (st.h * k).round() as i32, show);
            if show {
                let px: Vec<(i32, i32, i32, i32, [u8; 4])> = self.layer.bars(&m, &self.tok).iter().map(|b| ((b.0 * k).floor() as i32, (b.1 * k).floor() as i32, (b.2 * k).ceil() as i32, (b.3 * k).ceil().max(1.0) as i32, b.4)).collect();
                s.draw(&px);
            }
        }
        // the card: its own overlay under (or over) the word
        let want = if show { self.layer.placement(&m).map(|(l, t, w, h)| (st.x as f32 + l, st.y as f32 + t, w, h)) } else { None };
        if want != self.card_shown {
            self.card_shown = want;
            if let Some(ov) = &self.card {
                match want {
                    Some((x, y, w, h)) => {
                        ov.set_placement(Some(OverlayPlacement::Rect { x, y, width: w, height: h }));
                        ov.show();
                    }
                    None => ov.hide(),
                }
            }
        }
        self.layer.report(&self.app, &m);
        drop(m);
        if std::env::var_os("OUMU_E2E").is_some() {
            if let (Some((x, y, _, _)), Some(r)) = (self.card_shown, self.accept_rect) {
                let line = format!("{}:{}", ((x + r[0] + r[2] / 2.0) as f64 * k) as i32, ((y + r[1] + r[3] / 2.0) as f64 * k) as i32);
                if line != self.e2e_accept {
                    log::info!("card-accept {line}");
                    self.e2e_accept = line;
                }
            }
        }
    }

    fn tray_event(&mut self, e: rune::tray::TrayEvent, tx: &mut Tx) {
        match e {
            rune::tray::TrayEvent::Activate { .. } => crate::commands::open_panel_inner(&self.app, "review", true),
            rune::tray::TrayEvent::Menu(id) => match id.as_str() {
                "open" => crate::commands::open_panel_inner(&self.app, "review", true),
                "quit" => self.quit(tx),
                "update" => {
                    if crate::update::found().is_some() {
                        crate::update::install();
                    } else {
                        crate::update::check(true);
                        crate::commands::open_panel_inner(&self.app, "settings", true);
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
}

/// The tray's menu: the update item names the version once one is found.
fn tray_menu(update: Option<String>) -> Vec<rune::tray::TrayItem> {
    use rune::tray::TrayItem;
    vec![
        TrayItem::action("open", "Open Oumu"),
        TrayItem::action("update", match update {
            Some(v) => format!("Update to {v}"),
            None => "Check for updates".to_string(),
        }),
        TrayItem::Separator,
        TrayItem::action("quit", "Quit Oumu"),
    ]
}

impl Front {
    pub fn refresh_tray(&self) {
        if let Some(t) = &self.tray {
            t.set_menu(tray_menu(crate::update::found()));
        }
    }

    /// Leave (the tray's Quit, or an update's setup about to replace the exe).
    pub fn quit(&mut self, tx: &mut Tx) {
        self.app.exit(0);
        tx.quit();
    }
}

fn front_view(_: &Front, _: &mut rune::Cx<Front>) -> rune::El {
    rune::col()
}

/// An overlay for the tab, the button or the card.
fn surface(name: &'static str, size: (f32, f32), view: fn(&layer::SurfS, &mut rune::Cx<layer::SurfS>) -> rune::El) -> rune::ConfiguredOverlay<layer::SurfS> {
    let mut o = rune::ConfiguredOverlay::from(OverlaySpec { name, size, edge: rune::ScreenEdge::Top, margin: 0.0, align: rune::Align::Start, inset: 0.0, interactive: true, view });
    o.options = OverlayOptions { prewarm: true, ..Default::default() };
    o
}

/// The panel window: borderless and rounded, hidden until the logic shows it.
pub fn panel_spec(key: &str, state: panel::PanelS, size: (f32, f32), visible: bool) -> rune::WindowSpec<panel::PanelS> {
    let spec = rune::AppSpec { on_key: Some(panel::on_key), min_size: Some((380.0, 520.0)), ..rune::AppSpec::new("Oumu", size, || unreachable!("panel state comes from init"), panel::view) };
    let mut state = Some(state);
    let app = rune::App::new(spec).init(move || state.take().expect("one panel"));
    rune::WindowSpec::from_app(key.to_string(), app).options(rune::WindowOptions { transparent: true, borderless: true, always_on_top: true, visible, min_size: Some((380.0, 520.0)), ..Default::default() })
}

pub fn run(app: AppHandle, shared: Shared, dock_side: String, translate_on_start: bool) {
    app.set_primary_monitor(primary_monitor());
    if let Err(e) = crate::overlay::create_windows(&app, &dock_side) {
        log::error!("couldn't start: {e}");
        return;
    }
    let model = Arc::new(Mutex::new(Model::default()));
    model.lock().unwrap().refresh(&app);
    let mut front = Front::new(app.clone(), shared.clone(), model.clone());

    let mut rapp = rune::App::new(rune::AppSpec::new("Oumu", (10.0, 10.0), || unreachable!("state comes from init"), front_view)).args(Vec::<String>::new()).windowless().keep_running();
    let me = rapp.handle();

    // the overlays
    let size = |l: &str| app.window_state(l).map_or((40.0, 84.0), |s| (s.w.max(1.0) as f32, s.h.max(1.0) as f32));
    let mut surf = layer::SurfS::new(front.tok.clone());
    surf.app = Some(app.clone());
    surf.front = Some(me.clone());
    surf.side = dock_side.clone();
    let mk = |s: &layer::SurfS| layer::SurfS { tok: s.tok.clone(), kit: style::kit(&s.tok), app: s.app.clone(), front: s.front.clone(), side: s.side.clone(), ..layer::SurfS::new(s.tok.clone()) };
    front.tab = Some(rapp.overlay(surface("Oumu tab", size("tab"), layer::tab_view), mk(&surf)));
    front.button = Some(rapp.overlay(surface("Oumu button", size("oumubutton"), layer::button_view), mk(&surf)));
    front.card = Some(rapp.overlay(surface("Oumu suggestion", (layer::CARD_W, 200.0), layer::card_view), surf));
    // Linux: the underlines are an overlay too, made here with the others (click-through)
    #[cfg(not(windows))]
    {
        let mut o = rune::ConfiguredOverlay::from(OverlaySpec { name: "Oumu underlines", size: (10.0, 10.0), edge: rune::ScreenEdge::Top, margin: 0.0, align: rune::Align::Start, inset: 0.0, interactive: false, view: strips::view });
        o.options = OverlayOptions { input: rune::OverlayInput::None, ..Default::default() };
        front.strips = Some(strips::Strips::with_overlay(rapp.overlay(o, strips::StripS::default())));
        front.strips_tried = true;
    }

    // the panel
    let mut ps = panel::PanelS::new(front.tok.clone(), app.clone(), shared.clone(), model.clone());
    let onboarded = shared.lock().unwrap().settings.onboarded;
    if !onboarded {
        ps.view = "onboarding".into();
    }
    let (pw, ph) = app.window_state("panel").map_or((420.0, 720.0), |s| (s.w as f32, s.h as f32));
    let (papp, pwin) = rapp.window(panel_spec("panel", ps, (pw.max(380.0), ph.max(520.0)), false));
    let p2 = papp.clone();
    papp.update(move |s, _| s.me = Some(p2));
    front.panel = Some((papp, pwin));

    // the logic's events
    let me2 = me.clone();
    app.set_sink(move |ev| {
        me2.update(move |f, tx| f.on_event(ev, tx));
    });
    // start tucked behind the edge tab; the first run opens on the welcome
    crate::overlay::set_tucked(&app, &dock_side, onboarded, false, false);
    crate::remote::listen(app.clone());
    if translate_on_start {
        let h = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(800));
            crate::remote::translate_highlight(&h);
        });
    }
    crate::host::spawn(app.clone(), shared.clone());
    crate::debug_server::spawn(app.clone(), shared.clone());
    // Linux: the accessibility bus drops when the process that switched it on exits; once
    // the user agreed (the saved setting), each launch switches it back on
    #[cfg(target_os = "linux")]
    std::thread::spawn(|| {
        if !crate::ax::is_trusted() && crate::ax::consent_saved() {
            crate::ax::prompt_trust();
        }
    });
    crate::theme::watch(app.clone(), shared.clone());

    // the tray
    let me3 = me.clone();
    let tray = rune::tray::Tray::new(
        rune::tray::TraySpec::new("oumu", "Oumu", rune::tray::TrayIcon::rgba(32, 32, icon_rgba(32))).tooltip("Oumu".to_string()).menu(tray_menu(None)),
        move |e| me3.update(move |f, tx| f.tray_event(e, tx)),
    );
    match tray {
        Ok(t) => front.tray = Some(t),
        Err(e) => log::warn!("no tray icon: {e:?}"),
    }

    // placement and the underlines follow the logic every frame
    let me4 = me.clone();
    std::thread::Builder::new()
        .name("oumu-tick".into())
        .spawn(move || loop {
            std::thread::sleep(TICK);
            let at = std::time::Instant::now();
            me4.update(move |f, tx| {
                // a stall queues many ticks: say it once, with the longest wait (the first tick)
                static LOGGED: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
                let late = at.elapsed();
                if late > Duration::from_millis(250) {
                    let mut l = LOGGED.lock().unwrap();
                    if l.is_none_or(|t| t.elapsed() > Duration::from_secs(1)) {
                        *l = Some(std::time::Instant::now());
                        log::info!("ui thread was busy for {} ms", late.as_millis());
                    }
                }
                f.tick(tx)
            });
        })
        .expect("tick thread");

    if let Some((p, _)) = &front.panel {
        crate::update::set_handles(me.clone(), p.clone());
    }
    crate::update::start_schedule(shared.clone());
    log::info!("Oumu (rune) ready");
    let mut front = Some(front);
    rapp.init(move || front.take().expect("one front")).run();
}

/// Oumu's icon as straight RGBA.
pub fn icon_rgba(side: u32) -> Vec<u8> {
    match image::load_from_memory(include_bytes!("../../icons/128x128.png")) {
        Ok(im) => im.resize_exact(side, side, image::imageops::FilterType::Lanczos3).to_rgba8().into_raw(),
        Err(_) => [107u8, 91, 214, 255].repeat((side * side) as usize),
    }
}

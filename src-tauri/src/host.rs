//! The host loop: watches the focused text field of whatever app is in front,
//! keeps the document model, runs the engine, and computes underline geometry.

use crate::ax::{self, AXElement, Rect};
use crate::engine::{Alert, Engine};
use crate::score;
use crate::settings::Settings;
use crate::overlay;
use crate::wm;
use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

pub const OUR_BUNDLE: &str = "dev.cratusgod.oumu";

/// Apps whose text view is not a writing surface. A terminal shows its whole
/// transcript as one AXTextArea, so every line of it would be linted and
/// underlined (and the Oumu button would sit on the shell).
pub const BUILT_IN_SKIP: &[&str] = &[
    "com.apple.Terminal",
    "com.googlecode.iterm2",
    "dev.warp.Warp-Stable",
    "dev.warp.Warp",
    "com.github.wez.wezterm",
    "org.alacritty",
    "io.alacritty",
    "net.kovidgoyal.kitty",
    "com.mitchellh.ghostty",
    "co.zeit.hyper",
    "com.apple.Console",
    // Linux app ids (Wayland app_id / X11 class).
    "kitty",
    "Alacritty",
    "foot",
    "footclient",
    "org.wezfurlong.wezterm",
    "com.mitchellh.ghostty",
    "org.kde.konsole",
    "org.kde.yakuake",
    "org.gnome.Console",
    "org.gnome.Terminal",
    "gnome-terminal-server",
    "org.gnome.Ptyxis",
    "xfce4-terminal",
    "Xfce4-terminal",
    "com.raggesilver.BlackBox",
    "Tilix",
    "com.gexperts.Tilix",
    "terminator",
    "st-256color",
    "XTerm",
    "UXTerm",
    "Rio",
    "dev.warp.Warp",
    // Windows exe names (lower case).
    "windowsterminal.exe",
    "openconsole.exe",
    "conhost.exe",
    "cmd.exe",
    "powershell.exe",
    "pwsh.exe",
    "wezterm-gui.exe",
    "alacritty.exe",
    "mintty.exe",
];

pub fn built_in_skip(bundle_id: &str) -> bool {
    BUILT_IN_SKIP.contains(&bundle_id)
}

/// Stay out of this app: a terminal, or one the user added to the skip list.
pub fn skipped_app(bundle_id: &str, settings: &crate::settings::Settings) -> bool {
    !bundle_id.is_empty() && (built_in_skip(bundle_id) || settings.skip_apps.iter().any(|a| a.bundle_id == bundle_id))
}
const LINT_DEBOUNCE: Duration = Duration::from_millis(350);
/// Typing pause before the model judges the changed sentences.
const SPOT_DEBOUNCE: Duration = Duration::from_millis(3500);
/// Longer documents are spot-checked on demand only (the Spot check button).
const SPOT_MAX_WORDS: usize = 6000;
const MAX_CHARS: usize = 80_000;
const BUTTON_SIZE: f64 = 48.0;
const BUTTON_INSET: f64 = 14.0;

#[derive(Clone, Debug, Serialize)]
pub struct HostDoc {
    pub id: String,
    pub app_name: String,
    pub bundle_id: String,
    pub pid: i32,
    pub role: String,
    pub text: String,
    pub sel_start: usize,
    pub sel_end: usize,
    pub selected_text: String,
    pub frame: Rect,
    pub visible: Rect,
    pub version: u64,
    pub active: bool,
    pub truncated: bool,
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct Status {
    pub trusted: bool,
    pub frontmost: String,
    pub frontmost_bid: String,
    /// The app in front is one Oumu stays out of.
    pub skipped: bool,
    /// ... because it is a terminal (built in), not a user choice.
    pub skip_built_in: bool,
    pub has_doc: bool,
    pub linting: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct UnderlineRect {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub color: String,
    pub category: String,
    /// A whole-sentence (spot check) underline, drawn beneath word-level ones.
    #[serde(default)]
    pub sentence: bool,
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct Geometry {
    pub origin_x: f64,
    pub origin_y: f64,
    pub width: f64,
    pub height: f64,
    pub rects: Vec<UnderlineRect>,
    pub hovered: Option<String>,
    /// The pointer is over the open hover card (hit-tested here, not in the webview).
    #[serde(default)]
    pub over_card: bool,
}

#[derive(Default)]
pub struct HostState {
    pub doc: Option<HostDoc>,
    pub element: Option<AXElement>,
    pub alerts: Vec<Alert>,
    pub all_alerts: Vec<Alert>,
    /// Sentence-level suggestions from the spot check (LLM), kept across Harper re-lints.
    pub spot_alerts: Vec<Alert>,
    pub spot_cache: crate::features::spotcheck::Cache,
    pub spot_running: bool,
    pub spot_doc_id: String,
    pub spot_dirty: bool,
    pub spot_pending_since: Option<Instant>,
    /// Automatic spot checks wait until then (the key ran out of quota).
    pub spot_paused_until: Option<Instant>,
    /// Text hash the last automatic spot check was started for.
    pub spot_last_hash: u64,
    pub status: Status,
    pub geometry: Geometry,
    pub settings: Settings,
    pub history: Vec<crate::history::Session>,
    /// Extra hit rectangles (the hover card) reported by the overlay UI, in overlay-local coords.
    pub card_rects: Vec<Rect>,
    pub last_text_hash: u64,
    pub lint_pending_since: Option<Instant>,
    pub lint_dirty: bool,
    pub lost_focus_at: Option<Instant>,
    pub paused: bool,
    /// Window labels whose webview reported it mounted (test introspection).
    pub ui_ready: Vec<String>,
    pub ui_log: Vec<String>,
}

pub type Shared = Arc<Mutex<HostState>>;

impl HostState {
    /// The list the panel shows: Harper + rule alerts, then spot-check
    /// sentences, minus everything dismissed. Word-level alerts come first so
    /// a hover over a word inside a flagged sentence opens the word's card.
    pub fn visible_alerts(&self) -> Vec<Alert> {
        let chat = self.doc.as_ref().is_some_and(|d| crate::casual::is_chat_app(&d.bundle_id));
        let mut v = crate::features::spotcheck::merge_visible(&self.all_alerts, &self.spot_alerts, |a| self.settings.hides(a) || (chat && crate::casual::formal_only(&a.rule)));
        for a in &mut v {
            a.rule_dismissals = self.settings.rule_dismissals.get(&a.rule).copied().unwrap_or(0);
        }
        v
    }
}

fn hash_text(s: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

fn is_editable(el: &AXElement, role: &str) -> bool {
    if role == "AXSecureTextField" {
        return false;
    }
    if matches!(role, "AXTextArea" | "AXTextField" | "AXComboBox" | "AXSearchField") {
        return true;
    }
    if role == "AXWebArea" || role == "AXWindow" || role == "AXApplication" {
        return false;
    }
    el.has_attr("AXSelectedTextRange") && el.is_settable("AXValue")
}

/// The open panel's box. On Linux the underline layer stacks above the panel
/// (both are layer surfaces), so words under the panel get no underline.
#[cfg(target_os = "linux")]
fn covered_by_panel(app: &AppHandle) -> Option<Rect> {
    if !wm::is_visible(app, "panel") {
        return None;
    }
    let (x, y, w, h) = wm::frame(app, "panel")?;
    Some(Rect { x, y, w, h })
}

#[cfg(not(target_os = "linux"))]
fn covered_by_panel(_app: &AppHandle) -> Option<Rect> {
    None
}

/// A field with no scroll pane found is looked again after this long (the
/// walk can fail on a slow app, and then underlines would not be clipped).
const SCROLL_AREA_RETRY: Duration = Duration::from_secs(2);

thread_local! {
    /// The scroll pane around the focused field, found once per field (a walk
    /// up its ancestors) instead of every tick.
    static SCROLL_AREA: std::cell::RefCell<Option<(u64, Option<AXElement>, Instant)>> = const { std::cell::RefCell::new(None) };
}

fn scroll_area(el: &AXElement) -> Option<AXElement> {
    let key = el.identity_hash();
    SCROLL_AREA.with(|c| {
        if let Some((k, sa, at)) = c.borrow().as_ref() {
            if *k == key && (sa.is_some() || at.elapsed() < SCROLL_AREA_RETRY) {
                return sa.clone();
            }
        }
        let sa = el.enclosing_scroll_area();
        *c.borrow_mut() = Some((key, sa.clone(), Instant::now()));
        sa
    })
}

fn visible_frame(el: &AXElement, frame: Rect) -> Rect {
    let mut vis = frame;
    if let Some(sa) = scroll_area(el) {
        if let Some(f) = sa.frame() {
            if let Some(i) = vis.intersect(&f) {
                vis = i;
            }
        }
    }
    if let Some(w) = el.window() {
        if let Some(f) = w.frame() {
            if let Some(i) = vis.intersect(&f) {
                vis = i;
            }
        }
    }
    vis
}

/// Host loop ticks so far (the debug probe reports them).
pub static TICKS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn spawn(app: AppHandle, shared: Shared) {
    #[cfg(target_os = "linux")]
    if crate::linux::desktop::kind() == crate::linux::desktop::Kind::Hyprland {
        crate::linux::hyprland::watch_front(crate::pace::wake);
    }
    std::thread::Builder::new()
        .name("host-loop".into())
        .spawn(move || {
            let mut engine = Engine::new();
            let system = AXElement::system_wide();
            let mut last_geom_sig = String::new();
            let mut last_status_sig = String::new();
            let mut rects = RectCache::default();
            let mut was_watching = false;
            loop {
                TICKS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let t0 = Instant::now();
                tick(&app, &shared, &system, &mut engine, &mut last_geom_sig, &mut last_status_sig, &mut rects);
                let now_watching = watching(&shared);
                crate::pace::settle_memory(was_watching, now_watching);
                was_watching = now_watching;
                let tick = crate::pace::tick_for(now_watching);
                crate::pace::sleep(tick.saturating_sub(t0.elapsed()));
            }
        })
        .expect("host loop thread");
}

/// Whether the loop has something to watch closely: a document in front, a
/// pending check, or the panel out. Elsewhere than Linux nothing wakes the
/// loop early, so it always ticks quickly there.
fn watching(shared: &Shared) -> bool {
    if cfg!(not(target_os = "linux")) || !overlay::is_tucked() {
        return true;
    }
    shared
        .lock()
        .map(|st| st.doc.as_ref().is_some_and(|d| d.active) || st.lint_dirty || st.spot_dirty || st.spot_running)
        .unwrap_or(true)
}

/// Underline boxes from the last full placement. Placing them asks the app
/// for every line of every alert, so it is redone only when something that
/// moves them changed: the text, the alerts, the field's box, the panel over
/// it, the text on screen, or the position of the first and last alert on
/// screen (scrolling by less than a line).
#[derive(Default)]
struct RectCache {
    key: u64,
    at: Option<Instant>,
    rects: Vec<UnderlineRect>,
}

/// A full placement at least this often, in case text moved without the checks seeing it.
const RECTS_MAX_AGE: Duration = Duration::from_secs(4);
/// Up to this many alerts are placed without first asking which text is on screen.
const FEW_ALERTS: usize = 12;

/// Alerts at least partly inside the UTF-16 range `shown` (all of them when the app cannot say).
fn on_screen(alerts: &[Alert], shown: Option<(usize, usize)>) -> Vec<Alert> {
    match shown {
        Some((s, e)) => alerts.iter().filter(|a| a.u16_end >= s && a.u16_start <= e).cloned().collect(),
        None => alerts.to_vec(),
    }
}

fn rect_key(doc: &HostDoc, el: &AXElement, alerts: &[Alert], shown: Option<(usize, usize)>, hidden: Option<&Rect>) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let bits = |r: &Rect, h: &mut std::collections::hash_map::DefaultHasher| [r.x, r.y, r.w, r.h].iter().for_each(|v| v.to_bits().hash(h));
    doc.id.hash(&mut h);
    doc.version.hash(&mut h);
    shown.hash(&mut h);
    bits(&doc.visible, &mut h);
    bits(&doc.frame, &mut h);
    if let Some(p) = hidden {
        bits(p, &mut h);
    }
    for a in alerts.iter().take(80) {
        (&a.id, a.u16_start, a.u16_end).hash(&mut h);
    }
    // Scrolling moves the text but not the field: the first and last on-screen alert's boxes show it.
    // A box the app does not answer (some browsers, for text scrolled away) counts as a value too.
    for a in [alerts.first(), alerts.last()].into_iter().flatten() {
        match el.bounds_for_range(a.u16_start, 1) {
            Some(r) => bits(&r, &mut h),
            None => u64::MAX.hash(&mut h),
        }
    }
    h.finish()
}

fn place_underlines(el: &AXElement, alerts: &[Alert], vis: Rect, hidden: Option<&Rect>) -> Vec<UnderlineRect> {
    let mut out = Vec::new();
    for a in alerts.iter().take(80) {
        for r in rects_for_range(el, a.u16_start, a.u16_end.saturating_sub(a.u16_start).max(1)) {
            if hidden.is_some_and(|p| p.intersect(&r).is_some()) {
                continue;
            }
            if let Some(c) = r.intersect(&vis) {
                out.push(UnderlineRect {
                    id: a.id.clone(),
                    x: c.x - vis.x,
                    y: c.y - vis.y,
                    w: c.w,
                    h: c.h,
                    color: a.category.color().into(),
                    category: format!("{:?}", a.category).to_lowercase(),
                    sentence: a.kind.starts_with("Spot"),
                });
            }
        }
    }
    out
}

impl RectCache {
    fn get(&mut self, doc: &HostDoc, el: &AXElement, alerts: &[Alert], hidden: Option<&Rect>) -> Vec<UnderlineRect> {
        // A few alerts are cheap to place; past that only the ones on screen are.
        let shown = if alerts.len() > FEW_ALERTS { el.visible_text_range(&doc.visible) } else { None };
        let alerts = on_screen(alerts, shown);
        let key = rect_key(doc, el, &alerts, shown, hidden);
        let fresh = self.at.is_some_and(|t| t.elapsed() < RECTS_MAX_AGE);
        if key != self.key || !fresh {
            self.rects = place_underlines(el, &alerts, doc.visible, hidden);
            self.key = key;
            self.at = Some(Instant::now());
        }
        self.rects.clone()
    }
}

#[allow(clippy::too_many_arguments)]
fn tick(app: &AppHandle, shared: &Shared, system: &AXElement, engine: &mut Engine, last_geom_sig: &mut String, last_status_sig: &mut String, rects: &mut RectCache) {
    let trusted = ax::is_trusted();
    let front = ax::frontmost_app();
    let (front_pid, front_bid, front_name) = front.clone().unwrap_or((0, String::new(), String::new()));
    let ours = front_bid == OUR_BUNDLE;
    // Layer surfaces never become Hyprland's active window: Oumu is "in front"
    // when the keyboard focus is inside its own panel.
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    let ours = ours || front_pid == std::process::id() as i32 || ax::focus_is_ours();

    // An open panel keeps the keyboard (for Esc) only while the pointer is on
    // it. (The edge tab follows the pointer on its own thread, see overlay.rs.)
    if !overlay::is_tucked() {
        if let Some((mx, my)) = ax::mouse_location_top_left() {
            if wm::frame(app, "panel").is_some_and(|f| !overlay::near(f, mx, my)) {
                wm::release_keyboard(app, "panel");
            }
        }
    }

    let (paused, skipped, built_in) = shared
        .lock()
        .map(|s| (s.paused, skipped_app(&front_bid, &s.settings), built_in_skip(&front_bid)))
        .unwrap_or((false, false, false));

    // 1. Resolve the focused editable element (unless we are in front: keep the last one).
    let mut focused: Option<(AXElement, String)> = None;
    if trusted && !ours && !paused && !skipped {
        if let Some(el) = system.focused_element() {
            let role = el.role().unwrap_or_default();
            if el.pid() != std::process::id() as i32 && is_editable(&el, &role) {
                focused = Some((el, role));
            }
        }
    }

    let mut doc_event: Option<HostDoc> = None;
    let mut alerts_event: Option<Vec<Alert>> = None;
    let mut sel_event: Option<(usize, usize, String)> = None;
    let mut clear_overlays = false;
    let mut doc_cleared = false;
    let mut spot_job: Option<crate::settings::LlmConfig> = None;

    {
        let mut st = shared.lock().unwrap();
        st.status.trusted = trusted;
        st.status.frontmost = front_name.clone();
        st.status.frontmost_bid = front_bid.clone();
        st.status.skipped = skipped;
        st.status.skip_built_in = built_in;
        // The app the document lives in was just added to the skip list: let it go.
        if st.doc.as_ref().map(|d| skipped_app(&d.bundle_id, &st.settings)).unwrap_or(false) {
            st.doc = None;
            st.element = None;
            st.alerts.clear();
            st.all_alerts.clear();
            st.spot_alerts.clear();
            st.spot_doc_id.clear();
            st.status.has_doc = false;
            clear_overlays = true;
            doc_cleared = true;
            alerts_event = Some(vec![]);
        }

        if let Some((el, role)) = focused {
            // Switched element?
            let same = st.element.as_ref().map(|e| e.same_as(&el)).unwrap_or(false);
            if !same {
                st.element = Some(el.clone());
                st.doc = None;
                st.alerts.clear();
                st.all_alerts.clear();
                st.spot_alerts.clear();
                st.spot_doc_id.clear();
                st.spot_dirty = false;
                st.spot_last_hash = 0;
                st.last_text_hash = 0;
                st.lint_dirty = false;
                alerts_event = Some(vec![]);
            }
            st.lost_focus_at = None;
            let text = el.string("AXValue").unwrap_or_default();
            let truncated = text.chars().count() > MAX_CHARS;
            let text: String = if truncated { text.chars().take(MAX_CHARS).collect() } else { text };
            let (sel_start, sel_len) = el.range("AXSelectedTextRange").unwrap_or((0, 0));
            let frame = el.frame().unwrap_or_default();
            let visible = visible_frame(&el, frame);
            let h = hash_text(&text);
            let changed = h != st.last_text_hash;
            let selected_text = crate::textmap::slice_utf16(&text, sel_start, sel_start + sel_len);
            let doc_id = format!("{}-{}", el.pid(), el.identity_hash());
            let version = st.doc.as_ref().map(|d| d.version).unwrap_or(0) + if changed { 1 } else { 0 };
            let prev_sel = st.doc.as_ref().map(|d| (d.sel_start, d.sel_end));
            let doc = HostDoc {
                id: doc_id,
                app_name: front_name.clone(),
                bundle_id: front_bid.clone(),
                pid: front_pid,
                role,
                text: text.clone(),
                sel_start,
                sel_end: sel_start + sel_len,
                selected_text: selected_text.clone(),
                frame,
                visible,
                version,
                active: true,
                truncated,
            };
            if changed {
                st.last_text_hash = h;
                st.lint_pending_since = Some(Instant::now());
                st.lint_dirty = true;
                st.spot_dirty = true;
                st.spot_pending_since = Some(Instant::now());
                // Keep existing alerts only if they still match their original text (cheap
                // re-validation); spot-check sentences follow the text if they moved.
                let text_ref = &text;
                st.all_alerts.retain(|a| crate::textmap::slice_utf16(text_ref, a.u16_start, a.u16_end) == a.original);
                crate::features::spotcheck::re_anchor(&mut st.spot_alerts, text_ref);
                st.alerts = st.visible_alerts();
                doc_event = Some(doc.clone());
            } else if st.doc.is_none() {
                doc_event = Some(doc.clone());
            }
            if prev_sel != Some((sel_start, sel_start + sel_len)) {
                sel_event = Some((sel_start, sel_start + sel_len, selected_text));
            }
            st.doc = Some(doc);
            st.status.has_doc = true;
        } else if st.doc.is_some() {
            // No editable element in front. Keep the last document for the panel.
            if !ours {
                if st.lost_focus_at.is_none() {
                    st.lost_focus_at = Some(Instant::now());
                }
                let expired = st.lost_focus_at.map(|t| t.elapsed() > Duration::from_millis(900)).unwrap_or(false);
                if let Some(d) = st.doc.as_mut() {
                    if expired && d.active {
                        d.active = false;
                        clear_overlays = true;
                        doc_event = Some(d.clone());
                    }
                }
            }
        }

        // 2. Debounced lint.
        if st.lint_dirty && st.lint_pending_since.map(|t| t.elapsed() >= LINT_DEBOUNCE).unwrap_or(false) {
            if let Some(d) = st.doc.clone() {
                st.status.linting = true;
                let all = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| engine.check(&d.text))) {
                    Ok(v) => v,
                    Err(_) => {
                        log::error!("engine panicked on a {}-char document; keeping previous alerts", d.text.len());
                        st.all_alerts.clone()
                    }
                };
                st.all_alerts = all;
                st.alerts = st.visible_alerts();
                st.lint_dirty = false;
                st.status.linting = false;
                alerts_event = Some(st.alerts.clone());
                // history
                let sc = score::compute(&d.text, &st.alerts);
                let session = crate::history::Session {
                    doc_id: d.id.clone(),
                    app_name: d.app_name.clone(),
                    bundle_id: d.bundle_id.clone(),
                    title: crate::history::title_from_text(&d.text),
                    words: sc.words,
                    score: sc.overall,
                    last_seen: chrono::Utc::now().timestamp(),
                };
                if sc.words >= 3 {
                    crate::history::upsert(&mut st.history, session);
                    crate::history::save(&st.history);
                }
            }
        }
    }

    // 2b. Debounced spot check: once typing pauses, the model judges the
    // sentences that changed (the cache keeps the rest). Runs for any panel
    // view and for a tucked panel; the result arrives as host://alerts.
    {
        let mut st = shared.lock().unwrap();
        let paused = st.spot_paused_until.is_some_and(|t| Instant::now() < t);
        if st.settings.auto_spot_check && !paused && st.spot_dirty && !st.lint_dirty && !st.spot_running && st.spot_pending_since.map(|t| t.elapsed() >= SPOT_DEBOUNCE).unwrap_or(false) {
            if let Some(d) = st.doc.as_ref().filter(|d| d.active) {
                let words = score::word_count(&d.text);
                let hash = st.last_text_hash;
                st.spot_dirty = false;
                if (8..=SPOT_MAX_WORDS).contains(&words) && hash != st.spot_last_hash {
                    if let Ok(cfg) = crate::commands::effective_llm(&st.settings) {
                        st.spot_last_hash = hash;
                        spot_job = Some(cfg);
                    }
                }
            }
        }
    }
    if let Some(cfg) = spot_job {
        let (a, s) = (app.clone(), shared.clone());
        tauri::async_runtime::spawn(async move { crate::commands::spot_check_auto(a, s, cfg).await });
    }

    // 3. Emit document/alert/selection events.
    if let Some(d) = &doc_event {
        let _ = app.emit("host://document", d);
    }
    if doc_cleared {
        let _ = app.emit("host://document", Option::<HostDoc>::None);
    }
    if let Some(a) = &alerts_event {
        let _ = app.emit("host://alerts", a);
    }
    if let Some((s, e, t)) = &sel_event {
        let _ = app.emit("host://selection", serde_json::json!({"start": s, "end": e, "text": t}));
    }

    // 4. Geometry + overlays.
    let (doc, element, alerts, show_button, show_underlines, card_rects) = {
        let st = shared.lock().unwrap();
        (st.doc.clone(), st.element.clone(), st.alerts.clone(), st.settings.show_button, st.settings.show_underlines, st.card_rects.clone())
    };
    let active_doc = doc.as_ref().filter(|d| d.active && !clear_overlays);
    match (active_doc, element) {
        (Some(d), Some(el)) => {
            let vis = d.visible;
            let big_enough = vis.w >= 60.0 && vis.h >= 28.0;
            let mut geometry = Geometry { origin_x: vis.x, origin_y: vis.y, width: vis.w, height: vis.h, rects: vec![], hovered: None, over_card: false };
            let hidden = covered_by_panel(app);
            if big_enough && show_underlines {
                geometry.rects = rects.get(d, &el, &alerts, hidden.as_ref());
            }
            // hover detection
            let mut over_card = false;
            let mut over_card_body = false;
            if let Some((mx, my)) = ax::mouse_location_top_left() {
                let lx = mx - vis.x;
                let ly = my - vis.y;
                for r in &geometry.rects {
                    if lx >= r.x - 2.0 && lx <= r.x + r.w + 2.0 && ly >= r.y - 2.0 && ly <= r.y + r.h + 4.0 {
                        geometry.hovered = Some(r.id.clone());
                        break;
                    }
                }
                over_card = card_rects.iter().any(|c| c.contains(lx, ly));
                // The first rect is the card itself (the rest bridge it to its word).
                over_card_body = card_rects.first().is_some_and(|c| c.contains(lx, ly));
            }
            geometry.over_card = over_card;
            let sig = serde_json::to_string(&geometry).unwrap_or_default();
            if sig != *last_geom_sig {
                *last_geom_sig = sig;
                shared.lock().unwrap().geometry = geometry.clone();
                let _ = app.emit_to("underlines", "host://geometry", &geometry);
            }
            if app.get_webview_window("underlines").is_some() {
                if big_enough && show_underlines {
                    wm::set_position(app, "underlines", vis.x, vis.y);
                    wm::set_size(app, "underlines", vis.w.max(1.0), vis.h.max(1.0));
                    // Take the pointer only over the card's buttons. Over the word itself
                    // clicks and the scroll wheel still belong to the app; the card stays
                    // open regardless, since this loop hit-tests the pointer itself.
                    wm::set_passthrough(app, "underlines", !over_card_body);
                    if !wm::is_visible(app, "underlines") {
                        wm::show(app, "underlines");
                    }
                } else if wm::is_visible(app, "underlines") {
                    wm::hide(app, "underlines");
                }
            }
            if app.get_webview_window("oumubutton").is_some() {
                if big_enough && show_button && vis.h >= BUTTON_SIZE + 2.0 * BUTTON_INSET {
                    let x = vis.x + vis.w - BUTTON_SIZE - BUTTON_INSET;
                    let y = vis.y + vis.h - BUTTON_SIZE - BUTTON_INSET;
                    wm::set_position(app, "oumubutton", x, y);
                    if !wm::is_visible(app, "oumubutton") {
                        wm::show(app, "oumubutton");
                    }
                } else if wm::is_visible(app, "oumubutton") {
                    wm::hide(app, "oumubutton");
                }
            }
        }
        _ => {
            for label in ["underlines", "oumubutton"] {
                if wm::is_visible(app, label) {
                    wm::hide(app, label);
                }
            }
            if !last_geom_sig.is_empty() {
                last_geom_sig.clear();
                let mut st = shared.lock().unwrap();
                st.geometry = Geometry::default();
                // No card can be open without a field; a stale card rect would
                // make the next field's underline window catch clicks there.
                st.card_rects.clear();
                drop(st);
                let _ = app.emit_to("underlines", "host://geometry", Geometry::default());
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        let fullscreen = crate::linux::desktop::front().map(|w| w.fullscreen && w.pid != std::process::id() as i32).unwrap_or(false);
        let side = shared.lock().unwrap().settings.dock_side.clone();
        crate::overlay::fullscreen_guard(app, &side, fullscreen);
    }

    // 5. Status (only when changed).
    let status = shared.lock().unwrap().status.clone();
    let ssig = format!("{}|{}|{}|{}|{}", status.trusted, status.frontmost, status.has_doc, status.skipped, status.frontmost_bid);
    if ssig != *last_status_sig {
        *last_status_sig = ssig;
        let _ = app.emit("host://status", &status);
    }
}

/// Screen rects (one per visual line) for a UTF-16 range.
#[cfg(any(target_os = "linux", target_os = "windows"))]
fn rects_for_range(el: &AXElement, loc: usize, len: usize) -> Vec<Rect> {
    if len == 0 {
        return Vec::new();
    }
    el.rects_for_range(loc, len)
}

/// Screen rects (one per visual line) for a UTF-16 range.
#[cfg(target_os = "macos")]
fn rects_for_range(el: &AXElement, loc: usize, len: usize) -> Vec<Rect> {
    let mut out = Vec::new();
    if len == 0 {
        return out;
    }
    let first = el.line_for_index(loc);
    let last = el.line_for_index(loc + len - 1);
    if let (Some(l0), Some(l1)) = (first, last) {
        if l1 > l0 && l1 - l0 <= 40 {
            for line in l0..=l1 {
                if let Some((ls, ll)) = el.range_for_line(line) {
                    let s = loc.max(ls);
                    let e = (loc + len).min(ls + ll);
                    if e > s {
                        if let Some(r) = el.bounds_for_range(s, e - s) {
                            if !r.is_empty() {
                                out.push(r);
                            }
                        }
                    }
                }
            }
            if !out.is_empty() {
                return out;
            }
        }
    }
    if let Some(r) = el.bounds_for_range(loc, len) {
        if !r.is_empty() {
            out.push(r);
            return out;
        }
    }
    // Some text views answer an empty rect for a range that wraps lines (a
    // whole sentence): walk the range in short pieces and keep each piece's box.
    if len > 24 {
        let mut s = loc;
        while s < loc + len {
            let e = (s + 24).min(loc + len);
            if let Some(r) = el.bounds_for_range(s, e - s) {
                if !r.is_empty() {
                    match out.last_mut() {
                        Some(last) if (last.y - r.y).abs() < 2.0 && r.x >= last.x => {
                            let right = (r.x + r.w).max(last.x + last.w);
                            last.w = right - last.x;
                            last.h = last.h.max(r.h);
                        }
                        _ => out.push(r),
                    }
                }
            }
            s = e;
        }
        if out.is_empty() {
            log::debug!("no AX bounds for range {loc}+{len}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminals_are_skipped_and_the_user_list_adds_apps() {
        let mut s = crate::settings::Settings::default();
        assert!(skipped_app("com.apple.Terminal", &s));
        assert!(skipped_app("com.googlecode.iterm2", &s));
        assert!(built_in_skip("com.mitchellh.ghostty"));
        assert!(!skipped_app("com.apple.Notes", &s));
        assert!(!skipped_app("", &s), "no app in front is not a skip");
        s.skip_apps.push(crate::settings::SkipApp { bundle_id: "com.apple.Notes".into(), name: "Notes".into() });
        assert!(skipped_app("com.apple.Notes", &s));
        assert!(!built_in_skip("com.apple.Notes"), "a user choice is not built in");
    }
}

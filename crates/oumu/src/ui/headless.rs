//! Headless runs (Rune's tooling: `--snap`, `--script`, `--tree`, `--list-windows`): every
//! panel view and overlay as a window with demo state. No window opens and no input is
//! touched; settings live in a temp folder, so nothing touches the real Oumu.
//!
//!   oumu --list-windows
//!   oumu --window review --snap review.png --scale 2
//!   oumu --window card --script s.json --tree tree.json

use super::layer::{self, SurfS};
use super::panel::{self, PanelS};
use super::style::Tok;
use super::{Alert, Doc, Model, Score, Status};
use crate::host::{HostState, Shared};
use std::sync::{Arc, Mutex};

const FLAGS: &[&str] = &["--snap", "--script", "--tree", "--list-windows", "--list-scenes", "--video", "--out", "--strip", "--first-pixels"];

pub fn wanted() -> bool {
    std::env::args().any(|a| FLAGS.contains(&a.as_str()))
}

const TEXT: &str = "Their going to the libary tomorow to finish there report. In my opinion, I think that the the results was very very good, and it is important to note that we should delve into the data more deeply.";

fn alert(id: &str, cat: &str, kind: &str, title: &str, msg: &str, original: &str, sugg: &[&str]) -> Alert {
    let start = TEXT.find(original).unwrap_or(0);
    Alert {
        id: id.into(),
        category: cat.into(),
        kind: kind.into(),
        title: title.into(),
        message: msg.into(),
        start,
        end: start + original.len(),
        original: original.into(),
        suggestions: sugg.iter().map(|s| s.to_string()).collect(),
        rule: if kind == "Spelling" { String::new() } else { format!("harper::{}", title.replace(' ', "")) },
        rule_dismissals: if id == "a3" { 2 } else { 0 },
    }
}

pub fn demo_model() -> Model {
    let alerts = vec![
        alert("a1", "correctness", "Spelling", "Spelling", "“libary” isn't in the dictionary.", "libary", &["library", "liberty"]),
        alert("a2", "correctness", "Spelling", "Spelling", "“tomorow” isn't in the dictionary.", "tomorow", &["tomorrow"]),
        alert("a3", "correctness", "Grammar", "Repeated word", "The word “the” appears twice in a row.", "the the", &["the"]),
        alert("a4", "clarity", "Style", "Wordy phrase", "“In my opinion, I think” says the same thing twice.", "In my opinion, I think that", &["I think"]),
        alert("a5", "engagement", "Style", "Intensifier", "“very very” weakens the sentence. Try a stronger word.", "very very good", &["excellent", "strong"]),
        alert("a6", "delivery", "Style", "AI phrase", "“delve into” is common in AI text.", "delve into", &["dig into", "look at"]),
        alert("a7", "clarity", "Style", "Filler", "This phrase adds nothing.", "it is important to note that ", &[""]),
    ];
    Model {
        doc: Some(Doc { id: "d1".into(), app_name: "Notepad".into(), bundle_id: "notepad.exe".into(), text: TEXT.into(), ..Default::default() }),
        score: Some(Score {
            words: 37,
            sentences: 2,
            characters: TEXT.len() as u64,
            reading_time_sec: 10,
            flesch: 61.0,
            overall: Some(72.0),
            correctness_count: 3,
            clarity_count: 2,
            engagement_count: 1,
            delivery_count: 1,
            correctness_label: "3 issues".into(),
            clarity_label: "2 issues".into(),
            engagement_label: "1 issue".into(),
            delivery_label: "1 issue".into(),
            correctness_fill: 40,
            clarity_fill: 65,
            engagement_fill: 80,
            delivery_fill: 85,
            note: None,
        }),
        status: Status { trusted: true, frontmost: "Notepad".into(), frontmost_bid: "notepad.exe".into(), has_doc: true, ..Default::default() },
        initials: "CG".into(),
        alerts,
        ..Default::default()
    }
}

fn tok(shared: &Shared) -> Tok {
    let id = std::env::var("OUMU_THEME").unwrap_or_else(|_| shared.lock().unwrap().settings.theme.clone());
    Tok::from(&super::theme::pick(&id, None))
}

fn clear() -> rune::WindowOptions {
    rune::WindowOptions { transparent: true, borderless: true, min_size: Some((0.0, 0.0)), ..Default::default() }
}

fn surf_window(key: &'static str, size: (f32, f32), s: SurfS, view: fn(&SurfS, &mut rune::Cx<SurfS>) -> rune::El) -> rune::AnyWindowSpec {
    let mut s = Some(s);
    let app = rune::App::new(rune::AppSpec::new(key, size, || unreachable!(), view)).init(move || s.take().unwrap());
    rune::WindowSpec::from_app(key, app).options(clear()).boxed()
}

/// A panel window on one view, with demo state.
fn panel_window(key: &'static str, view: &str, app: &tauri::AppHandle, shared: &Shared, model: Option<Model>, prep: impl FnOnce(&mut PanelS) + 'static) -> rune::AnyWindowSpec {
    let model = Arc::new(Mutex::new(model.unwrap_or_else(demo_model)));
    let mut ps = PanelS::new(tok(shared), app.clone(), shared.clone(), model);
    ps.view = view.into();
    prep(&mut ps);
    let spec = super::panel_spec(key, ps, (420.0, 760.0), true);
    let h = spec.app.handle();
    let h2 = h.clone();
    h.update(move |s, _| s.me = Some(h2));
    spec.boxed()
}

pub fn main() {
    // a scratch data folder: headless runs never touch the real settings
    if std::env::var_os("OUMU_DATA_DIR").is_none() {
        let dir = std::env::temp_dir().join("oumu-headless");
        let _ = std::fs::create_dir_all(&dir);
        std::env::set_var("OUMU_DATA_DIR", &dir);
    }
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).try_init();
    let mut state = HostState::default();
    state.settings = crate::settings::load();
    state.settings.onboarded = true;
    // the logic sees the same demo field, so offline tools (AI Rewriter, score) really run
    state.doc = Some(crate::host::HostDoc {
        id: "d1".into(),
        app_name: "Notepad".into(),
        bundle_id: "notepad.exe".into(),
        pid: 0,
        role: "edit".into(),
        text: TEXT.into(),
        sel_start: 0,
        sel_end: 0,
        selected_text: String::new(),
        frame: Default::default(),
        visible: Default::default(),
        version: 1,
        active: true,
        truncated: false,
    });
    if state.settings.user_name.is_empty() {
        state.settings.user_name = "Cratus God".into();
    }
    let shared: Shared = Arc::new(Mutex::new(state));
    let app = tauri::AppHandle::new();
    app.manage(shared.clone());
    run(app, shared);
}

/// A newer version, as an update check would find it.
fn demo_release() -> shared::update::Release {
    shared::update::Release { version: "0.2.1".into(), url: "https://example.org/Oumu-Setup.exe".into(), name: "Oumu-Setup.exe".into(), sha256: "0".repeat(64), size: 1, page: String::new() }
}

pub fn run(app: tauri::AppHandle, shared: Shared) {
    let tk = tok(&shared);
    let surf = |open: bool| {
        let mut s = SurfS::new(tk.clone());
        s.alerts = 7;
        s.tab_open = open;
        s
    };
    let mut card = surf(false);
    let m = demo_model();
    card.card = Some((m.alerts[0].clone(), "correctness".into()));
    let mut card_long = surf(false);
    card_long.card = Some((m.alerts[3].clone(), "clarity".into()));
    let mut card_mute = surf(false);
    card_mute.card = Some((m.alerts[2].clone(), "correctness".into()));
    let mut button_lint = surf(false);
    button_lint.linting = true;

    let empty = Model { status: Status { frontmost: "Explorer".into(), ..Default::default() }, ..Default::default() };
    let mut windows = vec![
        panel_window("review", "review", &app, &shared, None, |_| {}),
        panel_window("review-empty", "review", &app, &shared, Some(empty), |_| {}),
        panel_window("chat", "chat", &app, &shared, None, |_| {}),
        panel_window("chat-thread", "chat", &app, &shared, None, |s| {
            s.chat = vec![("user".into(), "Summarize this page.".into(), false), ("assistant".into(), "They're going to the library tomorrow to finish their report, and the results look good.".into(), false)];
        }),
        panel_window("plagiarism", "plagiarism", &app, &shared, None, |_| {}),
        panel_window("tone", "tone", &app, &shared, None, |_| {}),
        panel_window("performance", "performance", &app, &shared, None, |_| {}),
        panel_window("paraphraser", "paraphraser", &app, &shared, None, |_| {}),
        panel_window("paraphraser-result", "paraphraser", &app, &shared, None, |s| {
            s.feats.insert("paraphrase".into(), panel::Feat { data: Some(serde_json::json!({ "rewritten": "They're heading to the library tomorrow to finish their report.", "used_selection": false })), ..Default::default() });
        }),
        panel_window("humanizer", "humanizer", &app, &shared, None, |_| {}),
        panel_window("reactions", "reactions", &app, &shared, None, |_| {}),
        panel_window("translate", "translate", &app, &shared, None, |_| {}),
        panel_window("explain", "explain", &app, &shared, None, |_| {}),
        panel_window("rewriter", "rewriter", &app, &shared, None, |_| {}),
        panel_window("detector", "detector", &app, &shared, None, |_| {}),
        panel_window("detector-loading", "detector", &app, &shared, None, |s| {
            s.feats.insert("detector".into(), panel::Feat { loading: true, started: Some(std::time::Instant::now()), ..Default::default() });
        }),
        panel_window("history", "history", &app, &shared, None, |_| {}),
        panel_window("onboarding", "onboarding", &app, &shared, None, |_| {}),
        panel_window("settings", "settings", &app, &shared, None, |s| s.upd_demo = Some((crate::update::Status::UpToDate, None))),
        panel_window("settings-update", "settings", &app, &shared, None, |s| s.upd_demo = Some((crate::update::Status::Found(demo_release()), Some("0.2.1".into())))),
        panel_window("review-update", "review", &app, &shared, None, |s| s.upd_demo = Some((crate::update::Status::Found(demo_release()), Some("0.2.1".into())))),
    ];
    windows.push(surf_window("tab", (40.0, 84.0), surf(false), layer::tab_view));
    windows.push(surf_window("tab-open", (40.0, 84.0), surf(true), layer::tab_view));
    windows.push(surf_window("button", (60.0, 60.0), surf(false), layer::button_view));
    windows.push(surf_window("button-linting", (60.0, 60.0), button_lint, layer::button_view));
    windows.push(surf_window("card", (layer::CARD_W, 220.0), card, layer::card_view));
    windows.push(surf_window("card-long", (layer::CARD_W, 260.0), card_long, layer::card_view));
    windows.push(surf_window("card-mute", (layer::CARD_W, 260.0), card_mute, layer::card_view));
    rune::run_windows(move |_| windows);
}

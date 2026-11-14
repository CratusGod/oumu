//! The docked panel: Review plus the AI tools, Chat, History and Settings, with the icon
//! rail. Ported from Panel.tsx and windows/views/*.tsx (the React UI is the reference).

use super::style::*;
use super::{Alert, Doc, Model, Score};
use crate::host::Shared;
use rune::{Cx, TextEdit};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::AppHandle;

const RAIL: &[(&str, &str, &str)] = &[
    ("chat", "chat", "AI Chat"),
    ("plagiarism", "incognito", "Plagiarism"),
    ("tone", "tone", "Tone Detector"),
    ("performance", "target", "Performance Score"),
    ("paraphraser", "paraphrase", "Paraphraser"),
    ("humanizer", "heart", "Humanizer"),
    ("reactions", "book", "Reader Reactions"),
    ("translate", "translate", "Translate"),
    ("explain", "glasses", "Explain It"),
    ("rewriter", "bolt", "AI Rewriter"),
    ("detector", "scanner", "AI Detector"),
];

const CATS: &[(&str, &str)] = &[("correctness", "Correctness"), ("clarity", "Clarity"), ("engagement", "Engagement"), ("delivery", "Delivery")];

const LANGUAGES: &[&str] = &[
    "English (American)", "English (British)", "Spanish", "French", "German", "Italian", "Portuguese (Brazil)", "Portuguese (Portugal)", "Dutch", "Swedish", "Norwegian", "Danish", "Finnish", "Polish", "Czech", "Hungarian", "Romanian", "Greek", "Turkish", "Russian", "Ukrainian", "Hebrew", "Arabic", "Hindi", "Bengali", "Urdu", "Tamil", "Telugu", "Indonesian", "Malay", "Vietnamese", "Thai", "Chinese (Simplified)", "Chinese (Traditional)", "Japanese", "Korean", "Filipino", "Swahili",
];
const STYLES: &[&str] = &["Humanize", "Academic", "Professional", "Streamlined", "Creative", "Simple", "Expand", "Shorten"];
/// (name, short description, glyph, guide, new)
const VOICES: &[(&str, &str, &str, &str, bool)] = &[
    ("The Naturalist", "Replace common AI patterns.", "face", "Write the way a thoughtful person talks: varied sentence length, concrete details, occasional asides, contractions, no stacked adjectives, no hollow transitions, no AI clichés.", true),
    ("The Precisionist", "Communicate efficiently.", "scissors", "Be economical. Short sentences, active voice, one idea per sentence, no filler, no intensifiers, precise verbs.", false),
    ("The Everyday Voice", "Sound clear and balanced in any context.", "cup", "Clear, friendly, balanced. Contractions are fine. Plain words. Warm but not gushing.", false),
    ("The Executive", "Be confident and assertive at work.", "compass", "Confident, direct, decisive. Lead with the point. No hedging. Crisp business vocabulary without jargon.", false),
    ("The Scholar", "Use formal, objective language for school.", "quill", "Formal, objective, third person, measured claims, precise terminology, no contractions, no slang.", false),
];
const AUDIENCES: &[(&str, &str)] = &[
    ("Online followers", "People who follow you on social platforms; skim fast, respond to authenticity and clear takeaways."),
    ("General readers", "Curious adults with no special background."),
    ("My manager", "Busy, outcome-focused, wants status, risks and asks."),
    ("Teammates", "Peers who know the context and want specifics and next steps."),
    ("Customers", "People deciding whether to trust and buy; care about benefits and proof."),
    ("Investors", "Evaluate traction, team, market and risk; ask hard questions."),
    ("Professor", "Expects structure, evidence and precise claims."),
    ("Friends", "Informal, supportive, react to feeling and story."),
];
const AUTO: &str = "Auto-detect";

/// A feature run: its steps advance while the call is in flight (useFeature).
#[derive(Clone, Default)]
pub struct Feat {
    pub loading: bool,
    pub started: Option<Instant>,
    pub data: Option<Value>,
    pub error: Option<String>,
}

impl Feat {
    fn step(&self, n: usize) -> usize {
        self.started.map_or(0, |t| ((t.elapsed().as_secs_f32() / 1.4) as usize).min(n.saturating_sub(1)))
    }
}

pub struct PanelS {
    pub tok: Tok,
    pub kit: rune_kit::Kit,
    pub app: AppHandle,
    pub shared: Shared,
    pub model: Arc<Mutex<Model>>,
    pub me: Option<rune::AppHandle<PanelS>>,
    pub view: String,
    toast: Option<(String, Instant)>,
    pub feats: HashMap<String, Feat>,
    tab: String,
    open: Option<String>,
    busy: Option<String>,
    spot_note: Option<String>,
    show_diff: bool,
    para_style: Option<String>,
    hum_creating: bool,
    hum_name: TextEdit,
    hum_sample: TextEdit,
    aud: (String, String),
    aud_picking: bool,
    aud_creating: bool,
    aud_name: TextEdit,
    aud_desc: TextEdit,
    tr_from: String,
    tr_args: Option<Value>,
    tr_original: String,
    tr_highlight: bool,
    rw_filter: String,
    rw_done: HashSet<String>,
    rw_sugg: HashMap<String, Option<Vec<String>>>,
    pub chat: Vec<(String, String, bool)>,
    chat_input: TextEdit,
    pub chat_req: Option<String>,
    key: TextEdit,
    key_busy: bool,
    key_err: Option<String>,
    ai_open: Option<bool>,
    local_open: bool,
    local: Option<Value>,
    local_model: String,
    adv_open: bool,
    adv_base: TextEdit,
    adv_model: TextEdit,
    adv_key: TextEdit,
    ping: Option<String>,
    name: TextEdit,
    searx: TextEdit,
    brave: TextEdit,
    searx_probe: Option<(bool, String)>,
    autostart: Option<Option<bool>>,
    /// the text the AI Rewriter last scanned and Tone last ran on (they re-run as it changes)
    rw_scanned: Option<String>,
    tone_ran: Option<String>,
}

impl PanelS {
    pub fn new(tok: Tok, app: AppHandle, shared: Shared, model: Arc<Mutex<Model>>) -> PanelS {
        let s = shared.lock().unwrap().settings.clone();
        PanelS {
            kit: kit(&tok),
            tok,
            app,
            shared,
            model,
            me: None,
            view: "review".into(),
            toast: None,
            feats: HashMap::new(),
            tab: "correctness".into(),
            open: None,
            busy: None,
            spot_note: None,
            show_diff: false,
            para_style: None,
            hum_creating: false,
            hum_name: TextEdit::new(""),
            hum_sample: TextEdit::new(""),
            aud: (AUDIENCES[0].0.into(), AUDIENCES[0].1.into()),
            aud_picking: false,
            aud_creating: false,
            aud_name: TextEdit::new(""),
            aud_desc: TextEdit::new(""),
            tr_from: AUTO.into(),
            tr_args: None,
            tr_original: String::new(),
            tr_highlight: false,
            rw_filter: "all".into(),
            rw_done: HashSet::new(),
            rw_sugg: HashMap::new(),
            chat: vec![],
            chat_input: TextEdit::new(""),
            chat_req: None,
            key: TextEdit::new(""),
            key_busy: false,
            key_err: None,
            ai_open: None,
            local_open: false,
            local: None,
            local_model: String::new(),
            adv_open: false,
            adv_base: TextEdit::new(s.llm.base_url.clone()),
            adv_model: TextEdit::new(s.llm.model.clone()),
            adv_key: TextEdit::new(s.llm.api_key.clone()),
            ping: None,
            name: TextEdit::new(s.user_name.clone()),
            searx: TextEdit::new(s.search.searxng_url.clone()),
            brave: TextEdit::new(s.search.brave_key.clone()),
            searx_probe: None,
            autostart: None,
            rw_scanned: None,
            tone_ran: None,
        }
    }

    fn settings(&self) -> crate::settings::Settings {
        self.shared.lock().unwrap().settings.clone()
    }

    fn save(&mut self, s: crate::settings::Settings) {
        if let Err(e) = crate::commands::set_settings(self.app.clone(), self.app.state::<Shared>(), s) {
            self.say(e);
        }
    }

    pub fn say(&mut self, m: impl Into<String>) {
        self.toast = Some((m.into(), Instant::now()));
        if let Some(me) = self.me.clone() {
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(2900));
                me.update(|s, _| {
                    if s.toast.as_ref().is_some_and(|t| t.1.elapsed() >= Duration::from_millis(2800)) {
                        s.toast = None;
                    }
                });
            });
        }
    }

    /// Run a feature in the background; its result lands in `feats[key]`.
    pub fn run(&mut self, key: &str, name: &str, args: Value) {
        log::info!("panel runs {name}");
        self.feats.insert(key.into(), Feat { loading: true, started: Some(Instant::now()), ..Default::default() });
        let (app, shared, me, key, name) = (self.app.clone(), self.shared.clone(), self.me.clone(), key.to_string(), name.to_string());
        // the steps advance on their own while the call runs
        if let Some(me) = me.clone() {
            std::thread::spawn(move || {
                for _ in 0..240 {
                    std::thread::sleep(Duration::from_millis(500));
                    me.update(|_, _| {});
                }
            });
        }
        tauri::async_runtime::spawn(async move {
            let r = crate::commands::run_feature_inner(&app, &shared, &name, args).await;
            if let Some(me) = me {
                me.update(move |s, _| {
                    let f = s.feats.entry(key).or_default();
                    f.loading = false;
                    match r {
                        Ok(v) => f.data = Some(v),
                        Err(e) => f.error = Some(e),
                    }
                });
            }
        });
    }

    fn feat(&self, key: &str) -> Feat {
        self.feats.get(key).cloned().unwrap_or_default()
    }

    fn reset(&mut self, key: &str) {
        self.feats.remove(key);
        self.show_diff = false;
    }

    /// Show a view; the ones that follow the text start on it.
    pub fn set_view(&mut self, v: &str) {
        self.view = v.to_string();
        self.doc_changed();
    }

    /// The text in the field changed (or a view that follows it opened): AI Rewriter rescans
    /// at once (it is offline); Tone runs once typing pauses for 1.5 s, at 12 words or more.
    pub fn doc_changed(&mut self) {
        let text = self.model.lock().unwrap().doc.as_ref().map(|d| d.text.clone());
        let Some(text) = text.filter(|t| !t.trim().is_empty()) else { return };
        match self.view.as_str() {
            "rewriter" if self.rw_scanned.as_deref() != Some(text.as_str()) && !self.feat("rewriter").loading => {
                self.rw_scanned = Some(text);
                self.run("rewriter", "rewriter_scan", json!({}));
            }
            "tone" if self.tone_ran.as_deref() != Some(text.as_str()) && text.split_whitespace().count() >= 12 => {
                let Some(me) = self.me.clone() else { return };
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(1500));
                    me.update(move |s, _| {
                        let now = s.model.lock().unwrap().doc.as_ref().map(|d| d.text.clone());
                        if s.view == "tone" && now.as_deref() == Some(text.as_str()) && s.tone_ran.as_deref() != Some(text.as_str()) && !s.feat("tone").loading {
                            s.tone_ran = Some(text);
                            s.run("tone", "tone", json!({}));
                        }
                    });
                });
            }
            _ => {}
        }
    }

    /// `oumu --translate`, Utsuri's Translate slice: translate what was handed over.
    pub fn translate_pending(&mut self) {
        self.view = "translate".into();
        let text = crate::commands::take_pending_translation().or_else(crate::commands::highlighted_text);
        match text {
            Some(t) if !t.trim().is_empty() => {
                self.tr_highlight = true;
                self.tr_original = t.clone();
                self.tr_from = AUTO.into();
                self.go_translate(json!({ "text": t }));
            }
            _ => self.say("Highlight some text first, in any app."),
        }
    }

    fn go_translate(&mut self, args: Value) {
        self.tr_args = Some(args.clone());
        let mut a = args;
        a["target"] = json!(self.settings().target_language);
        a["source"] = json!(if self.tr_from == AUTO { String::new() } else { self.tr_from.clone() });
        self.run("translate", "translate", a);
    }
}

type S = PanelS;

// ---------------------------------------------------------------- the frame

pub fn view(s: &S, cx: &mut Cx<S>) -> El {
    let k = &s.tok;
    let m = s.model.lock().unwrap();
    let st = s.settings();
    let rail_left = st.rail_side == "left";
    let mut content = col().key("content").grow(1.0).min_w(0.0).h_full().min_h(0.0).child(body(s, &m, &st, cx));
    if let Some((msg, _)) = &s.toast {
        content = content.child(row().key("toast").pin(Some(16.0), None, Some(16.0), Some(16.0)).radius(10.0).px(14.0).py(10.0).fill(c(k.bg_sunken())).border(1.0, c(k.border)).child(p(msg.clone(), 14.0, k.text)));
    }
    let rail = rail(s, &m, &st);
    // the backdrop: a soft gradient with faint brand and delivery glows
    let back = el().absolute(0.0, 0.0).w_full().h_full().linear_gradient(2.88, [(0.0, c(k.elevated)), (0.58, c(k.bg)), (1.0, c(k.bg))]);
    let glow1 = el().absolute(0.0, 0.0).w_full().h_full().radial_gradient([0.0, 0.0], [520.0, 400.0], [(0.0, c(k.brand).alpha(0.24)), (0.62, c(k.brand).alpha(0.0))]);
    let glow2 = el().absolute(0.0, 0.0).w_full().h_full().radial_gradient([1.0, 1.0], [420.0, 370.0], [(0.0, c(k.delivery).alpha(0.20)), (0.66, c(k.delivery).alpha(0.0))]);
    let mut r = row().key("panel").w_full().h_full().radius(14.0).clip_rounded(14.0).border(1.0, c(k.border_subdued)).child(back).child(glow1).child(glow2);
    r = if rail_left { r.child(rail).child(content) } else { r.child(content).child(rail) };
    r
}

/// Keys no control used: Esc tucks the panel away.
pub fn on_key(s: &mut S, ke: &rune::KeyEvent, _tx: &mut rune::Tx) -> bool {
    if matches!(ke.key, rune::Key::Escape) {
        crate::commands::tuck_panel_inner(&s.app);
        return true;
    }
    false
}

fn rail_btn(key: &str, ic: &str, color: [u8; 3], label: &str, active: bool, k: &Tok) -> El {
    let b = el().key(format!("rail/{key}")).size(36.0, 36.0).radius(12.0).center().cursor(rune::Cursor::Pointer).help(label).role("button").label(label).hover(c(k.subdued));
    let b = if active { b.fill(c(k.brand_tint())).border(2.0, c(k.brand)) } else { b };
    b.child(icon(ic, color, 22.0))
}

fn rail(s: &S, m: &Model, st: &crate::settings::Settings) -> El {
    let k = &s.tok;
    let v = s.view.as_str();
    let mut stack = col().mt(10.0).w(44.0).items_center().gap(2.0).py(8.0).radius(22.0).border(1.0, c(k.border_subdued)).fill(c(k.bg));
    let mut review = rail_btn("review", "mark", k.brand, "Oumu", v == "review", k).on_press(|s: &mut S| s.view = "review".into());
    let n = m.alerts.len();
    if n > 0 {
        review = review.child(row().absolute(20.0, 2.0).h(16.0).px(4.0).radius(8.0).center().fill(c(k.correctness)).child(text(if n > 99 { "99+".into() } else { n.to_string() }).font(ts(10.0, 700.0)).color(c(k.bg))));
    }
    stack = stack.child(review);
    for (id, ic, label) in RAIL {
        let id_s = id.to_string();
        stack = stack.child(rail_btn(id, ic, k.icon, label, v == *id, k).on_press(move |s: &mut S| s.set_view(&id_s)));
    }
    stack = stack.child(rail_btn("settings", "gear", k.icon, "Settings", v == "settings", k).on_press(|s: &mut S| s.view = "settings".into()));
    let tuck_ic = if st.dock_side == "right" { "chevron-right" } else { "chevron-left" };
    let avatar = el().key("avatar").size(32.0, 32.0).radius(16.0).center().fill(c(k.accent)).cursor(rune::Cursor::Pointer).help(st.user_name.clone()).child(text(m.initials.clone()).font(ts(12.0, 600.0)).color(c(k.text))).on_press(|s: &mut S| s.view = "settings".into());
    let side = if st.rail_side == "left" { [0.0, 1.0, 0.0, 0.0] } else { [0.0, 0.0, 0.0, 1.0] };
    col()
        .key("rail")
        .w(56.0)
        .h_full()
        .items_center()
        .pt(10.0)
        .pb(8.0)
        .fill(c(k.bg).alpha(0.55))
        .border_sides(side, c(k.border_subdued))
        .window_drag()
        .child(rail_btn("history", "clock", k.icon, "History", v == "history", k).on_press(|s: &mut S| s.view = "history".into()))
        .child(stack)
        .child(rune::spacer())
        .child(col().gap(10.0).items_center().child(rail_btn("tuck", tuck_ic, k.icon, "Tuck away", false, k).on_press(|s: &mut S| crate::commands::tuck_panel_inner(&s.app))).child(avatar))
}

fn scroll(e: El) -> El {
    col().key("scroll").w_full().grow(1.0).min_h(0.0).px(16.0).pb(16.0).scroll(rune::ScrollSpec { stick_bottom: false }).child(e.w_full())
}

fn body(s: &S, m: &Model, st: &crate::settings::Settings, _cx: &mut Cx<S>) -> El {
    match s.view.as_str() {
        "review" => review(s, m),
        "chat" => chat(s, m, st),
        "plagiarism" => plagiarism(s, m, st),
        "tone" => tone(s, m),
        "performance" => performance(s, m),
        "paraphraser" => paraphraser(s, m),
        "humanizer" => humanizer(s, m, st),
        "reactions" => reactions(s, m, st),
        "translate" => translate(s, m, st),
        "explain" => explain(s, m),
        "rewriter" => rewriter(s, m),
        "detector" => detector(s, m),
        "history" => history(s),
        "onboarding" => onboarding(s, st),
        _ => settings_view(s, st),
    }
}

fn no_doc(s: &S, m: &Model) -> El {
    let k = &s.tok;
    let stt = &m.status;
    if stt.skipped && !stt.frontmost.is_empty() {
        let body = if stt.skip_built_in { "A terminal shows a command transcript, not writing. Oumu stays out of terminals." } else { "You switched Oumu off in this app." };
        let mut e = empty("pencil", &format!("Oumu is off in {}", stt.frontmost), body, k);
        if !stt.skip_built_in {
            e = e.child(btn("resume", Btn::Ghost, &format!("Turn it on in {}", stt.frontmost), k).on_press(|s: &mut S| {
                let bid = s.model.lock().unwrap().status.frontmost_bid.clone();
                let mut st = s.settings();
                st.skip_apps.retain(|a| a.bundle_id != bid);
                s.save(st);
            }));
        }
        return scroll(e);
    }
    let front = if stt.frontmost.is_empty() { String::new() } else { format!(" (now in front: {})", stt.frontmost) };
    scroll(empty("pencil", "Click into a text field", &format!("Open any app and click where you write. Oumu follows your cursor{front}."), k))
}

// ---------------------------------------------------------------- Review

fn alert_mode(a: &Alert) -> &'static str {
    if a.suggestions.is_empty() {
        "note"
    } else if a.suggestions[0].is_empty() {
        "remove"
    } else {
        "replace"
    }
}

fn review(s: &S, m: &Model) -> El {
    let k = &s.tok;
    let Some(doc) = &m.doc else { return no_doc(s, m) };
    let running = m.spot_running || s.feat("spot").loading;
    let spot = btn("spot", Btn::Outline, "", k).px(12.0).py(7.0).help("Check every sentence for wordiness, tangles, long run-ons and repeats");
    let spot = if running { spot.child(spinner(k, 14.0)).child(t("Checking", 13.0, k.text)) } else { spot.child(icon("sparkle", k.clarity, 16.0)).child(t("Spot check", 13.0, k.text)).on_press(|s: &mut S| run_spot(s)) };
    let head = row().w_full().items_center().gap(10.0).py(14.0).child(text("Review suggestions").font(ts(20.0, 600.0)).color(c(k.text)).truncate().shrink().min_w(0.0)).child(row().radius(12.0).px(8.0).py(2.0).fill(c(k.border_subdued)).child(tb(m.alerts.len().to_string(), 13.0, k.text))).child(rune::spacer()).child(spot);
    let app_row = row().gap(10.0).items_center().mb(10.0).child(t(if doc.app_name.is_empty() { "This app".to_string() } else { doc.app_name.clone() }, 12.0, k.text_subdued)).child(
        t("Skip this app", 12.0, k.brand).cursor(rune::Cursor::Pointer).help("Oumu stays out of this app until you turn it back on in Settings").on_press(|s: &mut S| skip_app(s)),
    );
    let mut col_ = col().w_full().child(head).child(app_row);
    let spot_note = s.feat("spot").data.map(|r| {
        let n = r["suggestions"].as_u64().unwrap_or(0);
        if r["busy"].as_bool().unwrap_or(false) {
            "A check is already running.".to_string()
        } else if n == 0 {
            format!("Checked {} sentences. Nothing to change.", r["checked"].as_u64().unwrap_or(0) + r["from_cache"].as_u64().unwrap_or(0))
        } else {
            format!("{n} sentence suggestion{}.", if n == 1 { "" } else { "s" })
        }
    }).or_else(|| s.spot_note.clone());
    if running || spot_note.is_some() {
        col_ = col_.child(t(if running { "Checking your sentences with the model.".to_string() } else { spot_note.unwrap_or_default() }, 13.0, k.text_subdued).mb(8.0));
    }
    // the four category cards
    let mut cats = row().w_full().gap(6.0).mb(8.0);
    for (id, label) in CATS {
        let n = m.alerts.iter().filter(|a| a.category == *id).count();
        let fill = m.score.as_ref().map_or(100.0, |sc| fill_of(sc, id)).clamp(6.0, 100.0);
        let active = s.tab == *id;
        let colr = k.cat(id);
        let count = if n == 0 { row().size(20.0, 20.0).center().child(icon("check", k.brand, 12.0)) } else { row().h(20.0).px(6.0).radius(10.0).center().fill(c(colr)).child(tb(n.to_string(), 12.0, k.bg_sunken())) };
        let id_s = id.to_string();
        let mut card = col()
            .key(format!("cat/{id}"))
            .grow(1.0)
            .min_w(0.0)
            .items_center()
            .gap(7.0)
            .pt(10.0)
            .pb(9.0)
            .px(4.0)
            .radius(12.0)
            .hover(c(k.subdued))
            .cursor(rune::Cursor::Pointer)
            .help(format!("{label}: {n} suggestion{}", if n == 1 { "" } else { "s" }))
            .child(meter(fill / 100.0, 4.0, colr, k))
            .child(text(*label).font(ts(12.0, 500.0)).color(c(if active { k.text } else { k.text_subdued })).truncate())
            .child(count)
            .on_press(move |s: &mut S| {
                s.tab = id_s.clone();
                s.open = None;
            });
        if active {
            card = card.fill(c(k.bg_hover())).border(1.0, c(k.border_subdued));
        }
        cats = cats.child(card);
    }
    col_ = col_.child(cats);
    let list: Vec<&Alert> = m.alerts.iter().filter(|a| a.category == s.tab).collect();
    if list.is_empty() {
        let correct = s.tab == "correctness";
        col_ = col_.child(empty(if correct { "check" } else { "sparkle" }, &if correct { "Looking good".to_string() } else { format!("No {} suggestions", s.tab) }, if doc.text.trim().is_empty() { "Start typing and suggestions appear here." } else { "Nothing to fix in this category right now." }, k));
        return scroll(col_);
    }
    let open = s.open.clone().filter(|o| list.iter().any(|a| &a.id == o)).or_else(|| list.first().map(|a| a.id.clone()));
    for a in list {
        let colr = k.cat(&a.category);
        let dot = row().size(24.0, 24.0).radius(12.0).center().fill(c(k.bg_chip())).child(el().size(10.0, 10.0).radius(5.0).fill(c(colr)));
        let id = a.id.clone();
        if open.as_deref() == Some(a.id.as_str()) {
            let mode = alert_mode(a);
            let label = CATS.iter().find(|x| x.0 == a.category).map_or("", |x| x.1);
            let kind = row().w_full().gap(8.0).items_center().child(dot).child(t(format!("{label} · "), 15.0, k.text_subdued)).child(text(a.title.clone()).font(ts(15.0, 500.0)).color(c(k.text)).grow(1.0).min_w(0.0)).child({
                let msg = a.message.clone();
                title_btn("why", "info", "Why?", k).on_press(move |s: &mut S| s.say(msg.clone()))
            });
            let preview = match mode {
                "note" => p(a.original.clone(), 18.0, k.text),
                "replace" if a.original.split_whitespace().count() > 6 => diff(&a.original, &a.suggestions[0], 18.0, k),
                _ => {
                    let ins = if mode == "remove" { "Remove".to_string() } else { a.suggestions[0].clone() };
                    let after = context(&doc.text, a);
                    let full = format!("{} {}{}", a.original, ins, if after.is_empty() { String::new() } else { format!(" {after}") });
                    let n = a.original.len();
                    let ins_end = n + 1 + ins.len();
                    let mut spans = vec![rune::text::Span::new(0, n).color(c(k.text_muted)), if mode == "remove" { rune::text::Span::new(n + 1, ins_end).color(c(k.text_subdued)).size(14.0) } else { rune::text::Span::new(n + 1, ins_end).weight(600.0) }];
                    if ins_end < full.len() {
                        spans.push(rune::text::Span::new(ins_end, full.len()).color(c(k.text_subdued)));
                    }
                    text(full).wrap().w_full().font(ts(18.0, 420.0)).color(c(k.text)).spans(spans).strikes(vec![(0, n, Some(c(k.text_muted)))])
                }
            };
            let mut actions = row().gap(14.0).items_center().flex_wrap();
            if mode != "note" {
                let busy = s.busy.as_deref() == Some(a.id.as_str());
                let id2 = id.clone();
                actions = actions.child(disabled(btn("accept", Btn::Primary, if busy { "Applying" } else { "Accept" }, k).px(18.0).py(10.0), busy).on_press(move |s: &mut S| accept(s, &id2, 0)));
            }
            let id3 = id.clone();
            actions = actions.child(btn("dismiss", Btn::Ghost, "Dismiss", k).on_press(move |s: &mut S| {
                if let Err(e) = crate::commands::dismiss_alert_inner(&s.app, &s.shared, &id3) {
                    s.say(e);
                }
            }));
            for (i, sug) in a.suggestions.iter().enumerate().skip(1).take(3) {
                let id4 = id.clone();
                actions = actions.child(btn(&format!("alt/{i}"), Btn::Outline, if sug.is_empty() { "Remove" } else { sug }, k).px(12.0).py(6.0).on_press(move |s: &mut S| accept(s, &id4, i)));
            }
            col_ = col_.child(
                col().key(format!("alert/{id}")).w_full().pt(10.0).pb(16.0).gap(10.0).border_sides([0.0, 0.0, 1.0, 0.0], c(k.border_subdued))
                    .child(kind)
                    .child(preview)
                    .child(p(a.message.clone(), 14.0, k.text_subdued))
                    .child(actions),
            );
        } else {
            col_ = col_.child(
                row().key(format!("alert/{id}")).w_full().gap(12.0).py(14.0).items_start().cursor(rune::Cursor::Pointer).border_sides([0.0, 0.0, 1.0, 0.0], c(k.border_subdued))
                    .child(dot)
                    .child(col().grow(1.0).min_w(0.0).child(t(a.title.clone(), 14.0, k.text_subdued)).child(text(if a.original.trim().is_empty() { "Space".to_string() } else { a.original.trim().to_string() }).font(ts(17.0, 420.0)).color(c(k.text)).truncate()))
                    .on_press(move |s: &mut S| s.open = Some(id.clone())),
            );
        }
    }
    scroll(col_)
}

fn fill_of(sc: &Score, id: &str) -> f32 {
    (match id {
        "correctness" => sc.correctness_fill,
        "clarity" => sc.clarity_fill,
        "engagement" => sc.engagement_fill,
        _ => sc.delivery_fill,
    }) as f32
}

fn context(text: &str, a: &Alert) -> String {
    let after: String = text.chars().skip(a.end).take(24).collect();
    let after = after.split_whitespace().collect::<Vec<_>>().join(" ");
    if after.is_empty() { String::new() } else { format!("{after}…") }
}

fn accept(s: &mut S, id: &str, idx: usize) {
    s.busy = Some(id.to_string());
    match crate::commands::accept_alert_inner(&s.app, &s.shared, id, idx) {
        Ok(r) => s.say(if matches!(r.method, crate::apply::Method::Ax) { "Applied" } else { "Applied via paste" }),
        Err(e) => s.say(e),
    }
    s.busy = None;
}

fn skip_app(s: &mut S) {
    let Some(d) = s.model.lock().unwrap().doc.clone() else { return };
    let mut st = s.settings();
    if d.bundle_id.is_empty() || st.skip_apps.iter().any(|a| a.bundle_id == d.bundle_id) {
        return;
    }
    st.skip_apps.push(crate::settings::SkipApp { bundle_id: d.bundle_id.clone(), name: d.app_name.clone() });
    s.save(st);
    s.say(format!("Oumu is off in {}. Settings → Skipped apps turns it back on.", d.app_name));
}

fn run_spot(s: &mut S) {
    let words = s.model.lock().unwrap().doc.as_ref().map_or(0, |d| d.text.split_whitespace().count());
    if words < 8 {
        s.say("Write a few sentences first.");
        return;
    }
    s.spot_note = None;
    s.run("spot", "spot_check", json!({}));
}

// ---------------------------------------------------------------- shared result pieces

/// RewriteResult: the new text, Replace / Copy / Show changes / Back.
fn rewrite_result(s: &S, key: &'static str, original: &str, rewritten: &str, used_sel: bool, can_replace: bool) -> El {
    let k = &s.tok;
    let boxed = col().key("result").w_full().mt(12.0).radius(12.0).px(16.0).py(14.0).border(1.0, c(k.border)).child(if s.show_diff { diff(original, rewritten, 15.0, k) } else { p(rewritten.to_string(), 15.0, k.text).selectable(true) });
    let mut acts = row().gap(10.0).mt(12.0).flex_wrap();
    let rw = rewritten.to_string();
    if can_replace {
        let rw2 = rw.clone();
        acts = acts.child(btn("replace", Btn::Primary, if used_sel { "Replace selection" } else { "Replace text" }, k).on_press(move |s: &mut S| {
            let d = s.model.lock().unwrap().doc.clone();
            if let Some(d) = d {
                let (a, b) = if used_sel { (d.sel_start, d.sel_end) } else { (0, d.text.chars().count()) };
                match crate::commands::replace_range_inner(&s.app, &s.shared, a, b, &rw2) {
                    Ok(r) => s.say(if matches!(r.method, crate::apply::Method::Ax) { "Replaced" } else { "Replaced via paste" }),
                    Err(e) => s.say(e),
                }
            }
        }));
    }
    acts = acts
        .child(btn("copy", if can_replace { Btn::Outline } else { Btn::Primary }, "Copy", k).on_press_tx(move |s: &mut S, tx: &mut rune::Tx| {
            tx.copy(rw.clone());
            s.say("Copied");
        }))
        .child(btn("diff", Btn::Ghost, if s.show_diff { "Hide changes" } else { "Show changes" }, k).on_press(|s: &mut S| s.show_diff = !s.show_diff))
        .child(btn("back", Btn::Ghost, "Back", k).on_press(move |s: &mut S| s.reset(key)));
    col().w_full().child(boxed).child(acts)
}

fn error_box(e: &str, k: &Tok) -> El {
    let mut c_ = col().w_full().gap(8.0).child(notice(e.to_string(), false, k));
    let l = e.to_lowercase();
    if l.contains("key") || e.contains("AI") || l.contains("model") {
        c_ = c_.child(row().child(btn("setup-ai", Btn::Outline, "Set up AI in Settings", k).on_press(|s: &mut S| s.view = "settings".into())));
    }
    c_
}

fn doc_src(d: &Doc) -> String {
    if d.selected_text.is_empty() { d.text.clone() } else { d.selected_text.clone() }
}

fn str_of(v: &Value, k: &str) -> String {
    v[k].as_str().unwrap_or("").to_string()
}

/// .list-opt
fn list_opt(key: &str, selected: bool, k: &Tok) -> El {
    let e = col().key(key).w_full().radius(12.0).px(18.0).py(16.0).mb(10.0).border(1.0, c(k.border)).hover(c(k.subdued)).cursor(rune::Cursor::Pointer);
    if selected { e.fill(c(k.subdued)) } else { e }
}

// ---------------------------------------------------------------- Paraphraser / Humanizer / Reactions

fn paraphraser(s: &S, m: &Model) -> El {
    let k = &s.tok;
    let Some(doc) = &m.doc else { return no_doc(s, m) };
    let f = s.feat("paraphrase");
    let intro = f.data.as_ref().map(|d| str_of(d, "summary")).filter(|x| !x.is_empty()).unwrap_or_else(|| if doc.selected_text.is_empty() { "Rewrite your text in a new style, or select a passage to rewrite just that.".into() } else { "Rewrite the selected text in a new style.".into() });
    let mut c_ = col().w_full().child(hero("paraphrase", "Paraphraser", Some(&intro), k));
    if f.loading {
        c_ = c_.child(steps(&["Reading your text", "Rewriting", "Polishing"], f.step(3), k));
    } else if let Some(e) = &f.error {
        c_ = c_.child(error_box(e, k));
    } else if let Some(d) = &f.data {
        c_ = c_.child(rewrite_result(s, "paraphrase", &doc_src(doc), &str_of(d, "rewritten"), d["used_selection"].as_bool().unwrap_or(false), true));
    } else {
        c_ = c_.child(section_label("Change writing style", k));
        for st in STYLES {
            let sel = s.para_style.as_deref() == Some(*st);
            let st_s = st.to_string();
            c_ = c_.child(list_opt(&format!("style/{st}"), sel, k).child(t(*st, 16.0, k.text)).on_press(move |s: &mut S| {
                s.para_style = Some(st_s.clone());
                s.run("paraphrase", "paraphrase", json!({ "style": st_s }));
            }));
        }
    }
    col().w_full().h_full().child(titlebar("", vec![title_btn("reload", "refresh", "Reload", k).on_press(|s: &mut S| {
        if let Some(st) = s.para_style.clone() {
            s.run("paraphrase", "paraphrase", json!({ "style": st }));
        }
    })], k)).child(scroll(c_))
}

fn voice_card(key: &str, glyph: &str, name: &str, desc: &str, new: bool, k: &Tok) -> El {
    let mut inner = col().w_full();
    if new {
        inner = inner.child(row().child(row().radius(10.0).px(8.0).py(2.0).mb(8.0).fill(c(k.brand_chip())).child(tb("New", 12.0, k.brand_chip_ink()))));
    }
    inner = inner.child(row().gap(14.0).items_center().child(row().size(48.0, 48.0).radius(24.0).center().fill(c(k.purple_chip())).child(icon(glyph, k.purple_chip_ink(), 24.0))).child(col().grow(1.0).min_w(0.0).child(tb(name, 16.0, k.text)).child(p(desc, 14.0, k.text_subdued))));
    card(inner, k).key(key).mb(10.0).cursor(rune::Cursor::Pointer).hover(c(k.bg_hover()))
}

fn humanizer(s: &S, m: &Model, st: &crate::settings::Settings) -> El {
    let k = &s.tok;
    let Some(doc) = &m.doc else { return no_doc(s, m) };
    let f = s.feat("humanize");
    let msgs = ["Analyzing your document", "Adopting the voice", "Humanizing"];
    let head = row().w_full().h(44.0).items_center().pl(16.0).gap(10.0).child(icon("heart", k.text, 22.0)).child(tb("Humanizer", 17.0, k.text)).child(rune::spacer()).child(title_btn("reload", "refresh", "Reload", k).mr(12.0).on_press(|s: &mut S| s.reset("humanize")));
    let mut c_ = col().w_full();
    if f.loading {
        c_ = c_.child(col().w_full().items_center().pt(60.0).gap(16.0).child(spinner(k, 22.0)).child(t(msgs[f.step(3)], 16.0, k.text_subdued)));
    } else if let Some(e) = &f.error {
        c_ = c_.child(error_box(e, k));
    } else if let Some(d) = &f.data {
        c_ = c_.child(section_label(&format!("Rewritten in {}", str_of(d, "voice")), k)).child(rewrite_result(s, "humanize", &doc_src(doc), &str_of(d, "rewritten"), d["used_selection"].as_bool().unwrap_or(false), true));
        if let Some(ch) = d["changes"].as_array().filter(|a| !a.is_empty()) {
            let mut l = col().gap(4.0).mt(14.0);
            for x in ch {
                l = l.child(p(format!("•  {}", x.as_str().unwrap_or("")), 14.0, k.text_subdued));
            }
            c_ = c_.child(l);
        }
    } else if s.hum_creating {
        c_ = c_
            .child(section_label("Create a voice", k))
            .child(field_ph("hum-name", "Name", "e.g. The Storyteller", &s.hum_name, false, &s.kit, k, |s: &mut S| &mut s.hum_name))
            .child(field_area("hum-sample", "Writing sample (leave empty to use your current text)", "Paste something you wrote…", &s.hum_sample, &s.kit, k, |s: &mut S| &mut s.hum_sample))
            .child(row().gap(10.0).child(btn("save-voice", Btn::Primary, "Save voice", k).on_press(|s: &mut S| create_voice(s))).child(btn("cancel", Btn::Ghost, "Cancel", k).on_press(|s: &mut S| s.hum_creating = false)));
    } else {
        c_ = c_.child(card(row().gap(10.0).items_center().py(4.0).child(icon("plus", k.link, 20.0)).child(t("Create a voice", 16.0, k.link)), k).key("create-voice").cursor(rune::Cursor::Pointer).hover(c(k.bg_hover())).on_press(|s: &mut S| s.hum_creating = true).mb(24.0));
        for v in &st.custom_voices {
            let (n, dsc) = (v.name.clone(), v.description.clone());
            c_ = c_.child(voice_card(&format!("voice/{}", v.id), "pencil", &v.name, &v.description.chars().take(80).collect::<String>(), false, k).on_press(move |s: &mut S| s.run("humanize", "humanize", json!({ "voice_name": n, "voice_description": dsc }))));
        }
        for (name, desc, glyph, guide, new) in VOICES {
            let (n, g) = (name.to_string(), guide.to_string());
            c_ = c_.child(voice_card(&format!("voice/{name}"), glyph, name, desc, *new, k).on_press(move |s: &mut S| s.run("humanize", "humanize", json!({ "voice_name": n, "voice_description": g }))));
        }
    }
    col().w_full().h_full().child(head).child(scroll(c_))
}

fn create_voice(s: &mut S) {
    let sample = if s.hum_sample.text.trim().is_empty() { s.model.lock().unwrap().doc.as_ref().map(doc_src).unwrap_or_default() } else { s.hum_sample.text.clone() };
    let name = s.hum_name.text.clone();
    if name.trim().is_empty() {
        s.say("Give the voice a name first.");
        return;
    }
    s.say("Creating your voice…");
    // the form stays until the voice exists, so nothing typed is lost on an error
    run_then(s, "create_voice", json!({ "name": name, "sample": sample }), |s| {
        s.hum_creating = false;
        s.hum_name = TextEdit::new("");
        s.hum_sample = TextEdit::new("");
        s.say("Voice created");
    });
}

/// .li with a .num badge.
fn li(num: &str, q: bool, body: &str, bold: Option<&str>, k: &Tok) -> El {
    let (bg, ink) = if q { (k.q_bg(), k.q_ink()) } else { (k.brand_chip(), k.brand_chip_ink()) };
    let txt = match bold {
        Some(b) => text(format!("{b} · {body}")).wrap().grow(1.0).min_w(0.0).font(ts(16.0, 420.0)).color(c(k.text)).spans(vec![rune::text::Span::new(0, b.len()).weight(600.0)]),
        None => text(body.to_string()).wrap().grow(1.0).min_w(0.0).font(ts(16.0, 420.0)).color(c(k.text)),
    };
    row().w_full().gap(14.0).items_start().py(8.0).child(row().size(26.0, 26.0).radius(13.0).center().fill(c(bg)).child(tb(num, 13.0, ink))).child(txt)
}

fn run_reactions(s: &mut S) {
    let (n, d) = s.aud.clone();
    s.run("reactions", "reactions", json!({ "audience": n, "audience_desc": d }));
}

fn reactions(s: &S, m: &Model, st: &crate::settings::Settings) -> El {
    let k = &s.tok;
    if m.doc.is_none() {
        return no_doc(s, m);
    }
    let f = s.feat("reactions");
    let pick = row().key("audience").w_full().items_center().justify_between().radius(12.0).px(16.0).py(12.0).mt(10.0).mb(14.0).border(1.0, c(k.border)).cursor(rune::Cursor::Pointer)
        .child(row().gap(10.0).items_center().child(icon("users", k.text_subdued, 20.0)).child(t(s.aud.0.clone(), 16.0, k.text_subdued)))
        .child(icon("chevron-right", k.text, 18.0))
        .on_press(|s: &mut S| s.aud_picking = !s.aud_picking);
    let mut c_ = col().w_full().child(hero("book", "Reader Reactions", None, k)).child(pick);
    if s.aud_picking {
        let mut box_ = col().w_full().mb(16.0).child(section_label("Audience · Tailor the feedback to who you're writing for", k));
        let all: Vec<(String, String)> = st.custom_audiences.iter().map(|a| (a.name.clone(), a.description.clone())).chain(AUDIENCES.iter().map(|(n, d)| (n.to_string(), d.to_string()))).collect();
        for (n, d) in all {
            let sel = n == s.aud.0;
            let (n2, d2) = (n.clone(), d.clone());
            box_ = box_.child(list_opt(&format!("aud/{n}"), sel, k).child(t(n, 16.0, k.text)).child(p(d, 13.0, k.text_subdued)).on_press(move |s: &mut S| {
                s.aud = (n2.clone(), d2.clone());
                s.aud_picking = false;
                run_reactions(s);
            }));
        }
        if s.aud_creating {
            box_ = box_
                .child(field_ph("aud-name", "Audience name", "e.g. Hiring managers", &s.aud_name, false, &s.kit, k, |s: &mut S| &mut s.aud_name))
                .child(field_area("aud-desc", "Describe them", "Who they are and what they care about", &s.aud_desc, &s.kit, k, |s: &mut S| &mut s.aud_desc))
                .child(row().gap(10.0).child(btn("save-aud", Btn::Primary, "Save and recheck", k).on_press(|s: &mut S| {
                    let (n, d) = (s.aud_name.text.trim().to_string(), s.aud_desc.text.trim().to_string());
                    if n.is_empty() {
                        s.say("Give the audience a name first.");
                        return;
                    }
                    let (n2, d2) = (n.clone(), d.clone());
                    run_then(s, "create_audience", json!({ "name": n, "description": d }), move |s| {
                        s.aud = (n2.clone(), d2.clone());
                        s.aud_creating = false;
                        s.aud_picking = false;
                        s.aud_name = TextEdit::new("");
                        s.aud_desc = TextEdit::new("");
                        s.say("Audience saved");
                        run_reactions(s);
                    });
                })).child(btn("cancel", Btn::Ghost, "Cancel", k).on_press(|s: &mut S| s.aud_creating = false)));
        } else {
            box_ = box_.child(row().child(btn_icon("new-aud", Btn::Outline, "plus", k.text, "Create audience", k).on_press(|s: &mut S| s.aud_creating = true)));
        }
        c_ = c_.child(box_);
    }
    if f.loading {
        c_ = c_.child(steps(&["Reading as your audience", "Predicting reactions", "Writing questions"], f.step(3), k));
    } else if let Some(e) = &f.error {
        c_ = c_.child(error_box(e, k));
    } else if let Some(d) = &f.data {
        if d["too_short"].as_bool().unwrap_or(false) {
            c_ = c_.child(empty("pencil", "Not enough content to review", "Text must be a minimum of 25 words to receive feedback.", k));
        } else {
            c_ = c_.child(p(str_of(d, "summary"), 17.0, k.text).mt(4.0).mb(18.0)).child(tb(format!("{} will remember", s.aud.0), 20.0, k.text).mt(8.0).mb(10.0));
            for (i, r) in d["remember"].as_array().cloned().unwrap_or_default().iter().enumerate() {
                c_ = c_.child(li(&(i + 1).to_string(), false, r.as_str().unwrap_or(""), None, k));
            }
            c_ = c_.child(tb(format!("{} might ask", s.aud.0), 20.0, k.text).mt(22.0).mb(10.0));
            for q in d["ask"].as_array().cloned().unwrap_or_default() {
                c_ = c_.child(li("?", true, q["question"].as_str().unwrap_or(""), q["label"].as_str(), k));
            }
        }
    } else {
        c_ = c_.child(row().mt(12.0).child(btn("predict", Btn::Primary, "Predict reactions", k).on_press(|s: &mut S| run_reactions(s))));
    }
    col().w_full().h_full().child(titlebar("", vec![title_btn("reload", "refresh", "Reload", k).on_press(|s: &mut S| run_reactions(s))], k)).child(scroll(c_))
}

// ---------------------------------------------------------------- Tone / Performance

fn tone(s: &S, m: &Model) -> El {
    let k = &s.tok;
    if m.doc.is_none() {
        return no_doc(s, m);
    }
    let f = s.feat("tone");
    let mut c_ = col().w_full();
    if f.loading && f.data.is_none() {
        c_ = c_.child(col().w_full().items_center().pt(60.0).gap(16.0).child(spinner(k, 22.0)).child(t("Detecting your tone.", 16.0, k.text_subdued)));
    } else if let Some(e) = &f.error {
        c_ = c_.child(error_box(e, k));
    } else if let Some(d) = f.data.as_ref().filter(|d| !d["too_short"].as_bool().unwrap_or(false)) {
        let mut l = col().w_full().gap(8.0).mt(18.0);
        for (i, tn) in d["tones"].as_array().cloned().unwrap_or_default().iter().enumerate() {
            l = l.child(row().w_full().gap(12.0).items_center().radius(12.0).px(16.0).py(12.0).border(1.0, c(k.border)).child(el().size(10.0, 10.0).radius(5.0).fill(c(k.brand).alpha((1.0 - i as f32 * 0.3).max(0.2)))).child(t(tn["name"].as_str().unwrap_or(""), 16.0, k.text)).child(rune::spacer()).child(t(format!("{:.0}%", tn["confidence"].as_f64().unwrap_or(0.0) * 100.0), 14.0, k.text_subdued)));
        }
        c_ = c_.child(l).child(p(str_of(d, "summary"), 15.0, k.text_subdued).mt(18.0));
    } else {
        c_ = c_.child(empty("tone", "We couldn't detect your tone yet", "Add more text to get tone insights.", k)).child(row().w_full().justify_center().mt(12.0).child(btn("detect", Btn::Primary, "Detect tone", k).on_press(|s: &mut S| s.run("tone", "tone", json!({})))));
    }
    col().w_full().h_full().child(titlebar("Tone insights", vec![title_btn("reload", "refresh", "Reload", k).on_press(|s: &mut S| s.run("tone", "tone", json!({})))], k)).child(scroll(c_))
}

fn performance(s: &S, m: &Model) -> El {
    let k = &s.tok;
    if m.doc.is_none() {
        return no_doc(s, m);
    }
    let sc = m.score.as_ref();
    let overall = sc.and_then(|x| x.overall).map_or("–".to_string(), |o| format!("{o:.0}"));
    let note = sc.and_then(|x| x.note.clone()).unwrap_or_else(|| "The score represents the quality of this text. Increase it by addressing issues.".into());
    let head = row().w_full().gap(18.0).items_start().pt(24.0).pb(22.0).border_sides([0.0, 0.0, 1.0, 0.0], c(k.border_subdued))
        .child(col().w(76.0).items_center().child(text(overall).font(ts(44.0, 600.0)).color(c(k.text))).child(t("of 100", 15.0, k.text_subdued)))
        .child(col().grow(1.0).min_w(0.0).gap(6.0).child(tb("Overall score", 20.0, k.text)).child(p(note, 16.0, k.text_subdued)));
    let mut c_ = col().w_full().child(head);
    if let Some(x) = sc {
        for (id, label, fill, val, n) in [("correctness", "Correctness", x.correctness_fill, &x.correctness_label, x.correctness_count), ("clarity", "Clarity", x.clarity_fill, &x.clarity_label, x.clarity_count), ("engagement", "Engagement", x.engagement_fill, &x.engagement_label, x.engagement_count), ("delivery", "Delivery", x.delivery_fill, &x.delivery_label, x.delivery_count)] {
            let colr = k.cat(id);
            let mut v = row().w(92.0).gap(6.0).items_center();
            if n == 0 {
                v = v.child(icon("check", k.brand, 16.0));
            }
            v = v.child(t(val.clone(), 15.0, k.text_subdued));
            c_ = c_.child(row().w_full().gap(12.0).items_center().py(12.0)
                .child(row().w(132.0).gap(10.0).items_center().child(el().size(12.0, 12.0).radius(6.0).fill(c(colr))).child(t(label, 16.0, k.text)))
                .child(meter(fill as f32 / 100.0, 4.0, colr, k).grow(1.0))
                .child(v));
        }
        c_ = c_.child(section_label("Text statistics", k));
        for (l, v) in [("Words", x.words.to_string()), ("Characters", x.characters.to_string()), ("Sentences", x.sentences.to_string()), ("Reading time", format!("{} min", (x.reading_time_sec as f64 / 60.0).round().max(1.0))), ("Readability (Flesch)", format!("{:.0}", x.flesch))] {
            c_ = c_.child(kv(t(l, 15.0, k.text), t(v, 15.0, k.text_subdued), k));
        }
    }
    col().w_full().h_full().child(titlebar("Performance", vec![], k)).child(scroll(c_))
}

// ---------------------------------------------------------------- Translate / Explain / Rewriter / Detector / Plagiarism

fn translate(s: &S, m: &Model, st: &crate::settings::Settings) -> El {
    let k = &s.tok;
    let f = s.feat("translate");
    let has_run = f.data.is_some() || f.error.is_some() || f.loading;
    let detected = f.data.as_ref().map(|d| str_of(d, "detected_language")).unwrap_or_default();
    let from_opts: Vec<(String, String)> = std::iter::once(AUTO.to_string()).chain(LANGUAGES.iter().map(|l| l.to_string())).map(|l| {
        let label = if l == AUTO && !detected.is_empty() { format!("{AUTO} ({detected})") } else { l.clone() };
        (l, label)
    }).collect();
    let to_opts: Vec<(String, String)> = LANGUAGES.iter().map(|l| (l.to_string(), l.to_string())).collect();
    let from = select("from", from_opts, &s.tr_from, k, |s: &mut S, v| {
        s.tr_from = v;
        if let Some(a) = s.tr_args.clone() {
            s.go_translate(a);
        }
    });
    let to = select("to", to_opts, &st.target_language, k, |s: &mut S, v| {
        let mut st = s.settings();
        st.target_language = v;
        s.save(st);
        if let Some(a) = s.tr_args.clone() {
            s.go_translate(a);
        }
    });
    let mut c_ = col().w_full().child(hero("globe", "Translate", None, k)).child(from.mt(14.0));
    if has_run {
        c_ = c_.child(col().w_full().mt(12.0).mb(12.0).max_h(160.0).radius(12.0).px(16.0).py(14.0).border(1.0, c(k.border)).scroll(rune::ScrollSpec { stick_bottom: false }).child(p(s.tr_original.clone(), 15.0, k.text_subdued)));
    } else {
        let msg = if m.doc.is_some() { "Translate this text field, or text you highlighted in any app." } else { "Highlight text in any app (a web page, a PDF, a chat), then translate it here." };
        let mut acts = row().w_full().gap(10.0).justify_center().flex_wrap().mt(12.0).mb(12.0);
        if let Some(d) = &m.doc {
            if !d.selected_text.is_empty() {
                acts = acts.child(btn("tr-sel", Btn::Primary, "Translate selection", k).on_press(|s: &mut S| run_doc_translate(s, false)));
            }
            acts = acts.child(btn("tr-all", if d.selected_text.is_empty() { Btn::Primary } else { Btn::Outline }, "Translate all", k).on_press(|s: &mut S| run_doc_translate(s, true)));
        }
        acts = acts.child(btn("tr-hl", if m.doc.is_some() { Btn::Outline } else { Btn::Primary }, "Translate highlighted text", k).on_press(|s: &mut S| s.translate_pending()));
        c_ = c_.child(empty("translate", "Ready to translate.", msg, k)).child(acts);
    }
    c_ = c_.child(to);
    if f.loading {
        c_ = c_.child(steps(&["Detecting language", "Translating"], f.step(2), k));
    } else if let Some(e) = &f.error {
        c_ = c_.child(error_box(e, k).mt(10.0)).child(row().mt(12.0).child(btn("back", Btn::Ghost, "Back", k).on_press(|s: &mut S| s.reset("translate"))));
    } else if let Some(d) = &f.data {
        let tr = str_of(d, "translation");
        let shown_target = d["target"].as_str().map(String::from).unwrap_or_else(|| st.target_language.clone());
        let base = |x: &str| x.split('(').next().unwrap_or("").trim().to_lowercase();
        if !detected.is_empty() && base(&detected) == base(&shown_target) && tr.trim() == s.tr_original.trim() {
            c_ = c_.child(notice(format!("This text is already in {detected}. Pick another language above or below and it translates again."), false, k).mt(10.0));
        }
        c_ = c_.child(rewrite_result(s, "translate", &s.tr_original, &tr, d["used_selection"].as_bool().unwrap_or(false), !s.tr_highlight && m.doc.is_some()));
        c_ = c_.child(row().mt(12.0).child(btn("again", Btn::Ghost, "Translate again", k).on_press(|s: &mut S| {
            if let Some(a) = s.tr_args.clone() {
                s.go_translate(a);
            }
        })));
        for id in d["idioms"].as_array().cloned().unwrap_or_default() {
            let mut cd = col().w_full().gap(4.0).child(tb(format!("“{}”", id["text"].as_str().unwrap_or("")), 15.0, k.text));
            for (l, key) in [("Meaning", "meaning"), ("Origin", "origin"), ("Example", "example")] {
                let v = id[key].as_str().unwrap_or("");
                if !v.is_empty() && v != "null" {
                    cd = cd.child(p(format!("{l}  {v}"), 14.0, k.text_subdued));
                }
            }
            c_ = c_.child(card(cd, k).mt(12.0));
        }
    } else {
        c_ = c_.child(col().w_full().mt(12.0).radius(12.0).px(16.0).py(14.0).border(1.0, c(k.border)).child(t("The translation appears here.", 15.0, k.text_subdued)));
    }
    c_ = c_.child(p("From anywhere: drag a file with Utsuri, tap Shift, drop it on Translate. Or run oumu --translate to translate what you selected.", 12.0, k.text_subdued).align(rune::Align::Center).mt(14.0));
    col().w_full().h_full().child(titlebar("", vec![title_btn("info", "info", "Info", k).on_press(|s: &mut S| s.say("Translation runs through your configured model."))], k)).child(scroll(c_))
}

fn run_doc_translate(s: &mut S, whole: bool) {
    let d = s.model.lock().unwrap().doc.clone();
    s.tr_highlight = false;
    s.tr_from = AUTO.into();
    s.tr_original = d.map(|d| if whole { d.text } else { doc_src(&d) }).unwrap_or_default();
    s.go_translate(if whole { json!({ "whole": true }) } else { json!({}) });
}

fn explain(s: &S, m: &Model) -> El {
    let k = &s.tok;
    let Some(doc) = &m.doc else { return no_doc(s, m) };
    let f = s.feat("explain");
    let mut c_ = col().w_full().child(hero("book", "Explain It", None, k));
    if f.loading {
        c_ = c_.child(steps(&["Reading", "Explaining"], f.step(2), k));
    } else if let Some(e) = &f.error {
        c_ = c_.child(error_box(e, k));
    } else if let Some(d) = &f.data {
        c_ = c_
            .child(section_label(if d["used_selection"].as_bool().unwrap_or(false) { "Your selection, explained" } else { "The whole page, explained" }, k))
            .child(p(str_of(d, "summary"), 17.0, k.text))
            .child(col().w_full().mt(12.0).radius(12.0).px(16.0).py(14.0).border(1.0, c(k.border)).child(p(str_of(d, "explanation"), 15.0, k.text).selectable(true)));
        for tm in d["terms"].as_array().cloned().unwrap_or_default() {
            c_ = c_.child(card(col().w_full().gap(2.0).child(text("TERM").font(rune::tok::TextStyle { tracking: 1.0, ..ts(11.0, 700.0) }).color(c(k.text_subdued))).child(tb(tm["term"].as_str().unwrap_or(""), 16.0, k.text)).child(p(tm["definition"].as_str().unwrap_or(""), 14.0, k.text_subdued)), k).mt(10.0));
        }
        c_ = c_.child(row().mt(12.0).child(btn("back", Btn::Ghost, "Back", k).on_press(|s: &mut S| s.reset("explain"))));
    } else {
        let mut acts = row().w_full().gap(10.0).justify_center().mt(12.0);
        if !doc.selected_text.is_empty() {
            acts = acts.child(btn("ex-sel", Btn::Primary, "Explain selection", k).on_press(|s: &mut S| s.run("explain", "explain", json!({}))));
        }
        acts = acts.child(btn("ex-all", if doc.selected_text.is_empty() { Btn::Primary } else { Btn::Outline }, "Explain the whole page", k).on_press(|s: &mut S| s.run("explain", "explain", json!({ "whole": true }))));
        c_ = c_.child(empty("glasses", "Spot something confusing?", "Highlight text for a quick explainer, or explain the whole page in one click.", k)).child(acts);
    }
    col().w_full().h_full().child(titlebar("", vec![title_btn("info", "info", "Info", k).on_press(|s: &mut S| s.say("Explain It uses your configured model."))], k)).child(scroll(c_))
}

fn rescan(s: &mut S) {
    s.rw_done.clear();
    s.rw_sugg.clear();
    s.run("rewriter", "rewriter_scan", json!({}));
}

fn rewriter(s: &S, m: &Model) -> El {
    let k = &s.tok;
    if m.doc.is_none() {
        return no_doc(s, m);
    }
    let f = s.feat("rewriter");
    let mut c_ = col().w_full().child(
        col().w_full().pt(28.0).child(icon("bolt", k.text, 56.0)).child(row().gap(10.0).items_center().pt(18.0).pb(14.0).child(text("AI Rewriter").font(ts(28.0, 600.0)).color(c(k.text))).child(title_btn("about", "info", "About", k).on_press(|s: &mut S| s.say("AI Rewriter finds words and phrases AI uses frequently, which can also appear in human writing. It suggests alternatives AI uses less often.")))),
    );
    if f.loading && f.data.is_none() {
        c_ = c_.child(steps(&["Scanning your text", "Checking common AI phrases", "Generating report"], f.step(3), k));
    } else if let Some(e) = &f.error {
        c_ = c_.child(error_box(e, k));
    } else if let Some(d) = f.data.as_ref().filter(|d| !d["too_short"].as_bool().unwrap_or(false)) {
        let all = d["phrases"].as_array().cloned().unwrap_or_default();
        if all.is_empty() {
            c_ = c_.child(empty("check", "No AI phrases found", "I didn't find any common AI writing phrases.", k));
        } else {
            let total = all.len();
            let reviewed = s.rw_done.len();
            let mut counts: HashMap<String, usize> = HashMap::new();
            for ph in &all {
                if !s.rw_done.contains(&str_of(ph, "id")) {
                    *counts.entry(str_of(ph, "bucket")).or_default() += 1;
                }
            }
            c_ = c_
                .child(section_label(&format!("AI phrases reviewed · {reviewed}/{total}"), k))
                .child(meter(reviewed as f32 / total as f32, 6.0, k.brand, k).mt(8.0).mb(14.0))
                .child(p("I found words and phrases that AI uses frequently. Check the highlighted sections for rewritten versions.", 15.0, k.text_subdued))
                .child(section_label("Phrases more frequent in AI text", k));
            let mut filt = row().gap(8.0).my(12.0).flex_wrap();
            for (id, label) in [("all", "All AI phrases".to_string()), ("frequent", format!("Almost always AI · {}", counts.get("frequent").copied().unwrap_or(0))), ("often", format!("Usually AI · {}", counts.get("often").copied().unwrap_or(0))), ("sometimes", format!("Sometimes AI · {}", counts.get("sometimes").copied().unwrap_or(0)))] {
                let on = s.rw_filter == id;
                let id_s = id.to_string();
                let b = row().key(format!("filter/{id}")).radius(16.0).px(12.0).py(6.0).border(1.0, c(k.border)).cursor(rune::Cursor::Pointer).child(t(label, 13.0, k.text)).on_press(move |s: &mut S| s.rw_filter = id_s.clone());
                filt = filt.child(if on { b.fill(c(k.subdued)) } else { b });
            }
            c_ = c_.child(filt);
            let shown: Vec<&Value> = all.iter().filter(|ph| !s.rw_done.contains(&str_of(ph, "id")) && (s.rw_filter == "all" || str_of(ph, "bucket") == s.rw_filter)).collect();
            if shown.is_empty() {
                c_ = c_.child(empty("check", "All phrases reviewed", "You addressed all AI phrases found in your doc.", k));
            }
            for ph in shown {
                c_ = c_.child(phrase(s, ph, k));
            }
        }
    } else {
        c_ = c_.child(empty("bolt", "AI Rewriter is ready to go", "Write or paste text to see common AI phrases.", k)).child(row().w_full().justify_center().mt(12.0).child(btn("scan", Btn::Primary, "Scan my text", k).on_press(|s: &mut S| rescan(s))));
    }
    col().w_full().h_full().child(titlebar("", vec![title_btn("reload", "refresh", "Reload", k).on_press(|s: &mut S| rescan(s))], k)).child(scroll(c_))
}

fn phrase(s: &S, ph: &Value, k: &Tok) -> El {
    let id = str_of(ph, "id");
    let phrase = str_of(ph, "phrase");
    let sentence = str_of(ph, "sentence");
    let freq = match str_of(ph, "bucket").as_str() {
        "frequent" => "Over 50× frequency",
        "often" => "10-49× frequency",
        _ => "Less than 10× frequency",
    };
    let at = sentence.to_lowercase().find(&phrase.to_lowercase()).unwrap_or(0);
    let marked = text(sentence.clone()).wrap().w_full().font(ts(14.0, 420.0)).color(c(k.text_subdued)).spans(vec![rune::text::Span::new(at, (at + phrase.len()).min(sentence.len())).color(c(k.warn_ink())).bg(c(k.mark_bg()), 2.0)]);
    let mut box_ = col().key(format!("phrase/{id}")).w_full().radius(12.0).px(14.0).py(12.0).mb(10.0).gap(6.0).border(1.0, c(k.border))
        .child(col().w_full().gap(2.0).child(tb(phrase.clone(), 14.0, k.text)).child(t(format!("{} · {freq}", str_of(ph, "bucket_label")), 12.0, k.text_subdued)))
        .child(marked);
    match s.rw_sugg.get(&id) {
        Some(None) => box_ = box_.child(spinner(k, 22.0).mt(10.0)),
        Some(Some(list)) => {
            let mut l = col().w_full().gap(6.0).mt(10.0);
            for x in list {
                let (x2, sen, id2) = (x.clone(), sentence.clone(), id.clone());
                l = l.child(list_opt(&format!("sugg/{id}/{x}"), false, k).px(12.0).py(10.0).mb(0.0).child(p(x.clone(), 14.0, k.text)).on_press(move |s: &mut S| {
                    let d = s.model.lock().unwrap().doc.clone();
                    let Some(d) = d else { return };
                    match d.text.find(&sen) {
                        Some(b) => {
                            let start = d.text[..b].chars().count();
                            match crate::commands::replace_range_inner(&s.app, &s.shared, start, start + sen.chars().count(), &x2) {
                                Ok(_) => {
                                    s.rw_done.insert(id2.clone());
                                    s.say("Replaced");
                                }
                                Err(e) => s.say(e),
                            }
                        }
                        None => s.say("The sentence changed. Rescan."),
                    }
                }));
            }
            box_ = box_.child(l);
        }
        None => {}
    }
    let mut acts = row().gap(10.0).mt(10.0);
    if !matches!(s.rw_sugg.get(&id), Some(Some(_))) {
        let (id2, ph2, se2) = (id.clone(), phrase.clone(), sentence.clone());
        acts = acts.child(btn("rewrite", Btn::Primary, "Rewrite", k).px(12.0).py(7.0).on_press(move |s: &mut S| {
            s.rw_sugg.insert(id2.clone(), None);
            let (app, shared, me, id3) = (s.app.clone(), s.shared.clone(), s.me.clone(), id2.clone());
            let args = json!({ "sentence": se2, "phrase": ph2 });
            tauri::async_runtime::spawn(async move {
                let r = crate::commands::run_feature_inner(&app, &shared, "rewrite_phrase", args).await;
                if let Some(me) = me {
                    me.update(move |s, _| match r {
                        Ok(v) => {
                            s.rw_sugg.insert(id3, Some(v["suggestions"].as_array().cloned().unwrap_or_default().iter().filter_map(|x| x.as_str().map(String::from)).collect()));
                        }
                        Err(e) => {
                            s.rw_sugg.remove(&id3);
                            s.say(e);
                        }
                    });
                }
            });
        }));
    }
    let id4 = id.clone();
    acts = acts.child(btn("dismiss", Btn::Ghost, "Dismiss", k).px(12.0).py(7.0).on_press(move |s: &mut S| {
        s.rw_done.insert(id4.clone());
    }));
    box_.child(acts)
}

#[allow(clippy::too_many_arguments)]
fn uphook(ic: &str, title: &str, items: &[&str], bullet: &str, key: &'static str, feature: &'static str, bordered: bool, k: &Tok) -> El {
    let mut list = col().gap(4.0).mb(26.0);
    for it in items {
        list = list.child(row().gap(12.0).items_center().py(6.0).child(row().size(22.0, 22.0).radius(6.0).center().fill(c(k.brand)).child(icon(bullet, k.on_brand, 14.0))).child(t(*it, 16.0, k.text_subdued)));
    }
    let b = col().w_full().items_center().mt(10.0).px(24.0).pt(36.0).pb(28.0).radius(16.0)
        .child(icon(ic, k.text, 72.0))
        .child(text(title).wrap().w_full().font(ts(26.0, 600.0)).color(c(k.text)).align(rune::Align::Center).mt(18.0).mb(26.0))
        .child(list)
        .child(btn("check", Btn::Premium, "Check this text", k).on_press(move |s: &mut S| s.run(key, feature, json!({}))));
    if bordered { b.border(1.0, c(k.border)) } else { b }
}

fn detector(s: &S, m: &Model) -> El {
    let k = &s.tok;
    if m.doc.is_none() {
        return no_doc(s, m);
    }
    let f = s.feat("detector");
    let mut c_ = col().w_full().child(row().w_full().h(44.0).gap(8.0).items_center().justify_center().child(icon("scanner", k.text, 22.0)).child(tb("AI Detector", 15.0, k.text)));
    if f.loading {
        c_ = c_.child(steps(&["Scanning your text", "Measuring rhythm and vocabulary", "Checking patterns"], f.step(3), k));
    } else if let Some(e) = &f.error {
        c_ = c_.child(error_box(e, k));
    } else if let Some(d) = &f.data {
        if d["too_short"].as_bool().unwrap_or(false) {
            c_ = c_.child(empty("pencil", "Write a little more", "To receive feedback, your text must be 8 words or longer.", k));
        } else {
            let auth = d["authenticity"].as_f64().unwrap_or(0.0);
            let ai = d["ai_likelihood"].as_f64().unwrap_or(0.0);
            c_ = c_
                .child(row().w_full().gap(18.0).items_start().pt(24.0).pb(16.0).child(col().w(76.0).items_center().child(text(format!("{auth:.0}")).font(ts(44.0, 600.0)).color(c(k.text))).child(t("authentic", 15.0, k.text_subdued))).child(col().grow(1.0).min_w(0.0).gap(6.0).child(tb(str_of(d, "verdict"), 20.0, k.text)).child(p("Some parts of your text may contain patterns that resemble AI text. These can also occur in your writing. If you use generative AI, you can cite it.", 16.0, k.text_subdued))))
                .child(meter((auth / 100.0) as f32, 6.0, if ai > 55.0 { k.warning } else { k.brand }, k).mt(8.0).mb(14.0));
            for r in d["reasons"].as_array().cloned().unwrap_or_default() {
                c_ = c_.child(p(format!("•  {}", r.as_str().unwrap_or("")), 15.0, k.text_subdued));
            }
            let most = str_of(d, "most_ai_sentence");
            if !most.is_empty() {
                c_ = c_.child(col().w_full().mt(12.0).radius(12.0).px(16.0).py(14.0).border(1.0, c(k.border)).child(p(format!("“{most}”"), 15.0, k.text)));
            }
            let sg = &d["signals"];
            c_ = c_
                .child(section_label("Signals", k))
                .child(kv(t("Sentence rhythm (burstiness)", 15.0, k.text), t(format!("{:.2}", sg["burstiness"].as_f64().unwrap_or(0.0)), 15.0, k.text_subdued), k))
                .child(kv(t("Average sentence length", 15.0, k.text), t(format!("{:.1} words", sg["avg_sentence_len"].as_f64().unwrap_or(0.0)), 15.0, k.text_subdued), k))
                .child(kv(t("AI phrases per 100 words", 15.0, k.text), t(format!("{:.2}", sg["ai_phrases_per_100w"].as_f64().unwrap_or(0.0)), 15.0, k.text_subdued), k))
                .child(kv(t("Human markers", 15.0, k.text), t(sg["human_markers"].to_string(), 15.0, k.text_subdued), k))
                .child(notice(format!("Method: {}.", str_of(d, "method")), true, k).mt(14.0))
                .child(row().mt(12.0).child(btn("again", Btn::Outline, "Check again", k).on_press(|s: &mut S| s.run("detector", "detector", json!({})))));
        }
    } else {
        c_ = c_.child(uphook("scanner", "You're a true original.\nEnsure your work is, too.", &["Detect AI generated text", "Get a text authenticity score", "See which sentences stand out"], "check", "detector", "detector", true, k));
    }
    col().w_full().h_full().child(titlebar("", vec![title_btn("history", "clock", "History", k).on_press(|s: &mut S| s.view = "history".into())], k)).child(scroll(c_))
}

fn plagiarism(s: &S, m: &Model, st: &crate::settings::Settings) -> El {
    let k = &s.tok;
    if m.doc.is_none() {
        return no_doc(s, m);
    }
    let f = s.feat("plagiarism");
    let style = if st.citation_style.is_empty() { "APA".to_string() } else { st.citation_style.clone() };
    let styles: Vec<(String, String)> = ["APA", "MLA", "Chicago"].iter().map(|x| (x.to_string(), x.to_string())).collect();
    let head = row().w_full().items_center().justify_between().pt(10.0).pb(4.0).child(tb("Plagiarism and AI check", 20.0, k.text)).child(select("cite", styles, &style, k, |s: &mut S, v| {
        let mut st = s.settings();
        st.citation_style = v;
        s.save(st);
    }).w(130.0).py(6.0));
    let mut c_ = col().w_full().child(head);
    if f.loading {
        c_ = c_.child(steps(&["Picking distinctive phrases", "Searching the web", "Comparing sources", "Building citations"], f.step(4), k));
    } else if let Some(e) = &f.error {
        c_ = c_.child(error_box(e, k));
    } else if let Some(d) = &f.data {
        if d["too_short"].as_bool().unwrap_or(false) {
            c_ = c_.child(empty("pencil", "Write a little more", "Plagiarism checks need at least 20 words.", k));
        } else {
            let sources = d["sources"].as_array().cloned().unwrap_or_default();
            let searches = d["searches"].as_u64().unwrap_or(d["queries"].as_array().map_or(0, |q| q.len() as u64));
            let via = match d["search_via"].as_str() {
                Some("brave") => " Searched with your Brave key.",
                Some("searxng") => " Searched with your SearXNG.",
                _ => "",
            };
            let err = d["search_error"].as_str().map(|e| format!(" {e} Used the free search instead.")).unwrap_or_default();
            let title = if sources.is_empty() { "No matches found".to_string() } else { format!("{} source{} with matching text", sources.len(), if sources.len() == 1 { "" } else { "s" }) };
            c_ = c_.child(row().w_full().gap(18.0).items_start().pt(10.0).pb(16.0)
                .child(col().w(76.0).items_center().child(text(format!("{:.0}%", d["overall_pct"].as_f64().unwrap_or(0.0))).font(ts(44.0, 600.0)).color(c(k.text))).child(t("matched", 15.0, k.text_subdued)))
                .child(col().grow(1.0).min_w(0.0).gap(6.0).child(tb(title, 20.0, k.text)).child(p(format!("Checked {} words against the web with {searches} searches.{via}{err}", d["words"].as_u64().unwrap_or(0)), 16.0, k.text_subdued))));
            for src in sources {
                let cite = src["citations"][style.as_str()].as_str().or(src["citations"]["APA"].as_str()).unwrap_or("").to_string();
                let url = str_of(&src, "url");
                let (u2, c2) = (url.clone(), cite.clone());
                let title = if src["title"].as_str().unwrap_or("").is_empty() { str_of(&src, "site") } else { str_of(&src, "title") };
                let mut b = col().w_full().radius(12.0).px(14.0).py(12.0).mb(10.0).gap(4.0).border(1.0, c(k.border))
                    .child(row().w_full().justify_between().gap(8.0).child(tb(title, 15.0, k.text).grow(1.0).min_w(0.0)).child(tb(format!("{:.1}%", src["matched_pct"].as_f64().unwrap_or(0.0)), 15.0, k.plagiarism)))
                    .child(text(url).wrap_anywhere().w_full().font(ts(13.0, 420.0)).color(c(k.link)).cursor(rune::Cursor::Pointer).on_press(move |_s: &mut S| crate::commands::open_url(u2.clone())));
                let snip = str_of(&src, "snippet");
                if !snip.is_empty() {
                    b = b.child(p(format!("“…{snip}…”"), 14.0, k.text_subdued).my(6.0));
                }
                b = b.child(col().w_full().radius(8.0).px(10.0).py(8.0).mt(6.0).fill(c(k.bg_sunken())).child(p(cite, 13.0, k.text).selectable(true)))
                    .child(row().mt(8.0).child(btn("cite", Btn::Outline, "Copy citation", k).px(12.0).py(6.0).on_press_tx(move |s: &mut S, tx: &mut rune::Tx| {
                        tx.copy(c2.clone());
                        s.say("Citation copied");
                    })));
                c_ = c_.child(b);
            }
            c_ = c_.child(row().mt(12.0).child(btn("again", Btn::Outline, "Check again", k).on_press(|s: &mut S| s.run("plagiarism", "plagiarism", json!({})))));
        }
    } else {
        c_ = c_.child(uphook("incognito", "Check for plagiarism\nbefore you submit", &["Catch missing citations", "Detect text matches", "Create citations quickly"], "arrow", "plagiarism", "plagiarism", false, k));
    }
    col().w_full().h_full().child(titlebar("Plagiarism", vec![], k)).child(scroll(c_))
}

// ---------------------------------------------------------------- Chat

fn chat(s: &S, m: &Model, st: &crate::settings::Settings) -> El {
    let k = &s.tok;
    let mut body = col().key("chat-body").w_full().grow(1.0).min_h(0.0).px(16.0).scroll(rune::ScrollSpec { stick_bottom: true });
    if s.chat.is_empty() {
        let first = st.user_name.split_whitespace().next().unwrap_or("Hey").to_string();
        let mut chips = col().gap(10.0).items_start().pb(14.0).child(btn_icon("summarize", Btn::Outline, "chat", k.brand, "Summarize this page", k).on_press(|s: &mut S| send_chat(s, "Summarize this page.".into())));
        if !m.alerts.is_empty() {
            chips = chips.child(btn_icon("review", Btn::Outline, "mark", k.brand, &format!("Review {} suggestions", m.alerts.len()), k).on_press(|s: &mut S| s.view = "review".into()));
        }
        body = body.child(hero("chat", "Oumu AI Chat", None, k)).child(rune::spacer()).child(t(format!("{first}, how can I help?"), 18.0, k.text).pb(12.0)).child(chips);
    } else {
        for (role, content, pending) in &s.chat {
            if role == "user" {
                body = body.child(row().w_full().justify_end().my(6.0).child(col().max_w(320.0).radius(14.0).px(14.0).py(10.0).fill(c(k.subdued)).child(text(content.clone()).wrap().font(ts(15.0, 420.0)).color(c(k.text)).selectable(true))));
            } else if *pending && content.is_empty() {
                body = body.child(row().py(10.0).child(spinner(k, 14.0)));
            } else {
                body = body.child(col().w_full().py(10.0).child(p(content.clone(), 15.0, k.text).selectable(true)));
            }
        }
    }
    let mut composer = col().mx(16.0).mb(16.0).radius(16.0).padding(12.0).border(1.0, c(k.border));
    if let Some(d) = &m.doc {
        let words = d.text.split_whitespace().count();
        composer = composer.child(row().gap(10.0).items_center().radius(12.0).px(12.0).py(8.0).mb(10.0).border(1.0, c(k.border))
            .child(row().size(36.0, 36.0).radius(10.0).center().fill(c(k.warning)).child(icon("pencil", k.bg, 20.0)))
            .child(col().child(tb(if d.app_name.is_empty() { "Document".to_string() } else { d.app_name.clone() }, 14.0, k.text)).child(t(if d.selected_text.is_empty() { format!("{words} words") } else { "Selection".into() }, 14.0, k.text_subdued))));
    }
    let busy = s.chat_req.is_some();
    let can_send = !s.chat_input.text.trim().is_empty() && !busy;
    let input = rune_kit::TextInput::new(&s.chat_input).placeholder("Ask AI").width(None).on_edit(|s: &mut S| &mut s.chat_input).on_submit(|s: &mut S, _| {
        let t = s.chat_input.text.clone();
        send_chat(s, t);
    }).build(&s.kit).key("chat-input").w_full();
    let send = row().key("send").size(36.0, 36.0).radius(18.0).center().fill(c(if can_send { k.brand_strong } else { k.brand_subdued() })).cursor(rune::Cursor::Pointer).child(icon("arrow-up", if can_send { k.on_brand } else { k.text_muted }, 20.0));
    let send = if can_send {
        send.on_press(|s: &mut S| {
            let t = s.chat_input.text.clone();
            send_chat(s, t);
        })
    } else {
        send
    };
    composer = composer.child(input).child(row().w_full().items_center().justify_between().mt(6.0).child(title_btn("attach", "plus", "Attach the current selection", k).on_press(|s: &mut S| {
        let sel = s.model.lock().unwrap().doc.as_ref().map(|d| d.selected_text.clone()).unwrap_or_default();
        if !sel.is_empty() {
            let mut t = s.chat_input.text.clone();
            if !t.is_empty() {
                t.push_str("\n\n");
            }
            t.push_str(&sel);
            s.chat_input = TextEdit::new(t);
        }
    })).child(send));
    col().w_full().h_full().child(row().w_full().h(44.0).items_center().justify_end().pr(12.0).child(title_btn("new-chat", "edit", "New chat", k).on_press(|s: &mut S| {
        s.chat.clear();
        s.chat_req = None;
    }))).child(body).child(composer)
}

fn send_chat(s: &mut S, text: String) {
    let t = text.trim().to_string();
    if t.is_empty() || s.chat_req.is_some() {
        return;
    }
    s.chat.push(("user".into(), t, false));
    let messages: Vec<Value> = s.chat.iter().map(|(r, c, _)| json!({ "role": r, "content": c })).collect();
    s.chat.push(("assistant".into(), String::new(), true));
    s.chat_input = TextEdit::new("");
    let id = format!("r{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis());
    s.chat_req = Some(id.clone());
    let (app, me) = (s.app.clone(), s.me.clone());
    tauri::async_runtime::spawn(async move {
        let r = crate::commands::chat_send(app.clone(), app.state::<Shared>(), id, Value::Array(messages)).await;
        if let (Err(e), Some(me)) = (r, me) {
            me.update(move |s, _| {
                if let Some(last) = s.chat.last_mut() {
                    *last = ("assistant".into(), format!("Error: {e}"), false);
                }
                s.chat_req = None;
            });
        }
    });
}

/// llm://chunk for the chat reply streaming in.
pub fn chat_chunk(s: &mut S, req: &str, delta: &str, done: bool, error: Option<String>) {
    if s.chat_req.as_deref() != Some(req) {
        return;
    }
    if let Some(last) = s.chat.last_mut().filter(|l| l.0 == "assistant") {
        match &error {
            Some(e) => last.1.push_str(&format!("\n\nError: {e}")),
            None => last.1.push_str(delta),
        }
        last.2 = !done;
    }
    if done {
        s.chat_req = None;
    }
}

// ---------------------------------------------------------------- History / Onboarding / Settings

fn history(s: &S) -> El {
    let k = &s.tok;
    let h = serde_json::to_value(&s.shared.lock().unwrap().history).unwrap_or_default();
    let list = h.as_array().cloned().unwrap_or_default();
    let mut c_ = col().w_full();
    if list.is_empty() {
        c_ = c_.child(empty("clock", "Nothing yet", "Documents you write in show up here.", k));
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs_f64();
    for x in list.iter().rev().take(100) {
        let ago = now - x["last_seen"].as_f64().unwrap_or(now);
        let rel = if ago < 60.0 { "just now".to_string() } else if ago < 3600.0 { format!("{} min ago", (ago / 60.0) as u64) } else if ago < 86400.0 { format!("{} h ago", (ago / 3600.0) as u64) } else { format!("{} d ago", (ago / 86400.0) as u64) };
        let score = x["score"].as_f64().map(|v| format!(" · score {v:.0}")).unwrap_or_default();
        c_ = c_.child(col().w_full().py(12.0).border_sides([0.0, 0.0, 1.0, 0.0], c(k.border_subdued)).child(tb(x["title"].as_str().unwrap_or("Untitled"), 15.0, k.text)).child(t(format!("{} · {} words{score} · {rel}", x["app_name"].as_str().unwrap_or(""), x["words"].as_u64().unwrap_or(0)), 13.0, k.text_subdued)));
    }
    col().w_full().h_full().child(titlebar("History", vec![], k)).child(scroll(c_))
}

fn onboarding(s: &S, st: &crate::settings::Settings) -> El {
    let k = &s.tok;
    scroll(col().w_full()
        .child(hero("sparkle", "Choose how Oumu thinks", Some("Spelling and grammar run on this computer. The sidebar's AI tools (chat, paraphrase, spot check, tone and more) need a model. Pick one now; you can change it any time in Settings."), k))
        .child(ai_setup(s, st, true).mt(10.0)))
}

fn finish_onboarding(s: &mut S) {
    let mut st = s.settings();
    if !st.onboarded {
        st.onboarded = true;
        s.save(st);
    }
    s.view = "review".into();
}

/// .choice
fn choice(key: &str, ic: &str, title: &str, desc: &str, selected: bool, k: &Tok) -> El {
    col().key(key).w_full().radius(14.0).padding(16.0).mb(12.0).fill(c(if selected { k.brand_tint_soft() } else { k.bg })).border(1.0, c(if selected { k.brand } else { k.border })).hover(c(k.bg_hover())).cursor(rune::Cursor::Pointer)
        .child(row().gap(10.0).items_center().child(icon(ic, k.text, 20.0)).child(tb(title, 17.0, k.text)))
        .child(p(desc, 14.0, k.text_subdued).mt(6.0))
}

fn key_kind(key: &str) -> &'static str {
    let k = key.trim();
    if k.is_empty() {
        ""
    } else if k.starts_with("sk-ant-") {
        "Anthropic key"
    } else if k.starts_with("sk-or-") {
        "OpenRouter key"
    } else if k.starts_with("gsk_") {
        "Groq key"
    } else if k.starts_with("AIza") || k.starts_with("AQ.") {
        "Google AI key"
    } else if k.starts_with("sk-") {
        "OpenAI key"
    } else {
        "API key"
    }
}

/// AiSetup: one key field (provider and model detected), a local model, and Advanced.
fn ai_setup(s: &S, st: &crate::settings::Settings, onboarding: bool) -> El {
    let k = &s.tok;
    let llm = &st.llm;
    let local = llm.provider == crate::settings::Provider::Ollama;
    let connected = !local && llm.provider != crate::settings::Provider::None && !llm.api_key.trim().is_empty();
    let open = s.ai_open.unwrap_or(!connected && !local);
    let mut c_ = col().w_full();
    let mut key_choice = choice("ai-key", "lock", "Your AI key", "Google Gemini, OpenAI, Anthropic, OpenRouter or Groq. Paste the key; the provider and model are picked for you.", connected, k);
    if connected {
        key_choice = key_choice.child(t(format!("Connected · {}", llm.model), 13.0, k.ok_ink()).mt(8.0));
    }
    c_ = c_.child(key_choice.on_press(move |s: &mut S| s.ai_open = Some(!open)));
    if open {
        let kind = key_kind(&s.key.text);
        let mut cd = col().w_full().child(field_ph("key", if kind.is_empty() { "API key" } else { kind }, "AQ.… / AIza… / sk-…", &s.key, true, &s.kit, k, |s: &mut S| &mut s.key));
        if let Some(e) = &s.key_err {
            cd = cd.child(notice(e.clone(), false, k).mb(10.0));
        }
        let off = s.key_busy || s.key.text.trim().is_empty();
        let connect = disabled(btn("connect", Btn::Primary, if s.key_busy { "Checking the key…" } else { "Connect" }, k), off);
        let connect = if off { connect } else { connect.on_press(move |s: &mut S| connect_key(s, onboarding)) };
        let mut acts = row().gap(10.0).child(connect);
        if connected {
            acts = acts.child(btn("cancel-key", Btn::Ghost, "Cancel", k).on_press(|s: &mut S| {
                s.ai_open = Some(false);
                s.key_err = None;
            }));
        }
        cd = cd.child(acts).child(p("The key stays on this computer. Your account's models are listed and the best fit is picked; change it under Advanced. A free key: aistudio.google.com/apikey.", 12.0, k.text_muted).mt(6.0));
        c_ = c_.child(card(cd, k).mb(12.0));
    }
    let mut lc = choice("ai-local", "lock", "Local model (optional)", "Runs on this computer with Ollama: private, no key, no daily limit. Slower than a cloud model, and it needs a few GB of disk and GPU memory.", local, k);
    if local {
        lc = lc.child(t(format!("Using · {}", llm.model), 13.0, k.ok_ink()).mt(8.0));
    }
    c_ = c_.child(lc.on_press(|s: &mut S| {
        s.local_open = !s.local_open;
        if s.local_open {
            check_local(s);
        }
    }));
    if s.local_open {
        let mut cd = col().w_full().gap(8.0);
        match &s.local {
            None => cd = cd.child(t("Looking for Ollama…", 13.0, k.text_subdued)),
            Some(v) if !v["running"].as_bool().unwrap_or(false) => {
                cd = cd.child(notice("Ollama is not running on this computer.", false, k)).child(p("Install it from ollama.com and download a model (about 6 GB), then press Check again: ollama pull qwen3.5:9b", 13.0, k.text_subdued));
            }
            Some(v) => {
                let models: Vec<(String, String)> = v["models"].as_array().cloned().unwrap_or_default().iter().filter_map(|m| m.as_str().map(|x| (x.to_string(), x.to_string()))).collect();
                if models.is_empty() {
                    cd = cd.child(notice("Ollama is running but has no chat model. Run ollama pull qwen3.5:9b, then press Check again.", false, k));
                } else {
                    cd = cd.child(t("Model", 13.0, k.text_subdued)).child(select("local-model", models, &s.local_model, k, |s: &mut S, v| s.local_model = v));
                    let in_use = local && s.local_model == llm.model;
                    let off = in_use || s.local_model.is_empty();
                    let b = disabled(btn("use-local", Btn::Primary, if in_use { "In use" } else { "Use this model" }, k), off);
                    cd = cd.child(row().child(if off { b } else { b.on_press(move |s: &mut S| use_local(s, onboarding)) }));
                }
            }
        }
        cd = cd.child(row().child(btn("check-local", Btn::Ghost, "Check again", k).on_press(|s: &mut S| check_local(s))));
        c_ = c_.child(card(cd, k).mb(12.0));
    }
    if onboarding {
        c_ = c_.child(row().w_full().justify_center().mt(18.0).child(btn("skip", Btn::Ghost, "Skip for now · spelling and grammar work without AI", k).on_press(|s: &mut S| finish_onboarding(s))));
    }
    c_ = c_.child(row().key("advanced").gap(8.0).items_center().mt(18.0).mb(10.0).cursor(rune::Cursor::Pointer).child(icon(if s.adv_open { "chevron-down" } else { "chevron-right" }, k.text_subdued, 14.0)).child(t("Advanced", 14.0, k.text_subdued)).on_press(|s: &mut S| s.adv_open = !s.adv_open));
    if s.adv_open {
        let provs: Vec<(String, String)> = [("none", "Not set up"), ("gemini", "Google Gemini"), ("openai", "OpenAI-compatible"), ("anthropic", "Anthropic"), ("ollama", "Local (Ollama)")].iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
        let cur = serde_json::to_value(&llm.provider).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_else(|| "none".into());
        c_ = c_
            .child(t("Provider", 13.0, k.text_subdued).pb(6.0))
            .child(select("provider", provs, &cur, k, |s: &mut S, v| {
                let mut st = s.settings();
                let (base, model) = match v.as_str() {
                    "gemini" => ("https://generativelanguage.googleapis.com/v1beta", "gemini-3.6-flash"),
                    "openai" => ("https://api.openai.com", "gpt-4o-mini"),
                    "anthropic" => ("https://api.anthropic.com", "claude-sonnet-5-5"),
                    "ollama" => ("http://127.0.0.1:11434", "qwen3.5:9b"),
                    _ => ("", ""),
                };
                st.llm.provider = serde_json::from_value(json!(v)).unwrap_or(crate::settings::Provider::None);
                st.llm.base_url = base.into();
                st.llm.model = model.into();
                s.adv_base = TextEdit::new(base);
                s.adv_model = TextEdit::new(model);
                s.save(st);
            }).mb(14.0))
            .child(field("adv-base", "Base URL", &s.adv_base, false, &s.kit, k, |s: &mut S| &mut s.adv_base))
            .child(field("adv-model", "Model", &s.adv_model, false, &s.kit, k, |s: &mut S| &mut s.adv_model))
            .child(field("adv-key", "API key", &s.adv_key, true, &s.kit, k, |s: &mut S| &mut s.adv_key))
            .child(row().gap(10.0).items_center().flex_wrap()
                .child(btn("adv-save", Btn::Outline, "Save", k).on_press(|s: &mut S| {
                    let mut st = s.settings();
                    st.llm.base_url = s.adv_base.text.trim().into();
                    st.llm.model = s.adv_model.text.trim().into();
                    st.llm.api_key = s.adv_key.text.trim().into();
                    s.save(st);
                    s.say("Saved");
                }))
                .child(btn("ping", Btn::Outline, "Test connection", k).on_press(|s: &mut S| {
                    s.ping = Some("…".into());
                    let (app, shared, me) = (s.app.clone(), s.shared.clone(), s.me.clone());
                    tauri::async_runtime::spawn(async move {
                        let r = crate::commands::run_feature_inner(&app, &shared, "ping_llm", json!({})).await;
                        if let Some(me) = me {
                            me.update(move |s, _| {
                                s.ping = Some(match r {
                                    Ok(v) => format!("OK: {}", str_of(&v, "reply").chars().take(40).collect::<String>()),
                                    Err(e) => e,
                                });
                            });
                        }
                    });
                }))
                .child(t(s.ping.clone().unwrap_or_default(), 13.0, k.text_subdued)));
    }
    c_
}

fn connect_key(s: &mut S, onboarding: bool) {
    let key = s.key.text.trim().to_string();
    if key.is_empty() || s.key_busy {
        return;
    }
    s.key_busy = true;
    s.key_err = None;
    let me = s.me.clone();
    tauri::async_runtime::spawn(async move {
        let r = crate::commands::probe_key_inner(&key).await;
        if let Some(me) = me {
            me.update(move |s, _| {
                s.key_busy = false;
                match r {
                    Ok(v) => {
                        let mut st = s.settings();
                        st.llm.provider = serde_json::from_value(v["provider"].clone()).unwrap_or(crate::settings::Provider::None);
                        st.llm.base_url = str_of(&v, "base_url");
                        st.llm.model = str_of(&v, "model");
                        st.llm.api_key = key.clone();
                        s.adv_base = TextEdit::new(st.llm.base_url.clone());
                        s.adv_model = TextEdit::new(st.llm.model.clone());
                        s.adv_key = TextEdit::new(key);
                        let model = st.llm.model.clone();
                        s.save(st);
                        s.key = TextEdit::new("");
                        s.ai_open = Some(false);
                        s.say(format!("Connected · {model}"));
                        if onboarding {
                            finish_onboarding(s);
                        }
                    }
                    Err(e) => s.key_err = Some(e),
                }
            });
        }
    });
}

fn check_local(s: &mut S) {
    s.local = None;
    let st = s.settings();
    let base = (st.llm.provider == crate::settings::Provider::Ollama).then(|| st.llm.base_url.clone());
    let me = s.me.clone();
    tauri::async_runtime::spawn(async move {
        let v = crate::commands::local_models(base).await;
        if let Some(me) = me {
            me.update(move |s, _| {
                let st = s.settings();
                let models: Vec<String> = v["models"].as_array().cloned().unwrap_or_default().iter().filter_map(|m| m.as_str().map(String::from)).collect();
                s.local_model = if st.llm.provider == crate::settings::Provider::Ollama && models.contains(&st.llm.model) { st.llm.model.clone() } else { str_of(&v, "pick") };
                s.local = Some(v);
            });
        }
    });
}

fn use_local(s: &mut S, onboarding: bool) {
    let Some(v) = s.local.clone() else { return };
    let mut st = s.settings();
    st.llm.provider = crate::settings::Provider::Ollama;
    st.llm.base_url = str_of(&v, "base_url");
    st.llm.api_key = String::new();
    st.llm.model = s.local_model.clone();
    let model = st.llm.model.clone();
    s.save(st);
    s.local_open = false;
    s.say(format!("Using {model} on this computer"));
    if onboarding {
        finish_onboarding(s);
    }
}

fn toggle_row(key: &'static str, label: &str, on: bool, k: &Tok) -> El {
    kv(t(label, 15.0, k.text), check(key, on, k), k).cursor(rune::Cursor::Pointer).on_press(move |s: &mut S| {
        let mut st = s.settings();
        match key {
            "auto-spot" => st.auto_spot_check = !st.auto_spot_check,
            "show-button" => st.show_button = !st.show_button,
            _ => st.show_underlines = !st.show_underlines,
        }
        s.save(st);
    })
}

fn settings_view(s: &S, st: &crate::settings::Settings) -> El {
    let k = &s.tok;
    let mut themes: Vec<(String, String)> = super::theme::all().iter().map(|t| (t.id.to_string(), t.name.to_string())).collect();
    themes.push(("matugen".into(), "Matugen (follows the wallpaper)".into()));
    let mut c_ = col().w_full()
        .child(section_label("AI", k))
        .child(ai_setup(s, st, false))
        .child(section_label("Appearance", k))
        .child(t("Theme", 13.0, k.text_subdued).pb(6.0))
        .child(select("theme", themes, if st.theme.is_empty() { "default" } else { &st.theme }, k, |s: &mut S, v| {
            let mut st = s.settings();
            st.theme = v;
            s.save(st);
        }))
        .child(section_label("Suggestions", k))
        .child(toggle_row("auto-spot", "Spot-check sentences when I stop typing", st.auto_spot_check, k))
        .child(toggle_row("show-button", "Show the Oumu button next to text fields", st.show_button, k))
        .child(toggle_row("underlines", "Draw underlines in the app", st.show_underlines, k))
        .child(row().mt(12.0).child(btn("restore", Btn::Outline, &format!("Restore dismissed suggestions ({})", st.dismissed.len()), k).on_press(|s: &mut S| {
            let mut st = s.settings();
            st.dismissed.clear();
            s.save(st);
        })))
        .child(section_label("Learned", k))
        .child(p("Words you added from a spelling card, and checks you turned off from a card.", 13.0, k.text_subdued).mb(8.0));
    if st.dictionary.is_empty() && st.muted_rules.is_empty() {
        c_ = c_.child(t("Nothing yet.", 13.0, k.text_subdued));
    }
    for w in st.dictionary.iter() {
        let w2 = w.clone();
        c_ = c_.child(kv(t(w.clone(), 15.0, k.text), btn(&format!("word/{w}"), Btn::Outline, "Remove", k).px(10.0).py(4.0).on_press(move |s: &mut S| {
            let mut st = s.settings();
            st.dictionary.remove(&w2);
            s.save(st);
        }), k));
    }
    for r in st.muted_rules.iter() {
        let r2 = r.clone();
        c_ = c_.child(kv(t(format!("{} (off)", super::layer::rule_name(r)), 15.0, k.text), btn(&format!("rule/{r}"), Btn::Outline, "Turn on", k).px(10.0).py(4.0).on_press(move |s: &mut S| {
            let mut st = s.settings();
            st.muted_rules.remove(&r2);
            st.rule_dismissals.remove(&r2);
            s.save(st);
        }), k));
    }
    c_ = c_.child(section_label("Skipped apps", k)).child(p("Oumu stays out of terminals. Add other apps with \"Skip this app\" in the Review view.", 13.0, k.text_subdued).mb(8.0));
    if st.skip_apps.is_empty() {
        c_ = c_.child(t("No apps added.", 13.0, k.text_subdued));
    }
    for a in st.skip_apps.iter() {
        let bid = a.bundle_id.clone();
        c_ = c_.child(kv(t(if a.name.is_empty() { a.bundle_id.clone() } else { a.name.clone() }, 15.0, k.text), btn(&format!("app/{bid}"), Btn::Outline, "Turn on", k).px(10.0).py(4.0).on_press(move |s: &mut S| {
            let mut st = s.settings();
            st.skip_apps.retain(|x| x.bundle_id != bid);
            s.save(st);
        }), k));
    }
    let side_opts = vec![("left".to_string(), "Left".to_string()), ("right".to_string(), "Right".to_string())];
    let rail_opts = vec![("right".to_string(), "Right (like Grammarly)".to_string()), ("left".to_string(), "Left".to_string())];
    c_ = c_
        .child(section_label("You", k))
        .child(field("name", "Name (used for the avatar and greetings)", &s.name, false, &s.kit, k, |s: &mut S| &mut s.name))
        .child(row().mb(6.0).child(btn("save-name", Btn::Outline, "Save name", k).px(10.0).py(4.0).on_press(|s: &mut S| {
            let mut st = s.settings();
            st.user_name = s.name.text.trim().into();
            s.save(st);
        })))
        .child(section_label("Panel", k))
        .child(t("Dock side", 13.0, k.text_subdued).pb(6.0))
        .child(select("dock", side_opts, &st.dock_side, k, |s: &mut S, v| {
            let mut st = s.settings();
            st.dock_side = v;
            s.save(st);
        }).mb(14.0))
        .child(t("Icon rail", 13.0, k.text_subdued).pb(6.0))
        .child(select("rail", rail_opts, &st.rail_side, k, |s: &mut S, v| {
            let mut st = s.settings();
            st.rail_side = v;
            s.save(st);
        }))
        .child(section_label("Plagiarism search", k))
        .child(p("Plagiarism checks search the web for free without any setup. For steadier results, point Oumu at your own SearXNG or paste a Brave Search key. Oumu uses the Brave key first, then SearXNG, then the free search.", 13.0, k.text_subdued).mb(8.0))
        .child(field_ph("searx", "SearXNG address", "Optional, e.g. http://localhost:8888", &s.searx, false, &s.kit, k, |s: &mut S| &mut s.searx))
        .child(field_ph("brave", "Brave Search API key", "Optional", &s.brave, true, &s.kit, k, |s: &mut S| &mut s.brave))
        .child(row().gap(10.0).items_center()
            .child(btn("save-search", Btn::Outline, "Save", k).px(10.0).py(4.0).on_press(|s: &mut S| {
                let mut st = s.settings();
                st.search.searxng_url = s.searx.text.trim().into();
                st.search.brave_key = s.brave.text.trim().into();
                s.save(st);
                s.say("Saved");
            }))
            .child(btn("test-searx", Btn::Outline, "Test SearXNG", k).px(10.0).py(4.0).on_press(|s: &mut S| {
                let url = s.searx.text.trim().to_string();
                if url.is_empty() {
                    return;
                }
                let me = s.me.clone();
                tauri::async_runtime::spawn(async move {
                    let r = crate::commands::probe_searxng(url).await;
                    if let Some(me) = me {
                        me.update(move |s, _| {
                            s.searx_probe = Some(match r {
                                Ok(n) => (true, format!("Works: a test search found {n} results.")),
                                Err(e) => (false, e),
                            })
                        });
                    }
                });
            })));
    if let Some((ok, msg)) = &s.searx_probe {
        c_ = c_.child(t(msg.clone(), 13.0, if *ok { k.text_subdued } else { k.warn_ink() }).mt(6.0));
    }
    c_ = c_.child(section_label("App", k));
    let auto = s.autostart.unwrap_or_else(crate::commands::autostart_status);
    if let Some(on) = auto {
        c_ = c_.child(kv(t("Start Oumu with Windows", 15.0, k.text), check("autostart", on, k), k).cursor(rune::Cursor::Pointer).on_press(move |s: &mut S| match crate::commands::set_autostart(!on) {
            Ok(v) => s.autostart = Some(Some(v)),
            Err(e) => s.say(e),
        }));
    }
    c_ = c_.child(row().mt(12.0).child(btn("quit", Btn::Ghost, "Quit Oumu", k).on_press(|s: &mut S| s.app.exit(0))));
    col().w_full().h_full().child(titlebar("Settings", vec![], k)).child(scroll(c_))
}

/// Run a feature whose answer is only "done or not": `done` runs on success, an error is said.
fn run_then(s: &mut S, name: &str, args: Value, done: impl FnOnce(&mut S) + Send + 'static) {
    let (app, shared, me, name) = (s.app.clone(), s.shared.clone(), s.me.clone(), name.to_string());
    tauri::async_runtime::spawn(async move {
        let r = crate::commands::run_feature_inner(&app, &shared, &name, args).await;
        if let Some(me) = me {
            me.update(move |s, _| match r {
                Ok(_) => done(s),
                Err(e) => s.say(e),
            });
        }
    });
}

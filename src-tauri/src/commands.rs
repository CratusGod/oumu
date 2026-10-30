//! Tauri commands (and the plain functions behind them, shared with the debug probe).

use crate::engine::Alert;
use crate::features;
use crate::host::{HostDoc, Shared};
use crate::llm;
use crate::settings::{self, LlmConfig, Provider, Settings};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, State};

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

fn doc_snapshot(shared: &Shared) -> Option<(HostDoc, crate::ax::AXElement)> {
    let st = shared.lock().unwrap();
    match (&st.doc, &st.element) {
        (Some(d), Some(e)) => Some((d.clone(), e.clone())),
        _ => None,
    }
}

pub fn accept_alert_inner(app: &AppHandle, shared: &Shared, id: &str, suggestion: usize) -> Result<crate::apply::ApplyResult, String> {
    let alert: Alert = {
        let st = shared.lock().unwrap();
        st.alerts.iter().find(|a| a.id == id).cloned().ok_or("That suggestion is no longer available.")?
    };
    let replacement = alert.suggestions.get(suggestion).cloned().ok_or("This suggestion has no automatic fix.")?;
    let (doc, el) = doc_snapshot(shared).ok_or("No text field is connected.")?;
    let res = crate::apply::replace_range(&el, doc.pid, alert.u16_start, alert.u16_end - alert.u16_start, &replacement, &doc.text).map_err(err)?;
    // Update the model immediately so the panel does not flicker.
    {
        let mut st = shared.lock().unwrap();
        if let Some(d) = st.doc.as_mut() {
            d.text = res.new_text.clone();
            d.version += 1;
        }
        st.last_text_hash = 0; // force the loop to re-read and re-lint
        st.alerts.retain(|a| a.id != id);
        st.spot_alerts.retain(|a| a.id != id);
        let alerts = st.alerts.clone();
        let doc = st.doc.clone();
        drop(st);
        let _ = app.emit("host://alerts", &alerts);
        if let Some(d) = doc {
            let _ = app.emit("host://document", &d);
        }
    }
    Ok(res)
}

/// What a card asks Oumu to remember about an alert.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Forget {
    /// This alert, here and everywhere it comes back.
    Dismiss,
    /// The flagged word is spelled right: add it to the dictionary.
    LearnWord,
    /// Turn off the check that raised it.
    MuteRule,
}

/// Apply `what` to the alert `id`, save the settings and refresh the visible
/// alerts. The id is looked up in every list the host keeps, so a card opened
/// just before a re-lint still works; an unknown id is an error, not a no-op.
pub fn forget_alert_inner(app: &AppHandle, shared: &Shared, id: &str, what: Forget) -> Result<(), String> {
    let mut st = shared.lock().unwrap();
    let a = st
        .alerts
        .iter()
        .chain(st.all_alerts.iter())
        .chain(st.spot_alerts.iter())
        .find(|a| a.id == id)
        .cloned()
        .ok_or("That suggestion is gone; the text changed.")?;
    remember(&mut st.settings, &a, what);
    let _ = settings::save(&st.settings);
    st.alerts = st.visible_alerts();
    let alerts = st.alerts.clone();
    let s = st.settings.clone();
    drop(st);
    let _ = app.emit("host://alerts", &alerts);
    let _ = app.emit("settings://changed", &s);
    Ok(())
}

fn remember(s: &mut settings::Settings, a: &crate::engine::Alert, what: Forget) {
    match what {
        Forget::Dismiss => {
            s.dismissed.insert(a.stable_hash);
            *s.rule_dismissals.entry(a.rule.clone()).or_default() += 1;
        }
        Forget::LearnWord => {
            s.dictionary.insert(a.original.trim().to_lowercase());
        }
        Forget::MuteRule => {
            s.muted_rules.insert(a.rule.clone());
        }
    }
}

pub fn dismiss_alert_inner(app: &AppHandle, shared: &Shared, id: &str) -> Result<(), String> {
    forget_alert_inner(app, shared, id, Forget::Dismiss)
}

pub fn replace_range_inner(app: &AppHandle, shared: &Shared, start: usize, end: usize, text: &str) -> Result<crate::apply::ApplyResult, String> {
    let (doc, el) = doc_snapshot(shared).ok_or("No text field is connected.")?;
    let end = end.max(start);
    let res = crate::apply::replace_range(&el, doc.pid, start, end - start, text, &doc.text).map_err(err)?;
    {
        let mut st = shared.lock().unwrap();
        if let Some(d) = st.doc.as_mut() {
            d.text = res.new_text.clone();
            d.version += 1;
        }
        st.last_text_hash = 0;
        let doc = st.doc.clone();
        drop(st);
        if let Some(d) = doc {
            let _ = app.emit("host://document", &d);
        }
    }
    Ok(res)
}

fn dock_side(app: &AppHandle) -> String {
    app.state::<Shared>().lock().unwrap().settings.dock_side.clone()
}

/// Slide the panel out from the edge tab and switch it to `view`. `focus` also
/// makes it the key window (tray menu); the edge tab and Oumu button leave the
/// host app's text field focused.
pub fn open_panel_inner(app: &AppHandle, view: &str, focus: bool) {
    let side = dock_side(app);
    crate::overlay::set_tucked(app, &side, false, true, focus);
    let _ = app.emit_to("panel", "panel://view", json!({"view": view}));
}

/// Slide the panel away behind the edge tab.
pub fn tuck_panel_inner(app: &AppHandle) {
    let side = dock_side(app);
    crate::overlay::set_tucked(app, &side, true, true, false);
}

/// Whole-document variant that still honours an explicit `text` argument (tests, chat chips).
fn whole_args(args: &Value) -> Value {
    json!({"whole": true, "text": args["text"].clone()})
}

fn resolve_text(shared: &Shared, args: &Value) -> Result<(String, String, bool), String> {
    // returns (text, app_name, used_selection)
    if let Some(t) = args["text"].as_str() {
        if !t.trim().is_empty() {
            let app_name = shared.lock().unwrap().doc.as_ref().map(|d| d.app_name.clone()).unwrap_or_default();
            return Ok((t.to_string(), app_name, false));
        }
    }
    let st = shared.lock().unwrap();
    let d = st.doc.as_ref().ok_or("Click into a text field first, then try again.")?;
    let whole = args["whole"].as_bool().unwrap_or(false);
    if !whole && !d.selected_text.trim().is_empty() {
        return Ok((d.selected_text.clone(), d.app_name.clone(), true));
    }
    if d.text.trim().is_empty() {
        return Err("The text field is empty.".into());
    }
    Ok((d.text.clone(), d.app_name.clone(), false))
}

/// The model configuration a feature actually calls.
pub fn effective_llm(settings: &Settings) -> Result<LlmConfig, String> {
    const NO_KEY: &str = "No AI set up yet. Open Settings → AI and paste a key (Gemini, OpenAI, Anthropic, OpenRouter or Groq), or pick a local model.";
    match settings.llm.provider {
        Provider::None => Err(NO_KEY.into()),
        Provider::Ollama => Ok(settings.llm.clone()),
        _ if settings.llm.api_key.trim().is_empty() => Err(NO_KEY.into()),
        _ => Ok(settings.llm.clone()),
    }
}

/// Spot check the connected document (or `text` for tests): sentence-level
/// Clarity suggestions merged into the alert list and re-anchored to whatever
/// the document says once the model answers.
/// Take the spot-check slot for the connected document: `Ok(None)` while
/// another check runs (synchronous, so no lock is held across an await).
fn claim_spot(shared: &Shared) -> Result<Option<(String, String, features::spotcheck::Cache)>, String> {
    let mut st = shared.lock().unwrap();
    let (id, text) = st.doc.as_ref().map(|d| (d.id.clone(), d.text.clone())).ok_or("Click into a text field first, then try again.")?;
    if text.trim().is_empty() {
        return Err("The text field is empty.".into());
    }
    if st.spot_running {
        return Ok(None);
    }
    st.spot_running = true;
    Ok(Some((id, text, st.spot_cache.clone())))
}

async fn spot_check_inner(app: &AppHandle, shared: &Shared, cfg: Option<&LlmConfig>, args: &Value) -> Result<Value, String> {
    if let Some(t) = args["text"].as_str().filter(|t| !t.trim().is_empty()) {
        let mut cache = features::spotcheck::Cache::new();
        let started = std::time::Instant::now();
        let o = features::spotcheck::run(cfg, t, &mut cache, false).await.map_err(err)?;
        let mut v = serde_json::to_value(o).unwrap();
        v["ms"] = json!(started.elapsed().as_millis() as u64);
        return Ok(v);
    }
    // A manual check that lands while the host's automatic one is running
    // waits for it (the cache then answers for every unchanged sentence).
    let mut waited = 0u32;
    let (doc_id, text, mut cache) = loop {
        match claim_spot(shared)? {
            Some(v) => break v,
            None => {
                waited += 1;
                if waited > 450 {
                    return Ok(json!({"busy": true}));
                }
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        }
    };
    let started = std::time::Instant::now();
    let auto = args["auto"].as_bool().unwrap_or(false);
    let result = features::spotcheck::run(cfg, &text, &mut cache, auto).await;
    features::spotcheck::save_cache(&cache);
    let mut st = shared.lock().unwrap();
    st.spot_running = false;
    st.spot_cache = cache;
    let o = result.map_err(err)?;
    let same_doc = st.doc.as_ref().map(|d| d.id == doc_id).unwrap_or(false);
    if !same_doc {
        return Ok(json!({"stale": true, "checked": o.checked}));
    }
    let mut alerts = o.alerts.clone();
    if let Some(d) = st.doc.as_ref() {
        if d.text != text {
            features::spotcheck::re_anchor(&mut alerts, &d.text);
        }
    }
    st.spot_alerts = alerts;
    st.spot_doc_id = doc_id;
    st.alerts = st.visible_alerts();
    let list = st.alerts.clone();
    drop(st);
    let _ = app.emit("host://alerts", &list);
    Ok(json!({
        "sentences": o.sentences, "checked": o.checked, "from_cache": o.from_cache, "batches": o.batches,
        "skipped": o.skipped, "rejected": o.rejected, "rejections": o.rejections, "suggestions": o.alerts.len(), "ms": started.elapsed().as_millis() as u64,
    }))
}

/// Host-driven spot check: the host loop calls this once typing pauses, for
/// any panel view (or a tucked panel). Errors go to the log, not the user.
pub async fn spot_check_auto(app: AppHandle, shared: Shared, cfg: LlmConfig) {
    if let Err(e) = spot_check_inner(&app, &shared, Some(&cfg), &json!({"auto": true})).await {
        log::warn!("auto spot check: {e}");
        if out_of_quota(&e) {
            // Every model said no: stop spending pauses on refusals until
            // the quota has had time to come back. Manual checks still run.
            shared.lock().unwrap().spot_paused_until = Some(std::time::Instant::now() + SPOT_QUOTA_PAUSE);
        }
    }
}

/// How long automatic checks rest after the key ran out of quota.
const SPOT_QUOTA_PAUSE: std::time::Duration = std::time::Duration::from_secs(60 * 60);

fn out_of_quota(e: &str) -> bool {
    let e = e.to_lowercase();
    e.contains("429") || e.contains("quota") || e.contains("resource_exhausted") || e.contains("rate limit")
}

pub async fn run_feature_inner(app: &AppHandle, shared: &Shared, name: &str, args: Value) -> Result<Value, String> {
    let settings: Settings = shared.lock().unwrap().settings.clone();
    let llm_cfg = effective_llm(&settings);
    let cfg = llm_cfg.clone().unwrap_or_else(|_| settings.llm.clone());
    let needs_llm = !matches!(name, "create_audience" | "rewriter_scan" | "plagiarism" | "score" | "spot_check");
    if needs_llm {
        llm_cfg.clone()?;
    }
    let out = match name {
        // Without a model the spot check still runs its deterministic wordy-phrase rewrites.
        "spot_check" => spot_check_inner(app, shared, llm_cfg.as_ref().ok(), &args).await?,
        "paraphrase" => {
            let (text, _, sel) = resolve_text(shared, &args)?;
            let style = args["style"].as_str().unwrap_or("Professional");
            let mut v = features::paraphrase(&cfg, &text, style).await.map_err(err)?;
            v["used_selection"] = json!(sel);
            v
        }
        "humanize" => {
            let (text, _, sel) = resolve_text(shared, &args)?;
            let mut v = features::humanize(&cfg, &text, args["voice_name"].as_str().unwrap_or("The Everyday Voice"), args["voice_description"].as_str().unwrap_or("")).await.map_err(err)?;
            v["used_selection"] = json!(sel);
            v
        }
        "create_voice" => {
            let sample = args["sample"].as_str().map(String::from).unwrap_or_else(|| resolve_text(shared, &whole_args(&args)).map(|t| t.0).unwrap_or_default());
            let v = features::create_voice(&cfg, &sample, args["name"].as_str().unwrap_or("")).await.map_err(err)?;
            let voice = settings::Voice { id: format!("v{}", chrono::Utc::now().timestamp_millis()), name: v["name"].as_str().unwrap_or("My voice").into(), description: v["description"].as_str().unwrap_or("").into(), custom: true };
            let mut st = shared.lock().unwrap();
            st.settings.custom_voices.push(voice.clone());
            let _ = settings::save(&st.settings);
            serde_json::to_value(voice).unwrap()
        }
        "reactions" => {
            let (text, _, _) = resolve_text(shared, &whole_args(&args))?;
            features::reader_reactions(&cfg, &text, args["audience"].as_str().unwrap_or("Online followers"), args["audience_desc"].as_str().unwrap_or("")).await.map_err(err)?
        }
        "create_audience" => {
            let a = settings::Audience { id: format!("a{}", chrono::Utc::now().timestamp_millis()), name: args["name"].as_str().unwrap_or("My audience").into(), description: args["description"].as_str().unwrap_or("").into(), custom: true };
            let mut st = shared.lock().unwrap();
            st.settings.custom_audiences.push(a.clone());
            let _ = settings::save(&st.settings);
            serde_json::to_value(a).unwrap()
        }
        "translate" => {
            let (text, _, sel) = resolve_text(shared, &args)?;
            // "Auto-detect" (or nothing) lets the model detect the source language.
            let source = args["source"].as_str().map(str::trim).filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("auto"));
            let mut v = features::translate(&cfg, &text, args["target"].as_str().unwrap_or(&settings.target_language), source).await.map_err(err)?;
            v["used_selection"] = json!(sel);
            v
        }
        "explain" => {
            let whole = args["whole"].as_bool().unwrap_or(false);
            let (text, _, sel) = resolve_text(shared, &args)?;
            let mut v = features::explain(&cfg, &text, whole || !sel).await.map_err(err)?;
            v["used_selection"] = json!(sel);
            v
        }
        "tone" => {
            let (text, _, _) = resolve_text(shared, &whole_args(&args))?;
            features::tone(&cfg, &text).await.map_err(err)?
        }
        "detector" => {
            let (text, _, _) = resolve_text(shared, &whole_args(&args))?;
            features::detector::detect(&cfg, &text).await.map_err(err)?
        }
        "rewriter_scan" => {
            let (text, _, _) = resolve_text(shared, &whole_args(&args))?;
            let found = crate::aiphrases::find(&text);
            let words = crate::score::word_count(&text);
            json!({"phrases": found, "words": words, "too_short": words < 8})
        }
        "rewrite_phrase" => features::rewrite_phrase(&cfg, args["sentence"].as_str().unwrap_or(""), args["phrase"].as_str().unwrap_or("")).await.map_err(err)?,
        "summarize" => {
            let (text, _, _) = resolve_text(shared, &whole_args(&args))?;
            json!({"summary": features::summarize(&cfg, &text).await.map_err(err)?})
        }
        "plagiarism" => {
            let (text, _, _) = resolve_text(shared, &whole_args(&args))?;
            features::plagiarism::check(&settings.search, &text).await.map_err(err)?
        }
        "score" => {
            let st = shared.lock().unwrap();
            let d = st.doc.as_ref().ok_or("Click into a text field first.")?;
            serde_json::to_value(crate::score::compute(&d.text, &st.alerts)).unwrap()
        }
        "ping_llm" => {
            let r = llm::complete(&cfg, "Reply with the single word: pong", &[llm::msg("user", "ping")], false, 0.0).await.map_err(err)?;
            json!({"reply": r})
        }
        _ => return Err(format!("unknown feature {name}")),
    };
    keep_working_gemini(shared);
    let _ = app;
    Ok(out)
}

#[tauri::command]
pub fn get_state(state: State<'_, Shared>) -> Value {
    let st = state.lock().unwrap();
    json!({
        "doc": st.doc,
        "alerts": st.alerts,
        "status": st.status,
        "settings": st.settings,
        "history": st.history,
        "score": st.doc.as_ref().map(|d| crate::score::compute(&d.text, &st.alerts)),
        "initials": settings::initials(&st.settings.user_name),
        "spot_running": st.spot_running,
    })
}

// Linux: AT-SPI calls, sleeps and clipboard work; run on Tauri's worker pool, not the GTK main thread.
#[cfg_attr(target_os = "linux", tauri::command(async))]
#[cfg_attr(not(target_os = "linux"), tauri::command)]
pub fn accept_alert(app: AppHandle, state: State<'_, Shared>, id: String, suggestion: Option<usize>) -> Result<crate::apply::ApplyResult, String> {
    accept_alert_inner(&app, &state, &id, suggestion.unwrap_or(0))
}

#[tauri::command]
pub fn dismiss_alert(app: AppHandle, state: State<'_, Shared>, id: String) -> Result<(), String> {
    dismiss_alert_inner(&app, &state, &id)
}

#[tauri::command]
pub fn learn_word(app: AppHandle, state: State<'_, Shared>, id: String) -> Result<(), String> {
    forget_alert_inner(&app, &state, &id, Forget::LearnWord)
}

#[tauri::command]
pub fn mute_rule(app: AppHandle, state: State<'_, Shared>, id: String) -> Result<(), String> {
    forget_alert_inner(&app, &state, &id, Forget::MuteRule)
}


// Linux: AT-SPI calls, sleeps and clipboard work; run on Tauri's worker pool, not the GTK main thread.
#[cfg_attr(target_os = "linux", tauri::command(async))]
#[cfg_attr(not(target_os = "linux"), tauri::command)]
pub fn replace_range(app: AppHandle, state: State<'_, Shared>, start: usize, end: usize, text: String) -> Result<crate::apply::ApplyResult, String> {
    replace_range_inner(&app, &state, start, end, &text)
}

#[tauri::command]
pub async fn run_feature(app: AppHandle, state: State<'_, Shared>, name: String, args: Value) -> Result<Value, String> {
    let shared = state.inner().clone();
    run_feature_inner(&app, &shared, &name, args).await
}

#[tauri::command]
pub async fn chat_send(app: AppHandle, state: State<'_, Shared>, req_id: String, messages: Value) -> Result<String, String> {
    let (cfg, system) = {
        let st = state.lock().unwrap();
        let (doc_text, app_name, sel) = st.doc.as_ref().map(|d| (d.text.clone(), d.app_name.clone(), d.selected_text.clone())).unwrap_or_default();
        (effective_llm(&st.settings), features::chat_system(&st.settings.user_name, &app_name, &doc_text, &sel))
    };
    let cfg = match cfg {
        Ok(c) => c,
        Err(e) => {
            let _ = app.emit_to("panel", "llm://chunk", json!({"req_id": req_id, "delta": "", "done": true, "error": e}));
            return Err(e);
        }
    };
    let msgs = features::as_messages(&messages);
    let app2 = app.clone();
    let rid = req_id.clone();
    let result = llm::stream(&cfg, &system, &msgs, 0.5, |delta| {
        let _ = app2.emit_to("panel", "llm://chunk", json!({"req_id": rid, "delta": delta, "done": false}));
    })
    .await;
    match result {
        Ok(full) => {
            let _ = app.emit_to("panel", "llm://chunk", json!({"req_id": req_id, "delta": "", "done": true}));
            Ok(full)
        }
        Err(e) => {
            let _ = app.emit_to("panel", "llm://chunk", json!({"req_id": req_id, "delta": "", "done": true, "error": e.to_string()}));
            Err(e.to_string())
        }
    }
}

#[tauri::command]
pub fn get_settings(state: State<'_, Shared>) -> Settings {
    state.lock().unwrap().settings.clone()
}

#[tauri::command]
pub fn set_settings(app: AppHandle, state: State<'_, Shared>, settings: Settings) -> Result<(), String> {
    {
        let mut st = state.lock().unwrap();
        st.settings = settings.clone();
        settings::save(&st.settings).map_err(err)?;
        // Re-apply the dismissed, dictionary and turned-off-rule filters.
        st.alerts = st.visible_alerts();
        let _ = app.emit("host://alerts", &st.alerts);
    }
    crate::overlay::dock_panel(&app, &settings.dock_side);
    let _ = app.emit("settings://changed", &settings);
    Ok(())
}

#[tauri::command]
pub fn get_history(state: State<'_, Shared>) -> Vec<crate::history::Session> {
    state.lock().unwrap().history.clone()
}

#[tauri::command]
pub fn open_panel(app: AppHandle, view: Option<String>, focus: Option<bool>) {
    open_panel_inner(&app, view.as_deref().unwrap_or("review"), focus.unwrap_or(false));
}

#[tauri::command]
pub fn hide_panel(app: AppHandle) {
    tuck_panel_inner(&app);
}

/// The pointer entered (`true`) or left the edge tab. Where the desktop shows
/// the pointer (Hyprland, X11) the host loop already decides this; elsewhere
/// the tab's own hover is the only signal.
#[tauri::command]
pub fn tab_hover(app: AppHandle, open: bool) {
    // One decider: where the pointer is known, its thread owns the tab state.
    #[cfg(target_os = "linux")]
    if crate::linux::desktop::kind() != crate::linux::desktop::Kind::Wayland {
        return;
    }
    crate::overlay::set_tab_open(&app, &dock_side(&app), open);
}

/// Where the Matugen theme stands: Ryoku, matugen installed, template registered, palette written.
#[tauri::command]
pub fn matugen_status(state: State<'_, Shared>) -> crate::matugen::Status {
    let custom = state.lock().unwrap().settings.palette_path.clone();
    crate::matugen::status(&custom)
}

/// Install and register Oumu's matugen template (not on Ryoku).
#[tauri::command]
pub fn matugen_setup(state: State<'_, Shared>) -> Result<crate::matugen::Status, String> {
    let custom = state.lock().unwrap().settings.palette_path.clone();
    crate::matugen::setup(&custom)
}

/// Try a SearXNG address for plagiarism search; how many results a test search found.
#[tauri::command]
pub async fn probe_searxng(url: String) -> Result<usize, String> {
    features::plagiarism::probe_searxng(&url).await.map_err(|e| e.to_string())
}

/// The matugen palette (role -> "#rrggbb"), when the palette file has one.
#[tauri::command]
pub fn get_palette(state: State<'_, Shared>) -> Option<crate::theme::Palette> {
    let custom = state.lock().unwrap().settings.palette_path.clone();
    crate::theme::read_palette(&crate::theme::palette_path(&custom)?)
}

#[tauri::command]
pub fn tuck_panel(app: AppHandle) {
    tuck_panel_inner(&app);
}

/// Text highlighted in any app (Linux: the primary selection).
#[cfg_attr(target_os = "linux", tauri::command(async))]
#[cfg_attr(not(target_os = "linux"), tauri::command)]
pub fn highlighted_text() -> Option<String> {
    #[cfg(target_os = "linux")]
    return crate::linux::selection::highlighted_text();
    #[cfg(not(target_os = "linux"))]
    None
}

/// The highlight a `oumu --translate` request left for the Translate view.
#[tauri::command]
pub fn take_pending_translation() -> Option<String> {
    #[cfg(target_os = "linux")]
    return crate::linux::remote::take_pending();
    #[cfg(not(target_os = "linux"))]
    None
}

// Linux: AT-SPI calls, sleeps and clipboard work; run on Tauri's worker pool, not the GTK main thread.
#[cfg_attr(target_os = "linux", tauri::command(async))]
#[cfg_attr(not(target_os = "linux"), tauri::command)]
pub fn permission_prompt() -> bool {
    crate::ax::prompt_trust()
}

// Linux: AT-SPI calls, sleeps and clipboard work; run on Tauri's worker pool, not the GTK main thread.
#[cfg_attr(target_os = "linux", tauri::command(async))]
#[cfg_attr(not(target_os = "linux"), tauri::command)]
pub fn permission_status() -> bool {
    crate::ax::is_trusted()
}

#[tauri::command]
pub fn set_card_rects(state: State<'_, Shared>, rects: Vec<crate::ax::Rect>) {
    state.lock().unwrap().card_rects = rects;
}

#[tauri::command]
pub fn open_url(url: String) {
    // Only web links: the UI never needs to open files or other schemes from here.
    if !(url.starts_with("https://") || url.starts_with("http://") || cfg!(target_os = "macos") && url.starts_with("x-apple.systempreferences:")) {
        log::warn!("open_url: refused a non-web URL");
        return;
    }
    let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    if let Err(e) = std::process::Command::new(opener).arg(url).spawn() {
        log::warn!("open_url: {opener} failed: {e}");
    }
}

#[tauri::command]
pub fn ui_ready(state: State<'_, Shared>, label: String) {
    let mut st = state.lock().unwrap();
    if !st.ui_ready.contains(&label) {
        st.ui_ready.push(label);
    }
}

#[tauri::command]
pub fn ui_log(state: State<'_, Shared>, message: String) {
    log::warn!("ui: {message}");
    let mut st = state.lock().unwrap();
    st.ui_log.push(message);
    if st.ui_log.len() > 50 {
        st.ui_log.remove(0);
    }
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

// ---------------------------------------------------------------- key set-up

/// How long the one-word test reply may take before key setup gives up.
const KEY_TEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);

/// Bring-your-own-key with one field: detect the provider from the key's
/// prefix, list the account's models, pick a sensible default, and ping it.
pub async fn probe_key_inner(key: &str) -> Result<Value, String> {
    let k = key.trim();
    if k.is_empty() {
        return Err("Paste a key first.".into());
    }
    let (provider, base, prefs): (Provider, &str, &[&str]) = if k.starts_with("sk-ant-") {
        (Provider::Anthropic, "https://api.anthropic.com", &["claude-sonnet-5-5", "claude-sonnet-4-5", "claude-opus-5-5", "claude-haiku-4-5", "claude-3-5-sonnet"])
    } else if k.starts_with("sk-or-") {
        (Provider::OpenAI, "https://openrouter.ai/api/v1", &["openai/gpt-5.6-terra", "anthropic/claude-sonnet-5-5", "openai/gpt-5.5", "openai/gpt-4o-mini", "openai/gpt-4o"])
    } else if k.starts_with("gsk_") {
        (Provider::OpenAI, "https://api.groq.com/openai", &["llama-3.3-70b-versatile", "llama-3.1-8b-instant", "mixtral-8x7b-32768"])
    } else if k.starts_with("AIza") || k.starts_with("AQ.") {
        // AI Studio keys: `AIza` (older) and `AQ.` (newer, refused by Google's OpenAI-compatible route).
        (Provider::Gemini, "https://generativelanguage.googleapis.com/v1beta", &["gemini-3.8-flash", "gemini-2.5-flash"])
    } else {
        (Provider::OpenAI, "https://api.openai.com", &["gpt-5.6-terra", "gpt-5.6-luna", "gpt-5.5", "gpt-4.1-mini", "gpt-4o-mini", "gpt-4o"])
    };
    let mut cfg = LlmConfig { provider: provider.clone(), base_url: base.into(), api_key: k.into(), model: String::new() };
    let models = match provider {
        Provider::Anthropic => llm::anthropic_models(&cfg).await,
        Provider::Gemini => llm::gemini_models(&cfg).await,
        _ => llm::openai_models(&cfg).await,
    }
    .map_err(|e| format!("The key was not accepted: {e}"))?;
    // Gemini retires models for new keys while still listing them, and sheds
    // load per model (503): try the stable Flash models newest first and keep
    // the first that answers.
    if provider == Provider::Gemini {
        let candidates = llm::gemini_flash_candidates(&models);
        if !candidates.is_empty() {
            return probe_gemini(cfg, &candidates, models.len()).await;
        }
    }
    let pick = prefs
        .iter()
        .find(|p| models.iter().any(|m| m == *p))
        .map(|s| s.to_string())
        .or_else(|| prefs.iter().find_map(|p| models.iter().find(|m| m.starts_with(p)).cloned()))
        .or_else(|| models.iter().find(|m| !m.contains("embed") && !m.contains("whisper") && !m.contains("tts") && !m.contains("image")).cloned())
        .ok_or("The key works, but no chat model is available on it.")?;
    cfg.model = pick;
    let ping_msg = [llm::msg("user", "ping")];
    let ping = llm::complete(&cfg, "Reply with the single word: pong", &ping_msg, false, 0.0);
    let reply = tokio::time::timeout(KEY_TEST_TIMEOUT, ping)
        .await
        .map_err(|_| format!("The key works, but {} did not answer within {} seconds. Try again, or pick another model under Advanced.", cfg.model, KEY_TEST_TIMEOUT.as_secs()))?
        .map_err(|e| format!("The key works but the model test failed: {e}"))?;
    Ok(json!({
        "provider": serde_json::to_value(&provider).unwrap(),
        "base_url": base,
        "model": cfg.model,
        "models": models.len(),
        "reply": reply.trim(),
    }))
}

/// Models tried at most during Gemini key setup, and how long each may take.
const GEMINI_PROBE_MODELS: usize = 4;
const GEMINI_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

async fn probe_gemini(mut cfg: LlmConfig, candidates: &[String], listed: usize) -> Result<Value, String> {
    // One request: a busy or silent model hands over to the next Flash model
    // inside `llm`, and the one that answered is the one saved.
    cfg.model = candidates[0].clone();
    let ping_msg = [llm::msg("user", "ping")];
    let ping = llm::complete(&cfg, "Reply with the single word: pong", &ping_msg, false, 0.0);
    let limit = GEMINI_PROBE_TIMEOUT * GEMINI_PROBE_MODELS as u32;
    let reply = tokio::time::timeout(limit, ping)
        .await
        .map_err(|_| format!("The key works, but no Gemini model answered within {} seconds. Google may be busy; try again in a few minutes.", limit.as_secs()))?
        .map_err(|e| format!("The key works but the model test failed: {e}"))?;
    let model = llm::gemini_working_for(&cfg.model).unwrap_or(cfg.model.clone());
    Ok(json!({
        "provider": serde_json::to_value(&cfg.provider).unwrap(),
        "base_url": cfg.base_url,
        "model": model,
        "models": listed,
        "reply": reply.trim(),
    }))
}

/// When Gemini answered with another model than the saved one (the saved one
/// is busy or retired), save the one that works so later sessions start there.
fn keep_working_gemini(shared: &Shared) {
    let mut st = shared.lock().unwrap();
    if st.settings.llm.provider != Provider::Gemini {
        return;
    }
    if let Some(m) = llm::gemini_working_for(&st.settings.llm.model) {
        log::info!("Gemini: saving {m} in place of {}", st.settings.llm.model);
        st.settings.llm.model = m;
        let _ = settings::save(&st.settings);
    }
}

/// The local Ollama at `base`: is it running, its chat models, and the one to
/// suggest (a known good writer if present, else the first chat model).
pub async fn detect_ollama_inner(base: &str) -> Value {
    let models: Vec<String> = llm::ollama_models(base).await.unwrap_or_default();
    let chat: Vec<&String> = models.iter().filter(|m| !m.contains("embed")).collect();
    let prefs = ["qwen3.5:9b", "qwen3:8b", "qwen2.5:7b", "llama3.1:8b", "gemma3:12b", "gemma3:4b", "mistral:7b"];
    let pick = prefs
        .iter()
        .find(|p| chat.iter().any(|m| m == *p))
        .map(|s| s.to_string())
        .or_else(|| chat.first().map(|m| m.to_string()));
    json!({"running": !models.is_empty(), "base_url": base, "models": chat, "pick": pick})
}

/// Settings → AI → Local model: what the local Ollama offers.
#[tauri::command]
pub async fn local_models(base_url: Option<String>) -> Value {
    let base = base_url.filter(|b| !b.trim().is_empty()).unwrap_or_else(|| OLLAMA_BASE.into());
    detect_ollama_inner(base.trim_end_matches('/')).await
}

pub const OLLAMA_BASE: &str = "http://127.0.0.1:11434";

#[tauri::command]
pub async fn probe_key(key: String) -> Result<Value, String> {
    probe_key_inner(&key).await
}


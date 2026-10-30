//! One small client for three chat-completion shapes: Ollama, OpenAI-compatible,
//! and Anthropic. Streaming is delivered through a callback.

use crate::settings::{LlmConfig, Provider};
use anyhow::{anyhow, Context, Result};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Newer models (gpt-5.x, Claude 5) reject `temperature`; once a provider says
/// so, every later call leaves sampling parameters out.
static OPENAI_NO_SAMPLING: AtomicBool = AtomicBool::new(false);
static ANTHROPIC_NO_SAMPLING: AtomicBool = AtomicBool::new(false);

fn rejects_sampling(text: &str) -> bool {
    let t = text.to_lowercase();
    t.contains("temperature") && (t.contains("unsupported") || t.contains("deprecated") || t.contains("not supported") || t.contains("does not support"))
}

/// OpenRouter reserves credit for the model's whole output limit when a request
/// names none, and refuses with "... but can only afford N" when the balance is
/// short. N tokens is what this key can pay for: ask again with that cap.
fn affordable_tokens(text: &str) -> Option<u64> {
    let rest = &text[text.find("can only afford ")? + "can only afford ".len()..];
    let n: u64 = rest.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse().ok()?;
    (n > 0).then_some(n)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

pub fn msg(role: &str, content: impl Into<String>) -> Message {
    Message { role: role.into(), content: content.into() }
}

/// One client for every model request, so its connection pool keeps the TLS
/// session to the provider: a new client per request paid a full handshake
/// (and DNS lookup) every time. Cloning a `reqwest::Client` shares the pool.
fn client() -> Result<reqwest::Client> {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    if let Some(c) = CLIENT.get() {
        return Ok(c.clone());
    }
    let c = reqwest::Client::builder()
        .timeout(Duration::from_secs(180))
        // Idle connections are kept for a burst of requests (lint, translate, retry), not forever.
        .pool_idle_timeout(Duration::from_secs(90))
        .build()?;
    Ok(CLIENT.get_or_init(|| c).clone())
}

/// Non-streaming completion. With `json` the provider is asked for a JSON object.
pub async fn complete(cfg: &LlmConfig, system: &str, messages: &[Message], json: bool, temperature: f32) -> Result<String> {
    match cfg.provider {
        Provider::Ollama => ollama_complete(cfg, system, messages, json, temperature).await,
        Provider::OpenAI => openai_complete(cfg, system, messages, json, temperature).await,
        Provider::Anthropic => anthropic_complete(cfg, system, messages, json, temperature).await,
        Provider::Gemini => gemini_complete(cfg, system, messages, json, temperature).await,
        Provider::None => Err(anyhow!("No AI set up yet. Open Settings → AI and paste a key.")),
    }
}

/// Streaming completion; `on_delta` is called with each text chunk. Returns the full text.
pub async fn stream<F: FnMut(&str)>(cfg: &LlmConfig, system: &str, messages: &[Message], temperature: f32, mut on_delta: F) -> Result<String> {
    match cfg.provider {
        Provider::Ollama => ollama_stream(cfg, system, messages, temperature, &mut on_delta).await,
        Provider::OpenAI => openai_stream(cfg, system, messages, temperature, &mut on_delta).await,
        Provider::Anthropic => anthropic_stream(cfg, system, messages, temperature, &mut on_delta).await,
        Provider::Gemini => gemini_stream(cfg, system, messages, temperature, &mut on_delta).await,
        Provider::None => Err(anyhow!("No AI set up yet. Open Settings → AI and paste a key.")),
    }
}

/// Ask for a JSON object and parse it, tolerating prose around the braces.
pub async fn complete_json(cfg: &LlmConfig, system: &str, user: &str, temperature: f32) -> Result<Value> {
    let messages = vec![msg("user", user)];
    let mut last_err = None;
    for attempt in 0..2 {
        let sys = if attempt == 0 { system.to_string() } else { format!("{system}\nReturn ONLY a valid JSON object. No prose, no markdown fences.") };
        match complete(cfg, &sys, &messages, true, temperature).await {
            Ok(text) => match extract_json(&text) {
                Some(v) => return Ok(v),
                None => last_err = Some(anyhow!("model did not return JSON: {}", text.chars().take(200).collect::<String>())),
            },
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow!("no response")))
}

pub fn extract_json(text: &str) -> Option<Value> {
    if let Ok(v) = serde_json::from_str::<Value>(text.trim()) {
        if v.is_object() || v.is_array() {
            return Some(v);
        }
    }
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end <= start {
        return None;
    }
    serde_json::from_str::<Value>(&text[start..=end]).ok()
}

fn with_system(system: &str, messages: &[Message]) -> Vec<Value> {
    let mut v = Vec::with_capacity(messages.len() + 1);
    if !system.is_empty() {
        v.push(json!({"role": "system", "content": system}));
    }
    for m in messages {
        v.push(json!({"role": m.role, "content": m.content}));
    }
    v
}

fn base(cfg: &LlmConfig) -> String {
    cfg.base_url.trim_end_matches('/').to_string()
}

// ---------------- Ollama ----------------

async fn ollama_complete(cfg: &LlmConfig, system: &str, messages: &[Message], json: bool, temperature: f32) -> Result<String> {
    let mut body = json!({
        "model": cfg.model,
        "messages": with_system(system, messages),
        "stream": false,
        // Thinking models (qwen3.5) would spend most of a check reasoning first.
        "think": false,
        "options": {"temperature": temperature, "num_ctx": 8192},
    });
    if json {
        body["format"] = json!("json");
    }
    let res = client()?.post(format!("{}/api/chat", base(cfg))).json(&body).send().await.context("Ollama is not reachable")?;
    let status = res.status();
    let v: Value = res.json().await?;
    if !status.is_success() {
        return Err(anyhow!("Ollama error {}: {}", status, v));
    }
    Ok(v["message"]["content"].as_str().unwrap_or("").to_string())
}

async fn ollama_stream<F: FnMut(&str)>(cfg: &LlmConfig, system: &str, messages: &[Message], temperature: f32, on_delta: &mut F) -> Result<String> {
    let body = json!({
        "model": cfg.model,
        "messages": with_system(system, messages),
        "stream": true,
        "think": false,
        "options": {"temperature": temperature, "num_ctx": 8192},
    });
    let res = client()?.post(format!("{}/api/chat", base(cfg))).json(&body).send().await.context("Ollama is not reachable")?;
    if !res.status().is_success() {
        let t = res.text().await.unwrap_or_default();
        return Err(anyhow!("Ollama error: {t}"));
    }
    let mut full = String::new();
    let mut buf = String::new();
    let mut s = res.bytes_stream();
    while let Some(chunk) = s.next().await {
        let chunk = chunk?;
        buf.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(pos) = buf.find('\n') {
            let line = buf[..pos].trim().to_string();
            buf.drain(..=pos);
            if line.is_empty() {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<Value>(&line) {
                if let Some(t) = v["message"]["content"].as_str() {
                    if !t.is_empty() {
                        full.push_str(t);
                        on_delta(t);
                    }
                }
                if v["done"].as_bool() == Some(true) {
                    return Ok(full);
                }
            }
        }
    }
    Ok(full)
}

pub async fn ollama_models(base_url: &str) -> Result<Vec<String>> {
    let v: Value = client()?.get(format!("{}/api/tags", base_url.trim_end_matches('/'))).send().await?.json().await?;
    Ok(v["models"].as_array().map(|a| a.iter().filter_map(|m| m["name"].as_str().map(String::from)).collect()).unwrap_or_default())
}

// ---------------- OpenAI-compatible ----------------

fn openai_url(cfg: &LlmConfig) -> String {
    openai_endpoint(&base(cfg), "chat/completions")
}

/// OpenAI-compatible roots differ: `https://api.openai.com` and
/// `https://api.groq.com/openai` want `/v1/…` appended, OpenRouter's root
/// already ends in `/v1`, Google's compatibility root is `…/v1beta/openai`.
fn openai_endpoint(base: &str, path: &str) -> String {
    let b = base.trim_end_matches('/');
    if b.ends_with("/v1") || b.contains("/v1beta") {
        format!("{b}/{path}")
    } else {
        format!("{b}/v1/{path}")
    }
}

/// `GET /models` on an OpenAI-compatible root (ids only).
pub async fn openai_models(cfg: &LlmConfig) -> Result<Vec<String>> {
    let res = client()?.get(openai_endpoint(&base(cfg), "models")).bearer_auth(&cfg.api_key).send().await?;
    let status = res.status();
    let v: Value = res.json().await.unwrap_or(Value::Null);
    if !status.is_success() {
        return Err(anyhow!("API error {}: {}", status, v["error"]["message"].as_str().unwrap_or("")));
    }
    Ok(v["data"].as_array().map(|a| a.iter().filter_map(|m| m["id"].as_str().map(String::from)).collect()).unwrap_or_default())
}

/// `GET /v1/models` on Anthropic (ids only).
pub async fn anthropic_models(cfg: &LlmConfig) -> Result<Vec<String>> {
    let b = if cfg.base_url.contains("api.anthropic.com") || cfg.base_url.trim().is_empty() { "https://api.anthropic.com".to_string() } else { base(cfg) };
    let res = client()?.get(format!("{}/v1/models?limit=100", b.trim_end_matches('/'))).header("x-api-key", &cfg.api_key).header("anthropic-version", "2023-06-01").send().await?;
    let status = res.status();
    let v: Value = res.json().await.unwrap_or(Value::Null);
    if !status.is_success() {
        return Err(anyhow!("Anthropic error {}: {}", status, v["error"]["message"].as_str().unwrap_or("")));
    }
    Ok(v["data"].as_array().map(|a| a.iter().filter_map(|m| m["id"].as_str().map(String::from)).collect()).unwrap_or_default())
}

async fn openai_complete(cfg: &LlmConfig, system: &str, messages: &[Message], json: bool, temperature: f32) -> Result<String> {
    let mut cap: Option<u64> = None;
    let mut retried_sampling = false;
    loop {
        let mut body = json!({"model": cfg.model, "messages": with_system(system, messages)});
        if !OPENAI_NO_SAMPLING.load(Ordering::Relaxed) {
            body["temperature"] = json!(temperature);
        }
        if json {
            body["response_format"] = json!({"type": "json_object"});
        }
        if let Some(n) = cap {
            body["max_tokens"] = json!(n);
        }
        let res = client()?.post(openai_url(cfg)).bearer_auth(&cfg.api_key).json(&body).send().await?;
        let status = res.status();
        let v: Value = res.json().await?;
        if !status.is_success() {
            let text = v.to_string();
            if !retried_sampling && rejects_sampling(&text) {
                retried_sampling = true;
                OPENAI_NO_SAMPLING.store(true, Ordering::Relaxed);
                continue;
            }
            if cap.is_none() {
                if let Some(n) = affordable_tokens(&text) {
                    cap = Some(n);
                    continue;
                }
            }
            return Err(anyhow!("API error {}: {}", status, v["error"]["message"].as_str().unwrap_or(&text)));
        }
        return Ok(v["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string());
    }
}

async fn openai_stream<F: FnMut(&str)>(cfg: &LlmConfig, system: &str, messages: &[Message], temperature: f32, on_delta: &mut F) -> Result<String> {
    let mut cap: Option<u64> = None;
    let mut retried_sampling = false;
    let res = loop {
        let mut body = json!({"model": cfg.model, "messages": with_system(system, messages), "stream": true});
        if !OPENAI_NO_SAMPLING.load(Ordering::Relaxed) {
            body["temperature"] = json!(temperature);
        }
        if let Some(n) = cap {
            body["max_tokens"] = json!(n);
        }
        let r = client()?.post(openai_url(cfg)).bearer_auth(&cfg.api_key).json(&body).send().await?;
        if !r.status().is_success() {
            let t = r.text().await.unwrap_or_default();
            if !retried_sampling && rejects_sampling(&t) {
                retried_sampling = true;
                OPENAI_NO_SAMPLING.store(true, Ordering::Relaxed);
                continue;
            }
            if cap.is_none() {
                if let Some(n) = affordable_tokens(&t) {
                    cap = Some(n);
                    continue;
                }
            }
            return Err(anyhow!("API error: {t}"));
        }
        break r;
    };
    let mut full = String::new();
    let mut buf = String::new();
    let mut s = res.bytes_stream();
    while let Some(chunk) = s.next().await {
        buf.push_str(&String::from_utf8_lossy(&chunk?));
        while let Some(pos) = buf.find('\n') {
            let line = buf[..pos].trim().to_string();
            buf.drain(..=pos);
            if let Some(data) = line.strip_prefix("data:") {
                let data = data.trim();
                if data == "[DONE]" {
                    return Ok(full);
                }
                if let Ok(v) = serde_json::from_str::<Value>(data) {
                    if let Some(t) = v["choices"][0]["delta"]["content"].as_str() {
                        full.push_str(t);
                        on_delta(t);
                    }
                }
            }
        }
    }
    Ok(full)
}

// ---------------- Google Gemini (native API) ----------------
// AI Studio's newer keys (`AQ.`) are refused by Google's OpenAI-compatible
// route; the native API takes both those and the older `AIza` keys in the
// `x-goog-api-key` header.

fn gemini_body(system: &str, messages: &[Message], json: bool, temperature: f32) -> Value {
    let contents: Vec<Value> = messages
        .iter()
        .map(|m| json!({"role": if m.role == "assistant" { "model" } else { "user" }, "parts": [{"text": m.content}]}))
        .collect();
    let mut body = json!({"contents": contents, "generationConfig": {"temperature": temperature}});
    if !system.is_empty() {
        body["systemInstruction"] = json!({"parts": [{"text": system}]});
    }
    if json {
        body["generationConfig"]["responseMimeType"] = json!("application/json");
    }
    body
}

/// The answer text of one response (or one stream chunk); thinking parts are not part of it.
fn gemini_text(v: &Value) -> String {
    v["candidates"][0]["content"]["parts"]
        .as_array()
        .map(|parts| parts.iter().filter(|p| p["thought"].as_bool() != Some(true)).filter_map(|p| p["text"].as_str()).collect())
        .unwrap_or_default()
}

fn gemini_model_names(v: &Value) -> Vec<String> {
    v["models"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|m| m["supportedGenerationMethods"].as_array().is_some_and(|g| g.iter().any(|x| x == "generateContent")))
                .filter_map(|m| m["name"].as_str().map(|n| n.trim_start_matches("models/").to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// Stable `gemini-<version>-flash` models (no -lite, -preview, -image…) from
/// what the key's account lists, newest first.
pub fn gemini_flash_candidates(names: &[String]) -> Vec<String> {
    let mut found: Vec<(Vec<u32>, &String)> = names
        .iter()
        .filter_map(|n| {
            let ver = n.strip_prefix("gemini-")?.strip_suffix("-flash")?;
            let parts: Option<Vec<u32>> = ver.split('.').map(|p| p.parse().ok()).collect();
            Some((parts?, n))
        })
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.into_iter().map(|(_, n)| n.clone()).collect()
}

pub fn newest_gemini_flash(names: &[String]) -> Option<String> {
    gemini_flash_candidates(names).into_iter().next()
}

/// 503 "high demand" (Google sheds load per model) or 429 "quota exceeded"
/// (free-tier quotas are counted per model): another Flash model may answer.
fn gemini_try_another(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::SERVICE_UNAVAILABLE || status == reqwest::StatusCode::TOO_MANY_REQUESTS
}


/// Google retires models for new keys with "... update your code to use
/// models/<name> ...": that name.
pub fn suggested_gemini_model(err: &str) -> Option<String> {
    let rest = &err[err.find("to use models/")? + "to use models/".len()..];
    let name: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_')).collect();
    let name = name.trim_end_matches('.').to_string();
    (!name.is_empty()).then_some(name)
}

fn gemini_error(status: reqwest::StatusCode, v: &Value) -> anyhow::Error {
    anyhow!("Gemini error {}: {}", status, v["error"]["message"].as_str().unwrap_or(&v.to_string()))
}

pub async fn gemini_models(cfg: &LlmConfig) -> Result<Vec<String>> {
    let res = client()?.get(format!("{}/models?pageSize=1000", base(cfg))).header("x-goog-api-key", &cfg.api_key).send().await?;
    let status = res.status();
    let v: Value = res.json().await?;
    if !status.is_success() {
        return Err(gemini_error(status, &v));
    }
    Ok(gemini_model_names(&v))
}

/// How long a Gemini model may stay silent before the next one is tried. A
/// busy model can hang without answering at all (seen: 3.8 Flash, 90 s).
const GEMINI_FIRST_CHUNK: Duration = Duration::from_secs(20);
/// Models tried per request at most.
const GEMINI_MAX_MODELS: usize = 4;

/// (configured model, model that answered for it) for this session, so only
/// the first request after a model goes quiet pays the wait.
static GEMINI_WORKING: std::sync::Mutex<Option<(String, String)>> = std::sync::Mutex::new(None);
/// The key's model list, fetched once, the first time a model fails.
static GEMINI_LISTED: std::sync::Mutex<Option<(String, Vec<String>)>> = std::sync::Mutex::new(None);

/// The models to try, in order: the one that last answered for `configured`
/// (else `configured`), then the stable Flash models, newest first.
fn gemini_order(configured: &str, working: Option<&str>, listed: &[String]) -> Vec<String> {
    let mut out = vec![working.unwrap_or(configured).to_string()];
    for m in gemini_flash_candidates(listed) {
        if !out.contains(&m) {
            out.push(m);
        }
    }
    out.truncate(GEMINI_MAX_MODELS);
    out
}

/// Gemini 3 models think before answering by default; Oumu's tasks (fixes,
/// rewrites, translation) do not need it, and "low" answers about a third sooner.
fn with_thinking(mut body: Value, model: &str) -> Value {
    if model.starts_with("gemini-3") {
        body["generationConfig"]["thinkingConfig"] = json!({"thinkingLevel": "low"});
    }
    body
}

async fn gemini_listed(cfg: &LlmConfig) -> Vec<String> {
    if let Some((k, v)) = GEMINI_LISTED.lock().ok().and_then(|g| g.clone()) {
        if k == cfg.api_key {
            return v;
        }
    }
    let v = gemini_models(cfg).await.unwrap_or_default();
    if let Ok(mut g) = GEMINI_LISTED.lock() {
        *g = Some((cfg.api_key.clone(), v.clone()));
    }
    v
}

/// The model that answered in place of `configured` this session, if one did.
pub fn gemini_working_for(configured: &str) -> Option<String> {
    GEMINI_WORKING.lock().ok().and_then(|g| g.clone()).filter(|(c, _)| c == configured).map(|(_, w)| w)
}

type ByteStream = std::pin::Pin<Box<dyn futures_util::Stream<Item = reqwest::Result<Vec<u8>>> + Send>>;

enum Attempt {
    Answered(ByteStream, Vec<u8>),
    /// Busy, silent, retired or missing: worth trying another model.
    Skip(String),
    /// The key or the request is wrong: another model will not help.
    Fail(anyhow::Error),
}

async fn gemini_attempt(cfg: &LlmConfig, model: &str, body: &Value) -> Attempt {
    let url = format!("{}/models/{}:streamGenerateContent?alt=sse", base(cfg), model);
    let open = async {
        let res = client()?.post(url).header("x-goog-api-key", &cfg.api_key).json(&with_thinking(body.clone(), model)).send().await?;
        let status = res.status();
        if !status.is_success() {
            let v: Value = res.json().await.unwrap_or_default();
            return Ok(Err((status, gemini_error(status, &v))));
        }
        let mut s: ByteStream = Box::pin(res.bytes_stream().map(|c| c.map(|b| b.to_vec())));
        let first = s.next().await.transpose()?.unwrap_or_default();
        anyhow::Ok(Ok((s, first)))
    };
    match tokio::time::timeout(GEMINI_FIRST_CHUNK, open).await {
        Err(_) => Attempt::Skip(format!("{model} did not answer within {} s", GEMINI_FIRST_CHUNK.as_secs())),
        Ok(Err(e)) => Attempt::Skip(format!("{model}: {e}")),
        Ok(Ok(Ok((s, first)))) => Attempt::Answered(s, first),
        Ok(Ok(Err((status, e)))) => {
            let retired = suggested_gemini_model(&e.to_string()).is_some() || status == reqwest::StatusCode::NOT_FOUND;
            if gemini_try_another(status) || retired {
                Attempt::Skip(e.to_string())
            } else {
                Attempt::Fail(e)
            }
        }
    }
}

/// Stream one answer, moving to the next model when one is busy, silent or
/// retired, and remembering the model that answered.
async fn gemini_run<F: FnMut(&str)>(cfg: &LlmConfig, body: &Value, on_delta: &mut F) -> Result<String> {
    let working = gemini_working_for(&cfg.model);
    let mut order = vec![working.clone().unwrap_or_else(|| cfg.model.clone())];
    let mut listed_loaded = false;
    let mut last = String::new();
    let mut i = 0;
    while i < order.len() && i < GEMINI_MAX_MODELS {
        let model = order[i].clone();
        match gemini_attempt(cfg, &model, body).await {
            Attempt::Answered(s, first) => {
                if model != cfg.model && working.as_deref() != Some(model.as_str()) {
                    log::warn!("Gemini {} is not answering; using {model} for now. Pick it in Settings → AI to keep it.", cfg.model);
                }
                if let Ok(mut g) = GEMINI_WORKING.lock() {
                    *g = (model != cfg.model).then(|| (cfg.model.clone(), model.clone()));
                }
                return gemini_read(s, first, on_delta).await;
            }
            Attempt::Fail(e) => return Err(e),
            Attempt::Skip(why) => {
                log::warn!("Gemini: {why}");
                if let Some(next) = suggested_gemini_model(&why) {
                    if !order.contains(&next) {
                        order.insert(i + 1, next);
                    }
                }
                if !listed_loaded {
                    listed_loaded = true;
                    for m in gemini_order(&cfg.model, None, &gemini_listed(cfg).await) {
                        if !order.contains(&m) {
                            order.push(m);
                        }
                    }
                }
                last = why;
            }
        }
        i += 1;
    }
    Err(anyhow!("Google's Gemini models are busy right now; try again in a minute. ({last})"))
}

async fn gemini_read<F: FnMut(&str)>(mut s: ByteStream, first: Vec<u8>, on_delta: &mut F) -> Result<String> {
    let mut full = String::new();
    let mut buf = String::from_utf8_lossy(&first).into_owned();
    loop {
        while let Some(pos) = buf.find('\n') {
            let line = buf[..pos].trim().to_string();
            buf.drain(..=pos);
            if let Some(v) = line.strip_prefix("data:").and_then(|d| serde_json::from_str::<Value>(d.trim()).ok()) {
                let t = gemini_text(&v);
                if !t.is_empty() {
                    full.push_str(&t);
                    on_delta(&t);
                }
            }
        }
        match s.next().await {
            Some(chunk) => buf.push_str(&String::from_utf8_lossy(&chunk?)),
            None => break,
        }
    }
    Ok(full)
}

async fn gemini_complete(cfg: &LlmConfig, system: &str, messages: &[Message], json: bool, temperature: f32) -> Result<String> {
    gemini_run(cfg, &gemini_body(system, messages, json, temperature), &mut |_| {}).await
}

async fn gemini_stream<F: FnMut(&str)>(cfg: &LlmConfig, system: &str, messages: &[Message], temperature: f32, on_delta: &mut F) -> Result<String> {
    gemini_run(cfg, &gemini_body(system, messages, false, temperature), on_delta).await
}

// ---------------- Anthropic ----------------

fn anthropic_url(cfg: &LlmConfig) -> String {
    let b = if cfg.base_url.contains("api.anthropic.com") || cfg.base_url.trim().is_empty() { "https://api.anthropic.com".to_string() } else { base(cfg) };
    format!("{}/v1/messages", b.trim_end_matches('/'))
}

fn anthropic_messages(messages: &[Message]) -> Vec<Value> {
    messages.iter().map(|m| json!({"role": if m.role == "assistant" { "assistant" } else { "user" }, "content": m.content})).collect()
}

async fn anthropic_complete(cfg: &LlmConfig, system: &str, messages: &[Message], json: bool, temperature: f32) -> Result<String> {
    let sys = if json { format!("{system}\nRespond with a single JSON object and nothing else.") } else { system.to_string() };
    for attempt in 0..2 {
        let mut body = json!({"model": cfg.model, "max_tokens": 4096, "system": sys, "messages": anthropic_messages(messages)});
        if !ANTHROPIC_NO_SAMPLING.load(Ordering::Relaxed) {
            body["temperature"] = json!(temperature);
        }
        let res = client()?
            .post(anthropic_url(cfg))
            .header("x-api-key", &cfg.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await?;
        let status = res.status();
        let v: Value = res.json().await?;
        if !status.is_success() {
            if attempt == 0 && rejects_sampling(&v.to_string()) {
                ANTHROPIC_NO_SAMPLING.store(true, Ordering::Relaxed);
                continue;
            }
            return Err(anyhow!("Anthropic error {}: {}", status, v["error"]["message"].as_str().unwrap_or(&v.to_string())));
        }
        return Ok(v["content"][0]["text"].as_str().unwrap_or("").to_string());
    }
    unreachable!()
}

async fn anthropic_stream<F: FnMut(&str)>(cfg: &LlmConfig, system: &str, messages: &[Message], temperature: f32, on_delta: &mut F) -> Result<String> {
    let mut res = None;
    for attempt in 0..2 {
        let mut body = json!({"model": cfg.model, "max_tokens": 4096, "system": system, "messages": anthropic_messages(messages), "stream": true});
        if !ANTHROPIC_NO_SAMPLING.load(Ordering::Relaxed) {
            body["temperature"] = json!(temperature);
        }
        let r = client()?
            .post(anthropic_url(cfg))
            .header("x-api-key", &cfg.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await?;
        if !r.status().is_success() {
            let t = r.text().await.unwrap_or_default();
            if attempt == 0 && rejects_sampling(&t) {
                ANTHROPIC_NO_SAMPLING.store(true, Ordering::Relaxed);
                continue;
            }
            return Err(anyhow!("Anthropic error: {t}"));
        }
        res = Some(r);
        break;
    }
    let res = res.ok_or_else(|| anyhow!("no response"))?;
    let mut full = String::new();
    let mut buf = String::new();
    let mut s = res.bytes_stream();
    while let Some(chunk) = s.next().await {
        buf.push_str(&String::from_utf8_lossy(&chunk?));
        while let Some(pos) = buf.find('\n') {
            let line = buf[..pos].trim().to_string();
            buf.drain(..=pos);
            if let Some(data) = line.strip_prefix("data:") {
                if let Ok(v) = serde_json::from_str::<Value>(data.trim()) {
                    if v["type"] == "content_block_delta" {
                        if let Some(t) = v["delta"]["text"].as_str() {
                            full.push_str(t);
                            on_delta(t);
                        }
                    }
                    if v["type"] == "message_stop" {
                        return Ok(full);
                    }
                }
            }
        }
    }
    Ok(full)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gemini_body_maps_roles_system_and_json_mode() {
        let b = gemini_body("Be brief.", &[msg("user", "hi"), msg("assistant", "hello"), msg("user", "again")], true, 0.2);
        assert_eq!(b["systemInstruction"]["parts"][0]["text"], "Be brief.");
        let roles: Vec<&str> = b["contents"].as_array().unwrap().iter().map(|c| c["role"].as_str().unwrap()).collect();
        assert_eq!(roles, ["user", "model", "user"]);
        assert_eq!(b["contents"][1]["parts"][0]["text"], "hello");
        assert_eq!(b["generationConfig"]["responseMimeType"], "application/json");
        assert!((b["generationConfig"]["temperature"].as_f64().unwrap() - 0.2).abs() < 1e-6);
        let plain = gemini_body("", &[msg("user", "hi")], false, 0.0);
        assert!(plain.get("systemInstruction").is_none());
        assert!(plain["generationConfig"].get("responseMimeType").is_none());
    }

    #[test]
    fn picks_the_newest_stable_gemini_flash() {
        let names: Vec<String> = ["gemini-2.5-flash", "gemini-3.8-flash", "gemini-3.8-flash-lite", "gemini-3.9-flash-preview-11-2026", "gemini-3.8-pro", "gemini-3.8-flash-image", "gemma-3-27b-it", "gemini-3.10-flash"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(newest_gemini_flash(&names).as_deref(), Some("gemini-3.10-flash"));
        assert_eq!(gemini_flash_candidates(&names), ["gemini-3.10-flash", "gemini-3.8-flash", "gemini-2.5-flash"]);
        assert_eq!(newest_gemini_flash(&["gemma-3-27b-it".to_string()]), None);
    }

    #[test]
    fn reads_the_replacement_model_google_suggests() {
        let e = "Gemini error 404 Not Found: This model models/gemini-2.5-flash is no longer available to new users. Please update your code to use models/gemini-3.8-flash for the latest features and improvements.";
        assert_eq!(suggested_gemini_model(e).as_deref(), Some("gemini-3.8-flash"));
        assert_eq!(suggested_gemini_model("Gemini error 400: API key not valid."), None);
    }

    #[test]
    fn gemini_tries_the_remembered_model_then_older_flash_models() {
        let listed: Vec<String> = ["gemini-3.8-flash", "gemini-3.7-flash", "gemini-3.6-flash", "gemini-3.5-flash", "gemini-2.5-flash"].iter().map(|s| s.to_string()).collect();
        assert_eq!(gemini_order("gemini-3.8-flash", None, &listed), ["gemini-3.8-flash", "gemini-3.7-flash", "gemini-3.6-flash", "gemini-3.5-flash"]);
        // Once 3.6 answered for the configured 3.8, start there.
        assert_eq!(gemini_order("gemini-3.8-flash", Some("gemini-3.6-flash"), &listed)[0], "gemini-3.6-flash");
        // A pro model the user picked is tried first, then the Flash fallbacks.
        assert_eq!(gemini_order("gemini-3.8-pro", None, &listed)[..2], ["gemini-3.8-pro", "gemini-3.8-flash"]);
        assert_eq!(gemini_order("gemini-3.8-flash", None, &[]), ["gemini-3.8-flash"]);
    }

    #[test]
    fn gemini_3_models_think_low_and_older_ones_are_left_alone() {
        let b = gemini_body("", &[msg("user", "hi")], false, 0.0);
        assert_eq!(with_thinking(b.clone(), "gemini-3.6-flash")["generationConfig"]["thinkingConfig"]["thinkingLevel"], "low");
        assert!(with_thinking(b, "gemini-2.5-flash")["generationConfig"].get("thinkingConfig").is_none());
    }

    #[test]
    fn busy_or_out_of_quota_models_hand_over_but_bad_requests_do_not() {
        assert!(gemini_try_another(reqwest::StatusCode::SERVICE_UNAVAILABLE));
        // Free-tier quotas are counted per model: another Flash model may still have some.
        assert!(gemini_try_another(reqwest::StatusCode::TOO_MANY_REQUESTS));
        assert!(!gemini_try_another(reqwest::StatusCode::BAD_REQUEST));
        assert!(!gemini_try_another(reqwest::StatusCode::UNAUTHORIZED));
    }

    #[test]
    fn gemini_text_skips_thoughts_and_joins_parts() {
        let v = json!({"candidates": [{"content": {"parts": [{"text": "plan", "thought": true}, {"text": "po"}, {"text": "ng"}]}}]});
        assert_eq!(gemini_text(&v), "pong");
        assert_eq!(gemini_text(&json!({"candidates": []})), "");
    }

    #[test]
    fn gemini_model_names_drop_the_prefix_and_keep_chat_models() {
        let v = json!({"models": [
            {"name": "models/gemini-2.5-flash", "supportedGenerationMethods": ["generateContent", "countTokens"]},
            {"name": "models/text-embedding-004", "supportedGenerationMethods": ["embedContent"]}
        ]});
        assert_eq!(gemini_model_names(&v), ["gemini-2.5-flash"]);
    }

    #[test]
    fn reads_the_affordable_token_count_from_openrouter_402() {
        let msg = "This request requires more credits, or fewer max_tokens. You requested up to 65536 tokens, but can only afford 3333. To increase, visit https://openrouter.ai/settings/credits";
        assert_eq!(affordable_tokens(msg), Some(3333));
        assert_eq!(affordable_tokens("Insufficient credits"), None);
        assert_eq!(affordable_tokens("can only afford 0."), None);
    }

    #[test]
    fn extract_json_tolerates_prose_and_fences() {
        let v = extract_json("Sure! ```json\n{\"a\": 1}\n```").unwrap();
        assert_eq!(v["a"], 1);
        assert!(extract_json("no braces here").is_none());
        let v = extract_json("{\"x\":[1,2]}").unwrap();
        assert_eq!(v["x"][1], 2);
    }

    #[test]
    fn sampling_rejections_are_recognised() {
        assert!(rejects_sampling(r#"{"error":{"message":"`temperature` is deprecated for this model.","type":"invalid_request_error"}}"#));
        assert!(rejects_sampling(r#"{"error":{"message":"Unsupported value: 'temperature' does not support 0.2 with this model."}}"#));
        assert!(!rejects_sampling(r#"{"error":{"message":"Rate limit reached"}}"#));
    }

    #[test]
    fn openai_url_handles_v1_suffix() {
        let mut c = LlmConfig { provider: Provider::OpenAI, base_url: "https://openrouter.ai/api/v1".into(), api_key: "".into(), model: "m".into() };
        assert_eq!(openai_url(&c), "https://openrouter.ai/api/v1/chat/completions");
        c.base_url = "https://api.openai.com".into();
        assert_eq!(openai_url(&c), "https://api.openai.com/v1/chat/completions");
        c.base_url = "https://api.groq.com/openai".into();
        assert_eq!(openai_url(&c), "https://api.groq.com/openai/v1/chat/completions");
        c.base_url = "https://generativelanguage.googleapis.com/v1beta/openai".into();
        assert_eq!(openai_url(&c), "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions");
        assert_eq!(openai_endpoint("https://openrouter.ai/api/v1/", "models"), "https://openrouter.ai/api/v1/models");
    }
}

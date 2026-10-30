//! Persistent settings (JSON under Application Support).

use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;

/// The app's folder name under ~/.config (palette file) and the like.
pub const APP_DIR: &str = "oumu";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Ollama,
    OpenAI,
    Anthropic,
    /// Google's native Gemini API (AI Studio keys, `AIza` and `AQ.`).
    Gemini,
    /// No model set up yet. Also what an unknown provider in an older file
    /// loads as, so the rest of the file survives.
    #[serde(other)]
    None,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct LlmConfig {
    pub provider: Provider,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

impl Default for LlmConfig {
    /// Nothing set up: the AI features say to paste a key in Settings → AI.
    fn default() -> Self {
        LlmConfig { provider: Provider::None, base_url: String::new(), api_key: String::new(), model: String::new() }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Voice {
    pub id: String,
    pub name: String,
    pub description: String,
    pub custom: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Audience {
    pub id: String,
    pub name: String,
    pub description: String,
    pub custom: bool,
}

/// Plagiarism search, all optional: `brave_key` is the user's own Brave Search API
/// key, `searxng_url` the address of a SearXNG instance (usually self-hosted).
/// Neither set means the keyless chain (features/plagiarism.rs `search_route`).
/// Older files also carried a `provider` field; serde drops it.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SearchConfig {
    pub brave_key: String,
    pub searxng_url: String,
}

/// An app the user switched Oumu off in. Terminals are skipped without
/// being listed here (`host::BUILT_IN_SKIP`).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct SkipApp {
    pub bundle_id: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub llm: LlmConfig,
    pub dock_side: String, // "left" | "right"
    pub rail_side: String, // "right" | "left"
    pub user_name: String,
    pub search: SearchConfig,
    pub custom_voices: Vec<Voice>,
    pub custom_audiences: Vec<Audience>,
    pub dismissed: HashSet<u64>,
    /// Words added from a spelling card; never flagged as misspelled again (lowercase).
    pub dictionary: BTreeSet<String>,
    /// Checks turned off from a card (`Alert::rule`).
    pub muted_rules: BTreeSet<String>,
    /// Dismissals per rule, so the card can offer to turn a rule off.
    pub rule_dismissals: HashMap<String, u32>,
    pub target_language: String,
    pub citation_style: String,
    /// The floating Oumu button by the text field (older files call it `show_gbutton`).
    #[serde(alias = "show_gbutton")]
    pub show_button: bool,
    pub show_underlines: bool,
    pub onboarded: bool,
    /// Run the sentence spot check by itself when typing pauses.
    pub auto_spot_check: bool,
    /// Apps Oumu stays out of (added from the Review view's "Skip this app").
    pub skip_apps: Vec<SkipApp>,
    /// Colour theme id (ui/src/lib/theme.ts), or "matugen" to follow a palette file.
    pub theme: String,
    /// The matugen palette file; empty means Ryoku's, else ~/.config/<app>/colors.json.
    pub palette_path: String,
}

impl Settings {
    /// Whether the user said not to show this alert: dismissed, its check
    /// turned off, or a spelling note on a word they added to the dictionary.
    pub fn hides(&self, a: &crate::engine::Alert) -> bool {
        self.dismissed.contains(&a.stable_hash)
            || self.muted_rules.contains(&a.rule)
            || (a.kind == "Spelling" && self.dictionary.contains(&a.original.trim().to_lowercase()))
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            llm: LlmConfig::default(),
            dock_side: "left".into(),
            rail_side: "right".into(),
            user_name: default_user_name(),
            search: SearchConfig::default(),
            custom_voices: vec![],
            custom_audiences: vec![],
            dismissed: HashSet::new(),
            dictionary: BTreeSet::new(),
            muted_rules: BTreeSet::new(),
            rule_dismissals: HashMap::new(),
            target_language: "English (American)".into(),
            citation_style: "APA".into(),
            show_button: true,
            show_underlines: true,
            onboarded: false,
            auto_spot_check: true,
            skip_apps: vec![],
            theme: "default".into(),
            palette_path: String::new(),
        }
    }
}

/// The account's full name: `id -F` on macOS, the passwd GECOS field on Linux.
fn account_full_name() -> Option<String> {
    #[cfg(target_os = "macos")]
    let out = std::process::Command::new("id").arg("-F").output().ok()?;
    #[cfg(not(target_os = "macos"))]
    let out = std::process::Command::new("getent").args(["passwd", &std::env::var("USER").ok()?]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let raw = String::from_utf8_lossy(&out.stdout).trim().to_string();
    #[cfg(not(target_os = "macos"))]
    let raw = gecos_name(&raw)?;
    Some(raw)
}

/// `name:x:1000:1000:Full Name,room,phone:/home/name:/bin/zsh` -> `Full Name`.
#[cfg_attr(target_os = "macos", allow(dead_code))]
fn gecos_name(passwd_line: &str) -> Option<String> {
    let gecos = passwd_line.split(':').nth(4)?;
    let name = gecos.split(',').next()?.trim();
    (!name.is_empty()).then(|| name.to_string())
}

fn default_user_name() -> String {
    // The account's full name ("Alex Example") when the system keeps one.
    if let Some(full) = account_full_name() {
        if full.split_whitespace().count() >= 1 && full.len() <= 60 && !full.is_empty() {
            return full;
        }
    }
    std::env::var("USER").map(|u| {
        let mut c = u.chars();
        match c.next() {
            Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            None => "Writer".into(),
        }
    }).unwrap_or_else(|_| "Writer".into())
}

pub fn initials(name: &str) -> String {
    let parts: Vec<&str> = name.split_whitespace().collect();
    let mut s = String::new();
    for p in parts.iter().take(2) {
        if let Some(c) = p.chars().next() {
            s.push(c.to_ascii_uppercase());
        }
    }
    if s.is_empty() {
        "O".into()
    } else {
        s
    }
}

/// The data folder (settings, history) under ~/.local/share.
const DATA_DIR: &str = "dev.cratusgod.oumu";

pub fn support_dir() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    let dir = base.join(DATA_DIR);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn settings_path() -> PathBuf {
    support_dir().join("settings.json")
}

pub fn load() -> Settings {
    let mut s = match std::fs::read_to_string(settings_path()) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => Settings::default(),
    };
    migrate_user_name(&mut s);
    s
}

/// Older builds saved the capitalized login name ("Alexexample") because they
/// never asked the account for its full name. Replace that stale value with
/// the full name so the avatar initials match the real account ("MB").
fn migrate_user_name(s: &mut Settings) {
    let login_fallback = std::env::var("USER").map(|u| {
        let mut c = u.chars();
        match c.next() {
            Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            None => String::new(),
        }
    }).unwrap_or_default();
    let stale = s.user_name.trim().is_empty()
        || (!login_fallback.is_empty() && s.user_name == login_fallback);
    if stale {
        s.user_name = default_user_name();
    }
}

pub fn save(s: &Settings) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(s)?;
    std::fs::write(settings_path(), json)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    fn alert(kind: &str, rule: &str, original: &str, hash: u64) -> crate::engine::Alert {
        crate::engine::Alert {
            id: format!("{hash:x}-0"),
            category: crate::engine::Category::Correctness,
            kind: kind.into(),
            title: String::new(),
            message: String::new(),
            start: 0,
            end: original.chars().count(),
            u16_start: 0,
            u16_end: original.encode_utf16().count(),
            original: original.into(),
            suggestions: vec![],
            priority: 1,
            stable_hash: hash,
            rule: rule.into(),
            rule_dismissals: 0,
        }
    }

    #[test]
    fn hides_dismissed_alerts_turned_off_rules_and_learned_words() {
        let mut s = Settings::default();
        s.dismissed.insert(1);
        s.muted_rules.insert("SentenceCapitalization".into());
        s.dictionary.insert("oumu".into());
        assert!(s.hides(&alert("Spelling", "SpellCheck", "teh", 1)), "dismissed");
        assert!(s.hides(&alert("Capitalization", "SentenceCapitalization", "hello", 2)), "rule off");
        assert!(s.hides(&alert("Spelling", "SpellCheck", "Oumu ", 3)), "learned word, any case");
        assert!(!s.hides(&alert("Spelling", "SpellCheck", "teh", 4)), "same word, not dismissed");
        assert!(!s.hides(&alert("Style", "Other", "oumu", 5)), "the dictionary only covers spelling");
    }

    #[test]
    fn older_settings_files_load_without_the_learned_lists() {
        let s: Settings = serde_json::from_str(r#"{"dismissed":[7]}"#).unwrap();
        assert!(s.dictionary.is_empty() && s.muted_rules.is_empty() && s.rule_dismissals.is_empty());
    }

    #[test]
    fn gecos_full_name_is_the_first_comma_field() {
        assert_eq!(super::gecos_name("hilda:x:1000:1000:Hilda N,,,:/home/hilda:/bin/zsh").as_deref(), Some("Hilda N"));
        assert_eq!(super::gecos_name("hilda:x:1000:1000::/home/hilda:/bin/zsh"), None);
        assert_eq!(super::gecos_name("garbage"), None);
    }

    use super::*;

    #[test]
    fn initials_take_first_two_words() {
        assert_eq!(initials("Alex Example"), "AE");
        assert_eq!(initials("alex"), "A");
        assert_eq!(initials(""), "O");
        assert_eq!(initials("a b c"), "AB");
    }

    #[test]
    fn migrate_replaces_stale_login_name_but_keeps_custom_name() {
        let login = std::env::var("USER").unwrap_or_default();
        let mut c = login.chars();
        let stale = match c.next() {
            Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            None => String::new(),
        };
        let mut s = Settings::default();
        s.user_name = stale;
        migrate_user_name(&mut s);
        assert_eq!(s.user_name, default_user_name());

        let mut custom = Settings::default();
        custom.user_name = "Alice Example".into();
        migrate_user_name(&mut custom);
        assert_eq!(custom.user_name, "Alice Example");

        let mut empty = Settings::default();
        empty.user_name = "  ".into();
        migrate_user_name(&mut empty);
        assert!(!empty.user_name.trim().is_empty());
    }

    #[test]
    fn older_settings_files_load_with_defaults_for_new_fields() {
        let old = r#"{"llm":{"provider":"ollama","base_url":"http://127.0.0.1:11434","api_key":"","model":"qwen2.5:7b"},"dock_side":"right","rail_side":"right","user_name":"Alice Example","search":{"provider":"duckduckgo","brave_key":""},"custom_voices":[],"custom_audiences":[],"dismissed":[7],"target_language":"French","citation_style":"MLA","show_button":true,"show_underlines":true,"onboarded":true}"#;
        let s: Settings = serde_json::from_str(old).expect("old file loads");
        assert_eq!(s.user_name, "Alice Example");
        assert_eq!(s.dock_side, "right");
        assert!(s.dismissed.contains(&7));
        assert!(s.auto_spot_check, "new field takes its default");
    }

    #[test]
    fn a_removed_provider_loads_as_nothing_set_up_and_keeps_the_rest() {
        let old = r#"{"llm":{"provider":"retired","base_url":"https://example.invalid/v1","api_key":"","model":"m"},"dock_side":"right","retired":{"x":1},"dismissed":[9]}"#;
        let s: Settings = serde_json::from_str(old).expect("old file loads");
        assert_eq!(s.llm.provider, Provider::None);
        assert_eq!(s.dock_side, "right", "the rest of the file survives");
        assert!(s.dismissed.contains(&9));
    }

    #[test]
    fn a_local_model_setting_survives_loading() {
        let file = r#"{"llm":{"provider":"ollama","base_url":"http://127.0.0.1:11434","api_key":"","model":"qwen3.5:9b"}}"#;
        let s: Settings = serde_json::from_str(file).unwrap();
        assert_eq!(s.llm.provider, Provider::Ollama);
        assert_eq!(s.llm.model, "qwen3.5:9b");
    }

    #[test]
    fn settings_round_trip_through_json() {
        let mut s = Settings::default();
        s.dismissed.insert(42);
        s.llm.model = "x".into();
        let j = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&j).unwrap();
        assert!(back.dismissed.contains(&42));
        assert_eq!(back.llm.model, "x");
        assert_eq!(back.dock_side, "left");
    }
}

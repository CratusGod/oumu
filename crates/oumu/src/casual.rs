//! Chat apps. In Discord or Telegram, lowercase and "btw" are a register,
//! not mistakes: the capitalisation and formality notes stay out of them.

/// Window classes (Linux) and bundle ids (macOS) of chat apps, lowercase.
const CHAT_APPS: &[&str] = &[
    "discord", "vesktop", "webcord", "legcord", "armcord", "equibop", "com.hnc.discord",
    "org.telegram.desktop", "telegramdesktop", "ru.keepcoder.telegram",
    "signal", "org.whispersystems.signal-desktop",
    "slack", "com.tinyspeck.slackmacgap",
    "element", "im.riot.app", "fractal", "nheko", "cinny",
    "whatsapp", "zapzap", "whatsie", "net.whatsapp.whatsapp", "caprine",
];

/// Notes that only make sense in formal writing.
const FORMAL_ONLY: &[&str] = &["SentenceCapitalization", "CapitalizePersonalPronouns", "ByTheWay", "Tone down the punctuation"];

pub fn is_chat_app(bundle_id: &str) -> bool {
    let id = bundle_id.to_lowercase();
    CHAT_APPS.contains(&id.as_str())
}

/// Whether a note with this rule is left out of a chat app.
pub fn formal_only(rule: &str) -> bool {
    FORMAL_ONLY.contains(&rule)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_apps_are_recognised_by_class_in_any_case() {
        assert!(is_chat_app("vesktop") && is_chat_app("Vesktop") && is_chat_app("org.telegram.desktop"));
        assert!(!is_chat_app("zen") && !is_chat_app("org.gnome.TextEditor") && !is_chat_app(""));
        assert!(formal_only("CapitalizePersonalPronouns") && !formal_only("SpellCheck"));
    }
}

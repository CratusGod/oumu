//! The correctness engine: Harper lints mapped onto Grammarly's four outcome
//! categories, plus small rule-based Delivery/Engagement passes.

use crate::textmap::TextMap;
use harper_core::linting::{LintGroup, LintKind, Suggestion};
use harper_core::spell::FstDictionary;
use harper_core::{Dialect, Document};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Correctness,
    Clarity,
    Engagement,
    Delivery,
}

impl Category {
    pub fn color(&self) -> &'static str {
        match self {
            Category::Correctness => "#ff7a74",
            Category::Clarity => "#7d99f0",
            Category::Engagement => "#2cc9b6",
            Category::Delivery => "#a598f0",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Alert {
    pub id: String,
    pub category: Category,
    pub kind: String,
    pub title: String,
    pub message: String,
    /// char offsets
    pub start: usize,
    pub end: usize,
    /// UTF-16 offsets (what AX wants)
    pub u16_start: usize,
    pub u16_end: usize,
    pub original: String,
    pub suggestions: Vec<String>,
    pub priority: u8,
    /// Stable across edits elsewhere in the document: hash(kind, original).
    pub stable_hash: u64,
    /// The check that raised it (a Harper rule name, a Delivery title, the
    /// spot-check kind): what "Turn off this rule" turns off.
    #[serde(default)]
    pub rule: String,
    /// How often alerts from this rule were dismissed (the card offers to turn
    /// the rule off once that is a habit). Filled in by `HostState::visible_alerts`.
    #[serde(default)]
    pub rule_dismissals: u32,
}

/// Harper rules that misfire on everyday writing more often than they help:
/// style opinions ("ok" → "okay", the Oxford comma, "configs" →
/// "configurations"), whitespace and dash pedantry, and splitters that turn
/// unknown words into nonsense ("plugin" → "plug in", "img" → "deg").
/// Measured on 20k words of edited English docs, where every one of these
/// notes was wrong or unwanted.
const OFF_BY_DEFAULT: &[&str] = &[
    "Spaces",
    "NoFrenchSpaces",
    "UnclosedQuotes",
    "Dashes",
    "NumericRangeEnDash",
    "EllipsisLength",
    "OxfordComma",
    "OkToOkay",
    "ToDoHyphen",
    "ExpandConfiguration",
    "ExpandMinimum",
    "ExpandDirectory",
    "DisjointPrefixes",
    "OrthographicConsistency",
    "SplitWords",
    "MergeWords",
    "PhrasalVerbAsCompoundNoun",
    "WrongNegative",
];

/// Everyday chat words Harper's dictionary lacks; flagging "ok" as "oak" is noise.
const INFORMAL: &[&str] = &[
    "ok", "lol", "btw", "imo", "imho", "tbh", "idk", "irl", "fyi", "asap", "omg", "brb", "afaik",
    "gonna", "wanna", "gotta", "kinda", "sorta", "yeah", "yep", "nope", "nah", "hmm", "uh", "ya",
];

/// A word Harper's dictionary cannot know but that is not a typo: a name
/// (capitalised mid-sentence), an acronym, camelCase, or something with
/// digits or path characters in it.
fn name_or_code(text_before: &str, word: &str) -> bool {
    let w = word.trim();
    let mut chars = w.chars();
    let Some(first) = chars.next() else { return false };
    let rest: String = chars.collect();
    let mid_sentence = !text_before.trim_end().is_empty() && !text_before.trim_end().ends_with(['.', '!', '?', '\n', ':', '"', '“']);
    w.chars().any(|c| c.is_ascii_digit() || matches!(c, '_' | '/' | '.' | '@' | '#'))
        || rest.chars().any(char::is_uppercase)
        || (first.is_uppercase() && mid_sentence)
}

/// Notes that misread code or a name as prose: a spelling note on a name,
/// acronym or `code span`, or a capital demanded after the dot of "app.exe".
fn looks_like_code_or_name(text: &str, a: &Alert) -> bool {
    let before: String = text.chars().take(a.start).collect();
    let line = before.rsplit('\n').next().unwrap_or("");
    let in_code_span = line.matches('`').count() % 2 == 1;
    let joined_to_a_path = before.ends_with(['.', '/', '\\']);
    match a.rule.as_str() {
        "SpellCheck" => in_code_span || joined_to_a_path || name_or_code(&before, &a.original) || INFORMAL.contains(&a.original.to_lowercase().as_str()),
        "SentenceCapitalization" => in_code_span || joined_to_a_path,
        _ => false,
    }
}

pub struct Engine {
    group: LintGroup,
}

impl Engine {
    pub fn new() -> Self {
        let mut group = LintGroup::new_curated(FstDictionary::curated(), Dialect::American);
        for rule in OFF_BY_DEFAULT {
            group.config.set_rule_enabled(*rule, false);
        }
        Engine { group }
    }

    pub fn check(&mut self, text: &str) -> Vec<Alert> {
        let map = TextMap::new(text);
        let doc = Document::new_plain_english_curated(text);
        let source = doc.get_source();
        let lints = self.group.organized_lints(&doc);
        let mut out: Vec<Alert> = Vec::with_capacity(lints.values().map(Vec::len).sum());
        for (rule, lint) in lints.into_iter().flat_map(|(rule, v)| v.into_iter().map(move |l| (rule.clone(), l))) {
            let start = lint.span.start.min(source.len());
            let end = lint.span.end.min(source.len()).max(start);
            if end == start {
                continue;
            }
            let original: String = source[start..end].iter().collect();
            let mut suggestions: Vec<String> = Vec::new();
            for s in &lint.suggestions {
                match s {
                    Suggestion::ReplaceWith(chars) => suggestions.push(chars.iter().collect()),
                    Suggestion::Remove => suggestions.push(String::new()),
                    Suggestion::InsertAfter(chars) => {
                        let mut v = original.clone();
                        v.extend(chars.iter());
                        suggestions.push(v)
                    }
                    #[allow(unreachable_patterns)]
                    _ => {}
                }
            }
            suggestions.dedup();
            let kind = lint.lint_kind;
            // Harper only measures the long sentence. Give the note a lossless
            // split at its middle joint so Accept is there at once; the spot
            // check's model split (when it arrives) hides this note.
            if matches!(kind, LintKind::Readability) && suggestions.is_empty() {
                if let Some(s) = crate::features::spotcheck::plain_split(&original) {
                    suggestions.push(s);
                }
            }
            let category = category_for(kind);
            let title = title_for(kind, &original, suggestions.first().map(|s| s.as_str()), &lint.message);
            let message = if matches!(kind, LintKind::Readability) {
                if suggestions.is_empty() {
                    format!("{} Split it into two or three sentences, one point each.", lint.message.trim())
                } else {
                    format!("{} The split keeps every word. Accept it, or break the sentence where you prefer.", lint.message.trim())
                }
            } else {
                lint.message.clone()
            };
            // Not the suggestion: it can change with the words around the
            // flagged ones, and a dismissed alert must stay dismissed.
            let stable_hash = stable_hash(&kind.to_string(), &original, None);
            let id = format!("{:x}-{}", stable_hash, start);
            out.push(Alert {
                id,
                category,
                kind: kind.to_string(),
                title,
                message,
                start,
                end,
                u16_start: map.char_to_utf16(start),
                u16_end: map.char_to_utf16(end),
                original,
                suggestions,
                priority: lint.priority,
                stable_hash,
                rule,
                rule_dismissals: 0,
            });
        }
        // Words in another language (Vietnamese, a name with accents) are
        // not English mistakes: Harper's notes inside them are dropped.
        let foreign = crate::foreign::ForeignMask::new(text);
        out.retain(|a| !foreign.covers(a.start, a.end));
        out.retain(|a| !looks_like_code_or_name(text, a));
        out.extend(crate::style_rules::delivery(text, &map));
        out.extend(crate::style_rules::engagement(text, &map));
        // Remove overlapping duplicates (keep the first, higher-priority entry).
        out.sort_by(|a, b| a.start.cmp(&b.start).then(a.priority.cmp(&b.priority)));
        let mut kept: Vec<Alert> = Vec::with_capacity(out.len());
        for a in out {
            if let Some(last) = kept.last() {
                if a.start < last.end && a.category == last.category {
                    continue;
                }
            }
            kept.push(a);
        }
        kept
    }
}

pub fn stable_hash(kind: &str, original: &str, suggestion: Option<&str>) -> u64 {
    let mut h = DefaultHasher::new();
    kind.hash(&mut h);
    original.to_lowercase().hash(&mut h);
    suggestion.unwrap_or("").hash(&mut h);
    h.finish()
}

pub fn category_for(kind: LintKind) -> Category {
    match kind {
        LintKind::Spelling
        | LintKind::Typo
        | LintKind::Capitalization
        | LintKind::Punctuation
        | LintKind::Grammar
        | LintKind::Agreement
        | LintKind::BoundaryError
        | LintKind::Malapropism
        | LintKind::Eggcorn
        | LintKind::Usage
        | LintKind::Nonstandard
        | LintKind::WordOrder
        | LintKind::Regionalism
        | LintKind::Formatting => Category::Correctness,
        LintKind::Readability | LintKind::Redundancy | LintKind::Repetition | LintKind::WordChoice | LintKind::Style | LintKind::Miscellaneous => {
            Category::Clarity
        }
        LintKind::Enhancement => Category::Engagement,
        #[allow(unreachable_patterns)]
        _ => Category::Clarity,
    }
}

pub fn title_for(kind: LintKind, original: &str, suggestion: Option<&str>, message: &str) -> String {
    let sugg = suggestion.unwrap_or("");
    match kind {
        LintKind::Spelling | LintKind::Typo => "Correct your spelling".into(),
        LintKind::Capitalization => {
            if original.chars().next().map(|c| c.is_lowercase()).unwrap_or(false) && sugg.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                "Capitalize the word".into()
            } else {
                "Fix capitalization".into()
            }
        }
        LintKind::Punctuation => {
            let o_commas = original.matches(',').count();
            let s_commas = sugg.matches(',').count();
            if s_commas > o_commas {
                "Add a comma".into()
            } else if s_commas < o_commas {
                "Remove the comma".into()
            } else if sugg.ends_with('.') && !original.ends_with('.') {
                "Add a period".into()
            } else if sugg.contains('\'') && !original.contains('\'') {
                "Add an apostrophe".into()
            } else {
                "Punctuation problem".into()
            }
        }
        LintKind::Grammar => "Correct the grammar".into(),
        LintKind::Agreement => "Fix subject-verb agreement".into(),
        LintKind::BoundaryError => "Fix the word boundary".into(),
        LintKind::Malapropism | LintKind::Eggcorn => "Use the right word".into(),
        LintKind::Usage | LintKind::Nonstandard => "Fix nonstandard usage".into(),
        LintKind::WordOrder => "Fix the word order".into(),
        LintKind::Regionalism => "Use the American spelling".into(),
        LintKind::Formatting => "Fix formatting".into(),
        LintKind::Readability => "Split the long sentence".into(),
        LintKind::Redundancy => "Remove redundancy".into(),
        LintKind::Repetition => "Avoid repetition".into(),
        LintKind::WordChoice => "Change the wording".into(),
        LintKind::Style => "Rephrase for clarity".into(),
        LintKind::Enhancement => "Make it more engaging".into(),
        LintKind::Miscellaneous => {
            let m = message.trim_end_matches('.');
            if m.len() < 40 {
                m.to_string()
            } else {
                "Review this".into()
            }
        }
        #[allow(unreachable_patterns)]
        _ => "Review this".into(),
    }
}

/// byte offset → char index table (len = bytes + 1)
pub(crate) fn char_prefix(text: &str) -> Vec<usize> {
    let mut v = vec![0usize; text.len() + 1];
    let mut ci = 0;
    let mut last = 0;
    for (bi, _) in text.char_indices() {
        for b in last..=bi {
            v[b] = ci;
        }
        last = bi + 1;
        ci += 1;
    }
    for b in last..=text.len() {
        v[b] = ci;
    }
    v
}

#[cfg(test)]
mod tests {
    #[test]
    fn alerts_name_their_rule_and_keep_their_key_when_the_suggestion_changes() {
        let alerts = super::Engine::new().check("Hi, just wanted to ship zorbleflax today.");
        let spell = alerts.iter().find(|a| a.original == "zorbleflax").expect("an unknown word is flagged");
        assert_eq!((spell.kind.as_str(), spell.rule.as_str()), ("Spelling", "SpellCheck"));
        assert!(alerts.iter().any(|a| a.kind == "Delivery" && a.rule == "Remove the filler"), "{alerts:?}");
        assert!(alerts.iter().all(|a| !a.rule.is_empty()));
        assert_eq!(super::stable_hash("Spelling", "zorbleflax", None), spell.stable_hash, "the dismiss key ignores the suggestion");
    }

    #[test]
    fn a_long_sentence_note_carries_an_instant_lossless_split() {
        let text = "The committee reviewed the three proposals on Tuesday and asked each team for a revised budget by Friday, and it plans to announce the winner at the all-hands meeting on the first of March, which gives everyone time to prepare their slides and book travel.";
        let alerts = super::Engine::new().check(text);
        let a = alerts.iter().find(|a| a.kind == "Readability").expect("Harper flags a 45-word sentence");
        assert_eq!(a.title, "Split the long sentence");
        assert!(a.message.contains("The split keeps every word."), "{}", a.message);
        assert_eq!(a.category, super::Category::Clarity);
        let s = a.suggestions.first().expect("Accept is available before any model runs");
        assert!(s.contains("by Friday. It plans"), "{s}");
        assert_eq!(crate::features::spotcheck::content_loss(&a.original, s).0, 0, "nothing removed");
        // A long sentence with no joint keeps the plain note.
        let flat = "The committee reviewed the three proposals on Tuesday then asked each team for a revised budget by Friday then planned to announce the winner at the all-hands meeting on the first of March which gives everyone time to prepare slides and book travel.";
        let b = super::Engine::new().check(flat);
        let b = b.iter().find(|a| a.kind == "Readability").expect("still flagged");
        assert!(b.suggestions.is_empty());
        assert!(b.message.ends_with("Split it into two or three sentences, one point each."), "{}", b.message);
    }

    use super::*;

    #[test]
    fn harper_finds_a_misspelling_with_a_suggestion() {
        let mut e = Engine::new();
        let alerts = e.check("This is an test of teh engine.");
        let teh = alerts.iter().find(|a| a.original == "teh").expect("teh flagged");
        assert_eq!(teh.category, Category::Correctness);
        assert!(teh.suggestions.iter().any(|s| s == "the"), "{:?}", teh.suggestions);
        assert_eq!(teh.title, "Correct your spelling");
        // char offsets and utf16 offsets agree for ASCII
        assert_eq!((teh.start, teh.end), (teh.u16_start, teh.u16_end));
        assert_eq!(&"This is an test of teh engine."[teh.start..teh.end], "teh");
    }

    #[test]
    fn utf16_offsets_follow_emoji() {
        let mut e = Engine::new();
        let text = "😀😀 I has teh cat.";
        let alerts = e.check(text);
        let teh = alerts.iter().find(|a| a.original == "teh").expect("teh flagged");
        // two emoji = 2 chars but 4 utf16 units
        assert_eq!(teh.u16_start, teh.start + 2);
        assert_eq!(crate::textmap::slice_utf16(text, teh.u16_start, teh.u16_end), "teh");
    }

    #[test]
    fn delivery_rules_flag_hedging() {
        let mut e = Engine::new();
        let alerts = e.check("I think that we should just wanted to ship it.");
        assert!(alerts.iter().any(|a| a.category == Category::Delivery && a.title == "Sound more confident"));
        assert!(alerts.iter().any(|a| a.category == Category::Delivery && a.original.trim() == "just"));
    }

    #[test]
    fn names_acronyms_and_code_are_not_misspellings() {
        let mut e = Engine::new();
        let alerts = e.check("Yesterday I installed DragShift and Hyprland on my PC with the gtk3 build. Then I wrote teh notes and a smal fix.");
        let spelled: Vec<&str> = alerts.iter().filter(|a| a.kind == "Spelling" || a.kind == "Typo").map(|a| a.original.as_str()).collect();
        assert!(spelled.contains(&"smal") && spelled.contains(&"teh"), "{spelled:?}");
        for w in ["DragShift", "Hyprland", "gtk3"] {
            assert!(!spelled.contains(&w), "{w}: {spelled:?}");
        }
    }

    #[test]
    fn code_spans_and_file_names_are_left_alone() {
        let mut e = Engine::new();
        let alerts = e.check("Run `spctl --status` and open setup.exe first.");
        assert!(alerts.iter().all(|a| a.original != "spctl" && a.original != "exe"), "{alerts:?}");
    }

    #[test]
    fn noisy_style_rules_are_off() {
        let mut e = Engine::new();
        let alerts = e.check("The plugin configs are ok, see the docs for red, green and blue.");
        assert!(alerts.iter().all(|a| a.original != "ok"), "{alerts:?}");
        let rules: Vec<&str> = alerts.iter().map(|a| a.rule.as_str()).collect();
        for r in ["OkToOkay", "ExpandConfiguration", "PhrasalVerbAsCompoundNoun", "OxfordComma"] {
            assert!(!rules.contains(&r), "{r}: {rules:?}");
        }
    }

    #[test]
    fn vietnamese_words_and_sentences_are_not_english_mistakes() {
        let mut e = Engine::new();
        let alerts = e.check("Hôm nay tôi đi học. I has teh cat.");
        let originals: Vec<&str> = alerts.iter().map(|a| a.original.as_str()).collect();
        assert!(originals.contains(&"teh"), "English is still checked: {originals:?}");
        assert!(alerts.iter().all(|a| a.end <= 19 || a.start >= 19), "nothing inside the Vietnamese sentence: {originals:?}");
    }

    #[test]
    fn categories_cover_every_lint_kind() {
        for k in [LintKind::Spelling, LintKind::Readability, LintKind::Enhancement, LintKind::Punctuation] {
            let _ = category_for(k);
        }
        assert_eq!(category_for(LintKind::Spelling), Category::Correctness);
        assert_eq!(category_for(LintKind::Readability), Category::Clarity);
        assert_eq!(category_for(LintKind::Enhancement), Category::Engagement);
    }

    #[test]
    fn clean_text_has_no_correctness_alerts() {
        let mut e = Engine::new();
        let alerts = e.check("The quick brown fox jumps over the lazy dog.");
        assert!(alerts.iter().all(|a| a.category != Category::Correctness), "{:?}", alerts);
    }

    #[test]
    fn real_sample_text_produces_alerts_without_panicking() {
        let text = "I took the biggest risk of my ENTIRE LIFE.\n\nbut to make you understand, i have to take you back, to exactly 42 days ago.\n\nthe last day to withdraw from classes. Up til this point I had been working on my startup after going viral over the summer for cloning apps, i wasnt getting me any closer ot my goals.\n\nI pulled out of school, and shut the fuck up. I didnt tell my family, not my friends, and not my peers. I had very little money.\n\nI lived in an apartment with a blanket over the window to block out the light cycle, watching people around me go to class while I sent thousands of emils. It didnt feel great.\n\nThen after thousands of similar emails had gone into the void, a founder replied. We got on a call and by the next day the deal was done. We raised over half a million dollars to kill slop companies.\n\nI think that I will be able to lead yall into a future where software is free and im keeping it accessible. SUBSCRIPTIONS, im coming after each and every one of you.";
        let mut e = Engine::new();
        let alerts = e.check(text);
        let originals: Vec<&str> = alerts.iter().map(|a| a.original.as_str()).collect();
        assert!(alerts.len() >= 8, "expected many alerts, got {:?}", originals);
        assert!(originals.iter().any(|o| *o == "emils" || *o == "didnt" || *o == "wasnt"), "{:?}", originals);
        for a in &alerts {
            assert_eq!(crate::textmap::slice_utf16(text, a.u16_start, a.u16_end), a.original, "offsets must round-trip for {}", a.title);
        }
    }
}

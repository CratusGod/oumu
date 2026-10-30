//! Oumu's own Delivery and Engagement notes. Each rule only fires where it is
//! clearly right and carries a fix: a hedge that opens a sentence, "just" in
//! an email-hedge phrase, "very"/"really" before a word with a stronger
//! one-word replacement. Plain uses ("I just got home", "a kind of tool",
//! "really?") are left alone.

use crate::engine::{stable_hash, Alert, Category};
use crate::textmap::TextMap;
use regex::Regex;
use std::sync::OnceLock;

const DELIVERY_PRIORITY: u8 = 200;
const ENGAGEMENT_PRIORITY: u8 = 220;
/// A vague word is worth a note only once it keeps coming back.
const VAGUE_MIN_USES: usize = 3;

/// Where a rule may match.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Place {
    Anywhere,
    /// Only as the first words of a sentence.
    SentenceStart,
    /// Not right after "a", "the", "this", … ("a kind of tool" is a noun phrase).
    NotAfterDeterminer,
}

/// What Accept does with the match.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Fix {
    /// Drop the match; at a sentence start the next word gets the capital.
    Remove,
    /// Drop the first `n` characters of the match only ("just " of "just wanted").
    RemoveLead(usize),
    Replace(&'static str),
    /// "very good" → "excellent": the second word through `stronger_word`.
    Intensifier,
    /// Nothing safe to offer; the note explains.
    None,
}

struct Rule {
    re: Regex,
    place: Place,
    fix: Fix,
    title: &'static str,
    message: &'static str,
}

fn rules() -> &'static [Rule] {
    static RULES: OnceLock<Vec<Rule>> = OnceLock::new();
    RULES.get_or_init(|| {
        let mk = |pat: &str, place: Place, fix: Fix, title: &'static str, message: &'static str| Rule {
            re: Regex::new(pat).expect("delivery regex"),
            place,
            fix,
            title,
            message,
        };
        vec![
            mk(r"(?i)\bI (?:just )?think (?:that )?", Place::SentenceStart, Fix::Remove, "Sound more confident", "Opening with “I think” makes the point sound unsure. State it directly."),
            mk(r"(?i)\bI feel like ", Place::SentenceStart, Fix::Remove, "Sound more confident", "“I feel like” softens your point. Readers trust a direct claim more."),
            mk(r"(?i)\bI guess ", Place::SentenceStart, Fix::Remove, "Sound more confident", "“I guess” undercuts what follows. State it directly."),
            mk(r"(?i)\bjust (?:wanted|checking|following up|a quick|a reminder|to let you know|reaching out|to say)\b", Place::Anywhere, Fix::RemoveLead(5), "Remove the filler", "“Just” here apologises for writing. Without it the message sounds sure of itself."),
            mk(r"(?i)\b(?:kind|sort) of ", Place::NotAfterDeterminer, Fix::Remove, "Remove the hedge", "“Kind of” and “sort of” weaken the word after them."),
            mk(r"(?i)\b(?:actually|basically|honestly|literally),? ", Place::SentenceStart, Fix::Remove, "Remove the filler", "An opening filler word delays the point. Start with the point."),
            mk(r"(?i)\b(?:very|really) [a-z]+\b", Place::Anywhere, Fix::Intensifier, "Choose a stronger word", "One precise word reads stronger than an intensifier and a plain word."),
            mk(r"!{2,}", Place::Anywhere, Fix::Replace("!"), "Tone down the punctuation", "Several exclamation marks can read as unprofessional."),
            mk(r"(?i)\bsorry to bother you\b", Place::Anywhere, Fix::None, "Sound more confident", "Apologizing before asking undermines your request."),
        ]
    })
}

/// "good" → "excellent", for "very good" and "really good". Only words with
/// one clearly stronger replacement; everything else keeps its intensifier.
fn stronger_word(w: &str) -> Option<&'static str> {
    Some(match w {
        "good" => "excellent",
        "bad" => "terrible",
        "big" | "large" => "huge",
        "small" => "tiny",
        "happy" => "delighted",
        "sad" => "miserable",
        "tired" => "exhausted",
        "important" => "crucial",
        "cold" => "freezing",
        "hot" => "scorching",
        "old" => "ancient",
        "scared" | "afraid" => "terrified",
        "angry" => "furious",
        "hungry" => "starving",
        "beautiful" => "gorgeous",
        "ugly" => "hideous",
        "smart" => "brilliant",
        "clean" => "spotless",
        "dirty" => "filthy",
        "loud" => "deafening",
        "quiet" => "silent",
        "rich" => "wealthy",
        "busy" => "swamped",
        "interesting" => "fascinating",
        "funny" => "hilarious",
        "nice" => "lovely",
        "worried" => "anxious",
        "excited" => "thrilled",
        "surprised" => "astonished",
        "tall" => "towering",
        "boring" => "tedious",
        "difficult" => "arduous",
        "sure" => "certain",
        _ => return None,
    })
}

fn at_sentence_start(text: &str, byte: usize) -> bool {
    let before = text[..byte].trim_end_matches(|c: char| c == ' ' || c == '\t' || c == '"' || c == '“' || c == '(');
    before.is_empty() || before.ends_with(['.', '!', '?', '\n'])
}

fn after_determiner(text: &str, byte: usize) -> bool {
    const DETERMINERS: &[&str] = &["a", "an", "the", "this", "that", "these", "those", "what", "some", "any", "every", "one", "same", "which", "no", "my", "your", "his", "her", "our", "their", "its"];
    let prev = text[..byte].split_whitespace().next_back().unwrap_or("").to_lowercase();
    DETERMINERS.contains(&prev.as_str())
}

fn capitalised_like(word: &str, model: &str) -> String {
    if model.chars().next().is_some_and(char::is_uppercase) {
        let mut c = word.chars();
        c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
    } else {
        word.to_string()
    }
}

/// The span (byte range) Accept replaces and what it puts there, or None to skip the match.
fn resolve(text: &str, rule: &Rule, bs: usize, be: usize) -> Option<(usize, usize, Option<String>)> {
    let m = &text[bs..be];
    match rule.fix {
        Fix::Remove if rule.place == Place::SentenceStart => {
            // Take the next letter along so the sentence keeps its capital.
            let next = text[be..].chars().next().filter(|c| c.is_alphabetic());
            match next {
                Some(c) => Some((bs, be + c.len_utf8(), Some(c.to_uppercase().collect()))),
                None => Some((bs, be, Some(String::new()))),
            }
        }
        Fix::Remove => Some((bs, be, Some(String::new()))),
        Fix::RemoveLead(n) => Some((bs, bs + n.min(m.len()), Some(String::new()))),
        Fix::Replace(r) => Some((bs, be, Some(r.to_string()))),
        Fix::Intensifier => {
            let (lead, word) = m.split_once(' ')?;
            let strong = stronger_word(&word.to_lowercase())?;
            Some((bs, be, Some(capitalised_like(strong, lead))))
        }
        Fix::None => Some((bs, be, None)),
    }
}

pub fn delivery(text: &str, map: &TextMap) -> Vec<Alert> {
    let chars = crate::engine::char_prefix(text);
    let mut out = Vec::new();
    for rule in rules() {
        for m in rule.re.find_iter(text) {
            let placed = match rule.place {
                Place::Anywhere => true,
                Place::SentenceStart => at_sentence_start(text, m.start()),
                Place::NotAfterDeterminer => !after_determiner(text, m.start()),
            };
            if !placed {
                continue;
            }
            let Some((bs, be, fix)) = resolve(text, rule, m.start(), m.end()) else { continue };
            let (start, end) = (chars[bs], chars[be]);
            let original = text[bs..be].to_string();
            let suggestions: Vec<String> = fix.into_iter().collect();
            let stable = stable_hash("Delivery", original.trim(), None);
            out.push(Alert {
                id: format!("{:x}-{}", stable, start),
                category: Category::Delivery,
                kind: "Delivery".into(),
                title: rule.title.into(),
                message: rule.message.into(),
                start,
                end,
                u16_start: map.char_to_utf16(start),
                u16_end: map.char_to_utf16(end),
                original,
                suggestions,
                priority: DELIVERY_PRIORITY,
                stable_hash: stable,
                rule: rule.title.into(),
                rule_dismissals: 0,
            });
        }
    }
    out
}

/// One note per vague word ("thing", "stuff", …) used three times or more.
pub fn engagement(text: &str, map: &TextMap) -> Vec<Alert> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"(?i)\b(?:things?|stuff|a lot|nice|good)\b").unwrap());
    let chars = crate::engine::char_prefix(text);
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for m in re.find_iter(text) {
        let word = m.as_str().to_lowercase();
        if !seen.insert(word.clone()) {
            continue;
        }
        let count = re.find_iter(text).filter(|x| x.as_str().eq_ignore_ascii_case(&word)).count();
        if count < VAGUE_MIN_USES {
            continue;
        }
        let (start, end) = (chars[m.start()], chars[m.end()]);
        let stable = stable_hash("Engagement", &word, None);
        out.push(Alert {
            id: format!("{:x}-{}", stable, start),
            category: Category::Engagement,
            kind: "Engagement".into(),
            title: "Choose a more specific word".into(),
            message: format!("“{word}” appears {count} times. A more specific word keeps readers engaged."),
            start,
            end,
            u16_start: map.char_to_utf16(start),
            u16_end: map.char_to_utf16(end),
            original: m.as_str().to_string(),
            suggestions: vec![],
            priority: ENGAGEMENT_PRIORITY,
            stable_hash: stable,
            rule: "Overused words".into(),
            rule_dismissals: 0,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notes(text: &str) -> Vec<(String, Vec<String>)> {
        let map = TextMap::new(text);
        delivery(text, &map).into_iter().map(|a| (a.original, a.suggestions)).collect()
    }

    fn fixed(text: &str) -> String {
        let map = TextMap::new(text);
        let mut out = text.to_string();
        let mut alerts = delivery(text, &map);
        alerts.sort_by(|a, b| b.start.cmp(&a.start));
        for a in alerts {
            if let Some(s) = a.suggestions.first() {
                let bs = out.char_indices().nth(a.start).map(|(i, _)| i).unwrap_or(out.len());
                let be = out.char_indices().nth(a.end).map(|(i, _)| i).unwrap_or(out.len());
                out.replace_range(bs..be, s);
            }
        }
        out
    }

    #[test]
    fn hedges_only_count_when_they_open_a_sentence() {
        assert_eq!(fixed("I think that we should ship it."), "We should ship it.");
        assert_eq!(fixed("Done. I feel like it works."), "Done. It works.");
        assert!(notes("Do you know what I think about it?").is_empty());
    }

    #[test]
    fn just_is_only_flagged_in_email_hedges() {
        assert_eq!(fixed("Hi, just wanted to check in."), "Hi, wanted to check in.");
        assert!(notes("I just got home.").is_empty());
        assert!(notes("It was just in time.").is_empty());
    }

    #[test]
    fn kind_of_after_a_determiner_is_a_noun_phrase() {
        assert!(notes("It is a kind of tool.").is_empty());
        assert_eq!(fixed("It was kind of slow."), "It was slow.");
    }

    #[test]
    fn intensifiers_need_a_one_word_replacement() {
        assert_eq!(fixed("The food was very good."), "The food was excellent.");
        assert_eq!(fixed("Very tired today."), "Exhausted today.");
        assert!(notes("It is very likely.").is_empty(), "no single stronger word");
        assert!(notes("Really? Fine.").is_empty());
    }

    #[test]
    fn opening_fillers_and_punctuation() {
        assert_eq!(fixed("Actually, the plan works."), "The plan works.");
        assert!(notes("It actually works.").is_empty());
        assert_eq!(fixed("We won!!!"), "We won!");
    }

    #[test]
    fn vague_words_need_three_uses() {
        let two = "Good food and good friends.";
        assert!(engagement(two, &TextMap::new(two)).is_empty());
        let three = "A good day, a good meal, a good night.";
        assert_eq!(engagement(three, &TextMap::new(three)).len(), 1);
    }
}

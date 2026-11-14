//! Grammar fixes from the spot check. The model returns the sentence with
//! only its grammar corrected ("fix"); this turns that into word-sized edits,
//! refuses anything that is really a rewrite, and makes one alert per edit so
//! the underline sits on "has", not on the whole sentence.

use crate::engine::{stable_hash, Alert, Category};
use crate::features::spotcheck::Sentence;
use crate::textmap::TextMap;

/// A fix may touch this many words of the sentence (or a quarter of it, if more).
const MAX_CHANGED_WORDS: usize = 3;
/// One edit replaces at most this many words, and puts in at most one more.
const MAX_EDIT_WORDS: usize = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FixReject {
    Identical,
    OnlyCaseOrPunctuation,
    TooManyChanges,
    EditTooBig,
    /// A word dropped that is not a doubled word ("the the").
    DropsAWord,
    /// A word added that is not a small function word ("to", "a", "the", ...).
    AddsContent,
    /// A word swapped for an unrelated one: a rewrite, not a grammar fix.
    UnrelatedWord,
    /// A number appears that the sentence did not have.
    AddsANumber,
}

/// Words a grammar fix may insert on its own: a missing "to", article or helper verb.
const INSERTABLE: &[&str] = &["to", "a", "an", "the", "of", "is", "are", "was", "were", "be", "been", "have", "has", "had", "will", "would", "it", "that", "for", "in", "on", "at"];

/// Pairs that are grammar, not spelling, so they look nothing alike.
const CONFUSABLE: &[(&str, &str)] = &[
    ("is", "are"), ("was", "were"), ("has", "have"), ("do", "does"), ("don't", "doesn't"), ("isn't", "aren't"),
    ("wasn't", "weren't"), ("hasn't", "haven't"), ("a", "an"), ("then", "than"), ("there", "their"),
    ("there", "they're"), ("their", "they're"), ("your", "you're"), ("its", "it's"), ("who's", "whose"),
    ("lose", "loose"), ("affect", "effect"), ("fewer", "less"), ("me", "i"), ("who", "whom"),
];

fn edit_distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + usize::from(a[i - 1] != b[j - 1]));
        }
        prev = cur;
    }
    prev[b.len()]
}

/// Whether `to` can be a grammar fix of `from`: another form of the same
/// word (live/lives, give/gives, it's/its), a known confusable pair, or the
/// same words joined by a hyphen ("three day" → "three-day").
fn related(from: &[String], to: &[String]) -> bool {
    let joined = |v: &[String]| v.join("-").to_lowercase();
    if joined(from) == joined(to) {
        return true;
    }
    if from.len() != to.len() {
        return false;
    }
    from.iter().zip(to).all(|(a, b)| {
        let (a, b) = (a.to_lowercase(), b.to_lowercase());
        let stem = a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count();
        CONFUSABLE.iter().any(|&(x, y)| (a == x && b == y) || (a == y && b == x))
            || (stem >= 3 && edit_distance(&a, &b) <= 3)
            || (stem >= 2 && edit_distance(&a, &b) <= 1)
    })
}

/// One change: original words `from..to` (token indexes) become `with`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub from: usize,
    pub to: usize,
    pub with: Vec<String>,
}

/// A whitespace-separated word: its char range in the sentence, without the
/// punctuation that trails or leads it (apostrophes and hyphens stay).
#[derive(Clone, Debug)]
struct Token {
    start: usize,
    end: usize,
    core: String,
}

fn is_edge_punct(c: char) -> bool {
    matches!(c, '.' | ',' | ';' | ':' | '!' | '?' | '"' | '“' | '”' | '(' | ')' | '[' | ']')
}

fn tokens(s: &str) -> Vec<Token> {
    let chars: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < chars.len() && !chars[j].is_whitespace() {
            j += 1;
        }
        let (mut a, mut b) = (i, j);
        while a < b && is_edge_punct(chars[a]) {
            a += 1;
        }
        while b > a && is_edge_punct(chars[b - 1]) {
            b -= 1;
        }
        if a < b {
            out.push(Token { start: a, end: b, core: chars[a..b].iter().collect() });
        }
        i = j;
    }
    out
}

/// Word-level diff (longest common subsequence on exact words).
fn diff(a: &[Token], b: &[Token]) -> Vec<Edit> {
    let (n, m) = (a.len(), b.len());
    let mut lcs = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if a[i].core == b[j].core { lcs[i + 1][j + 1] + 1 } else { lcs[i + 1][j].max(lcs[i][j + 1]) };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut edits: Vec<Edit> = Vec::new();
    let mut open: Option<Edit> = None;
    while i < n || j < m {
        if i < n && j < m && a[i].core == b[j].core {
            if let Some(e) = open.take() {
                edits.push(e);
            }
            i += 1;
            j += 1;
            continue;
        }
        let e = open.get_or_insert(Edit { from: i, to: i, with: vec![] });
        if j < m && (i == n || lcs[i][j + 1] >= lcs[i + 1][j]) {
            e.with.push(b[j].core.clone());
            j += 1;
        } else {
            e.to = i + 1;
            i += 1;
        }
    }
    if let Some(e) = open {
        edits.push(e);
    }
    edits
}

/// The edits that turn `orig` into the model's `fixed`, if they are a grammar
/// fix and not a rewrite. Case-only changes ("i" → "I") are dropped: Harper
/// covers capitals, and casual writing keeps its lowercase.
pub fn check(orig: &str, fixed: &str) -> Result<Vec<Edit>, FixReject> {
    let (a, b) = (tokens(orig), tokens(fixed));
    let all = diff(&a, &b);
    if all.is_empty() {
        return Err(FixReject::Identical);
    }
    let edits: Vec<Edit> = all
        .into_iter()
        .filter(|e| {
            let was: Vec<String> = a[e.from..e.to].iter().map(|t| t.core.to_lowercase()).collect();
            let now: Vec<String> = e.with.iter().map(|w| w.to_lowercase()).collect();
            was != now
        })
        .collect();
    if edits.is_empty() {
        return Err(FixReject::OnlyCaseOrPunctuation);
    }
    let changed: usize = edits.iter().map(|e| (e.to - e.from).max(e.with.len())).sum();
    if changed > MAX_CHANGED_WORDS.max(a.len() / 4) {
        return Err(FixReject::TooManyChanges);
    }
    if edits.iter().any(|e| e.to - e.from > MAX_EDIT_WORDS || e.with.len() > (e.to - e.from) + 1) {
        return Err(FixReject::EditTooBig);
    }
    let has_digit = |w: &str| w.chars().any(|c| c.is_ascii_digit());
    for e in &edits {
        let was: Vec<String> = a[e.from..e.to].iter().map(|t| t.core.clone()).collect();
        if e.with.iter().any(|w| has_digit(w)) && !was.iter().any(|w| has_digit(w)) {
            return Err(FixReject::AddsANumber);
        }
        if e.with.is_empty() {
            // Only a doubled word may go: "the the cat".
            let doubled = was.len() == 1 && [e.from.checked_sub(1), Some(e.to)].into_iter().flatten().any(|i| a.get(i).is_some_and(|t| t.core.eq_ignore_ascii_case(&was[0])));
            if !doubled {
                return Err(FixReject::DropsAWord);
            }
        } else if was.is_empty() {
            if !e.with.iter().all(|w| INSERTABLE.contains(&w.to_lowercase().as_str())) {
                return Err(FixReject::AddsContent);
            }
        } else {
            // A replacement may also carry one inserted function word: "go" → "to go".
            let core: Vec<String> = if e.with.len() == was.len() + 1 && INSERTABLE.contains(&e.with[0].to_lowercase().as_str()) { e.with[1..].to_vec() } else { e.with.clone() };
            if !related(&was, &core) {
                return Err(FixReject::UnrelatedWord);
            }
        }
    }
    Ok(edits)
}

/// One Correctness alert per edit, positioned in the document. An inserted
/// word ("a", a missing "to") is anchored on the word after it (or before it,
/// at the end of the sentence) so there is something to underline.
pub fn alerts(map: &TextMap, sent: &Sentence, edits: &[Edit], why: &str) -> Vec<Alert> {
    let toks = tokens(&sent.text);
    let chars: Vec<char> = sent.text.chars().collect();
    let slice = |a: usize, b: usize| chars[a..b].iter().collect::<String>();
    let mut out = Vec::new();
    // the model's replacement words, spelled right ("recieve" -> "recieved" becomes "received")
    let fixed: Vec<Edit> = edits.iter().map(|e| Edit { from: e.from, to: e.to, with: e.with.iter().map(|w| spelled(w)).collect() }).collect();
    let edits = &fixed;
    for e in edits {
        let (start, end, suggestion) = if e.from < e.to {
            (toks[e.from].start, toks[e.to - 1].end, e.with.join(" "))
        } else if e.from < toks.len() {
            let next = &toks[e.from];
            (next.start, next.end, format!("{} {}", e.with.join(" "), next.core))
        } else if let Some(prev) = toks.last() {
            (prev.start, prev.end, format!("{} {}", prev.core, e.with.join(" ")))
        } else {
            continue;
        };
        let original = slice(start, end);
        let (doc_start, doc_end) = (sent.start + start, sent.start + end);
        let hash = stable_hash("SpotGrammar", &format!("{}|{}", sent.text.trim(), original), None);
        let message = if reason_fits(why, &toks, edits) { why.trim().to_string() } else { plain_reason(&original, e, &suggestion) };
        out.push(Alert {
            id: format!("spot-{:x}-{}", hash, doc_start),
            category: Category::Correctness,
            kind: "SpotGrammar".into(),
            title: "Fix the grammar".into(),
            message,
            start: doc_start,
            end: doc_end,
            u16_start: map.char_to_utf16(doc_start),
            u16_end: map.char_to_utf16(doc_end),
            original,
            suggestions: vec![suggestion],
            priority: 60,
            stable_hash: hash,
            rule: "AI grammar check".into(),
            rule_dismissals: 0,
        });
    }
    out
}

/// `word` with its spelling corrected when the offline checker (Harper) is sure it's
/// misspelled and has a suggestion; otherwise as it is. Models fix the grammar of a word
/// and keep its misspelling ("recieve" -> "recieved").
fn spelled(word: &str) -> String {
    use std::sync::Mutex;
    static ENGINE: Mutex<Option<crate::engine::Engine>> = Mutex::new(None);
    if word.is_empty() || !word.chars().all(|c| c.is_alphabetic() || c == '\'') {
        return word.to_string();
    }
    let Ok(mut g) = ENGINE.lock() else { return word.to_string() };
    let engine = g.get_or_insert_with(crate::engine::Engine::new);
    let fix = engine
        .check(word)
        .into_iter()
        .find(|a| (a.kind == "Spelling" || a.kind == "Typo") && a.original == word)
        .and_then(|a| a.suggestions.into_iter().find(|s| !s.is_empty() && !s.contains(' ')));
    match fix {
        // keep the writer's capital
        Some(s) if word.chars().next().is_some_and(char::is_uppercase) => {
            let mut c = s.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or(s)
        }
        Some(s) => s,
        None => word.to_string(),
    }
}

/// Whether the model's reason talks only about words this fix changes: every word it
/// quotes must be one of them. Models explain edits they never made ("incorrect article
/// 'a'" when only "need" changed); such a reason is replaced by a plain one.
fn reason_fits(why: &str, toks: &[Token], edits: &[Edit]) -> bool {
    if why.trim().is_empty() {
        return false;
    }
    let mut changed: std::collections::HashSet<String> = std::collections::HashSet::new();
    for e in edits {
        changed.extend(toks[e.from..e.to].iter().map(|t| t.core.to_lowercase()));
        changed.extend(e.with.iter().map(|w| w.to_lowercase()));
    }
    let quoted = quoted_words(why);
    !quoted.is_empty() && quoted.iter().all(|w| changed.contains(w))
}

/// The words inside quotes ('...', "...", ‘...’, “...”) of `s`, lowercased.
fn quoted_words(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let chars: Vec<char> = s.chars().collect();
    let opens = |c: char| matches!(c, '\'' | '"' | '‘' | '“');
    let closes = |c: char| matches!(c, '\'' | '"' | '’' | '”');
    let mut i = 0;
    while i < chars.len() {
        // an opening quote starts a quotation unless it's an apostrophe inside a word ("isn't")
        if opens(chars[i]) && (i == 0 || !chars[i - 1].is_alphanumeric()) {
            // its end: the next closing quote not followed by a letter ("can't" keeps going)
            let end = (i + 1..chars.len()).find(|&j| closes(chars[j]) && chars.get(j + 1).is_none_or(|c| !c.is_alphanumeric()));
            if let Some(end) = end {
                let inner: String = chars[i + 1..end].iter().collect();
                for w in inner.split_whitespace() {
                    let w: String = w.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'').to_lowercase();
                    if !w.is_empty() {
                        out.push(w);
                    }
                }
                i = end + 1;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// A reason that is always true: what this one edit does.
fn plain_reason(original: &str, e: &Edit, suggestion: &str) -> String {
    if e.with.is_empty() {
        format!("Remove the repeated “{original}”.")
    } else if e.from == e.to {
        format!("A word is missing here: “{suggestion}”.")
    } else {
        format!("“{original}” should be “{suggestion}” here.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn applied(orig: &str, fixed: &str) -> Vec<(String, String)> {
        let edits = check(orig, fixed).expect("a grammar fix");
        let sent = Sentence { start: 0, end: orig.chars().count(), text: orig.into(), line: 0, judge: true };
        alerts(&TextMap::new(orig), &sent, &edits, "").into_iter().map(|a| (a.original, a.suggestions[0].clone())).collect()
    }

    #[test]
    fn agreement_fixes_become_word_sized_edits() {
        assert_eq!(applied("Many studies has shown that it matters.", "Many studies have shown that it matters."), vec![("has".into(), "have".into())]);
        assert_eq!(applied("its kinda broken lol", "it's kinda broken lol"), vec![("its".into(), "it's".into())]);
        assert_eq!(
            applied("I would like a three day extension, please.", "I would like a three-day extension, please."),
            vec![("three day".into(), "three-day".into())]
        );
    }

    #[test]
    fn a_missing_word_is_anchored_on_its_neighbour() {
        assert_eq!(applied("I want go home now.", "I want to go home now."), vec![("go".into(), "to go".into())]);
    }

    #[test]
    fn content_changes_dressed_as_grammar_are_refused() {
        assert_eq!(check("We met on the first of March in Hanoi.", "We met on March in Hanoi."), Err(FixReject::DropsAWord));
        assert_eq!(check("The launch moved to March and the team agreed.", "The launch moved to March 1 and the team agreed."), Err(FixReject::AddsANumber));
        assert_eq!(check("It asked for budgets and plans to announce a winner.", "It asked for budgets. They plan to announce a winner."), Err(FixReject::UnrelatedWord));
        assert_eq!(check("We shipped it yesterday morning.", "We shipped it early yesterday morning."), Err(FixReject::AddsContent));
        assert!(check("I saw the the cat.", "I saw the cat.").is_ok(), "a doubled word may go");
        assert!(check("Their going home then.", "They're going home then.").is_ok());
        assert!(check("I has a cat.", "I have a cat.").is_ok());
    }

    #[test]
    fn rewrites_and_cosmetic_changes_are_refused() {
        assert_eq!(check("i think its fine", "I think its fine"), Err(FixReject::OnlyCaseOrPunctuation));
        assert_eq!(check("We met, and talked.", "We met and talked."), Err(FixReject::Identical));
        assert_eq!(
            check("The team was happy with the results of the test.", "Everyone loved what the tests showed us today."),
            Err(FixReject::TooManyChanges)
        );
    }

    fn messages(orig: &str, fixed: &str, why: &str) -> Vec<(String, String, String)> {
        let edits = check(orig, fixed).expect("a grammar fix");
        let sent = Sentence { start: 0, end: orig.chars().count(), text: orig.into(), line: 0, judge: true };
        alerts(&TextMap::new(orig), &sent, &edits, why).into_iter().map(|a| (a.original, a.suggestions[0].clone(), a.message)).collect()
    }

    // The cases below are real answers the model gave (Oumu's spot-check cache, 2026-11-14).

    #[test]
    fn a_misspelling_the_model_kept_is_spelled_right() {
        let got = messages("She recieve the mesage yesterday and definately agree.", "She recieved the mesage yesterday and definately agreed.", "Incorrect verb form: 'recieve' and 'agree' should be in the past tense ('recieved' and 'agreed') due to 'yesterday'.");
        assert_eq!(got[0].1, "received", "{got:?}");
        assert_eq!(got[1].1, "agreed");
        // the reason quoted "recieved" (and "yesterday", not changed): replaced by a true one
        assert_eq!(got[0].2, "“recieve” should be “received” here.");
    }

    #[test]
    fn a_reason_about_words_that_did_not_change_is_replaced() {
        let got = messages("This is an example of a sentense that need some fixes.", "This is an example of a sentense that needs some fixes.", "Incorrect article 'a' before vowel sound, and subject-verb agreement 'need' to 'needs'.");
        assert_eq!(got, [("need".to_string(), "needs".to_string(), "“need” should be “needs” here.".to_string())]);
    }

    #[test]
    fn a_reason_about_the_changed_words_is_kept() {
        let got = messages("Their is a mistake in this sentence.", "There is a mistake in this sentence.", "Wrong homophone 'Their' used instead of 'There'.");
        assert_eq!(got[0].2, "Wrong homophone 'Their' used instead of 'There'.");
        // apostrophes inside a quoted word don't cut it short
        assert_eq!(quoted_words("Missing apostrophe in contraction 'can't'."), ["can't"]);
        assert_eq!(quoted_words("It isn't 'wrong' here."), ["wrong"]);
    }
}

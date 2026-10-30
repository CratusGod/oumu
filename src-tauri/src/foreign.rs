//! Text that is not English. Harper only knows English, so it flags every
//! Vietnamese word as a misspelling and every Vietnamese sentence as broken
//! grammar. Those notes are noise; this finds the spans to leave alone.

/// Share of words with non-English letters that makes a sentence foreign.
const FOREIGN_SHARE: f64 = 0.3;

/// Letters outside ASCII (ô, đ, é, …): an English word never needs one,
/// except the few loanwords Harper's dictionary already knows.
fn has_foreign_letter(word: &str) -> bool {
    word.chars().any(|c| c.is_alphabetic() && !c.is_ascii())
}

/// Vietnamese syllables typed without accents that are not English words
/// ("toi khong biet nhe"), plus common chat shorthand (ko, dc, bn).
const PLAIN_VIETNAMESE: &[&str] = &[
    "anh", "em", "oi", "roi", "nhe", "nha", "toi", "minh", "khong", "ko", "kh", "duoc", "dc", "chua",
    "nhieu", "gui", "xem", "hom", "vay", "nhung", "nguoi", "biet", "muon", "thich", "cung", "dang",
    "voi", "nhu", "thi", "vao", "lai", "giup", "nhau", "bao", "gio", "luon", "vui", "buon", "dau",
    "ngay", "thang", "nam", "tuan", "sang", "chieu", "dem", "xong", "hay", "qua",
    "bn", "mk", "ns", "vs", "ak", "ah", "uk", "uh", "ban", "cam", "on", "mai", "nay", "di",
];
/// The words above that are also everyday English: they count only next to
/// at least two unmistakably Vietnamese ones.
const ALSO_ENGLISH: &[&str] = &["ban", "cam", "on", "mai", "nay", "di", "hay", "qua", "ah", "uh", "vs", "dem", "nam", "em"];

fn plain_vietnamese_words(words: &[&str]) -> usize {
    let lower: Vec<String> = words.iter().map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase()).collect();
    let strong = lower.iter().filter(|w| PLAIN_VIETNAMESE.contains(&w.as_str()) && !ALSO_ENGLISH.contains(&w.as_str())).count();
    if strong < 2 {
        return strong;
    }
    lower.iter().filter(|w| PLAIN_VIETNAMESE.contains(&w.as_str())).count()
}

fn is_sentence_end(c: char) -> bool {
    matches!(c, '.' | '!' | '?' | '\n')
}

/// Char ranges (start, end) of sentences written mostly in another language.
fn foreign_sentences(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut sentence = String::new();
    let mut flush = |start: usize, end: usize, s: &mut String| {
        let words: Vec<&str> = s.split_whitespace().filter(|w| w.chars().any(char::is_alphabetic)).collect();
        let foreign = words.iter().filter(|w| has_foreign_letter(w)).count() + plain_vietnamese_words(&words);
        if foreign >= 2 && foreign as f64 >= words.len() as f64 * FOREIGN_SHARE {
            out.push((start, end));
        }
        s.clear();
    };
    for (i, c) in text.chars().enumerate() {
        sentence.push(c);
        if is_sentence_end(c) {
            flush(start, i + 1, &mut sentence);
            start = i + 1;
        }
    }
    flush(start, text.chars().count(), &mut sentence);
    out
}

/// Whether Harper's note on chars `start..end` lands on another language:
/// a word with non-English letters, or a sentence that is mostly such words.
/// Built once per text, then asked about every note.
pub struct ForeignMask {
    sentences: Vec<(usize, usize)>,
    chars: Vec<char>,
}

impl ForeignMask {
    pub fn new(text: &str) -> Self {
        ForeignMask { sentences: foreign_sentences(text), chars: text.chars().collect() }
    }

    pub fn covers(&self, start: usize, end: usize) -> bool {
        let end = end.min(self.chars.len());
        let start = start.min(end);
        self.chars[start..end].iter().any(|c| c.is_alphabetic() && !c.is_ascii())
            || self.sentences.iter().any(|&(s, e)| start < e && s < end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vietnamese_sentences_are_foreign_english_ones_are_not() {
        let t = "Hôm nay tôi đi học. I has teh cat.";
        let m = ForeignMask::new(t);
        assert!(m.covers(0, 3), "Hôm");
        assert!(m.covers(14, 16), "đi");
        let teh = t.find("teh").unwrap();
        let teh = t[..teh].chars().count();
        assert!(!m.covers(teh, teh + 3));
        assert!(m.covers(4, 7), "nay: an ASCII word inside a Vietnamese sentence");
    }

    #[test]
    fn vietnamese_without_accents_is_foreign_too() {
        let t = "ok anh oi, em gui file roi nhe. The report is finished.";
        let m = ForeignMask::new(t);
        assert!(m.covers(3, 6), "anh");
        let rep = t.find("report").unwrap();
        assert!(!m.covers(rep, rep + 6));
        let en = "I will be on the train by noon, then we can talk.";
        assert!(!ForeignMask::new(en).covers(0, en.len()), "English with on/then is not Vietnamese");
    }

    #[test]
    fn one_accented_name_does_not_make_a_sentence_foreign() {
        let t = "I met José at the cafe and we has lunch.";
        let m = ForeignMask::new(t);
        let at = |w: &str| t[..t.find(w).unwrap()].chars().count();
        let has = at("has");
        assert!(!m.covers(has, has + 3));
        let jose = at("José");
        assert!(m.covers(jose, jose + 4), "the name itself is left alone");
    }
}

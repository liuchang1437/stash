//! Where a query matched, for highlighting.
//!
//! Ranking (`search.rs`) only needs a score. For the hits that are actually
//! shown, this works out which characters matched, including Chinese
//! characters matched through their pinyin, and hands them to the UI as
//! UTF-16 ranges, the unit JavaScript strings are indexed in.

use nucleo_matcher::pattern::Pattern;
use nucleo_matcher::{Matcher, Utf32Str};
use pinyin::ToPinyin;

/// Half-open range of UTF-16 code units.
pub type Range = [u32; 2];

/// A row is cut when its first match starts later than this character, so
/// the match is not hidden behind the ellipsis at the end of the row.
const LEAD: usize = 12;
/// Characters kept before the first match when cutting.
const KEEP: usize = 6;
/// Longest text returned for a row.
const ROW_CHARS: usize = 200;
/// Most pieces returned by `terms`.
const MAX_TERMS: usize = 16;

/// A text in one of the forms it is matched in: as is, as full pinyin or as
/// pinyin initials. `source[i]` is the character of the original text that
/// produced `chars[i]`.
#[derive(Default)]
struct Form {
    chars: Vec<char>,
    source: Vec<usize>,
}

/// The text itself, then its full pinyin and initials if it contains Chinese.
/// Pinyin forms weigh a little less, as in the index.
fn forms(text: &str) -> Vec<(Form, f32)> {
    let mut plain = Form::default();
    let mut full = Form::default();
    let mut initials = Form::default();
    let mut chinese = false;
    for (i, c) in text.chars().enumerate() {
        plain.chars.push(c);
        plain.source.push(i);
        match c.to_pinyin() {
            Some(p) => {
                chinese = true;
                for (form, s) in [(&mut full, p.plain()), (&mut initials, p.first_letter())] {
                    for pc in s.chars() {
                        form.chars.push(pc);
                        form.source.push(i);
                    }
                }
            }
            None => {
                for form in [&mut full, &mut initials] {
                    form.chars.push(c);
                    form.source.push(i);
                }
            }
        }
    }
    let mut out = vec![(plain, 1.0)];
    if chinese {
        out.push((full, 0.9));
        out.push((initials, 0.9));
    }
    out
}

/// What part of a text a query matched.
#[derive(Debug)]
pub struct Found {
    /// How many words of the query matched.
    pub words: usize,
    score: f32,
    /// Matched characters of the original text, sorted.
    pub chars: Vec<usize>,
}

impl Found {
    fn beats(&self, other: &Found) -> bool {
        (self.words, self.score) > (other.words, other.score)
    }
}

/// Words (positive atoms) in the query.
pub fn word_count(pattern: &Pattern) -> usize {
    pattern.atoms.iter().filter(|a| !a.negative).count()
}

/// Matches each word of `pattern` against `text`, as is or as pinyin,
/// whichever matches best. Unlike ranking, a word that does not match is
/// skipped rather than failing the whole text, so a row still shows the
/// words it does contain.
pub fn find(pattern: &Pattern, matcher: &mut Matcher, text: &str) -> Option<Found> {
    let mut best: Option<Found> = None;
    let mut indices = Vec::new();
    for (form, weight) in forms(text) {
        let hay = Utf32Str::Unicode(&form.chars);
        let mut found = Found { words: 0, score: 0.0, chars: Vec::new() };
        for atom in pattern.atoms.iter().filter(|a| !a.negative) {
            indices.clear();
            if let Some(score) = atom.indices(hay, matcher, &mut indices) {
                found.words += 1;
                found.score += score as f32 * weight;
                found.chars.extend(indices.iter().map(|&i| form.source[i as usize]));
            }
        }
        if found.words > 0 && best.as_ref().is_none_or(|b| found.beats(b)) {
            best = Some(found);
        }
    }
    best.map(|mut found| {
        found.chars.sort_unstable();
        found.chars.dedup();
        found
    })
}

/// The trimmed line of `text` that matches the most words (best score on
/// ties, earlier line on equal scores). Shows why a long text was found.
pub fn best_line<'a>(pattern: &Pattern, matcher: &mut Matcher, text: &'a str) -> Option<(&'a str, Found)> {
    let mut best: Option<(&str, Found)> = None;
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if let Some(found) = find(pattern, matcher, line) {
            if best.as_ref().is_none_or(|(_, b)| found.beats(b)) {
                best = Some((line, found));
            }
        }
    }
    best
}

/// Text for a list row with its matches as UTF-16 ranges. When the first
/// match starts late, the beginning is replaced with `…` so the match stays
/// in view (the UI truncates the end of a row).
pub fn row(line: &str, chars: &[usize]) -> (String, Vec<Range>) {
    let all: Vec<char> = line.chars().collect();
    let start = match chars.first() {
        Some(&first) if first > LEAD => {
            // Rather start at a word than in the middle of one.
            let start = first - KEEP;
            (start - 4..start)
                .rev()
                .find(|&i| all[i].is_whitespace())
                .map_or(start, |i| i + 1)
        }
        _ => 0,
    };
    let mut text = String::new();
    let mut shift = 0;
    if start > 0 {
        text.push('…');
        shift = 1;
    }
    text.extend(all.iter().skip(start).take(ROW_CHARS));
    let kept: Vec<usize> = chars
        .iter()
        .filter(|&&c| c >= start && c < start + ROW_CHARS)
        .map(|&c| c - start + shift)
        .collect();
    let ranges = ranges(&text, &kept);
    (text, ranges)
}

/// Merges sorted character indices of `text` into UTF-16 ranges.
fn ranges(text: &str, chars: &[usize]) -> Vec<Range> {
    let mut out: Vec<Range> = Vec::new();
    let mut wanted = chars.iter().copied().peekable();
    let mut pos = 0;
    for (i, c) in text.chars().enumerate() {
        let len = c.len_utf16() as u32;
        if wanted.peek() == Some(&i) {
            wanted.next();
            match out.last_mut() {
                Some(r) if r[1] == pos => r[1] += len,
                _ => out.push([pos, pos + len]),
            }
        }
        pos += len;
    }
    out
}

/// Pieces of `text` that a word of the query matches literally (ignoring
/// case) or as pinyin, e.g. `zb` → `周报`. The preview highlights every
/// occurrence of them. Fuzzy matches are left out: scattered letters all
/// over a long text are noise.
pub fn terms(pattern: &Pattern, text: &str) -> Vec<String> {
    fn add(out: &mut Vec<String>, term: String) {
        if out.len() < MAX_TERMS && !out.iter().any(|t| t.to_lowercase() == term.to_lowercase()) {
            out.push(term);
        }
    }
    let lower = text.to_lowercase();
    let mut text_forms: Option<Vec<(Form, f32)>> = None;
    let mut out: Vec<String> = Vec::new();
    for atom in pattern.atoms.iter().filter(|a| !a.negative) {
        let needle = atom.needle_text().to_string().to_lowercase();
        if needle.is_empty() {
            continue;
        }
        if lower.contains(&needle) {
            add(&mut out, needle.clone());
        }
        // Single letters would light up half of any Chinese text.
        if needle.len() < 2 || !needle.bytes().all(|b| b.is_ascii_alphabetic()) {
            continue;
        }
        let text_forms = text_forms.get_or_insert_with(|| forms(text));
        let Some(((plain, _), pinyin)) = text_forms.split_first() else {
            continue;
        };
        let needle: Vec<char> = needle.chars().collect();
        for (form, _) in pinyin {
            for term in pinyin_terms(plain, form, &needle) {
                add(&mut out, term);
            }
        }
    }
    out
}

/// Runs of Chinese characters in `plain` whose pinyin `form` contains
/// `needle`, starting at the beginning of a syllable.
fn pinyin_terms(plain: &Form, form: &Form, needle: &[char]) -> Vec<String> {
    let mut out = Vec::new();
    let n = needle.len();
    if form.chars.len() < n {
        return out;
    }
    for start in 0..=form.chars.len() - n {
        let syllable_start = start == 0 || form.source[start] != form.source[start - 1];
        if !syllable_start || form.chars[start..start + n] != *needle {
            continue;
        }
        let (first, last) = (form.source[start], form.source[start + n - 1]);
        let run = &plain.chars[first..=last];
        if run.iter().all(|c| c.to_pinyin().is_some()) {
            out.push(run.iter().collect());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use nucleo_matcher::pattern::{CaseMatching, Normalization};
    use nucleo_matcher::Config;

    fn pattern(q: &str) -> Pattern {
        Pattern::parse(q, CaseMatching::Smart, Normalization::Smart)
    }

    fn marked(text: &str, ranges: &[Range]) -> String {
        let units: Vec<u16> = text.encode_utf16().collect();
        ranges
            .iter()
            .map(|[a, b]| String::from_utf16_lossy(&units[*a as usize..*b as usize]))
            .collect::<Vec<_>>()
            .join("|")
    }

    #[test]
    fn marks_words_in_any_order() {
        let mut m = Matcher::new(Config::DEFAULT);
        let found = find(&pattern("report weekly"), &mut m, "weekly report").unwrap();
        assert_eq!(found.words, 2);
        let (text, ranges) = row("weekly report", &found.chars);
        assert_eq!(marked(&text, &ranges), "weekly|report");
    }

    #[test]
    fn maps_pinyin_back_to_characters() {
        let mut m = Matcher::new(Config::DEFAULT);
        for q in ["zb", "zhoubao"] {
            let found = find(&pattern(q), &mut m, "本周周报").unwrap();
            let (text, ranges) = row("本周周报", &found.chars);
            assert_eq!(marked(&text, &ranges), "周报", "query {q}");
        }
    }

    #[test]
    fn skips_words_that_do_not_match() {
        let mut m = Matcher::new(Config::DEFAULT);
        let found = find(&pattern("deploy zzzz"), &mut m, "deploy script").unwrap();
        assert_eq!(found.words, 1);
    }

    #[test]
    fn picks_the_line_with_most_words() {
        let mut m = Matcher::new(Config::DEFAULT);
        let text = "first line\nsecond has token\nthird has token and key";
        let (line, found) = best_line(&pattern("token key"), &mut m, text).unwrap();
        assert_eq!(line, "third has token and key");
        assert_eq!(found.words, 2);
    }

    #[test]
    fn cuts_rows_so_late_matches_stay_visible() {
        let line = format!("{}needle", "x".repeat(40));
        let (text, ranges) = row(&line, &(40..46).collect::<Vec<_>>());
        assert!(text.starts_with('…'));
        assert_eq!(marked(&text, &ranges), "needle");
    }

    #[test]
    fn ranges_count_utf16_units() {
        // 😀 takes two UTF-16 units.
        let (text, ranges) = row("😀ab", &[1, 2]);
        assert_eq!(ranges, vec![[2, 4]]);
        assert_eq!(marked(&text, &ranges), "ab");
    }

    #[test]
    fn terms_include_literal_and_pinyin_matches() {
        let terms = terms(&pattern("zb TODO"), "本周周报 todo 其他");
        assert_eq!(terms, vec!["周报", "todo"]);
        // A syllable has to match from its start.
        assert!(super::terms(&pattern("ou"), "周报").is_empty());
    }
}

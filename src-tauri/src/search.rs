//! In-memory search over clipboard history and snippets.
//!
//! Everything is fuzzy-matched with nucleo against the title, the body and
//! a pinyin transliteration (full and initials) of both, then ranked by match
//! quality combined with frecency.

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32String};
use pinyin::ToPinyin;
use serde::{Deserialize, Serialize};

use crate::db::{ClipRow, Usage};
use crate::highlight::{self, Range};
use crate::snippets::Snippet;
use crate::template::Segment;

/// Only this many characters of a body are indexed.
const INDEXED_CHARS: usize = 1000;
/// Characters of the body sent to the UI for the preview pane.
const PREVIEW_CHARS: usize = 4000;

#[derive(Debug, Clone, PartialEq)]
pub enum ItemRef {
    Clip(i64),
    Snippet(String),
    /// Calculator result; not stored in the index.
    Calc(String),
}

impl ItemRef {
    pub fn key(&self) -> String {
        match self {
            ItemRef::Clip(id) => format!("c:{id}"),
            ItemRef::Snippet(path) => format!("s:{path}"),
            ItemRef::Calc(value) => format!("=:{value}"),
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        if let Some(id) = key.strip_prefix("c:") {
            id.parse().ok().map(ItemRef::Clip)
        } else if let Some(value) = key.strip_prefix("=:") {
            Some(ItemRef::Calc(value.to_string()))
        } else {
            key.strip_prefix("s:").map(|p| ItemRef::Snippet(p.to_string()))
        }
    }
}

pub struct Entry {
    pub item: ItemRef,
    pub title: String,
    pub body: String,
    pub tags: Vec<String>,
    pub source: Option<String>,
    pub pinned: bool,
    pub use_count: u32,
    /// Last copy or use, unix seconds. Drives recency ranking.
    pub last_used_at: i64,
    haystacks: Vec<(Utf32String, f32)>,
}

impl Entry {
    fn new(item: ItemRef, title: String, body: String, tags: Vec<String>) -> Self {
        let mut entry = Entry {
            item,
            title,
            body,
            tags,
            source: None,
            pinned: false,
            use_count: 0,
            last_used_at: 0,
            haystacks: Vec::new(),
        };
        entry.build_haystacks();
        entry
    }

    fn build_haystacks(&mut self) {
        let body: String = self.body.chars().take(INDEXED_CHARS).collect();
        let mut hay = Vec::new();
        let is_snippet = matches!(self.item, ItemRef::Snippet(_));
        if is_snippet {
            let title = format!("{} {}", self.title, self.tags.join(" "));
            push_with_pinyin(&mut hay, &title, 1.25);
        }
        push_with_pinyin(&mut hay, &body, 1.0);
        self.haystacks = hay;
    }

    pub fn from_clip(row: &ClipRow) -> Self {
        let mut entry = Entry::new(
            ItemRef::Clip(row.id),
            clip_title(&row.content),
            row.content.clone(),
            Vec::new(),
        );
        entry.source = row.source.clone();
        entry.pinned = row.pinned;
        entry.use_count = row.use_count;
        entry.last_used_at = row.last_used_at;
        entry
    }

    pub fn from_snippet(snippet: &Snippet, usage: Usage) -> Self {
        let mut entry = Entry::new(
            ItemRef::Snippet(snippet.path.clone()),
            snippet.title.clone(),
            snippet.body.clone(),
            snippet.tags.clone(),
        );
        entry.use_count = usage.use_count;
        entry.last_used_at = usage.last_used_at;
        entry
    }
}

fn clip_title(content: &str) -> String {
    let line = content
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    line.chars().take(200).collect()
}

fn push_with_pinyin(hay: &mut Vec<(Utf32String, f32)>, text: &str, weight: f32) {
    hay.push((Utf32String::from(text), weight));
    if let Some((full, initials)) = pinyin_forms(text) {
        hay.push((Utf32String::from(full.as_str()), weight * 0.9));
        hay.push((Utf32String::from(initials.as_str()), weight * 0.9));
    }
}

/// Full pinyin and initials, keeping non-Chinese characters as they are.
/// Returns None when the text contains no Chinese characters.
fn pinyin_forms(text: &str) -> Option<(String, String)> {
    let mut full = String::new();
    let mut initials = String::new();
    let mut any = false;
    for c in text.chars() {
        match c.to_pinyin() {
            Some(p) => {
                any = true;
                full.push_str(p.plain());
                initials.push_str(p.first_letter());
            }
            None => {
                full.push(c);
                initials.push(c);
            }
        }
    }
    any.then_some((full, initials))
}

/// Which items a search covers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    #[default]
    All,
    Clip,
    Snippet,
    Pinned,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Filter {
    #[serde(default)]
    pub source: Source,
    /// Only snippets with this tag (case-insensitive).
    #[serde(default)]
    pub tag: Option<String>,
}

impl Filter {
    /// No scope picked: everything, plus the calculator.
    pub fn is_all(&self) -> bool {
        self.source == Source::All && self.tag.is_none()
    }

    fn snippets_only(&self) -> bool {
        self.source == Source::Snippet || self.tag.is_some()
    }

    fn accepts(&self, e: &Entry) -> bool {
        if let Some(tag) = &self.tag {
            let tag = tag.to_lowercase();
            return e.tags.iter().any(|t| t.to_lowercase() == tag);
        }
        match self.source {
            Source::All => true,
            Source::Clip => matches!(e.item, ItemRef::Clip(_)),
            Source::Snippet => matches!(e.item, ItemRef::Snippet(_)),
            Source::Pinned => e.pinned,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub key: String,
    pub kind: &'static str,
    /// For a clip, the first line, or the line that matched the query.
    pub title: String,
    /// Matched parts of `title`.
    pub title_marks: Vec<Range>,
    /// A snippet found by its body: the line that matched.
    pub context: Option<String>,
    pub context_marks: Vec<Range>,
    /// Pieces of `preview` the preview pane highlights.
    pub terms: Vec<String>,
    pub preview: String,
    pub tags: Vec<String>,
    pub source: Option<String>,
    pub pinned: bool,
    pub use_count: u32,
    pub last_used_at: i64,
    pub chars: usize,
    /// A snippet with variables: what it pastes with the defaults. Set by
    /// `commands::search`, which can read the clipboard.
    pub rendered: Option<Vec<Segment>>,
}

impl Hit {
    fn from(entry: &Entry) -> Self {
        Hit {
            key: entry.item.key(),
            kind: match entry.item {
                ItemRef::Clip(_) => "clip",
                ItemRef::Snippet(_) => "snippet",
                ItemRef::Calc(_) => "calc",
            },
            title: entry.title.clone(),
            title_marks: Vec::new(),
            context: None,
            context_marks: Vec::new(),
            terms: Vec::new(),
            preview: entry.body.chars().take(PREVIEW_CHARS).collect(),
            tags: entry.tags.clone(),
            source: entry.source.clone(),
            pinned: entry.pinned,
            use_count: entry.use_count,
            last_used_at: entry.last_used_at,
            chars: entry.body.chars().count(),
            rendered: None,
        }
    }

    /// A search result with what matched marked. A clip whose first line
    /// lacks some of the words shows the line that has the most instead;
    /// a snippet keeps its title and shows that line as `context`.
    fn matched(entry: &Entry, pattern: &Pattern, matcher: &mut Matcher) -> Self {
        let mut hit = Hit::from(entry);
        let in_title = highlight::find(pattern, matcher, &entry.title);
        let title_words = in_title.as_ref().map_or(0, |f| f.words);
        // The match can only come from the indexed part of the body.
        let indexed: String = entry.body.chars().take(INDEXED_CHARS).collect();
        let in_body = if title_words < highlight::word_count(pattern) {
            highlight::best_line(pattern, matcher, &indexed).filter(|(_, f)| f.words > title_words)
        } else {
            None
        };
        match (&entry.item, in_title, in_body) {
            (ItemRef::Clip(_), _, Some((line, found))) => {
                (hit.title, hit.title_marks) = highlight::row(line, &found.chars);
            }
            (ItemRef::Snippet(_), None, Some((line, found))) => {
                let (context, marks) = highlight::row(line, &found.chars);
                hit.context = Some(context);
                hit.context_marks = marks;
            }
            (_, Some(found), _) => {
                (hit.title, hit.title_marks) = highlight::row(&entry.title, &found.chars);
            }
            _ => {}
        }
        hit.terms = highlight::terms(pattern, &hit.preview);
        hit
    }

    /// Calculator row: `title` is the result, `preview` the expression.
    pub fn calculation(expression: &str, result: String) -> Self {
        Hit {
            key: ItemRef::Calc(result.clone()).key(),
            kind: "calc",
            chars: result.chars().count(),
            rendered: None,
            title: result,
            title_marks: Vec::new(),
            context: None,
            context_marks: Vec::new(),
            terms: Vec::new(),
            preview: expression.to_string(),
            tags: Vec::new(),
            source: None,
            pinned: false,
            use_count: 0,
            last_used_at: 0,
        }
    }
}

#[derive(Default)]
pub struct Index {
    clips: Vec<Entry>,
    snippets: Vec<Entry>,
}

impl Index {
    pub fn set_clips(&mut self, rows: &[ClipRow]) {
        self.clips = rows.iter().map(Entry::from_clip).collect();
    }

    pub fn upsert_clip(&mut self, row: &ClipRow) {
        self.remove(&ItemRef::Clip(row.id));
        self.clips.push(Entry::from_clip(row));
    }

    pub fn set_snippets(&mut self, entries: Vec<Entry>) {
        self.snippets = entries;
    }

    pub fn remove(&mut self, item: &ItemRef) {
        self.clips.retain(|e| &e.item != item);
        self.snippets.retain(|e| &e.item != item);
    }

    pub fn get(&self, item: &ItemRef) -> Option<&Entry> {
        self.clips
            .iter()
            .chain(self.snippets.iter())
            .find(|e| &e.item == item)
    }

    pub fn get_mut(&mut self, item: &ItemRef) -> Option<&mut Entry> {
        self.clips
            .iter_mut()
            .chain(self.snippets.iter_mut())
            .find(|e| &e.item == item)
    }

    /// Texts of the `n` most recently copied or used clips, newest first.
    /// Pinning does not affect this order.
    pub fn recent_clips(&self, n: usize) -> Vec<String> {
        self.recent_entries(n)
            .into_iter()
            .map(|e| e.body.clone())
            .collect()
    }

    /// The `n` most recently copied or used clips, newest first.
    pub fn recent_entries(&self, n: usize) -> Vec<&Entry> {
        let mut clips: Vec<&Entry> = self.clips.iter().collect();
        clips.sort_by(|a, b| {
            b.last_used_at
                .cmp(&a.last_used_at)
                .then(clip_id(b).cmp(&clip_id(a)))
        });
        clips.truncate(n);
        clips
    }

    /// Snippet tags, most common first, with how many snippets carry each.
    pub fn tags(&self) -> Vec<TagCount> {
        let mut out: Vec<TagCount> = Vec::new();
        for tag in self.snippets.iter().flat_map(|e| &e.tags) {
            let lower = tag.to_lowercase();
            match out.iter_mut().find(|t| t.name.to_lowercase() == lower) {
                Some(t) => t.count += 1,
                None => out.push(TagCount { name: tag.clone(), count: 1 }),
            }
        }
        out.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
        out
    }

    pub fn query(&self, query: &str, filter: &Filter, now: i64, limit: usize) -> Vec<Hit> {
        let all = self
            .clips
            .iter()
            .chain(self.snippets.iter())
            .filter(|e| filter.accepts(e));
        let query = query.trim();

        if query.is_empty() {
            if filter.snippets_only() {
                return all_snippets(all.collect(), now).into_iter().take(limit).map(Hit::from).collect();
            }
            // Pinned first, then whatever was touched most recently. Snippets
            // that were never used stay out of the default list.
            let mut entries: Vec<&Entry> = all.filter(|e| e.last_used_at > 0 || e.pinned).collect();
            entries.sort_by(|a, b| {
                b.pinned
                    .cmp(&a.pinned)
                    .then(b.last_used_at.cmp(&a.last_used_at))
                    .then(clip_id(b).cmp(&clip_id(a)))
            });
            return entries.into_iter().take(limit).map(Hit::from).collect();
        }

        let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
        let mut matcher = Matcher::new(Config::DEFAULT);
        let mut scored: Vec<(f32, &Entry)> = all
            .filter_map(|e| {
                let best = e
                    .haystacks
                    .iter()
                    .filter_map(|(hay, w)| {
                        pattern
                            .score(hay.slice(..), &mut matcher)
                            .map(|s| s as f32 * w)
                    })
                    .fold(0.0f32, f32::max);
                (best > 0.0).then(|| (best + rank_bonus(e, now), e))
            })
            .collect();
        scored.sort_by(|a, b| b.0.total_cmp(&a.0));
        scored
            .into_iter()
            .take(limit)
            .map(|(_, e)| Hit::matched(e, &pattern, &mut matcher))
            .collect()
    }
}

#[derive(Debug, PartialEq, Serialize)]
pub struct TagCount {
    pub name: String,
    pub count: usize,
}

/// Browsing snippets: the used ones by frecency, then the rest by title
/// (Chinese titles by pinyin).
fn all_snippets(entries: Vec<&Entry>, now: i64) -> Vec<&Entry> {
    let (mut used, mut unused): (Vec<&Entry>, Vec<&Entry>) =
        entries.into_iter().partition(|e| e.last_used_at > 0);
    used.sort_by(|a, b| rank_bonus(b, now).total_cmp(&rank_bonus(a, now)));
    unused.sort_by_cached_key(|e| {
        pinyin_forms(&e.title)
            .map_or_else(|| e.title.clone(), |(full, _)| full)
            .to_lowercase()
    });
    used.extend(unused);
    used
}

/// Newer clips have larger ids; breaks ties between copies in the same second.
fn clip_id(e: &Entry) -> i64 {
    match e.item {
        ItemRef::Clip(id) => id,
        _ => 0,
    }
}

/// Frecency bonus added on top of the match score (which is roughly 20–30
/// points per matched character).
fn rank_bonus(e: &Entry, now: i64) -> f32 {
    let age_days = ((now - e.last_used_at).max(0) as f32) / 86_400.0;
    let recency = if e.last_used_at > 0 { 40.0 / (1.0 + age_days / 2.0) } else { 0.0 };
    let frequency = (1.0 + e.use_count as f32).ln() * 15.0;
    let kind = if matches!(e.item, ItemRef::Snippet(_)) { 15.0 } else { 0.0 };
    let pinned = if e.pinned { 25.0 } else { 0.0 };
    recency + frequency + kind + pinned
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip(id: i64, content: &str, t: i64) -> ClipRow {
        ClipRow {
            id,
            content: content.into(),
            source: None,
            last_used_at: t,
            use_count: 0,
            pinned: false,
        }
    }

    fn snippet(path: &str, title: &str, tags: &[&str], body: &str, last_used_at: i64) -> Entry {
        let snippet = Snippet {
            path: path.into(),
            title: title.into(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            body: body.into(),
        };
        Entry::from_snippet(&snippet, Usage { use_count: (last_used_at > 0) as u32, last_used_at })
    }

    fn keys(hits: &[Hit]) -> Vec<&str> {
        hits.iter().map(|h| h.key.as_str()).collect()
    }

    fn scope(source: Source) -> Filter {
        Filter { source, tag: None }
    }

    #[test]
    fn matches_pinyin_initials() {
        let mut index = Index::default();
        index.set_clips(&[clip(1, "你好世界", 10), clip(2, "hello", 10)]);
        let hits = index.query("nhsj", &Filter::default(), 10, 10);
        assert_eq!(hits[0].key, "c:1");
        assert_eq!(hits[0].title_marks, vec![[0, 4]]);
    }

    #[test]
    fn empty_query_lists_recent_first() {
        let mut index = Index::default();
        index.set_clips(&[clip(1, "old", 1), clip(2, "new", 5)]);
        let hits = index.query("", &Filter::default(), 10, 10);
        assert_eq!(keys(&hits), ["c:2", "c:1"]);
    }

    #[test]
    fn scopes_limit_the_items() {
        let mut index = Index::default();
        let mut pinned = clip(2, "note b", 5);
        pinned.pinned = true;
        index.set_clips(&[clip(1, "note a", 5), pinned]);
        index.set_snippets(vec![
            snippet("x.md", "note x", &["work"], "", 5),
            snippet("y.md", "note y", &["Home"], "", 5),
        ]);
        let search = |filter: &Filter| {
            let mut k: Vec<String> = keys(&index.query("note", filter, 10, 10)).into_iter().map(String::from).collect();
            k.sort();
            k
        };
        assert_eq!(search(&scope(Source::Clip)), ["c:1", "c:2"]);
        assert_eq!(search(&scope(Source::Snippet)), ["s:x.md", "s:y.md"]);
        assert_eq!(search(&scope(Source::Pinned)), ["c:2"]);
        let tagged = Filter { source: Source::Snippet, tag: Some("home".into()) };
        assert_eq!(search(&tagged), ["s:y.md"]);
    }

    #[test]
    fn snippet_scope_lists_unused_snippets_too() {
        let mut index = Index::default();
        index.set_snippets(vec![
            snippet("b.md", "报告", &[], "", 0),
            snippet("a.md", "Apple", &[], "", 0),
            snippet("u.md", "Used", &[], "", 5),
        ]);
        assert!(index.query("", &Filter::default(), 10, 10).len() == 1);
        // Used first, then by title, Chinese by pinyin (报告 = baogao).
        let hits = index.query("", &scope(Source::Snippet), 10, 10);
        assert_eq!(keys(&hits), ["s:u.md", "s:a.md", "s:b.md"]);
    }

    #[test]
    fn clips_show_the_line_that_matched() {
        let mut index = Index::default();
        index.set_clips(&[clip(1, "first line\nsecond line has the token", 5)]);
        let hits = index.query("token", &Filter::default(), 10, 10);
        assert_eq!(hits[0].title, "…has the token");
        assert_eq!(hits[0].terms, ["token"]);
    }

    #[test]
    fn snippets_show_the_body_line_as_context() {
        let mut index = Index::default();
        index.set_snippets(vec![snippet("r.md", "Report", &[], "intro\nsee deploy notes", 5)]);
        let hit = &index.query("deploy", &Filter::default(), 10, 10)[0];
        assert_eq!(hit.title, "Report");
        assert_eq!(hit.context.as_deref(), Some("see deploy notes"));
        assert_eq!(hit.context_marks, vec![[4, 10]]);
    }

    #[test]
    fn counts_tags_case_insensitively() {
        let mut index = Index::default();
        index.set_snippets(vec![
            snippet("a.md", "a", &["work", "mail"], "", 0),
            snippet("b.md", "b", &["Work"], "", 0),
        ]);
        let tags = index.tags();
        assert_eq!(tags[0], TagCount { name: "work".into(), count: 2 });
        assert_eq!(tags[1], TagCount { name: "mail".into(), count: 1 });
    }

    #[test]
    fn item_keys_round_trip() {
        for item in [
            ItemRef::Clip(7),
            ItemRef::Snippet("a/b:c.md".into()),
            ItemRef::Calc("-1.5e20".into()),
        ] {
            assert_eq!(ItemRef::parse(&item.key()), Some(item));
        }
    }
}

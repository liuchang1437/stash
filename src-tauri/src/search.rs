//! In-memory search over clipboard history and snippets.
//!
//! Everything is fuzzy-matched with nucleo against the title, the body and
//! a pinyin transliteration (full and initials) of both, then ranked by match
//! quality combined with frecency.

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32String};
use pinyin::ToPinyin;
use serde::Serialize;

use crate::db::{ClipRow, Usage};
use crate::snippets::Snippet;

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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub key: String,
    pub kind: &'static str,
    pub title: String,
    pub preview: String,
    pub tags: Vec<String>,
    pub source: Option<String>,
    pub pinned: bool,
    pub use_count: u32,
    pub last_used_at: i64,
    pub chars: usize,
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
            preview: entry.body.chars().take(PREVIEW_CHARS).collect(),
            tags: entry.tags.clone(),
            source: entry.source.clone(),
            pinned: entry.pinned,
            use_count: entry.use_count,
            last_used_at: entry.last_used_at,
            chars: entry.body.chars().count(),
        }
    }

    /// Calculator row: `title` is the result, `preview` the expression.
    pub fn calculation(expression: &str, result: String) -> Self {
        Hit {
            key: ItemRef::Calc(result.clone()).key(),
            kind: "calc",
            chars: result.chars().count(),
            title: result,
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
        let mut clips: Vec<&Entry> = self.clips.iter().collect();
        clips.sort_by(|a, b| {
            b.last_used_at
                .cmp(&a.last_used_at)
                .then(clip_id(b).cmp(&clip_id(a)))
        });
        clips.into_iter().take(n).map(|e| e.body.clone()).collect()
    }

    pub fn query(&self, query: &str, now: i64, limit: usize) -> Vec<Hit> {
        let all = self.clips.iter().chain(self.snippets.iter());
        let query = query.trim();

        if query.is_empty() {
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
        scored.into_iter().take(limit).map(|(_, e)| Hit::from(e)).collect()
    }
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

    #[test]
    fn matches_pinyin_initials() {
        let mut index = Index::default();
        index.set_clips(&[clip(1, "你好世界", 10), clip(2, "hello", 10)]);
        let hits = index.query("nhsj", 10, 10);
        assert_eq!(hits[0].key, "c:1");
    }

    #[test]
    fn empty_query_lists_recent_first() {
        let mut index = Index::default();
        index.set_clips(&[clip(1, "old", 1), clip(2, "new", 5)]);
        let hits = index.query("", 10, 10);
        assert_eq!(hits.iter().map(|h| h.key.as_str()).collect::<Vec<_>>(), ["c:2", "c:1"]);
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

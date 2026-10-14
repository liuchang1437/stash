//! Snippet templates.
//!
//! Syntax:
//! - `{{date}}` / `{{date:yyyy-MM-dd}}`, `{{time}}`, `{{datetime}}` – current time
//! - `{{clipboard}}` – clipboard text at the moment of pasting
//! - `{{clipboard:N}}` – N-th most recent clipboard history entry (1 = latest)
//! - `{{uuid}}` – random UUID v4
//! - `{{cursor}}` – where the caret ends up after pasting
//! - `{{name}}`, `{{name=default}}`, `{{name:a|b|c}}` – asked from the user
//!
//! In what is filled in for a `{{name}}` (and in its default), `$N` stands
//! for the N-th most recent clipboard history entry, like `{{clipboard:N}}`;
//! one more `$` makes it literal (`$$1` → `$1`).
//!
//! A value can be passed through operations, applied left to right:
//! `{{clipboard | trim | lower}}`, `{{name=x | upper}}` (see `Filter`). Only
//! a trailing `| op` with a known operation name counts, so `{{x=a|b}}` keeps
//! `a|b` as its default; choices (`{{name:a|b}}`) take none, since there `|`
//! separates the options.
//!
//! An unterminated `{{` is kept as literal text.

use std::collections::HashMap;

use serde::Serialize;

#[derive(Debug, Clone, PartialEq)]
pub enum Part {
    Text(String),
    Cursor,
    /// A value worked out when rendering, then passed through the filters.
    Value(Source, Vec<Filter>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    Date(String),
    Clipboard,
    /// 1-based position in the clipboard history, newest first.
    ClipHistory(usize),
    Uuid,
    Input(Field),
}

/// An operation on a variable's value, written `{{var | name}}`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Filter {
    /// Strips leading and trailing whitespace, line breaks included.
    Trim,
    Upper,
    Lower,
    /// Joins the lines with single spaces, each trimmed, blank ones dropped.
    Oneline,
    /// Percent-encodes; in open mode the value is then not encoded again.
    Url,
}

impl Filter {
    fn from_name(name: &str) -> Option<Filter> {
        Some(match name {
            "trim" => Filter::Trim,
            "upper" => Filter::Upper,
            "lower" => Filter::Lower,
            "oneline" => Filter::Oneline,
            "url" => Filter::Url,
            _ => return None,
        })
    }

    fn apply(self, value: &str) -> String {
        match self {
            Filter::Trim => value.trim().to_string(),
            Filter::Upper => value.to_uppercase(),
            Filter::Lower => value.to_lowercase(),
            Filter::Oneline => value
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
                .join(" "),
            Filter::Url => percent_encode(value),
        }
    }
}

fn apply_filters(value: String, filters: &[Filter]) -> String {
    filters.iter().fold(value, |v, f| f.apply(&v))
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Field {
    pub name: String,
    pub default: String,
    pub options: Vec<String>,
}

pub fn parse(src: &str) -> Vec<Part> {
    let mut parts = Vec::new();
    let mut text = String::new();
    let mut rest = src;
    while let Some(start) = rest.find("{{") {
        let Some(len) = rest[start + 2..].find("}}") else {
            break;
        };
        let inner = &rest[start + 2..start + 2 + len];
        text.push_str(&rest[..start]);
        match parse_var(inner) {
            Some(part) => {
                if !text.is_empty() {
                    parts.push(Part::Text(std::mem::take(&mut text)));
                }
                parts.push(part);
            }
            None => {
                text.push_str("{{");
                text.push_str(inner);
                text.push_str("}}");
            }
        }
        rest = &rest[start + 2 + len + 2..];
    }
    text.push_str(rest);
    if !text.is_empty() {
        parts.push(Part::Text(text));
    }
    parts
}

const BUILTINS: [&str; 6] = ["date", "time", "datetime", "clipboard", "uuid", "cursor"];

/// The name, the separator (`:`, `=` or empty) and the rest, trimmed.
fn split_head(inner: &str) -> (&str, &str, &str) {
    match inner.find([':', '=']) {
        Some(i) => (inner[..i].trim(), &inner[i..i + 1], inner[i + 1..].trim()),
        None => (inner, "", ""),
    }
}

/// Splits the trailing `| op`s with known operation names off `inner`.
fn split_filters(inner: &str) -> (&str, Vec<Filter>) {
    let mut rest = inner;
    let mut filters = Vec::new();
    while let Some(bar) = rest.rfind('|') {
        let Some(filter) = Filter::from_name(rest[bar + 1..].trim()) else {
            break;
        };
        filters.push(filter);
        rest = rest[..bar].trim_end();
    }
    filters.reverse();
    (rest, filters)
}

fn parse_var(inner: &str) -> Option<Part> {
    let inner = inner.trim();
    let (head, sep, _) = split_head(inner);
    let choice = sep == ":" && !BUILTINS.contains(&head);
    let (inner, filters) = if choice { (inner, Vec::new()) } else { split_filters(inner) };
    let (name, sep, arg) = split_head(inner);
    if name.is_empty() || name.contains(char::is_whitespace) || name.contains('|') {
        return None;
    }
    let fmt = |default: &str| {
        if sep == ":" && !arg.is_empty() {
            arg.to_string()
        } else {
            default.to_string()
        }
    };
    let source = match name {
        "date" => Source::Date(fmt("yyyy-MM-dd")),
        "time" => Source::Date(fmt("HH:mm")),
        "datetime" => Source::Date(fmt("yyyy-MM-dd HH:mm")),
        "clipboard" if sep == ":" => Source::ClipHistory(arg.parse().ok().filter(|n| *n > 0)?),
        "clipboard" => Source::Clipboard,
        "uuid" => Source::Uuid,
        "cursor" => return Some(Part::Cursor),
        _ => {
            let (default, options) = match sep {
                "=" => (arg.to_string(), Vec::new()),
                ":" => {
                    let options: Vec<String> = arg
                        .split('|')
                        .map(|o| o.trim().to_string())
                        .filter(|o| !o.is_empty())
                        .collect();
                    (options.first().cloned().unwrap_or_default(), options)
                }
                _ => (String::new(), Vec::new()),
            };
            Source::Input(Field {
                name: name.to_string(),
                default,
                options,
            })
        }
    };
    Some(Part::Value(source, filters))
}

/// Whether rendering needs the current clipboard text.
pub fn uses_clipboard(parts: &[Part]) -> bool {
    parts.iter().any(|p| matches!(p, Part::Value(Source::Clipboard, _)))
}

/// Fields the user has to fill in, deduplicated by name (first one wins).
pub fn fields(parts: &[Part]) -> Vec<Field> {
    let mut out: Vec<Field> = Vec::new();
    for part in parts {
        if let Part::Value(Source::Input(field), _) = part {
            if !out.iter().any(|f| f.name == field.name) {
                out.push(field.clone());
            }
        }
    }
    out
}

/// How many clipboard history entries the template refers to, including
/// `$N` in the values filled in (defaults for the rest).
pub fn history_depth(parts: &[Part], values: &HashMap<String, String>) -> usize {
    parts
        .iter()
        .filter_map(|p| match p {
            Part::Value(Source::ClipHistory(n), _) => Some(*n),
            Part::Value(Source::Input(field), _) => value_refs(input_value(field, values)).max(),
            _ => None,
        })
        .max()
        .unwrap_or(0)
}

fn input_value<'a>(field: &'a Field, values: &'a HashMap<String, String>) -> &'a str {
    values.get(&field.name).unwrap_or(&field.default)
}

/// A piece of a filled-in value: literal text, or a `$N` reference.
enum Piece<'a> {
    Text(&'a str),
    Ref(usize),
}

/// Splits `value` at `$N` references (`＄N` too, as in the calculator). A
/// run of several `$` before the digits is literal text with one `$`
/// dropped; `$0` and a `$` without digits stay as they are.
fn pieces(value: &str) -> Vec<Piece<'_>> {
    let mut out = Vec::new();
    let mut text_from = 0;
    let mut i = 0;
    while i < value.len() {
        let rest = &value[i..];
        let dollars: usize = rest
            .chars()
            .take_while(|c| matches!(c, '$' | '＄'))
            .map(char::len_utf8)
            .sum();
        if dollars == 0 {
            i += rest.chars().next().map_or(1, char::len_utf8);
            continue;
        }
        let digits = rest[dollars..].bytes().take_while(u8::is_ascii_digit).count();
        let n: usize = rest[dollars..dollars + digits].parse().unwrap_or(0);
        if n > 0 {
            let first = rest.chars().next().map_or(1, char::len_utf8);
            out.push(Piece::Text(&value[text_from..i]));
            if dollars > first {
                // `$$1`: drop the escaping `$` and keep the rest as text.
                text_from = i + first;
            } else {
                out.push(Piece::Ref(n));
                text_from = i + dollars + digits;
            }
        }
        i += dollars + digits;
    }
    out.push(Piece::Text(&value[text_from..]));
    out.retain(|p| !matches!(p, Piece::Text("")));
    out
}

fn value_refs(value: &str) -> impl Iterator<Item = usize> + '_ {
    pieces(value).into_iter().filter_map(|p| match p {
        Piece::Ref(n) => Some(n),
        Piece::Text(_) => None,
    })
}

/// A filled-in value with its `$N` references replaced by clipboard history
/// entries (empty beyond the end of `history`).
fn expand_refs(value: &str, history: &[String]) -> String {
    pieces(value)
        .into_iter()
        .map(|p| match p {
            Piece::Text(t) => t,
            Piece::Ref(n) => history.get(n - 1).map_or("", String::as_str),
        })
        .collect()
}

/// Rendered text and how many characters the caret must move left after
/// pasting to land on `{{cursor}}`.
pub struct Rendered {
    pub text: String,
    pub cursor_back: usize,
}

/// `history` holds clipboard history texts, newest first (`history_depth` of
/// them, or all there are); entries beyond its end render as empty text.
///
/// With `url_encode`, variable values that land inside a URL (after a
/// `scheme://`) are percent-encoded, so `https://x.com/s?q={{q}}` works with
/// any input. A value that *is* the whole URL (`{{url}}`) stays untouched,
/// and so does one already encoded with `| url`.
pub fn render(
    parts: &[Part],
    values: &HashMap<String, String>,
    clipboard: &str,
    history: &[String],
    url_encode: bool,
) -> Rendered {
    let now = chrono::Local::now();
    let mut text = String::new();
    let mut cursor_at = None;
    for part in parts {
        match part {
            Part::Text(t) => text.push_str(t),
            Part::Cursor => {
                if cursor_at.is_none() {
                    cursor_at = Some(text.len());
                }
            }
            Part::Value(source, filters) => {
                let raw = match source {
                    Source::Uuid => uuid::Uuid::new_v4().to_string(),
                    _ => source_text(source, values, clipboard, history, &now),
                };
                let value = apply_filters(raw, filters);
                if url_encode && !filters.contains(&Filter::Url) && text.contains("://") {
                    text.push_str(&percent_encode(&value));
                } else {
                    text.push_str(&value);
                }
            }
        }
    }
    let cursor_back = cursor_at
        .map(|at| text[at..].replace("\r\n", "\n").chars().count())
        .unwrap_or(0);
    Rendered { text, cursor_back }
}

/// What `source` stands for, before its filters. A UUID is empty here; only
/// `render` makes one.
fn source_text(
    source: &Source,
    values: &HashMap<String, String>,
    clipboard: &str,
    history: &[String],
    now: &chrono::DateTime<chrono::Local>,
) -> String {
    match source {
        Source::Date(f) => now.format(&to_strftime(f)).to_string(),
        Source::Clipboard => clipboard.to_string(),
        Source::ClipHistory(n) => history.get(n - 1).cloned().unwrap_or_default(),
        Source::Uuid => String::new(),
        Source::Input(field) => expand_refs(input_value(field, values), history),
    }
}

/// Where a piece of rendered text comes from.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SegmentKind {
    /// The template's own text.
    Text,
    /// A value the user fills in.
    Input,
    /// Filled in automatically: date, clipboard, UUID.
    Auto,
    /// Where the caret ends up; always empty.
    Cursor,
}

/// A piece of rendered text. `name` is the field name of an input, or what
/// an automatic value is: `date`, `clipboard`, `clipboard:N` or `uuid`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Segment {
    pub text: String,
    pub kind: SegmentKind,
    pub name: Option<String>,
}

/// Like `render` (without automatic URL encoding), but keeps variable values
/// apart from the surrounding text for a live preview. A UUID is left empty:
/// the real one is only made when pasting, so any value shown here would be
/// wrong.
pub fn render_segments(
    parts: &[Part],
    values: &HashMap<String, String>,
    clipboard: &str,
    history: &[String],
) -> Vec<Segment> {
    let now = chrono::Local::now();
    let mut out: Vec<Segment> = Vec::new();
    let mut push = |text: String, kind: SegmentKind, name: Option<String>| match out.last_mut() {
        Some(last) if kind == SegmentKind::Text && last.kind == SegmentKind::Text => last.text.push_str(&text),
        _ => out.push(Segment { text, kind, name }),
    };
    for part in parts {
        match part {
            Part::Text(t) => push(t.clone(), SegmentKind::Text, None),
            Part::Cursor => push(String::new(), SegmentKind::Cursor, None),
            Part::Value(source, filters) => {
                let (kind, name) = match source {
                    Source::Input(field) => (SegmentKind::Input, field.name.clone()),
                    Source::Date(_) => (SegmentKind::Auto, "date".to_string()),
                    Source::Clipboard => (SegmentKind::Auto, "clipboard".to_string()),
                    Source::ClipHistory(n) => (SegmentKind::Auto, format!("clipboard:{n}")),
                    Source::Uuid => (SegmentKind::Auto, "uuid".to_string()),
                };
                let raw = source_text(source, values, clipboard, history, &now);
                push(apply_filters(raw, filters), kind, Some(name));
            }
        }
    }
    out
}

/// Percent-encodes everything except RFC 3986 unreserved characters.
fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Converts `yyyy-MM-dd HH:mm:ss` style patterns to chrono's strftime syntax.
fn to_strftime(pattern: &str) -> String {
    const TOKENS: [(&str, &str); 8] = [
        ("yyyy", "%Y"),
        ("yy", "%y"),
        ("MM", "%m"),
        ("dd", "%d"),
        ("HH", "%H"),
        ("hh", "%I"),
        ("mm", "%M"),
        ("ss", "%S"),
    ];
    let mut out = String::new();
    let mut rest = pattern;
    'outer: while let Some(c) = rest.chars().next() {
        for (token, spec) in TOKENS {
            if let Some(stripped) = rest.strip_prefix(token) {
                out.push_str(spec);
                rest = stripped;
                continue 'outer;
            }
        }
        if c == '%' {
            out.push_str("%%");
        } else {
            out.push(c);
        }
        rest = &rest[c.len_utf8()..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_inputs_and_builtins() {
        let parts = parse("Hi {{name=Bob}}, env {{env:prod|dev}} {{cursor}}!");
        let f = fields(&parts);
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].default, "Bob");
        assert_eq!(f[1].options, vec!["prod", "dev"]);
        assert!(parts.contains(&Part::Cursor));
    }

    #[test]
    fn renders_with_cursor() {
        let parts = parse("a{{cursor}}bc{{x}}");
        let mut values = HashMap::new();
        values.insert("x".to_string(), "中文".to_string());
        let r = render(&parts, &values, "", &[], false);
        assert_eq!(r.text, "abc中文");
        assert_eq!(r.cursor_back, 4);
    }

    #[test]
    fn keeps_invalid_braces_literal() {
        let parts = parse("{{ not a var }} and {{unclosed} {{clipboard:0}} {{clipboard:x}}");
        let r = render(&parts, &HashMap::new(), "", &[], false);
        assert_eq!(r.text, "{{ not a var }} and {{unclosed} {{clipboard:0}} {{clipboard:x}}");
    }

    #[test]
    fn url_encodes_values_inside_urls() {
        let mut values = HashMap::new();
        values.insert("q".to_string(), "rust 中文&a=b".to_string());
        values.insert("url".to_string(), "https://a.com/x?y=1".to_string());

        let parts = parse("https://www.baidu.com/s?wd={{q}}");
        let r = render(&parts, &values, "", &[], true);
        assert_eq!(r.text, "https://www.baidu.com/s?wd=rust%20%E4%B8%AD%E6%96%87%26a%3Db");

        let whole = render(&parse("{{url}}"), &values, "", &[], true);
        assert_eq!(whole.text, "https://a.com/x?y=1");

        let pasted = render(&parts, &values, "", &[], false);
        assert_eq!(pasted.text, "https://www.baidu.com/s?wd=rust 中文&a=b");
    }

    #[test]
    fn renders_clipboard_history() {
        let parts = parse("{{clipboard:3}}-{{clipboard:2}}-{{clipboard:1}}-{{clipboard:9}}|{{clipboard}}");
        assert_eq!(history_depth(&parts, &HashMap::new()), 9);
        let history = ["C", "B", "A"].map(String::from);
        let r = render(&parts, &HashMap::new(), "now", &history, false);
        assert_eq!(r.text, "A-B-C-|now");
    }

    #[test]
    fn expands_clip_refs_in_values() {
        let history = ["new", "old"].map(String::from);
        let expand = |v: &str| expand_refs(v, &history);
        assert_eq!(expand("$1/$2/$3"), "new/old/");
        assert_eq!(expand("a$1b ＄2"), "anewb old");
        assert_eq!(expand("$$1 and $$$2 and ＄＄1"), "$1 and $$2 and ＄1");
        assert_eq!(expand("$ $0 $$ $x 100$"), "$ $0 $$ $x 100$");
        assert_eq!(expand("中文$1中文"), "中文new中文");

        let parts = parse("{{a}}-{{b=$2}}-{{c=$$9}}");
        let mut values = HashMap::new();
        values.insert("a".to_string(), "$1".to_string());
        assert_eq!(history_depth(&parts, &values), 2);
        assert_eq!(render(&parts, &values, "", &history, false).text, "new-old-$9");
        values.insert("c".to_string(), "$5".to_string());
        assert_eq!(history_depth(&parts, &values), 5);

        let segs = render_segments(&parts, &values, "", &history);
        assert_eq!(segs[0].text, "new");
        assert_eq!(segs[0].kind, SegmentKind::Input);

        // The clip lands inside a URL, so it is encoded like typed text.
        let url = parse("https://x.com/s?q={{q}}");
        values.insert("q".to_string(), "$1 & $2".to_string());
        let r = render(&url, &values, "", &["a b".to_string(), "c".to_string()], true);
        assert_eq!(r.text, "https://x.com/s?q=a%20b%20%26%20c");
    }

    fn shape(segs: &[Segment]) -> Vec<(&str, SegmentKind, Option<&str>)> {
        segs.iter()
            .map(|s| (s.text.as_str(), s.kind, s.name.as_deref()))
            .collect()
    }

    #[test]
    fn segments_mark_variables() {
        use SegmentKind::*;
        let parts = parse("rg '{{reason:a|b}}' {{file=x.log}}{{cursor}}");
        let mut values = HashMap::new();
        values.insert("reason".to_string(), "b".to_string());
        let segs = render_segments(&parts, &values, "", &[]);
        assert_eq!(
            shape(&segs),
            [
                ("rg '", Text, None),
                ("b", Input, Some("reason")),
                ("' ", Text, None),
                ("x.log", Input, Some("file")),
                ("", Cursor, None),
            ]
        );
    }

    #[test]
    fn segments_mark_automatic_values() {
        use SegmentKind::*;
        let parts = parse("{{clipboard}}/{{clipboard:2}}/{{uuid}}/{{date:yyyy}}");
        let history = ["new", "old"].map(String::from);
        let segs = render_segments(&parts, &HashMap::new(), "now", &history);
        let year = chrono::Local::now().format("%Y").to_string();
        assert_eq!(
            shape(&segs),
            [
                ("now", Auto, Some("clipboard")),
                ("/", Text, None),
                ("old", Auto, Some("clipboard:2")),
                ("/", Text, None),
                // The real UUID is only made when pasting.
                ("", Auto, Some("uuid")),
                ("/", Text, None),
                (year.as_str(), Auto, Some("date")),
            ]
        );
    }

    #[test]
    fn parses_filters() {
        use Filter::*;
        let filters = |src: &str| match parse(src).as_slice() {
            [Part::Value(_, f)] => f.clone(),
            other => panic!("{src}: {other:?}"),
        };
        assert_eq!(filters("{{clipboard | trim}}"), [Trim]);
        assert_eq!(filters("{{clipboard:2|trim|upper}}"), [Trim, Upper]);
        assert_eq!(filters("{{date:HH:mm | lower }}"), [Lower]);
        assert_eq!(filters("{{q | oneline | url}}"), [Oneline, Url]);
        assert_eq!(filters("{{uuid|upper}}"), [Upper]);

        // A default keeps a `|` that isn't followed by an operation.
        let parts = parse("{{x=a|b}}{{y=a | b | trim}}");
        let f = fields(&parts);
        assert_eq!((f[0].default.as_str(), f[1].default.as_str()), ("a|b", "a | b"));

        // In choices `|` separates the options, even ones named like operations.
        let parts = parse("{{case:upper|lower}}");
        assert_eq!(fields(&parts)[0].options, ["upper", "lower"]);
        assert_eq!(filters("{{case:upper|lower}}"), []);

        // An unknown operation leaves the whole thing literal.
        let r = render(&parse("{{clipboard | nope}}{{a|b}}"), &HashMap::new(), "x", &[], false);
        assert_eq!(r.text, "{{clipboard | nope}}{{a|b}}");
        assert!(matches!(parse("{{cursor | trim}}").as_slice(), [Part::Cursor]));
    }

    #[test]
    fn applies_filters() {
        let history = ["  last\n".to_string()];
        let mut values = HashMap::new();
        values.insert("q".to_string(), " line one\n\n  line two \r\n".to_string());
        values.insert("ref".to_string(), "$1".to_string());
        let render_str = |src: &str| render(&parse(src), &values, "\t Clip Board \n", &history, false).text;
        assert_eq!(render_str("[{{clipboard | trim}}]"), "[Clip Board]");
        assert_eq!(render_str("[{{clipboard | trim | upper}}]"), "[CLIP BOARD]");
        assert_eq!(render_str("[{{clipboard|lower|trim}}]"), "[clip board]");
        assert_eq!(render_str("[{{q | oneline}}]"), "[line one line two]");
        assert_eq!(render_str("[{{ref | trim}}][{{clipboard:1|trim}}]"), "[last][last]");
        assert_eq!(render_str("[{{q}}]"), "[ line one\n\n  line two \r\n]");
        assert_eq!(render_str("{{q|trim|url}}"), "line%20one%0A%0A%20%20line%20two");
        let id = render_str("{{uuid | upper}}");
        assert_eq!(id, id.to_uppercase());
        assert_eq!(id.len(), 36);

        // `| url` is not encoded a second time when opening.
        values.insert("s".to_string(), "a b".to_string());
        let open = |src: &str| render(&parse(src), &values, "", &[], true).text;
        assert_eq!(open("https://x/?q={{s | url}}"), "https://x/?q=a%20b");
        assert_eq!(open("https://x/?q={{s | upper}}"), "https://x/?q=A%20B");

        // Same field, an operation per occurrence; the preview shows the result.
        let parts = parse("{{s}}/{{s | upper}}");
        assert_eq!(fields(&parts).len(), 1);
        let segs = render_segments(&parts, &values, "", &[]);
        assert_eq!(shape(&segs)[2], ("A B", SegmentKind::Input, Some("s")));
    }

    #[test]
    fn converts_date_patterns() {
        assert_eq!(to_strftime("yyyy-MM-dd HH:mm"), "%Y-%m-%d %H:%M");
        assert_eq!(to_strftime("100%"), "100%%");
    }
}

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
//! An unterminated `{{` is kept as literal text.

use std::collections::HashMap;

use serde::Serialize;

#[derive(Debug, Clone, PartialEq)]
pub enum Part {
    Text(String),
    Date(String),
    Clipboard,
    /// 1-based position in the clipboard history, newest first.
    ClipHistory(usize),
    Uuid,
    Cursor,
    Input(Field),
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

fn parse_var(inner: &str) -> Option<Part> {
    let inner = inner.trim();
    let split = inner.find([':', '=']);
    let (name, sep, arg) = match split {
        Some(i) => (inner[..i].trim(), &inner[i..i + 1], inner[i + 1..].trim()),
        None => (inner, "", ""),
    };
    if name.is_empty() || name.contains(char::is_whitespace) {
        return None;
    }
    let fmt = |default: &str| {
        if sep == ":" && !arg.is_empty() {
            arg.to_string()
        } else {
            default.to_string()
        }
    };
    Some(match name {
        "date" => Part::Date(fmt("yyyy-MM-dd")),
        "time" => Part::Date(fmt("HH:mm")),
        "datetime" => Part::Date(fmt("yyyy-MM-dd HH:mm")),
        "clipboard" if sep == ":" => Part::ClipHistory(arg.parse().ok().filter(|n| *n > 0)?),
        "clipboard" => Part::Clipboard,
        "uuid" => Part::Uuid,
        "cursor" => Part::Cursor,
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
            Part::Input(Field {
                name: name.to_string(),
                default,
                options,
            })
        }
    })
}

/// Fields the user has to fill in, deduplicated by name (first one wins).
pub fn fields(parts: &[Part]) -> Vec<Field> {
    let mut out: Vec<Field> = Vec::new();
    for part in parts {
        if let Part::Input(field) = part {
            if !out.iter().any(|f| f.name == field.name) {
                out.push(field.clone());
            }
        }
    }
    out
}

/// How many clipboard history entries the template refers to.
pub fn history_depth(parts: &[Part]) -> usize {
    parts
        .iter()
        .filter_map(|p| match p {
            Part::ClipHistory(n) => Some(*n),
            _ => None,
        })
        .max()
        .unwrap_or(0)
}

/// Rendered text and how many characters the caret must move left after
/// pasting to land on `{{cursor}}`.
pub struct Rendered {
    pub text: String,
    pub cursor_back: usize,
}

/// `history` holds clipboard history texts, newest first; entries beyond its
/// end render as empty text.
///
/// With `url_encode`, variable values that land inside a URL (after a
/// `scheme://`) are percent-encoded, so `https://x.com/s?q={{q}}` works with
/// any input. A value that *is* the whole URL (`{{url}}`) stays untouched.
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
    let push_value = |text: &mut String, value: &str| {
        if url_encode && text.contains("://") {
            text.push_str(&percent_encode(value));
        } else {
            text.push_str(value);
        }
    };
    for part in parts {
        match part {
            Part::Text(t) => text.push_str(t),
            Part::Date(f) => push_value(&mut text, &now.format(&to_strftime(f)).to_string()),
            Part::Clipboard => push_value(&mut text, clipboard),
            Part::ClipHistory(n) => {
                push_value(&mut text, history.get(n - 1).map_or("", String::as_str))
            }
            Part::Uuid => text.push_str(&uuid::Uuid::new_v4().to_string()),
            Part::Cursor => {
                if cursor_at.is_none() {
                    cursor_at = Some(text.len());
                }
            }
            Part::Input(field) => {
                let value = values.get(&field.name).unwrap_or(&field.default);
                push_value(&mut text, value);
            }
        }
    }
    let cursor_back = cursor_at
        .map(|at| text[at..].replace("\r\n", "\n").chars().count())
        .unwrap_or(0);
    Rendered { text, cursor_back }
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
        assert_eq!(history_depth(&parts), 9);
        let history = ["C", "B", "A"].map(String::from);
        let r = render(&parts, &HashMap::new(), "now", &history, false);
        assert_eq!(r.text, "A-B-C-|now");
    }

    #[test]
    fn converts_date_patterns() {
        assert_eq!(to_strftime("yyyy-MM-dd HH:mm"), "%Y-%m-%d %H:%M");
        assert_eq!(to_strftime("100%"), "100%%");
    }
}

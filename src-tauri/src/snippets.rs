//! Snippets live as one Markdown file each inside a user-chosen directory,
//! so the directory can be synced with any file sync tool or git:
//!
//! ```markdown
//! ---
//! title: Weekly report
//! tags: [work, report]
//! ---
//! Body with {{variables}}
//! ```

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Snippet {
    /// Path relative to the snippets directory, `/`-separated. Acts as the id.
    pub path: String,
    pub title: String,
    pub tags: Vec<String>,
    pub body: String,
}

pub fn load_all(dir: &Path) -> Vec<Snippet> {
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<Snippet>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            walk(root, &path, out);
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("md")) {
            if let Ok(text) = fs::read_to_string(&path) {
                let rel = relative(root, &path);
                let stem = path.file_stem().unwrap_or_default().to_string_lossy();
                let (title, tags, body) = parse(&text, &stem);
                out.push(Snippet { path: rel, title, tags, body });
            }
        }
    }
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// Splits optional front matter from the body. Only `title` and `tags` are
/// understood; other keys are ignored.
pub fn parse(text: &str, fallback_title: &str) -> (String, Vec<String>, String) {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut title = String::new();
    let mut tags = Vec::new();
    let mut body = text;

    if let Some(after_open) = text.strip_prefix("---\n").or_else(|| text.strip_prefix("---\r\n")) {
        let mut offset = 0;
        for line in after_open.split_inclusive('\n') {
            offset += line.len();
            let trimmed = line.trim();
            if trimmed == "---" {
                body = &after_open[offset..];
                break;
            }
            if let Some((key, value)) = trimmed.split_once(':') {
                let value = value.trim();
                match key.trim() {
                    "title" => title = unquote(value).to_string(),
                    "tags" => {
                        tags = value
                            .trim_start_matches('[')
                            .trim_end_matches(']')
                            .split(',')
                            .map(|t| unquote(t.trim()).to_string())
                            .filter(|t| !t.is_empty())
                            .collect()
                    }
                    _ => {}
                }
            }
        }
        // Without a closing `---` the whole file is treated as body.
        if body == text {
            title.clear();
            tags.clear();
        }
    }

    if title.is_empty() {
        title = fallback_title.to_string();
    }
    (title, tags, body.to_string())
}

fn unquote(s: &str) -> &str {
    s.strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .or_else(|| s.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
        .unwrap_or(s)
}

fn serialize(title: &str, tags: &[String], body: &str) -> String {
    let mut out = String::from("---\n");
    out.push_str(&format!("title: {}\n", title.trim()));
    if !tags.is_empty() {
        out.push_str(&format!("tags: [{}]\n", tags.join(", ")));
    }
    out.push_str("---\n");
    out.push_str(body);
    out
}

/// Writes a snippet. With `existing` the file is overwritten in place;
/// otherwise a new file named after the title is created.
pub fn save(
    dir: &Path,
    existing: Option<&str>,
    title: &str,
    tags: &[String],
    body: &str,
) -> io::Result<String> {
    fs::create_dir_all(dir)?;
    let target = match existing {
        Some(rel) => resolve(dir, rel)?,
        None => unique_path(dir, &file_name_for(title)),
    };
    fs::write(&target, serialize(title, tags, body))?;
    Ok(relative(dir, &target))
}

pub fn delete(dir: &Path, rel: &str) -> io::Result<()> {
    fs::remove_file(resolve(dir, rel)?)
}

/// Resolves a relative snippet path, refusing anything that escapes `dir`.
fn resolve(dir: &Path, rel: &str) -> io::Result<PathBuf> {
    let path = Path::new(rel);
    let safe = path
        .components()
        .all(|c| matches!(c, std::path::Component::Normal(_)));
    if !safe {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid snippet path"));
    }
    Ok(dir.join(path))
}

fn file_name_for(title: &str) -> String {
    let cleaned: String = title
        .trim()
        .chars()
        .map(|c| if r#"<>:"/\|?*"#.contains(c) || c.is_control() { '-' } else { c })
        .take(60)
        .collect();
    let cleaned = cleaned.trim().trim_end_matches('.').to_string();
    if cleaned.is_empty() {
        "snippet".to_string()
    } else {
        cleaned
    }
}

fn unique_path(dir: &Path, stem: &str) -> PathBuf {
    let mut candidate = dir.join(format!("{stem}.md"));
    let mut n = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{stem} {n}.md"));
        n += 1;
    }
    candidate
}

/// Creates the directory with an example snippet the first time.
pub fn ensure_dir(dir: &Path) -> io::Result<()> {
    if dir.exists() {
        return Ok(());
    }
    fs::create_dir_all(dir)?;
    let example = "---\ntitle: 示例：邮件回复\ntags: [example]\n---\n{{name=您好}}，\n\n感谢来信，我会在 {{date:MM月dd日}} 前回复：{{cursor}}\n\n祝好\n";
    fs::write(dir.join("示例.md"), example)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_front_matter() {
        let (title, tags, body) = parse("---\ntitle: Hello\ntags: [a, \"b\"]\n---\nbody\n", "file");
        assert_eq!(title, "Hello");
        assert_eq!(tags, vec!["a", "b"]);
        assert_eq!(body, "body\n");
    }

    #[test]
    fn falls_back_to_file_name() {
        let (title, tags, body) = parse("just text", "file");
        assert_eq!(title, "file");
        assert!(tags.is_empty());
        assert_eq!(body, "just text");
    }

    #[test]
    fn rejects_escaping_paths() {
        assert!(resolve(Path::new("C:/x"), "../evil.md").is_err());
        assert!(resolve(Path::new("C:/x"), "a/b.md").is_ok());
    }
}

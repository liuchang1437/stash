use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_opener::OpenerExt;

use crate::calc;
use crate::config::Config;
use crate::placement::Layout;
use crate::search::{Filter, Hit, Index, ItemRef, TagCount};
use crate::template::{self, Field, Part, Segment};
use crate::{
    hide_launcher, now, platform, register_hotkey, reload_snippets, snippets, watch_snippets, yank,
    AppState, ManageRequest, MANAGE_WINDOW,
};

type CmdResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

fn parse_key(key: &str) -> CmdResult<ItemRef> {
    ItemRef::parse(key).ok_or_else(|| format!("无效的条目：{key}"))
}

#[tauri::command]
pub fn search(state: State<'_, AppState>, query: String, filter: Filter) -> Vec<Hit> {
    let index = state.index.read().unwrap();
    let mut hits = index.query(&query, &filter, now(), 100);
    render_snippets(&index, &mut hits);
    // A scope (clipboard, snippets, a tag) is for browsing; no calculator.
    if !filter.is_all() {
        return hits;
    }
    let clip = |n: usize| index.recent_clips(n).into_iter().nth(n - 1);
    if let Some(calculation) = calc::calculate(&query, clip) {
        hits.insert(0, Hit::calculation(&calculation.expression, calculation.result));
    }
    hits
}

/// Lets the preview show what each snippet with variables would paste,
/// using the defaults. The clipboard is read at most once.
fn render_snippets(index: &Index, hits: &mut [Hit]) {
    let mut clipboard: Option<String> = None;
    for hit in hits.iter_mut().filter(|h| h.kind == "snippet") {
        let Some(entry) = ItemRef::parse(&hit.key).and_then(|item| index.get(&item)) else {
            continue;
        };
        let parts = template::parse(&entry.body);
        if parts.iter().all(|p| matches!(p, Part::Text(_))) {
            continue;
        }
        let current = if parts.contains(&Part::Clipboard) {
            clipboard.get_or_insert_with(|| platform::read_clipboard_text().unwrap_or_default())
        } else {
            ""
        };
        let history = index.recent_clips(template::history_depth(&parts));
        hit.rendered = Some(template::render_segments(&parts, &HashMap::new(), current, &history));
    }
}

/// Snippet tags for `#` completion in the popover.
#[tauri::command]
pub fn snippet_tags(state: State<'_, AppState>) -> Vec<TagCount> {
    state.index.read().unwrap().tags()
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum Activation {
    Done,
    NeedsInput { title: String, fields: Vec<Field> },
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Put on the clipboard and paste into the previous window.
    Paste,
    /// Only put on the clipboard.
    Copy,
    /// Open as a URL in the default browser.
    Open,
}

/// Pastes, copies or opens a clip, calculator result or rendered snippet.
/// Snippets with input variables first return `NeedsInput`; the UI then
/// calls again with `values`.
#[tauri::command]
pub fn activate(
    app: AppHandle,
    state: State<'_, AppState>,
    key: String,
    mode: Mode,
    values: Option<HashMap<String, String>>,
) -> CmdResult<Activation> {
    let item = parse_key(&key)?;
    let (text, cursor_back) = match &item {
        ItemRef::Calc(value) => (value.clone(), 0),
        ItemRef::Clip(_) | ItemRef::Snippet(_) => {
            let (title, body) = {
                let index = state.index.read().unwrap();
                let entry = index.get(&item).ok_or("条目不存在")?;
                (entry.title.clone(), entry.body.clone())
            };
            if matches!(item, ItemRef::Clip(_)) {
                (body, 0)
            } else {
                let parts = template::parse(&body);
                let fields = template::fields(&parts);
                if !fields.is_empty() && values.is_none() {
                    return Ok(Activation::NeedsInput { title, fields });
                }
                let (clipboard, history) = template_inputs(&state, &parts);
                let rendered = template::render(
                    &parts,
                    &values.unwrap_or_default(),
                    &clipboard,
                    &history,
                    mode == Mode::Open,
                );
                (rendered.text, rendered.cursor_back)
            }
        }
    };

    if mode == Mode::Open {
        let url = as_web_url(&text).ok_or("内容不是网址，无法用浏览器打开")?;
        app.opener()
            .open_url(url, None::<&str>)
            .map_err(|e| format!("打开浏览器失败：{e}"))?;
        hide_launcher(&app);
    } else {
        deliver(&app, text, cursor_back, mode == Mode::Paste);
    }
    record_use(&state, &item)?;
    Ok(Activation::Done)
}

/// Clipboard text and history entries a template refers to.
fn template_inputs(state: &AppState, parts: &[Part]) -> (String, Vec<String>) {
    let clipboard = if parts.contains(&Part::Clipboard) {
        platform::read_clipboard_text().unwrap_or_default()
    } else {
        String::new()
    };
    let history = state
        .index
        .read()
        .unwrap()
        .recent_clips(template::history_depth(parts));
    (clipboard, history)
}

fn record_use(state: &AppState, item: &ItemRef) -> CmdResult<()> {
    let now = now();
    match item {
        ItemRef::Clip(id) => state.db.lock().unwrap().touch_clip(*id, now).map_err(err)?,
        ItemRef::Snippet(path) => state.db.lock().unwrap().touch_snippet(path, now).map_err(err)?,
        ItemRef::Calc(_) => return Ok(()),
    }
    if let Some(entry) = state.index.write().unwrap().get_mut(item) {
        entry.use_count += 1;
        entry.last_used_at = now;
    }
    Ok(())
}

/// Returns an http(s) URL if `text` is a single URL (optionally surrounded by
/// whitespace). `www.` addresses get `https://`. Other schemes are refused so
/// that copied text can never launch arbitrary protocol handlers.
fn as_web_url(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() || text.chars().any(char::is_whitespace) {
        return None;
    }
    let lower = text.to_ascii_lowercase();
    if lower.starts_with("https://") || lower.starts_with("http://") {
        Some(text.to_string())
    } else if lower.starts_with("www.") {
        Some(format!("https://{text}"))
    } else {
        None
    }
}

/// Puts `text` on the clipboard and, when pasting, switches back to the
/// previous window, sends Ctrl+V and optionally restores the old clipboard.
fn deliver(app: &AppHandle, text: String, cursor_back: usize, paste: bool) {
    let state = app.state::<AppState>();
    let target = state.prev_window.load(Ordering::SeqCst);
    let restore = paste && state.config.read().unwrap().restore_clipboard;
    let app = app.clone();
    thread::spawn(move || {
        let original = if restore { platform::read_clipboard_text() } else { None };
        platform::write_clipboard_text(&text);
        if !paste {
            hide_launcher(&app);
            return;
        }
        // Activate the target while we still own the foreground, then hide.
        platform::activate_window(target);
        hide_launcher(&app);
        thread::sleep(Duration::from_millis(120));
        platform::send_paste();
        if cursor_back > 0 {
            thread::sleep(Duration::from_millis(30));
            platform::send_left(cursor_back);
        }
        yank::after_paste(&app, target, text, cursor_back, original.clone());
        if let Some(original) = original {
            // Give the target app time to read the clipboard first.
            thread::sleep(Duration::from_millis(500));
            platform::write_clipboard_text(&original);
        }
    });
}

#[tauri::command]
pub fn toggle_pin(state: State<'_, AppState>, key: String) -> CmdResult<bool> {
    let ItemRef::Clip(id) = parse_key(&key)? else {
        return Err("只有剪贴板记录可以置顶".into());
    };
    let mut index = state.index.write().unwrap();
    let entry = index.get_mut(&ItemRef::Clip(id)).ok_or("条目不存在")?;
    let pinned = !entry.pinned;
    state.db.lock().unwrap().set_pinned(id, pinned).map_err(err)?;
    entry.pinned = pinned;
    Ok(pinned)
}

#[tauri::command]
pub fn delete_item(state: State<'_, AppState>, key: String) -> CmdResult<()> {
    let item = parse_key(&key)?;
    match &item {
        ItemRef::Clip(id) => state.db.lock().unwrap().delete_clip(*id).map_err(err)?,
        ItemRef::Snippet(path) => snippets::delete(&state.snippets_dir(), path).map_err(err)?,
        ItemRef::Calc(_) => return Err("计算结果不能删除".into()),
    }
    state.index.write().unwrap().remove(&item);
    Ok(())
}

#[tauri::command]
pub fn get_snippet(state: State<'_, AppState>, key: String) -> CmdResult<snippets::Snippet> {
    let item = parse_key(&key)?;
    let ItemRef::Snippet(path) = &item else {
        return Err("不是 snippet".into());
    };
    let index = state.index.read().unwrap();
    let entry = index.get(&item).ok_or("条目不存在")?;
    Ok(snippets::Snippet {
        path: path.clone(),
        title: entry.title.clone(),
        tags: entry.tags.clone(),
        body: entry.body.clone(),
    })
}

/// Creates (`path == None`) or overwrites a snippet; returns its item key.
#[tauri::command]
pub fn save_snippet(
    app: AppHandle,
    state: State<'_, AppState>,
    path: Option<String>,
    title: String,
    tags: Vec<String>,
    body: String,
) -> CmdResult<String> {
    if title.trim().is_empty() {
        return Err("标题不能为空".into());
    }
    let saved = snippets::save(&state.snippets_dir(), path.as_deref(), &title, &tags, &body)
        .map_err(err)?;
    reload_snippets(&app);
    Ok(ItemRef::Snippet(saved).key())
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    config: Config,
    autostart: bool,
    #[serde(default)]
    default_snippets_dir: String,
    #[serde(default)]
    hotkey_error: Option<String>,
}

#[tauri::command]
pub fn get_settings(app: AppHandle, state: State<'_, AppState>) -> Settings {
    Settings {
        config: state.config.read().unwrap().clone(),
        autostart: app.autolaunch().is_enabled().unwrap_or(false),
        default_snippets_dir: state.default_snippets_dir.to_string_lossy().into_owned(),
        hotkey_error: state.hotkey_error.lock().unwrap().clone(),
    }
}

#[tauri::command]
pub fn save_settings(app: AppHandle, state: State<'_, AppState>, settings: Settings) -> CmdResult<()> {
    let mut new = settings.config;
    new.hotkey = new.hotkey.trim().to_string();
    new.swap_hotkey = new.swap_hotkey.trim().to_string();
    if !new.swap_hotkey.is_empty() {
        if new.swap_hotkey.eq_ignore_ascii_case(&new.hotkey) {
            return Err("「换一条」快捷键不能和唤起快捷键相同".into());
        }
        new.swap_hotkey
            .parse::<tauri_plugin_global_shortcut::Shortcut>()
            .map_err(|e| format!("无法识别快捷键 “{}”：{e}", new.swap_hotkey))?;
    }
    new.history_limit = new.history_limit.max(10);
    let old = state.config.read().unwrap().clone();

    if new.hotkey != old.hotkey || state.hotkey_error.lock().unwrap().is_some() {
        if let Err(e) = register_hotkey(&app, &new.hotkey) {
            let _ = register_hotkey(&app, &old.hotkey);
            return Err(e);
        }
        *state.hotkey_error.lock().unwrap() = None;
    }

    new.save(&state.config_path).map_err(err)?;
    let dir_changed = new.snippets_dir.trim() != old.snippets_dir.trim();
    let limit = new.history_limit;
    *state.config.write().unwrap() = new;

    if dir_changed {
        watch_snippets(&app);
    }
    if limit < old.history_limit {
        let removed = state.db.lock().unwrap().prune(limit).map_err(err)?;
        let mut index = state.index.write().unwrap();
        for id in removed {
            index.remove(&ItemRef::Clip(id));
        }
    }

    let autolaunch = app.autolaunch();
    if settings.autostart != autolaunch.is_enabled().unwrap_or(false) {
        let result = if settings.autostart { autolaunch.enable() } else { autolaunch.disable() };
        result.map_err(|e| format!("设置开机启动失败：{e}"))?;
    }
    Ok(())
}

#[tauri::command]
pub fn hide_window(app: AppHandle) {
    hide_launcher(&app);
}

#[tauri::command]
pub fn open_snippets_dir(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let dir = state.snippets_dir();
    snippets::ensure_dir(&dir).map_err(err)?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(err)
}

#[tauri::command]
pub fn place_popover(app: AppHandle, layout: Layout) {
    crate::place_popover(&app, &layout);
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalcRef {
    n: usize,
    value: String,
    source: Option<String>,
    last_used_at: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalcDetail {
    result: String,
    expression: String,
    refs: Vec<CalcRef>,
}

/// `$N` numbers mentioned in a query, in order, without duplicates.
fn clip_refs(query: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut chars = query.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '$' && c != '＄' {
            continue;
        }
        let mut digits = String::new();
        while let Some(d) = chars.peek().filter(|d| d.is_ascii_digit()) {
            digits.push(*d);
            chars.next();
        }
        if let Ok(n) = digits.parse::<usize>() {
            if n > 0 && !out.contains(&n) {
                out.push(n);
            }
        }
    }
    out
}

/// The calculation for a query plus where each `$N` came from.
#[tauri::command]
pub fn calc_detail(state: State<'_, AppState>, query: String) -> Option<CalcDetail> {
    let index = state.index.read().unwrap();
    let refs = clip_refs(&query);
    let depth = refs.iter().copied().max().unwrap_or(0);
    let recent = index.recent_entries(depth);
    let clip = |n: usize| recent.get(n - 1).map(|e| e.body.clone());
    let calculation = calc::calculate(&query, clip)?;
    let refs = refs
        .into_iter()
        .filter_map(|n| {
            let e = recent.get(n - 1)?;
            Some(CalcRef {
                n,
                value: e.body.trim().chars().take(80).collect(),
                source: e.source.clone(),
                last_used_at: e.last_used_at,
            })
        })
        .collect();
    Some(CalcDetail {
        result: calculation.result,
        expression: calculation.expression,
        refs,
    })
}

/// Rendered snippet split into pieces, so the UI can mark which part comes
/// from which variable while the user fills them in.
#[tauri::command]
pub fn preview_snippet(
    state: State<'_, AppState>,
    key: String,
    values: HashMap<String, String>,
) -> CmdResult<Vec<Segment>> {
    let item = parse_key(&key)?;
    let body = state
        .index
        .read()
        .unwrap()
        .get(&item)
        .map(|e| e.body.clone())
        .ok_or("条目不存在")?;
    let parts = template::parse(&body);
    let (clipboard, history) = template_inputs(&state, &parts);
    Ok(template::render_segments(
        &parts, &values, &clipboard, &history,
    ))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditableText {
    text: String,
    /// Characters after `{{cursor}}`, as in `template::Rendered`.
    cursor_back: usize,
}

/// The whole text an item pastes, for editing it in the preview before
/// pasting: a clip's full body (the preview is truncated), or a snippet
/// rendered with `values` (defaults for the rest), UUIDs included.
#[tauri::command]
pub fn editable_text(
    state: State<'_, AppState>,
    key: String,
    values: HashMap<String, String>,
) -> CmdResult<EditableText> {
    let item = parse_key(&key)?;
    let body = state
        .index
        .read()
        .unwrap()
        .get(&item)
        .map(|e| e.body.clone())
        .ok_or("条目不存在")?;
    if !matches!(item, ItemRef::Snippet(_)) {
        return Ok(EditableText { text: body, cursor_back: 0 });
    }
    let parts = template::parse(&body);
    let (clipboard, history) = template_inputs(&state, &parts);
    let rendered = template::render(&parts, &values, &clipboard, &history, false);
    Ok(EditableText {
        text: rendered.text,
        cursor_back: rendered.cursor_back,
    })
}

#[derive(Serialize)]
pub struct TemplatePreview {
    fields: Vec<Field>,
    segments: Vec<Segment>,
}

/// A template in the snippet editor, before it is saved: its input fields
/// and what it renders to with `values` (defaults for the rest).
#[tauri::command]
pub fn preview_template(
    state: State<'_, AppState>,
    body: String,
    values: HashMap<String, String>,
) -> TemplatePreview {
    let parts = template::parse(&body);
    let (clipboard, history) = template_inputs(&state, &parts);
    TemplatePreview {
        fields: template::fields(&parts),
        segments: template::render_segments(&parts, &values, &clipboard, &history),
    }
}

/// Pastes, copies or opens text the UI derived from an item (a reformatted
/// calculation, a quoted address, a table as Markdown…). `key`, when given,
/// is the item it came from and gets its use recorded.
#[tauri::command]
pub fn activate_text(
    app: AppHandle,
    state: State<'_, AppState>,
    key: Option<String>,
    text: String,
    mode: Mode,
) -> CmdResult<()> {
    if mode == Mode::Open {
        let url = as_web_url(&text).ok_or("内容不是网址，无法用浏览器打开")?;
        app.opener()
            .open_url(url, None::<&str>)
            .map_err(|e| format!("打开浏览器失败：{e}"))?;
        hide_launcher(&app);
    } else {
        deliver(&app, text, 0, mode == Mode::Paste);
    }
    if let Some(key) = key {
        record_use(&state, &parse_key(&key)?)?;
    }
    Ok(())
}

/// Solscan page for a Solana address (32–44 base58 characters) or
/// transaction signature (64–88).
fn solana_explorer_url(text: &str) -> Option<String> {
    const BASE58: &str = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    let text = text.trim();
    if !text.chars().all(|c| BASE58.contains(c)) {
        return None;
    }
    match text.len() {
        32..=44 => Some(format!("https://solscan.io/account/{text}")),
        64..=88 => Some(format!("https://solscan.io/tx/{text}")),
        _ => None,
    }
}

#[tauri::command]
pub fn open_explorer(app: AppHandle, state: State<'_, AppState>, key: String) -> CmdResult<()> {
    let item = parse_key(&key)?;
    let body = state
        .index
        .read()
        .unwrap()
        .get(&item)
        .map(|e| e.body.clone())
        .ok_or("条目不存在")?;
    let url = solana_explorer_url(&body).ok_or("不是 Solana 地址或交易签名")?;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| format!("打开浏览器失败：{e}"))?;
    hide_launcher(&app);
    record_use(&state, &item)
}

#[tauri::command]
pub fn open_manage(app: AppHandle, view: String, key: Option<String>, body: Option<String>) {
    crate::open_manage(&app, ManageRequest { view, key, body });
}

#[tauri::command]
pub fn manage_request(state: State<'_, AppState>) -> Option<ManageRequest> {
    state.manage_request.lock().unwrap().clone()
}

#[tauri::command]
pub fn close_manage(app: AppHandle) {
    if let Some(window) = app.get_webview_window(MANAGE_WINDOW) {
        let _ = window.hide();
    }
}

#[tauri::command]
pub fn chip_swap(app: AppHandle) {
    // Off the IPC thread: swapping sleeps between key strokes.
    thread::spawn(move || yank::swap_now(&app));
}

#[tauri::command]
pub fn chip_undo(app: AppHandle) {
    thread::spawn(move || yank::undo(&app));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_clip_refs() {
        assert_eq!(clip_refs("$1 + $12 * $1 - ＄3"), [1, 12, 3]);
        assert!(clip_refs("$ 1 + $0").is_empty());
    }

    #[test]
    fn builds_solscan_urls() {
        let address = "3wCvYmrYvcLDhJF4B4EZkCfYXXx9us8XabcdefghijkL";
        assert_eq!(
            solana_explorer_url(address).as_deref(),
            Some("https://solscan.io/account/3wCvYmrYvcLDhJF4B4EZkCfYXXx9us8XabcdefghijkL")
        );
        let signature = "5".repeat(87);
        assert!(solana_explorer_url(&signature).unwrap().contains("/tx/"));
        assert_eq!(
            solana_explorer_url("0OIl0OIl0OIl0OIl0OIl0OIl0OIl0OIl"),
            None
        );
        assert_eq!(solana_explorer_url("short"), None);
    }

    #[test]
    fn recognizes_web_urls() {
        assert_eq!(as_web_url(" https://a.com/x?q=1\n").as_deref(), Some("https://a.com/x?q=1"));
        assert_eq!(as_web_url("HTTP://A.COM").as_deref(), Some("HTTP://A.COM"));
        assert_eq!(as_web_url("www.baidu.com").as_deref(), Some("https://www.baidu.com"));
        for text in ["", "hello", "https://a.com and more", "file:///C:/x", "javascript:alert(1)", "ms-settings:"] {
            assert_eq!(as_web_url(text), None, "{text}");
        }
    }
}

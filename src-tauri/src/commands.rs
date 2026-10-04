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
use crate::search::{Hit, ItemRef};
use crate::template::{self, Field, Part};
use crate::{
    hide_launcher, now, platform, register_hotkey, reload_snippets, snippets, watch_snippets,
    AppState,
};

type CmdResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

fn parse_key(key: &str) -> CmdResult<ItemRef> {
    ItemRef::parse(key).ok_or_else(|| format!("无效的条目：{key}"))
}

#[tauri::command]
pub fn search(state: State<'_, AppState>, query: String) -> Vec<Hit> {
    let index = state.index.read().unwrap();
    let mut hits = index.query(&query, now(), 100);
    let clip = |n: usize| index.recent_clips(n).into_iter().nth(n - 1);
    if let Some(calculation) = calc::calculate(&query, clip) {
        hits.insert(0, Hit::calculation(&calculation.expression, calculation.result));
    }
    hits
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
                let clipboard = if parts.contains(&Part::Clipboard) {
                    platform::read_clipboard_text().unwrap_or_default()
                } else {
                    String::new()
                };
                let history = state
                    .index
                    .read()
                    .unwrap()
                    .recent_clips(template::history_depth(&parts));
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

#[cfg(test)]
mod tests {
    use super::*;

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

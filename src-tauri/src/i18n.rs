//! UI language for the few texts Rust shows itself: the tray menu, error
//! messages returned to the UI and the first-run example snippet. The
//! frontend has its own dictionaries (`src/lib/locales/`) and asks for the
//! language with `commands::language`.

use std::fmt::Display;
use std::sync::atomic::{AtomicU8, Ordering};

use serde::Serialize;

use crate::platform;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    Zh,
    En,
}

static CURRENT: AtomicU8 = AtomicU8::new(Lang::Zh as u8);

/// The language for `config.language`: `zh`, `en`, or anything else (empty)
/// to follow the system, which means Chinese for a Chinese Windows display
/// language and English otherwise.
pub fn resolve(setting: &str) -> Lang {
    match setting {
        "zh" => Lang::Zh,
        "en" => Lang::En,
        _ if platform::prefers_chinese() => Lang::Zh,
        _ => Lang::En,
    }
}

pub fn set(lang: Lang) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
}

pub fn current() -> Lang {
    if CURRENT.load(Ordering::Relaxed) == Lang::En as u8 {
        Lang::En
    } else {
        Lang::Zh
    }
}

/// Picks the text for the current language.
pub fn tr(zh: &'static str, en: &'static str) -> &'static str {
    match current() {
        Lang::Zh => zh,
        Lang::En => en,
    }
}

// Messages with arguments.

pub fn invalid_hotkey(hotkey: &str, e: impl Display) -> String {
    match current() {
        Lang::Zh => format!("无法识别快捷键 “{hotkey}”：{e}"),
        Lang::En => format!("Unrecognized hotkey \"{hotkey}\": {e}"),
    }
}

pub fn hotkey_taken(hotkey: &str, e: impl Display) -> String {
    match current() {
        Lang::Zh => format!("注册快捷键 “{hotkey}” 失败，可能已被其他程序占用：{e}"),
        Lang::En => format!("Could not register the hotkey \"{hotkey}\"; another program may be using it: {e}"),
    }
}

pub fn invalid_item(key: &str) -> String {
    match current() {
        Lang::Zh => format!("无效的条目：{key}"),
        Lang::En => format!("Invalid item: {key}"),
    }
}

pub fn browser_failed(e: impl Display) -> String {
    match current() {
        Lang::Zh => format!("打开浏览器失败：{e}"),
        Lang::En => format!("Could not open the browser: {e}"),
    }
}

pub fn autostart_failed(e: impl Display) -> String {
    match current() {
        Lang::Zh => format!("设置开机启动失败：{e}"),
        Lang::En => format!("Could not change start at login: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_setting_wins() {
        assert_eq!(resolve("zh"), Lang::Zh);
        assert_eq!(resolve("en"), Lang::En);
    }
}

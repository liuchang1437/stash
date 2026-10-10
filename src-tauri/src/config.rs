use std::fs;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub hotkey: String,
    /// Empty means the default directory under Documents.
    pub snippets_dir: String,
    /// Unpinned clips kept in history.
    pub history_limit: usize,
    /// Copies longer than this many characters are not recorded.
    pub max_clip_chars: usize,
    /// Executable names whose copies are never recorded (case-insensitive).
    pub ignored_apps: Vec<String>,
    /// Put the previous clipboard text back after pasting.
    pub restore_clipboard: bool,
    /// Reopening within this many seconds keeps the previous query;
    /// 0 always starts empty.
    pub keep_query_seconds: u64,
    /// Open the popover at the text caret of the focused app (or the mouse
    /// when the caret is unknown) instead of the upper middle of the screen.
    pub follow_caret: bool,
    /// Show the preview panel next to the list as soon as the popover opens,
    /// instead of only after → is pressed.
    pub auto_peek: bool,
    /// Switch the popover's input method to English when it opens; search
    /// matches pinyin anyway, and Shift switches back.
    pub english_input: bool,
    /// Hotkey that swaps the text just pasted for an earlier clip while the
    /// post-paste chip is visible. Empty disables swapping.
    pub swap_hotkey: String,
    /// UI language: `zh`, `en`, or empty to follow the Windows display
    /// language (`i18n::resolve`).
    pub language: String,
    /// Width of the popover's list card, logical px (`CARD_WIDTHS`).
    pub card_width: u32,
    /// Width of the preview beside it, logical px (`PEEK_WIDTHS`).
    pub peek_width: u32,
}

/// Allowed list widths: narrower and the status bar no longer fits. The
/// settings inputs in SettingsView.svelte use the same bounds.
const CARD_WIDTHS: RangeInclusive<u32> = 400..=800;
/// Allowed preview widths.
const PEEK_WIDTHS: RangeInclusive<u32> = 320..=1000;

impl Default for Config {
    fn default() -> Self {
        Config {
            hotkey: "Alt+Space".into(),
            snippets_dir: String::new(),
            history_limit: 5000,
            max_clip_chars: 100_000,
            ignored_apps: ["1Password.exe", "KeePass.exe", "KeePassXC.exe", "Bitwarden.exe"]
                .map(String::from)
                .to_vec(),
            restore_clipboard: true,
            keep_query_seconds: 60,
            follow_caret: false,
            auto_peek: true,
            english_input: true,
            swap_hotkey: "Alt+V".into(),
            language: String::new(),
            card_width: 440,
            peek_width: 480,
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Self {
        let mut config: Config = fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        config.clamp_sizes();
        config
    }

    /// Keeps the popover widths in their allowed ranges, whatever the file says.
    pub fn clamp_sizes(&mut self) {
        self.card_width = self.card_width.clamp(*CARD_WIDTHS.start(), *CARD_WIDTHS.end());
        self.peek_width = self.peek_width.clamp(*PEEK_WIDTHS.start(), *PEEK_WIDTHS.end());
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, serde_json::to_string_pretty(self)?)
    }

    pub fn snippets_path(&self, default: &Path) -> PathBuf {
        if self.snippets_dir.trim().is_empty() {
            default.to_path_buf()
        } else {
            PathBuf::from(self.snippets_dir.trim())
        }
    }

    pub fn is_ignored(&self, app: Option<&str>) -> bool {
        app.is_some_and(|app| self.ignored_apps.iter().any(|i| i.eq_ignore_ascii_case(app)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popover_widths_are_clamped() {
        let mut config: Config = serde_json::from_str(r#"{"cardWidth": 100, "peekWidth": 5000}"#).unwrap();
        config.clamp_sizes();
        assert_eq!((config.card_width, config.peek_width), (400, 1000));
        // Missing fields keep the defaults.
        let config: Config = serde_json::from_str("{}").unwrap();
        assert_eq!((config.card_width, config.peek_width), (440, 480));
    }
}

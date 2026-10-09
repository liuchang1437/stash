//! After a paste, a small non-activating "chip" window under the caret lets
//! the user swap what was just pasted for an earlier clip, or take it back.
//!
//! Swapping works like Alt+Tab: hold Alt and tap V to step through earlier
//! clips (the chip shows the candidate), release Alt to replace the pasted
//! text in place. The text is removed by selecting it with Shift+← (the
//! paste then overwrites the selection) or, in terminals, with Backspace.
//! The Alt+V hotkey is only registered while the chip is visible.

use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

use crate::placement::Layout;
use crate::{apply_bounds, native_handle, platform, AppState, CHIP_WINDOW};

/// Longest text we are willing to erase key by key.
const MAX_ERASE_CHARS: usize = 2000;
const CHOICES: usize = 12;
const CHIP_WIDTH: f64 = 560.0;
const CHIP_HEIGHT: f64 = 52.0;
const CHIP_MARGIN: f64 = 8.0;

/// Executables whose windows are terminals: text is removed with Backspace
/// because Shift+← does not select there.
const TERMINALS: &[&str] = &[
    "windowsterminal.exe",
    "openconsole.exe",
    "conhost.exe",
    "cmd.exe",
    "powershell.exe",
    "pwsh.exe",
    "wezterm-gui.exe",
    "alacritty.exe",
    "mintty.exe",
    "tabby.exe",
    "windterm.exe",
    "mobaxterm.exe",
    "putty.exe",
    "kitty.exe",
    "xshell.exe",
    "termius.exe",
];

pub fn is_terminal(process: Option<&str>) -> bool {
    process.is_some_and(|p| TERMINALS.iter().any(|t| t.eq_ignore_ascii_case(p)))
}

pub struct LastPaste {
    target: platform::WindowHandle,
    /// What can be in the target; `choices[0]` is the text pasted first.
    choices: Vec<String>,
    /// Index of the text currently in the target.
    current: usize,
    /// Index picked while Alt is held; applied on release.
    pending: usize,
    /// Caret distance from the end of the pasted text (`{{cursor}}`).
    cursor_back: usize,
    terminal: bool,
    /// Clipboard text to put back after each swap.
    restore: Option<String>,
    /// A thread is waiting for Alt to be released.
    waiting: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChipState {
    /// `pasted` | `choosing` | `replaced`
    phase: &'static str,
    index: usize,
    total: usize,
    preview: String,
    can_swap: bool,
    hotkey: String,
}

/// Number of caret steps the text occupies (CRLF is one step).
fn caret_len(text: &str) -> usize {
    text.replace("\r\n", "\n").chars().count()
}

/// Which earlier clips may replace `pasted`.
fn swap_choices(pasted: &str, recent: Vec<String>, terminal: bool) -> Vec<String> {
    let mut choices = vec![pasted.to_string()];
    for text in recent {
        let fits = !text.trim().is_empty()
            && caret_len(&text) <= MAX_ERASE_CHARS
            && !(terminal && text.contains('\n'));
        if fits && !choices.contains(&text) {
            choices.push(text);
        }
        if choices.len() > CHOICES {
            break;
        }
    }
    choices
}

fn can_erase(text: &str, terminal: bool) -> bool {
    caret_len(text) <= MAX_ERASE_CHARS && !(terminal && text.contains('\n'))
}

fn preview(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let short: String = line.chars().take(60).collect();
    if short.chars().count() < line.chars().count() || text.trim().lines().count() > 1 {
        format!("{short}…")
    } else {
        short
    }
}

/// Called right after a successful paste.
pub fn after_paste(
    app: &AppHandle,
    target: platform::WindowHandle,
    text: String,
    cursor_back: usize,
    restore: Option<String>,
) {
    let state = app.state::<AppState>();
    // Looked up now, in the pasted-into window, so the chip sits by the text.
    *state.chip_anchor.lock().unwrap() = crate::caret_anchor(app, target);
    let terminal = is_terminal(platform::process_name(target).as_deref());
    let recent = state.index.read().unwrap().recent_clips(CHOICES * 2);
    let choices = if can_erase(&text, terminal) {
        swap_choices(&text, recent, terminal)
    } else {
        vec![text.clone()]
    };
    let can_swap = choices.len() > 1;
    *state.last_paste.lock().unwrap() = Some(LastPaste {
        target,
        choices,
        current: 0,
        pending: 0,
        cursor_back,
        terminal,
        restore,
        waiting: false,
    });
    if can_swap {
        register_swap_hotkey(app);
    }
    let total = state
        .last_paste
        .lock()
        .unwrap()
        .as_ref()
        .map_or(1, |p| p.choices.len());
    show_chip(app, chip_state(app, "pasted", 0, total, &text, can_swap));
    schedule_hide(app, Duration::from_millis(3500));
}

fn chip_state(
    app: &AppHandle,
    phase: &'static str,
    index: usize,
    total: usize,
    text: &str,
    can_swap: bool,
) -> ChipState {
    ChipState {
        phase,
        index,
        total,
        preview: preview(text),
        can_swap,
        hotkey: app
            .state::<AppState>()
            .config
            .read()
            .unwrap()
            .swap_hotkey
            .clone(),
    }
}

fn show_chip(app: &AppHandle, chip: ChipState) {
    let Some(window) = app.get_webview_window(CHIP_WINDOW) else {
        return;
    };
    let state = app.state::<AppState>();
    if let Some(anchor) = *state.chip_anchor.lock().unwrap() {
        let flip = anchor.space().below < CHIP_HEIGHT + 8.0;
        let layout = Layout {
            width: CHIP_WIDTH,
            height: CHIP_HEIGHT,
            card_x: CHIP_MARGIN,
            margin: CHIP_MARGIN,
            flip,
        };
        apply_bounds(&window, anchor.place(&layout));
    }
    let _ = app.emit_to(CHIP_WINDOW, "chip", chip);
    let handle = native_handle(&window);
    if handle != 0 {
        platform::show_without_focus(handle);
    } else {
        let _ = window.show();
    }
}

pub fn hide_chip(app: &AppHandle) {
    let state = app.state::<AppState>();
    state.chip_seq.fetch_add(1, Ordering::SeqCst);
    *state.last_paste.lock().unwrap() = None;
    unregister_swap_hotkey(app);
    if let Some(window) = app.get_webview_window(CHIP_WINDOW) {
        // Mirror `show_chip`: shown natively, so hidden natively too.
        let handle = native_handle(&window);
        if handle != 0 {
            platform::hide_window(handle);
        } else {
            let _ = window.hide();
        }
    }
}

/// Hides the chip after `delay` unless something touched it meanwhile.
fn schedule_hide(app: &AppHandle, delay: Duration) {
    let seq = app
        .state::<AppState>()
        .chip_seq
        .fetch_add(1, Ordering::SeqCst)
        + 1;
    let app = app.clone();
    thread::spawn(move || {
        thread::sleep(delay);
        let state = app.state::<AppState>();
        let busy = state
            .last_paste
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|p| p.waiting);
        if state.chip_seq.load(Ordering::SeqCst) == seq && !busy {
            hide_chip(&app);
        }
    });
}

fn register_swap_hotkey(app: &AppHandle) {
    let state = app.state::<AppState>();
    let text = state.config.read().unwrap().swap_hotkey.trim().to_string();
    if text.is_empty() {
        return;
    }
    let Ok(shortcut) = text.parse::<Shortcut>() else {
        return;
    };
    let mut slot = state.swap_shortcut.lock().unwrap();
    if slot.is_some() {
        return;
    }
    if app.global_shortcut().register(shortcut).is_ok() {
        *slot = Some(shortcut);
    }
}

fn unregister_swap_hotkey(app: &AppHandle) {
    let state = app.state::<AppState>();
    let taken = state.swap_shortcut.lock().unwrap().take();
    if let Some(shortcut) = taken {
        let _ = app.global_shortcut().unregister(shortcut);
    }
}

pub fn is_swap_hotkey(app: &AppHandle, shortcut: &Shortcut) -> bool {
    app.state::<AppState>()
        .swap_shortcut
        .lock()
        .unwrap()
        .is_some_and(|s| s.id() == shortcut.id())
}

/// Alt+V pressed: step to the next candidate; the swap happens once Alt is
/// released.
pub fn on_swap_hotkey(app: &AppHandle) {
    platform::mask_menu_key();
    let state = app.state::<AppState>();
    let (chip, wait) = {
        let mut guard = state.last_paste.lock().unwrap();
        let Some(paste) = guard.as_mut() else {
            return;
        };
        if paste.choices.len() < 2 {
            return;
        }
        paste.pending = next_index(paste.pending, paste.current, paste.choices.len());
        let chip = chip_state(
            app,
            "choosing",
            paste.pending,
            paste.choices.len(),
            &paste.choices[paste.pending],
            true,
        );
        let wait = !paste.waiting;
        paste.waiting = true;
        (chip, wait)
    };
    show_chip(app, chip);
    state.chip_seq.fetch_add(1, Ordering::SeqCst);
    if wait {
        let app = app.clone();
        thread::spawn(move || {
            let start = Instant::now();
            while platform::alt_down() && start.elapsed() < Duration::from_secs(10) {
                thread::sleep(Duration::from_millis(15));
            }
            apply_pending(&app);
        });
    }
}

/// Chip button: swap to the next candidate right away.
pub fn swap_now(app: &AppHandle) {
    {
        let state = app.state::<AppState>();
        let mut guard = state.last_paste.lock().unwrap();
        let Some(paste) = guard.as_mut() else {
            return;
        };
        if paste.choices.len() < 2 || paste.waiting {
            return;
        }
        paste.pending = next_index(paste.current, paste.current, paste.choices.len());
        paste.waiting = true;
    }
    apply_pending(app);
}

/// Next index after `from`, skipping `current`, wrapping around.
fn next_index(from: usize, current: usize, len: usize) -> usize {
    let mut i = (from + 1) % len;
    if i == current {
        i = (i + 1) % len;
    }
    i
}

/// Removes the text that is in the target right now. With `select`, the
/// text is only selected (outside terminals) so that a paste replaces it.
fn erase(paste: &LastPaste, select: bool) {
    let text = &paste.choices[paste.current];
    if paste.cursor_back > 0 {
        platform::send_right(paste.cursor_back);
    }
    let n = caret_len(text);
    if paste.terminal {
        platform::send_backspace(n);
    } else {
        platform::send_shift_left(n);
        if !select {
            platform::send_backspace(1);
        }
    }
}

fn apply_pending(app: &AppHandle) {
    let state = app.state::<AppState>();
    let (text, restore, chip) = {
        let mut guard = state.last_paste.lock().unwrap();
        let Some(paste) = guard.as_mut() else {
            return;
        };
        paste.waiting = false;
        if paste.pending == paste.current {
            return;
        }
        if platform::foreground_window() != paste.target {
            drop(guard);
            hide_chip(app);
            return;
        }
        erase(paste, true);
        paste.current = paste.pending;
        paste.cursor_back = 0;
        let text = paste.choices[paste.current].clone();
        let chip = chip_state(
            app,
            "replaced",
            paste.current,
            paste.choices.len(),
            &text,
            true,
        );
        (text, paste.restore.clone(), chip)
    };
    thread::sleep(Duration::from_millis(30));
    platform::write_clipboard_text(&text);
    platform::send_paste();
    show_chip(app, chip);
    schedule_hide(app, Duration::from_millis(3000));
    if let Some(original) = restore {
        thread::sleep(Duration::from_millis(500));
        platform::write_clipboard_text(&original);
    }
}

/// Chip button: remove what was pasted. The chip closes either way; the
/// text is only erased while the pasted-into window still has focus, since
/// keystrokes would otherwise land somewhere else.
pub fn undo(app: &AppHandle) {
    let state = app.state::<AppState>();
    {
        let guard = state.last_paste.lock().unwrap();
        if let Some(paste) = guard.as_ref() {
            let safe = !paste.waiting
                && platform::foreground_window() == paste.target
                && can_erase(&paste.choices[paste.current], paste.terminal);
            if safe {
                erase(paste, false);
            }
        }
    }
    hide_chip(app);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_crlf_as_one_step() {
        assert_eq!(caret_len("a\r\nb中"), 4);
    }

    #[test]
    fn terminals_only_get_single_line_choices() {
        let recent = ["x", "two\nlines", "y", "x", " "]
            .map(String::from)
            .to_vec();
        assert_eq!(swap_choices("x", recent.clone(), true), ["x", "y"]);
        assert_eq!(swap_choices("x", recent, false), ["x", "two\nlines", "y"]);
    }

    #[test]
    fn cycles_past_the_current_text() {
        assert_eq!(next_index(0, 0, 3), 1);
        assert_eq!(next_index(2, 0, 3), 1);
        assert_eq!(next_index(1, 2, 3), 0);
        assert_eq!(next_index(0, 1, 2), 0);
    }

    #[test]
    fn previews_first_line() {
        assert_eq!(preview("  hello\nworld"), "hello…");
        assert_eq!(preview("short"), "short");
    }

    #[test]
    fn recognizes_terminals() {
        assert!(is_terminal(Some("WindowsTerminal.exe")));
        assert!(!is_terminal(Some("Code.exe")));
        assert!(!is_terminal(None));
    }
}

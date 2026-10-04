//! Placeholder so the crate compiles on platforms that are not implemented yet.

/// Opaque native window handle.
pub type WindowHandle = isize;

pub fn spawn_clipboard_listener<F: Fn() + Send + Sync + 'static>(_on_change: F) {}

pub fn read_clipboard_for_history() -> Option<String> {
    None
}

pub fn read_clipboard_text() -> Option<String> {
    None
}

pub fn write_clipboard_text(_text: &str) -> bool {
    false
}

pub fn foreground_window() -> WindowHandle {
    0
}

pub fn is_own_window(_hwnd: WindowHandle) -> bool {
    false
}

pub fn process_name(_hwnd: WindowHandle) -> Option<String> {
    None
}

pub fn activate_window(_hwnd: WindowHandle) {}

pub fn send_paste() {}

pub fn send_left(_count: usize) {}

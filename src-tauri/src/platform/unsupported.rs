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

pub fn send_right(_count: usize) {}

pub fn send_shift_left(_count: usize) {}

pub fn send_backspace(_count: usize) {}

pub fn mask_menu_key() {}

pub fn alt_down() -> bool {
    false
}

pub fn caret_rect(_hwnd: WindowHandle) -> Option<super::Rect> {
    None
}

pub fn work_area(_x: i32, _y: i32) -> Option<super::Rect> {
    None
}

pub fn set_bounds(_hwnd: WindowHandle, _x: i32, _y: i32, _width: i32, _height: i32) -> bool {
    false
}

pub fn make_non_activating(_hwnd: WindowHandle) {}

pub fn show_without_focus(_hwnd: WindowHandle) {}

pub fn hide_window(_hwnd: WindowHandle) {}

pub fn ime_to_english(_hwnd: WindowHandle) {}

pub fn prefers_chinese() -> bool {
    false
}

use std::ffi::c_void;
use std::path::Path;
use std::ptr::{null, null_mut};
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;

use windows_sys::Win32::Foundation::{
    CloseHandle, GlobalFree, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
};
use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;
use windows_sys::Win32::Graphics::Gdi::{
    ClientToScreen, GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows_sys::Win32::System::DataExchange::{
    AddClipboardFormatListener, CloseClipboard, EmptyClipboard, GetClipboardData,
    IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
};
use windows_sys::Win32::System::Ole::CF_UNICODETEXT;
use windows_sys::Win32::System::Threading::{
    AttachThreadInput, GetCurrentProcessId, GetCurrentThreadId, OpenProcess,
    QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::Input::Ime::{ImmGetDefaultIMEWnd, IMC_SETCONVERSIONMODE};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
    VIRTUAL_KEY, VK_BACK, VK_CONTROL, VK_LEFT, VK_MENU, VK_RIGHT, VK_SHIFT, VK_V,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetForegroundWindow, GetGUIThreadInfo,
    GetMessageW, GetWindowLongPtrW, GetWindowRect, GetWindowThreadProcessId, IsIconic,
    RegisterClassW, SendMessageTimeoutW, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos,
    ShowWindow, TranslateMessage, GUITHREADINFO, GWL_EXSTYLE, GWL_STYLE, HWND_MESSAGE,
    HWND_TOPMOST, MSG, SMTO_ABORTIFHUNG, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SWP_NOZORDER, SW_HIDE, SW_RESTORE, SW_SHOWNOACTIVATE, WM_CLIPBOARDUPDATE,
    WM_IME_CONTROL, WNDCLASSW, WS_CAPTION, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_POPUP, WS_SYSMENU, WS_THICKFRAME,
};

use super::Rect;

/// Native window handle stored as an integer so it can cross threads.
pub type WindowHandle = isize;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn registered_format(cell: &'static OnceLock<u32>, name: &str) -> u32 {
    *cell.get_or_init(|| unsafe { RegisterClipboardFormatW(wide(name).as_ptr()) })
}

/// Set by password managers (and by us) to ask monitors to skip this content.
fn fmt_exclude_monitor() -> u32 {
    static F: OnceLock<u32> = OnceLock::new();
    registered_format(&F, "ExcludeClipboardContentFromMonitorProcessing")
}

/// DWORD 0 means "do not put this in clipboard history".
fn fmt_can_include_history() -> u32 {
    static F: OnceLock<u32> = OnceLock::new();
    registered_format(&F, "CanIncludeInClipboardHistory")
}

/// Legacy marker used by some password managers.
fn fmt_viewer_ignore() -> u32 {
    static F: OnceLock<u32> = OnceLock::new();
    registered_format(&F, "Clipboard Viewer Ignore")
}

/// RAII guard for OpenClipboard/CloseClipboard. Another process may briefly
/// hold the clipboard, so opening is retried.
struct OpenedClipboard;

impl OpenedClipboard {
    fn open() -> Option<Self> {
        for _ in 0..20 {
            if unsafe { OpenClipboard(null_mut()) } != 0 {
                return Some(OpenedClipboard);
            }
            thread::sleep(Duration::from_millis(10));
        }
        None
    }
}

impl Drop for OpenedClipboard {
    fn drop(&mut self) {
        unsafe { CloseClipboard() };
    }
}

unsafe fn get_unicode_text() -> Option<String> {
    if IsClipboardFormatAvailable(CF_UNICODETEXT as u32) == 0 {
        return None;
    }
    let handle = GetClipboardData(CF_UNICODETEXT as u32);
    if handle.is_null() {
        return None;
    }
    let ptr = GlobalLock(handle) as *const u16;
    if ptr.is_null() {
        return None;
    }
    // Bound the scan by the allocation size in case the data is not terminated.
    let max = GlobalSize(handle) / 2;
    let mut len = 0;
    while len < max && *ptr.add(len) != 0 {
        len += 1;
    }
    let text = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len));
    GlobalUnlock(handle);
    Some(text)
}

unsafe fn excluded_from_history() -> bool {
    if IsClipboardFormatAvailable(fmt_exclude_monitor()) != 0
        || IsClipboardFormatAvailable(fmt_viewer_ignore()) != 0
    {
        return true;
    }
    let fmt = fmt_can_include_history();
    if IsClipboardFormatAvailable(fmt) != 0 {
        let handle = GetClipboardData(fmt);
        if !handle.is_null() {
            let ptr = GlobalLock(handle) as *const u32;
            if !ptr.is_null() {
                let allowed = *ptr != 0;
                GlobalUnlock(handle);
                return !allowed;
            }
        }
    }
    false
}

/// Current clipboard text, unless the owner asked monitors to ignore it.
pub fn read_clipboard_for_history() -> Option<String> {
    let _clip = OpenedClipboard::open()?;
    unsafe {
        if excluded_from_history() {
            return None;
        }
        get_unicode_text()
    }
}

pub fn read_clipboard_text() -> Option<String> {
    let _clip = OpenedClipboard::open()?;
    unsafe { get_unicode_text() }
}

unsafe fn set_global(format: u32, bytes: &[u8]) -> bool {
    let handle = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1));
    if handle.is_null() {
        return false;
    }
    let ptr = GlobalLock(handle) as *mut u8;
    if ptr.is_null() {
        GlobalFree(handle);
        return false;
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
    GlobalUnlock(handle);
    // On success the system owns the memory.
    if SetClipboardData(format, handle as *mut c_void).is_null() {
        GlobalFree(handle);
        return false;
    }
    true
}

/// Writes text to the clipboard, marked so that clipboard monitors
/// (including our own and Windows' Win+V history) skip it.
pub fn write_clipboard_text(text: &str) -> bool {
    let Some(_clip) = OpenedClipboard::open() else {
        return false;
    };
    unsafe {
        EmptyClipboard();
        let utf16: Vec<u8> = wide(text).iter().flat_map(|c| c.to_le_bytes()).collect();
        let ok = set_global(CF_UNICODETEXT as u32, &utf16);
        set_global(fmt_exclude_monitor(), &0u32.to_le_bytes());
        set_global(fmt_can_include_history(), &0u32.to_le_bytes());
        ok
    }
}

static ON_CLIPBOARD_CHANGE: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

unsafe extern "system" fn listener_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_CLIPBOARDUPDATE {
        // Writers often reopen the clipboard right after the first update
        // (e.g. OleFlushClipboard); opening it immediately makes them fail.
        thread::sleep(Duration::from_millis(100));
        if let Some(callback) = ON_CLIPBOARD_CHANGE.get() {
            callback();
        }
        return 0;
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

/// Starts a thread with a message-only window that receives
/// WM_CLIPBOARDUPDATE and invokes `on_change` for every clipboard change.
pub fn spawn_clipboard_listener<F: Fn() + Send + Sync + 'static>(on_change: F) {
    if ON_CLIPBOARD_CHANGE.set(Box::new(on_change)).is_err() {
        return;
    }
    thread::spawn(|| unsafe {
        let class = wide("StashClipboardListener");
        let instance = GetModuleHandleW(null());
        let wc = WNDCLASSW {
            lpfnWndProc: Some(listener_proc),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            null(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            null_mut(),
            instance,
            null(),
        );
        if hwnd.is_null() || AddClipboardFormatListener(hwnd) == 0 {
            eprintln!("clipboard listener could not be installed");
            return;
        }
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}

pub fn foreground_window() -> WindowHandle {
    unsafe { GetForegroundWindow() as WindowHandle }
}

pub fn is_own_window(hwnd: WindowHandle) -> bool {
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd as HWND, &mut pid);
        pid == GetCurrentProcessId()
    }
}

/// Executable file name (e.g. `chrome.exe`) of the process owning `hwnd`.
pub fn process_name(hwnd: WindowHandle) -> Option<String> {
    if hwnd == 0 {
        return None;
    }
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd as HWND, &mut pid);
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut size);
        CloseHandle(process);
        if ok == 0 {
            return None;
        }
        let path = String::from_utf16_lossy(&buf[..size as usize]);
        Path::new(&path)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
    }
}

/// Brings `hwnd` back to the foreground. Attaching to the current foreground
/// thread's input queue lifts the foreground-lock restriction.
pub fn activate_window(hwnd: WindowHandle) {
    if hwnd == 0 {
        return;
    }
    unsafe {
        let target = hwnd as HWND;
        if IsIconic(target) != 0 {
            ShowWindow(target, SW_RESTORE);
        }
        let current_thread = GetCurrentThreadId();
        let foreground_thread = GetWindowThreadProcessId(GetForegroundWindow(), null_mut());
        let attach = foreground_thread != 0 && foreground_thread != current_thread;
        if attach {
            AttachThreadInput(current_thread, foreground_thread, 1);
        }
        SetForegroundWindow(target);
        if attach {
            AttachThreadInput(current_thread, foreground_thread, 0);
        }
    }
}

fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if up { KEYEVENTF_KEYUP } else { 0 },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn send(inputs: &[INPUT]) {
    unsafe {
        SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32);
    }
}

pub fn send_paste() {
    send(&[
        key(VK_CONTROL, false),
        key(VK_V, false),
        key(VK_V, true),
        key(VK_CONTROL, true),
    ]);
}

fn tap(vk: VIRTUAL_KEY, count: usize) -> Vec<INPUT> {
    (0..count)
        .flat_map(|_| [key(vk, false), key(vk, true)])
        .collect()
}

pub fn send_left(count: usize) {
    send(&tap(VK_LEFT, count));
}

pub fn send_right(count: usize) {
    send(&tap(VK_RIGHT, count));
}

/// Selects `count` characters to the left of the caret.
pub fn send_shift_left(count: usize) {
    let mut inputs = vec![key(VK_SHIFT, false)];
    inputs.extend(tap(VK_LEFT, count));
    inputs.push(key(VK_SHIFT, true));
    send(&inputs);
}

pub fn send_backspace(count: usize) {
    send(&tap(VK_BACK, count));
}

/// Taps an unassigned virtual key while Alt is held, so that releasing Alt
/// after our Alt+V hotkey does not open the target window's menu bar.
pub fn mask_menu_key() {
    const VK_UNASSIGNED: VIRTUAL_KEY = 0xE8;
    send(&tap(VK_UNASSIGNED, 1));
}

pub fn alt_down() -> bool {
    unsafe { (GetAsyncKeyState(VK_MENU as i32) as u16 & 0x8000) != 0 }
}

// ---------------------------------------------------------------------------
// Caret lookup

fn to_rect(r: &RECT) -> Rect {
    Rect {
        left: r.left,
        top: r.top,
        right: r.right,
        bottom: r.bottom,
    }
}

fn window_rect(hwnd: HWND) -> Option<Rect> {
    let mut r: RECT = unsafe { std::mem::zeroed() };
    (unsafe { GetWindowRect(hwnd, &mut r) } != 0).then(|| to_rect(&r))
}

/// A usable caret: non-degenerate height and inside the target window.
fn plausible(caret: Rect, window: Option<Rect>) -> Option<Rect> {
    if caret.bottom <= caret.top || (caret.left == 0 && caret.top == 0) {
        return None;
    }
    match window {
        Some(w) if !w.contains(caret.left, caret.top) => None,
        _ => Some(Rect {
            right: caret.right.max(caret.left + 1),
            ..caret
        }),
    }
}

/// Classic Win32 caret (edit controls, Notepad, most native apps).
fn gui_thread_info(hwnd: HWND) -> Option<GUITHREADINFO> {
    unsafe {
        let thread = GetWindowThreadProcessId(hwnd, null_mut());
        let mut info: GUITHREADINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
        (GetGUIThreadInfo(thread, &mut info) != 0).then_some(info)
    }
}

fn win32_caret(info: &GUITHREADINFO) -> Option<Rect> {
    if info.hwndCaret.is_null() {
        return None;
    }
    let r = info.rcCaret;
    let mut tl = POINT {
        x: r.left,
        y: r.top,
    };
    let mut br = POINT {
        x: r.right,
        y: r.bottom,
    };
    unsafe {
        if ClientToScreen(info.hwndCaret, &mut tl) == 0
            || ClientToScreen(info.hwndCaret, &mut br) == 0
        {
            return None;
        }
    }
    Some(Rect {
        left: tl.x,
        top: tl.y,
        right: br.x,
        bottom: br.y,
    })
}

/// Caret of `hwnd` (the foreground window) in physical screen pixels.
/// Tries the Win32 caret first, then MSAA's OBJID_CARET (Chromium, Electron,
/// many custom editors), then UI Automation: TextPattern2's caret range
/// (WinUI, WPF), or TextPattern's selection for providers without it
/// (Windows Terminal, conhost). The accessibility calls run on a worker
/// thread with a short timeout because a hung target would block them.
pub fn caret_rect(hwnd: WindowHandle) -> Option<Rect> {
    if hwnd == 0 {
        return None;
    }
    let target = hwnd as HWND;
    let bounds = window_rect(target);
    let info = gui_thread_info(target);
    if let Some(caret) = info
        .as_ref()
        .and_then(win32_caret)
        .and_then(|c| plausible(c, bounds))
    {
        return Some(caret);
    }
    let focus = info
        .map(|i| i.hwndFocus as WindowHandle)
        .filter(|h| *h != 0)
        .unwrap_or(hwnd);
    let (tx, rx) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let found = accessibility::caret(focus);
        let _ = tx.send(found);
    });
    rx.recv_timeout(Duration::from_millis(250))
        .ok()
        .flatten()
        .and_then(|c| plausible(c, bounds))
}

mod accessibility {
    use std::ffi::c_void;
    use std::ptr::null_mut;

    use windows::core::{Interface, BOOL};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_MULTITHREADED,
    };
    use windows::Win32::System::Ole::{
        SafeArrayAccessData, SafeArrayDestroy, SafeArrayUnaccessData,
    };
    use windows::Win32::System::Variant::VARIANT;
    use windows::Win32::UI::Accessibility::{
        AccessibleObjectFromWindow, CUIAutomation, IAccessible, IUIAutomation,
        IUIAutomationTextPattern, IUIAutomationTextPattern2, IUIAutomationTextRange,
        TextUnit_Character, UIA_TextPattern2Id, UIA_TextPatternId,
    };
    use windows::Win32::UI::WindowsAndMessaging::OBJID_CARET;

    use super::{Rect, WindowHandle};

    pub fn caret(focus: WindowHandle) -> Option<Rect> {
        unsafe {
            let initialized = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
            let found = msaa(focus).or_else(|| uia());
            if initialized {
                CoUninitialize();
            }
            found
        }
    }

    unsafe fn msaa(focus: WindowHandle) -> Option<Rect> {
        let mut raw: *mut c_void = null_mut();
        AccessibleObjectFromWindow(
            HWND(focus as *mut c_void),
            OBJID_CARET.0 as u32,
            &IAccessible::IID,
            &mut raw,
        )
        .ok()?;
        if raw.is_null() {
            return None;
        }
        let acc = IAccessible::from_raw(raw);
        let (mut x, mut y, mut w, mut h) = (0, 0, 0, 0);
        acc.accLocation(&mut x, &mut y, &mut w, &mut h, &VARIANT::from(0i32))
            .ok()?;
        (h > 0).then(|| Rect {
            left: x,
            top: y,
            right: x + w.max(1),
            bottom: y + h,
        })
    }

    unsafe fn uia() -> Option<Rect> {
        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
        let element = automation.GetFocusedElement().ok()?;
        let range = match element.GetCurrentPatternAs::<IUIAutomationTextPattern2>(UIA_TextPattern2Id) {
            Ok(pattern) => {
                let mut active = BOOL(0);
                pattern.GetCaretRange(&mut active).ok()?
            }
            // Providers with only TextPattern (Windows Terminal, conhost)
            // report a collapsed selection at the cursor when nothing is
            // selected; with a selection, its start is close enough.
            Err(_) => {
                let pattern: IUIAutomationTextPattern =
                    element.GetCurrentPatternAs(UIA_TextPatternId).ok()?;
                let selection = pattern.GetSelection().ok()?;
                if selection.Length().ok()? < 1 {
                    return None;
                }
                selection.GetElement(0).ok()?
            }
        };
        caret_from_range(&range)
    }

    /// Caret-sized rectangle at the start of `range`.
    unsafe fn caret_from_range(range: &IUIAutomationTextRange) -> Option<Rect> {
        if let Some(rect) = first_rect(range) {
            return Some(Rect {
                right: rect.left + 1,
                ..rect
            });
        }
        // A collapsed range often has no bounding box; use the character
        // the caret sits in front of.
        let wider = range.Clone().ok()?;
        wider.ExpandToEnclosingUnit(TextUnit_Character).ok()?;
        first_rect(&wider).map(|r| Rect {
            right: r.left + 1,
            ..r
        })
    }

    /// First rectangle of a range: GetBoundingRectangles returns a flat array
    /// of doubles, four per rectangle (left, top, width, height).
    unsafe fn first_rect(range: &IUIAutomationTextRange) -> Option<Rect> {
        let array = range.GetBoundingRectangles().ok()?;
        if array.is_null() {
            return None;
        }
        let count = (*array).rgsabound[0].cElements as usize;
        let mut data: *mut c_void = null_mut();
        let mut rect = None;
        if count >= 4 && SafeArrayAccessData(array, &mut data).is_ok() {
            let v = std::slice::from_raw_parts(data as *const f64, 4);
            if v[3] > 0.0 {
                rect = Some(Rect {
                    left: v[0] as i32,
                    top: v[1] as i32,
                    right: (v[0] + v[2]) as i32,
                    bottom: (v[1] + v[3]) as i32,
                });
            }
            let _ = SafeArrayUnaccessData(array);
        }
        let _ = SafeArrayDestroy(array);
        rect
    }
}

// ---------------------------------------------------------------------------
// Window placement

/// Work area (screen minus taskbar) of the monitor nearest to the point.
pub fn work_area(x: i32, y: i32) -> Option<Rect> {
    unsafe {
        let monitor = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        (GetMonitorInfoW(monitor, &mut info) != 0).then(|| to_rect(&info.rcWork))
    }
}

/// Moves and resizes in one step (no intermediate frame at the old size).
pub fn set_bounds(hwnd: WindowHandle, x: i32, y: i32, width: i32, height: i32) -> bool {
    if hwnd == 0 {
        return false;
    }
    unsafe {
        SetWindowPos(
            hwnd as HWND,
            null_mut(),
            x,
            y,
            width,
            height,
            SWP_NOZORDER | SWP_NOACTIVATE,
        ) != 0
    }
}

/// Makes a window that never takes focus when shown or clicked, and stays
/// out of Alt+Tab.
pub fn make_non_activating(hwnd: WindowHandle) {
    if hwnd == 0 {
        return;
    }
    unsafe {
        let hwnd = hwnd as HWND;
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(
            hwnd,
            GWL_EXSTYLE,
            ex_style | (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW) as isize,
        );
        // Undecorated Tauri windows keep WS_CAPTION and hide it by handling
        // WM_NCCALCSIZE. Once the window is a tool window shown without
        // activation, Windows draws that caption as a small "title ×" bar,
        // so drop every frame style and make it a plain popup.
        let frame = (WS_CAPTION | WS_THICKFRAME | WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX) as isize;
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        SetWindowLongPtrW(hwnd, GWL_STYLE, (style & !frame) | WS_POPUP as isize);
        SetWindowPos(
            hwnd,
            null_mut(),
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

/// Switches the input method of the control that has the keyboard focus in
/// `hwnd`'s thread to English (alphanumeric) mode. In a WebView the focus is
/// a child window of the WebView2 process, so the request goes to that
/// child's IME window. Microsoft Pinyin, Sogou, WeChat and other IMM-aware
/// IMEs honour it; Shift switches back to Chinese as usual.
pub fn ime_to_english(hwnd: WindowHandle) {
    if hwnd == 0 {
        return;
    }
    unsafe {
        let focus = gui_thread_info(hwnd as HWND)
            .map(|i| i.hwndFocus)
            .filter(|h| !h.is_null())
            .unwrap_or(hwnd as HWND);
        let ime = ImmGetDefaultIMEWnd(focus);
        if ime.is_null() {
            return;
        }
        // Conversion mode 0 = alphanumeric. Time-limited: the IME window
        // lives in another process and must not be able to hang us.
        let mut result = 0usize;
        SendMessageTimeoutW(
            ime,
            WM_IME_CONTROL,
            IMC_SETCONVERSIONMODE as WPARAM,
            0,
            SMTO_ABORTIFHUNG,
            200,
            &mut result,
        );
    }
}

/// Hides a window shown with `show_without_focus`. Tauri's own `hide()`
/// is a no-op there: it still believes the window is hidden because it
/// never saw it being shown.
pub fn hide_window(hwnd: WindowHandle) {
    if hwnd != 0 {
        unsafe {
            ShowWindow(hwnd as HWND, SW_HIDE);
        }
    }
}

/// Shows a window on top without activating it.
pub fn show_without_focus(hwnd: WindowHandle) {
    if hwnd == 0 {
        return;
    }
    unsafe {
        ShowWindow(hwnd as HWND, SW_SHOWNOACTIVATE);
        SetWindowPos(
            hwnd as HWND,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}

/// Whether the Windows display language is Chinese (any region).
pub fn prefers_chinese() -> bool {
    // The primary language is the low 10 bits of the LANGID; LANG_CHINESE = 0x04.
    unsafe { GetUserDefaultUILanguage() & 0x3ff == 0x04 }
}

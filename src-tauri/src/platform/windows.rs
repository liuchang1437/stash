use std::ffi::c_void;
use std::path::Path;
use std::ptr::{null, null_mut};
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;

use windows_sys::Win32::Foundation::{CloseHandle, GlobalFree, HWND, LPARAM, LRESULT, WPARAM};
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
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
    VK_CONTROL, VK_LEFT, VK_V,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetForegroundWindow, GetMessageW,
    GetWindowThreadProcessId, IsIconic, RegisterClassW, SetForegroundWindow, ShowWindow,
    TranslateMessage, HWND_MESSAGE, MSG, SW_RESTORE, WM_CLIPBOARDUPDATE, WNDCLASSW,
};

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

pub fn send_left(count: usize) {
    let inputs: Vec<INPUT> = (0..count)
        .flat_map(|_| [key(VK_LEFT, false), key(VK_LEFT, true)])
        .collect();
    send(&inputs);
}

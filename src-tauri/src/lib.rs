mod calc;
mod commands;
mod config;
mod db;
mod highlight;
mod migrate;
mod placement;
mod platform;
mod search;
mod snippets;
mod template;
mod yank;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use notify::{RecursiveMode, Watcher};
use serde::Serialize;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewWindow, WindowEvent,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use config::Config;
use db::Db;
use placement::{Anchor, AnchorKind, Layout, Space};
use search::{Entry, Index, ItemRef};

/// The popover that opens at the caret.
const MAIN_WINDOW: &str = "main";
/// Non-activating strip shown under the caret after a paste.
pub(crate) const CHIP_WINDOW: &str = "chip";
/// Regular window for settings and the snippet editor.
pub(crate) const MANAGE_WINDOW: &str = "manage";

/// Popover geometry used before the UI reports its real size, logical px.
const POPOVER_WIDTH: f64 = 480.0;
const POPOVER_HEIGHT: f64 = 360.0;
const POPOVER_MARGIN: f64 = 20.0;
/// Mirrors CARD_W / GAP / PEEK_W in src/lib/layout.ts, logical px.
const CARD_WIDTH: f64 = 440.0;
const PANEL_GAP: f64 = 8.0;
const PEEK_WIDTH: f64 = 480.0;

pub struct AppState {
    config: RwLock<Config>,
    config_path: PathBuf,
    default_snippets_dir: PathBuf,
    db: Mutex<Db>,
    index: RwLock<Index>,
    /// Window that was in the foreground before the launcher appeared;
    /// pasting goes back to it.
    prev_window: AtomicIsize,
    paused: AtomicBool,
    hotkey_error: Mutex<Option<String>>,
    snippet_watcher: Mutex<Option<notify::RecommendedWatcher>>,
    /// When the launcher was last hidden; decides whether the UI keeps its
    /// previous query on the next show.
    hidden_at: Mutex<Option<Instant>>,
    /// Where the popover is placed (upper middle of the screen, or the caret).
    anchor: Mutex<Option<Anchor>>,
    /// Caret (or mouse) of the last paste, for the chip.
    chip_anchor: Mutex<Option<Anchor>>,
    main_shortcut: Mutex<Option<Shortcut>>,
    /// Alt+V, registered only while the post-paste chip is visible.
    swap_shortcut: Mutex<Option<Shortcut>>,
    last_paste: Mutex<Option<yank::LastPaste>>,
    /// Bumped whenever the chip changes; pending auto-hides compare it.
    chip_seq: AtomicU64,
    /// Last request for the manage window, read by its UI when it loads.
    manage_request: Mutex<Option<ManageRequest>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManageRequest {
    /// `settings` or `edit`
    pub view: String,
    /// Snippet to edit; none for a new one.
    pub key: Option<String>,
    /// Initial body of a new snippet.
    pub body: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ShownPayload {
    keep_query: bool,
    anchor: AnchorKind,
    space: Space,
    /// Executable name of the window that pastes will go to.
    target_app: Option<String>,
    /// Open the preview panel right away (`config.auto_peek`).
    auto_peek: bool,
}

impl AppState {
    fn snippets_dir(&self) -> PathBuf {
        self.config
            .read()
            .unwrap()
            .snippets_path(&self.default_snippets_dir)
    }
}

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn show_launcher(app: &AppHandle) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
        return;
    };
    yank::hide_chip(app);
    let state = app.state::<AppState>();
    let foreground = platform::foreground_window();
    if !platform::is_own_window(foreground) {
        state.prev_window.store(foreground, Ordering::SeqCst);
    }
    let target = state.prev_window.load(Ordering::SeqCst);

    // Payload `keepQuery` tells the UI to keep its previous query.
    let visible = window.is_visible().unwrap_or(false);
    let keep_query = if visible {
        true
    } else {
        let keep_for = Duration::from_secs(state.config.read().unwrap().keep_query_seconds);
        state
            .hidden_at
            .lock()
            .unwrap()
            .is_some_and(|t| t.elapsed() < keep_for)
    };

    let (follow_caret, auto_peek, english_input) = {
        let config = state.config.read().unwrap();
        (config.follow_caret, config.auto_peek, config.english_input)
    };
    // By default the popover opens in the upper middle of the screen, like a
    // launcher; with `follow_caret` it opens at the caret instead. (The
    // post-paste chip always goes to the caret: see `caret_anchor`.)
    let anchor = if follow_caret {
        find_anchor(&window, (!platform::is_own_window(foreground)).then_some(target))
    } else {
        center_anchor(&window, auto_peek)
    };
    *state.anchor.lock().unwrap() = Some(anchor);
    if !visible {
        let space = anchor.space();
        let layout = Layout {
            width: POPOVER_WIDTH,
            height: POPOVER_HEIGHT,
            card_x: POPOVER_MARGIN,
            margin: POPOVER_MARGIN,
            flip: space.below < POPOVER_HEIGHT && space.above > space.below,
        };
        apply_bounds(&window, anchor.place(&layout));
    }

    let _ = window.show();
    let _ = window.set_focus();
    if english_input {
        // The WebView's focused child window only gets the keyboard a
        // moment after the window does; switch its IME once it has.
        let hwnd = native_handle(&window);
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(80));
            platform::ime_to_english(hwnd);
        });
    }
    let _ = app.emit_to(
        MAIN_WINDOW,
        "launcher-shown",
        ShownPayload {
            keep_query,
            anchor: anchor.kind,
            space: anchor.space(),
            target_app: platform::process_name(target),
            auto_peek,
        },
    );
}

/// Upper middle of the monitor under the mouse. The card and, when it opens
/// by itself, the preview beside it are centered together.
fn center_anchor(window: &WebviewWindow, auto_peek: bool) -> Anchor {
    let point = window
        .cursor_position()
        .map(|p| (p.x as i32, p.y as i32))
        .unwrap_or((0, 0));
    let (work, scale) = monitor_at(window, point);
    let width = if auto_peek {
        CARD_WIDTH + PANEL_GAP + PEEK_WIDTH
    } else {
        CARD_WIDTH
    };
    Anchor::centered(work, scale, width)
}

/// Where the post-paste chip goes: the caret of the window that was pasted
/// into, else the mouse pointer.
pub(crate) fn caret_anchor(app: &AppHandle, target: platform::WindowHandle) -> Option<Anchor> {
    let window = app.get_webview_window(MAIN_WINDOW)?;
    Some(find_anchor(&window, (target != 0).then_some(target)))
}

/// Caret of `target` when it can be found, else the mouse pointer.
fn find_anchor(window: &WebviewWindow, target: Option<platform::WindowHandle>) -> Anchor {
    let caret = target.and_then(platform::caret_rect);
    let point = caret
        .map(|c| (c.left, c.bottom))
        .or_else(|| {
            window
                .cursor_position()
                .ok()
                .map(|p| (p.x as i32, p.y as i32))
        })
        .unwrap_or((0, 0));
    let (work, scale) = monitor_at(window, point);
    match caret {
        Some(c) => Anchor::caret(c, work, scale),
        None => Anchor::mouse(point.0, point.1, work, scale),
    }
}

/// Work area and scale factor of the monitor containing `point`.
fn monitor_at(window: &WebviewWindow, (x, y): (i32, i32)) -> (platform::Rect, f64) {
    let monitors = window.available_monitors().unwrap_or_default();
    let monitor = monitors
        .iter()
        .find(|m| {
            let (p, s) = (m.position(), m.size());
            x >= p.x && x < p.x + s.width as i32 && y >= p.y && y < p.y + s.height as i32
        })
        .cloned()
        .or_else(|| window.primary_monitor().ok().flatten());
    let (bounds, scale) = match monitor {
        Some(m) => {
            let (p, s) = (m.position(), m.size());
            (
                platform::Rect {
                    left: p.x,
                    top: p.y,
                    right: p.x + s.width as i32,
                    bottom: p.y + s.height as i32,
                },
                m.scale_factor(),
            )
        }
        None => (
            platform::Rect {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            },
            1.0,
        ),
    };
    (platform::work_area(x, y).unwrap_or(bounds), scale)
}

pub(crate) fn native_handle(window: &WebviewWindow) -> platform::WindowHandle {
    #[cfg(windows)]
    {
        window
            .hwnd()
            .map(|h| h.0 as platform::WindowHandle)
            .unwrap_or(0)
    }
    #[cfg(not(windows))]
    {
        let _ = window;
        0
    }
}

/// Moves and resizes a window (physical px) in one step where possible.
pub(crate) fn apply_bounds(window: &WebviewWindow, (x, y, w, h): (i32, i32, i32, i32)) {
    if !platform::set_bounds(native_handle(window), x, y, w, h) {
        let _ = window.set_size(PhysicalSize::new(w.max(1) as u32, h.max(1) as u32));
        let _ = window.set_position(PhysicalPosition::new(x, y));
    }
}

/// Re-places the popover after the UI changed its size or direction.
pub(crate) fn place_popover(app: &AppHandle, layout: &Layout) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
        return;
    };
    if let Some(anchor) = *app.state::<AppState>().anchor.lock().unwrap() {
        apply_bounds(&window, anchor.place(layout));
    }
}

pub(crate) fn open_manage(app: &AppHandle, request: ManageRequest) {
    hide_launcher(app);
    *app.state::<AppState>().manage_request.lock().unwrap() = Some(request.clone());
    if let Some(window) = app.get_webview_window(MANAGE_WINDOW) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
    let _ = app.emit_to(MANAGE_WINDOW, "manage-open", request);
}

fn hide_launcher(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        // Hide can be requested twice (explicitly, then again on blur); only
        // the first one marks the time.
        if window.is_visible().unwrap_or(false) {
            *app.state::<AppState>().hidden_at.lock().unwrap() = Some(Instant::now());
        }
        let _ = window.hide();
    }
}

fn toggle_launcher(app: &AppHandle) {
    let active = app
        .get_webview_window(MAIN_WINDOW)
        .map(|w| w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false))
        .unwrap_or(false);
    if active {
        hide_launcher(app);
    } else {
        show_launcher(app);
    }
}

fn register_hotkey(app: &AppHandle, hotkey: &str) -> Result<(), String> {
    let shortcut: Shortcut = hotkey
        .parse()
        .map_err(|e| format!("无法识别快捷键 “{hotkey}”：{e}"))?;
    let shortcuts = app.global_shortcut();
    let state = app.state::<AppState>();
    let mut current = state.main_shortcut.lock().unwrap();
    if let Some(old) = current.take() {
        let _ = shortcuts.unregister(old);
    }
    shortcuts
        .register(shortcut)
        .map_err(|e| format!("注册快捷键 “{hotkey}” 失败，可能已被其他程序占用：{e}"))?;
    *current = Some(shortcut);
    Ok(())
}

fn on_clipboard_change(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.paused.load(Ordering::SeqCst) {
        return;
    }
    let Some(text) = platform::read_clipboard_for_history() else {
        return;
    };
    if text.trim().is_empty() {
        return;
    }
    let source = platform::process_name(platform::foreground_window());
    let (limit, max_chars, ignored) = {
        let config = state.config.read().unwrap();
        (
            config.history_limit,
            config.max_clip_chars,
            config.is_ignored(source.as_deref()),
        )
    };
    if ignored || text.chars().count() > max_chars {
        return;
    }

    let result = {
        let db = state.db.lock().unwrap();
        db.record_clip(&text, source.as_deref(), now())
            .and_then(|row| Ok((row, db.prune(limit)?)))
    };
    match result {
        Ok((row, removed)) => {
            let mut index = state.index.write().unwrap();
            index.upsert_clip(&row);
            for id in removed {
                index.remove(&ItemRef::Clip(id));
            }
        }
        Err(e) => eprintln!("failed to record clip: {e}"),
    }
    let _ = app.emit("index-changed", ());
}

fn reload_snippets(app: &AppHandle) {
    let state = app.state::<AppState>();
    let list = snippets::load_all(&state.snippets_dir());
    let usage = state.db.lock().unwrap().snippet_usage().unwrap_or_default();
    let entries = list
        .iter()
        .map(|s| Entry::from_snippet(s, usage.get(&s.path).copied().unwrap_or_default()))
        .collect();
    state.index.write().unwrap().set_snippets(entries);
    let _ = app.emit("index-changed", ());
}

/// (Re)starts watching the snippets directory and reloads it. Changes are
/// debounced so a burst of file events (e.g. a sync) triggers one reload.
fn watch_snippets(app: &AppHandle) {
    let state = app.state::<AppState>();
    let dir = state.snippets_dir();
    if let Err(e) = snippets::ensure_dir(&dir) {
        eprintln!("cannot create snippets dir {}: {e}", dir.display());
    }

    let (tx, rx) = mpsc::channel::<()>();
    let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if res.is_ok() {
            let _ = tx.send(());
        }
    })
    .and_then(|mut w| w.watch(&dir, RecursiveMode::Recursive).map(|_| w));
    match watcher {
        // Replacing the old watcher drops its sender, ending its thread.
        Ok(w) => *state.snippet_watcher.lock().unwrap() = Some(w),
        Err(e) => eprintln!("cannot watch {}: {e}", dir.display()),
    }

    let handle = app.clone();
    thread::spawn(move || {
        while rx.recv().is_ok() {
            while rx.recv_timeout(Duration::from_millis(300)).is_ok() {}
            reload_snippets(&handle);
        }
    });
    reload_snippets(app);
}

fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "打开 Stash", true, None::<&str>)?;
    let pause = CheckMenuItem::with_id(app, "pause", "暂停记录剪贴板", true, false, None::<&str>)?;
    let snippets = MenuItem::with_id(app, "snippets", "打开 Snippets 目录", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &show,
            &pause,
            &PredefinedMenuItem::separator(app)?,
            &snippets,
            &settings,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("Stash")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => show_launcher(app),
            "pause" => {
                app.state::<AppState>().paused.fetch_xor(true, Ordering::SeqCst);
            }
            "snippets" => {
                let _ = commands::open_snippets_dir(app.clone(), app.state::<AppState>());
            }
            "settings" => open_manage(
                app,
                ManageRequest {
                    view: "settings".into(),
                    key: None,
                    body: None,
                },
            ),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_launcher(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_launcher(app)
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    // The plugin calls this while holding its shortcut-table
                    // lock, and register/unregister take that same lock: doing
                    // either from here (e.g. hide_chip dropping Alt+V when the
                    // popover opens) deadlocks the main thread. Handle the
                    // press on another thread so the lock is released first.
                    let app = app.clone();
                    let shortcut = *shortcut;
                    thread::spawn(move || {
                        if yank::is_swap_hotkey(&app, &shortcut) {
                            yank::on_swap_hotkey(&app);
                        } else {
                            toggle_launcher(&app);
                        }
                    });
                })
                .build(),
        )
        .setup(|app| {
            let paths = app.path();
            let data_dir = paths.app_data_dir()?;
            migrate::data_dir(&data_dir);
            std::fs::create_dir_all(&data_dir)?;
            let config_path = paths.app_config_dir()?.join("config.json");
            let config = Config::load(&config_path);

            let mut default_snippets_dir = paths.document_dir()?.join("Stash Snippets");
            // Only the default location is renamed; a custom one is the user's.
            if config.snippets_dir.trim().is_empty() {
                default_snippets_dir = migrate::snippets_dir(&default_snippets_dir);
            }

            let db = Db::open(&data_dir.join("stash.db"))?;
            let mut index = Index::default();
            index.set_clips(&db.all_clips()?);
            let hotkey = config.hotkey.clone();

            app.manage(AppState {
                config: RwLock::new(config),
                config_path,
                default_snippets_dir,
                db: Mutex::new(db),
                index: RwLock::new(index),
                prev_window: AtomicIsize::new(0),
                paused: AtomicBool::new(false),
                hotkey_error: Mutex::new(None),
                snippet_watcher: Mutex::new(None),
                hidden_at: Mutex::new(None),
                anchor: Mutex::new(None),
                chip_anchor: Mutex::new(None),
                main_shortcut: Mutex::new(None),
                swap_shortcut: Mutex::new(None),
                last_paste: Mutex::new(None),
                chip_seq: AtomicU64::new(0),
                manage_request: Mutex::new(None),
            });

            // The windows are `"create": false` in tauri.conf.json and built
            // only now: Tauri would otherwise create them before this hook
            // runs, and a page that loads fast (the release build) calls
            // commands needing AppState before it is managed, which panics.
            let handle = app.handle();
            for config in &app.config().app.windows {
                tauri::WebviewWindowBuilder::from_config(handle, config)?.build()?;
            }
            if let Some(chip) = handle.get_webview_window(CHIP_WINDOW) {
                platform::make_non_activating(native_handle(&chip));
            }
            if let Err(e) = register_hotkey(handle, &hotkey) {
                *app.state::<AppState>().hotkey_error.lock().unwrap() = Some(e);
                open_manage(
                    handle,
                    ManageRequest {
                        view: "settings".into(),
                        key: None,
                        body: None,
                    },
                );
            }
            watch_snippets(handle);
            let listener_handle = handle.clone();
            platform::spawn_clipboard_listener(move || on_clipboard_change(&listener_handle));
            build_tray(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                if window.label() == MAIN_WINDOW {
                    hide_launcher(window.app_handle());
                } else {
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::search,
            commands::snippet_tags,
            commands::activate,
            commands::toggle_pin,
            commands::delete_item,
            commands::get_snippet,
            commands::save_snippet,
            commands::get_settings,
            commands::save_settings,
            commands::hide_window,
            commands::open_snippets_dir,
            commands::place_popover,
            commands::calc_detail,
            commands::preview_snippet,
            commands::preview_template,
            commands::activate_text,
            commands::open_explorer,
            commands::open_manage,
            commands::manage_request,
            commands::close_manage,
            commands::chip_swap,
            commands::chip_undo,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

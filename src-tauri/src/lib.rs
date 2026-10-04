mod calc;
mod commands;
mod config;
mod db;
mod migrate;
mod platform;
mod search;
mod snippets;
mod template;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::mpsc;
use std::sync::{Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use notify::{RecursiveMode, Watcher};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewWindow, WindowEvent};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use config::Config;
use db::Db;
use search::{Entry, Index, ItemRef};

const MAIN_WINDOW: &str = "main";

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
    let state = app.state::<AppState>();
    let foreground = platform::foreground_window();
    if !platform::is_own_window(foreground) {
        state.prev_window.store(foreground, Ordering::SeqCst);
    }

    // Payload `true` tells the UI to keep its previous query.
    let keep_query = if window.is_visible().unwrap_or(false) {
        true
    } else {
        let keep_for = Duration::from_secs(state.config.read().unwrap().keep_query_seconds);
        state
            .hidden_at
            .lock()
            .unwrap()
            .is_some_and(|t| t.elapsed() < keep_for)
    };

    let _ = place_on_cursor_monitor(&window);
    let _ = window.show();
    let _ = window.set_focus();
    let _ = app.emit("launcher-shown", keep_query);
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

/// Centers the window horizontally, a fifth of the way down, on the monitor
/// under the mouse cursor.
fn place_on_cursor_monitor(window: &WebviewWindow) -> tauri::Result<()> {
    let cursor = window.cursor_position()?;
    let monitor = window
        .available_monitors()?
        .into_iter()
        .find(|m| {
            let (p, s) = (m.position(), m.size());
            cursor.x >= p.x as f64
                && cursor.x < p.x as f64 + s.width as f64
                && cursor.y >= p.y as f64
                && cursor.y < p.y as f64 + s.height as f64
        })
        .or(window.primary_monitor()?);
    let Some(monitor) = monitor else {
        return Ok(());
    };
    let (p, s) = (monitor.position(), monitor.size());
    // outer_size is in the current monitor's scale; convert to the target's.
    let scale = monitor.scale_factor() / window.scale_factor()?;
    let width = window.outer_size()?.width as f64 * scale;
    let x = p.x as f64 + (s.width as f64 - width) / 2.0;
    let y = p.y as f64 + s.height as f64 / 5.0;
    window.set_position(PhysicalPosition::new(x as i32, y as i32))
}

fn register_hotkey(app: &AppHandle, hotkey: &str) -> Result<(), String> {
    let shortcut: Shortcut = hotkey
        .parse()
        .map_err(|e| format!("无法识别快捷键 “{hotkey}”：{e}"))?;
    let shortcuts = app.global_shortcut();
    let _ = shortcuts.unregister_all();
    shortcuts
        .register(shortcut)
        .map_err(|e| format!("注册快捷键 “{hotkey}” 失败，可能已被其他程序占用：{e}"))
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
            "settings" => {
                show_launcher(app);
                let _ = app.emit("open-settings", ());
            }
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
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        toggle_launcher(app);
                    }
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
            });

            let handle = app.handle();
            if let Err(e) = register_hotkey(handle, &hotkey) {
                *app.state::<AppState>().hotkey_error.lock().unwrap() = Some(e);
                show_launcher(handle);
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
                hide_launcher(window.app_handle());
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::search,
            commands::activate,
            commands::toggle_pin,
            commands::delete_item,
            commands::get_snippet,
            commands::save_snippet,
            commands::get_settings,
            commands::save_settings,
            commands::hide_window,
            commands::open_snippets_dir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

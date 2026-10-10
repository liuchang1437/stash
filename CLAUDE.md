# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Stash (随手) is a Windows tray app built with Tauri 2 (Rust) and SvelteKit/Svelte 5 in SPA mode. Alt+Space opens a popover in the upper middle of the screen (or, with `followCaret`, at the text caret of the focused app) that searches clipboard history and Markdown snippets together, evaluates calculator expressions, and pastes the chosen result back into that app. All UI text and the README are written in Chinese. The UI uses a dark, monospace "terminal" theme (`src/app.css`).

## Commands

```bash
npm install
npm run tauri dev -- --no-watch   # run the app (tray only; open the window with Alt+Space)
npm run tauri build               # release bundle
npm run check                     # svelte-check / TypeScript (the only frontend lint)
cd src-tauri && cargo test        # all Rust unit tests
cd src-tauri && cargo test calc::tests::modulo   # a single test
```

- Stop any running `stash.exe` before you run `cargo test` or `cargo build`. Windows locks the executable, so the build fails while the app is running.
- There are no frontend tests. Rust tests live in `#[cfg(test)]` modules at the bottom of each source file.
- Without Windows you can still type-check the Windows code: `RUSTC_BOOTSTRAP=1 cargo check -Zbuild-std=std,panic_abort --target x86_64-pc-windows-msvc` (needs the `rust-src` component; set `CC_x86_64_pc_windows_msvc`/`AR_x86_64_pc_windows_msvc` to a no-op script so the bundled SQLite C code is skipped).

## Architecture

**Rust owns all state; the frontend is a thin view.** `AppState` in `src-tauri/src/lib.rs` holds the config, the SQLite `Db`, the in-memory search `Index`, and the previously focused window handle. The frontend calls commands defined in `commands.rs` through the typed wrappers in `src/lib/api.ts`. A new command must be added in three places: `commands.rs`, the `generate_handler!` list in `lib.rs`, and `api.ts`.

**Search runs entirely in memory.** At startup, `search::Index` is built from the DB's clips and from the snippet files. Each `Entry` precomputes nucleo haystacks: title, body truncated to 1000 characters, and pinyin full/initials when the text contains Chinese. Clips are updated incrementally (`upsert_clip`/`remove`); snippets are replaced as a whole on reload. Ranking is the match score plus a frecency bonus (`rank_bonus`). Calculator rows are not stored in the index; `commands::search` prepends them, and only when no scope is set.

**Scopes and highlighting.** `search(query, filter)` takes a `Filter`: `source` (`all | clip | snippet | pinned`) and an optional snippet `tag`. The popover sets it with Ctrl+Tab or a `#…` completion (`snippet_tags` supplies the tags). With an empty query, a snippet scope lists every snippet, not just used ones. Ranking only computes scores. For the hits it returns, `highlight.rs` works out what matched: each word separately, as is or through pinyin mapped back to the Chinese characters. The UI receives UTF-16 ranges (`titleMarks`, `contextMarks`), so it can slice JS strings directly. If a clip's first line lacks some of the words, the best-matching line becomes its `title`. A snippet keeps its title and gets that line as `context`. `terms` lists the literal and pinyin matches; the preview highlights every occurrence of them, but not scattered fuzzy matches.

**Items are addressed by string keys** (`search::ItemRef`): `c:<clip id>`, `s:<snippet relative path>`, `=:<calc result>`. Every command takes these keys.

**Activation is two-phase.** `activate(key, mode, values)` with `mode` ∈ `paste | copy | open`:
- If a snippet has input variables and no `values` were passed, it returns `NeedsInput { fields }`.
- The popover then expands the fields inline under the selected row and calls `activate` again with the values. While the user types, `preview_snippet` returns the rendered text as `Segment`s (`template::render_segments`), and the side preview shows them with the field being edited highlighted. A segment's `kind` is `text | input | auto | cursor`. A UUID renders empty in previews, because the real one is only made at paste time.
- Before that, search hits for snippets with variables already carry `rendered` (their defaults filled in, set by `commands::search`, which reads the clipboard at most once). The preview shows this result by default; Tab switches to the template.
- `activate_text(key, text, mode)` pastes text the UI derived from an item (calculator formats, "paste as…" transformations in `src/lib/kinds.ts`); `key` only records usage.
- Usage stats are recorded only after the action succeeds.

**Paste flow** (`commands::deliver`, runs on a spawned thread):
1. Write the text to the clipboard.
2. Re-activate the window that was in front before the launcher opened (`prev_window`). This must happen *before* hiding the launcher, while Stash still owns the foreground.
3. Hide the launcher.
4. Send Ctrl+V, then press ← as many times as needed to land on `{{cursor}}`.
5. Hand over to `yank::after_paste`, which shows the post-paste chip (see below).
6. Optionally restore the previous clipboard text after 500 ms.

**Stash's own clipboard writes are invisible to its own listener.** `platform::write_clipboard_text` adds the `ExcludeClipboardContentFromMonitorProcessing` and `CanIncludeInClipboardHistory=0` formats, and the WM_CLIPBOARDUPDATE listener skips content carrying those formats. That listener is a message-only window on its own thread. Do not record pasted text through some other path.

**Snippets: files are the source of truth, stats live in the DB.** Each snippet is one `.md` file with front matter containing `title` and `tags` (`snippets.rs`). Its id is the path relative to the snippets directory. A `notify` watcher with a 300 ms debounce reloads the whole snippets directory after any change. Use counts live in the `snippet_usage` table so that using a snippet never modifies (and re-syncs) the synced directory. Keep it that way.

**Clipboard history numbering is shared.** `{{clipboard:N}}` in templates and `$N` in calculator expressions both mean the N-th most recently copied *or used* clip, with 1 = latest. Both resolve through `Index::recent_clips`. Pinning does not affect this order.

**Template engine** (`template.rs`): `parse` → `fields` → `render`. With `url_encode` (open mode), a value is percent-encoded only when the rendered text before it already contains `://`, so `https://x/s?q={{q}}` gets encoded but a bare `{{url}}` does not. If you add a variable or change the syntax, also update `src/lib/template.ts` and the README table. In that file, `tokens` mirrors `parse`/`parse_var` for highlighting and error messages, and `VARIABLES` lists what the editor offers.

**Snippet editor** (`SnippetEditor.svelte`). It uses CodeMirror 6, set up in `src/lib/editor.ts`. The component imports that module dynamically, so only the manage window loads CodeMirror. Typing `{{` completes a variable. Ctrl+K opens a menu that inserts a variable, or turns the selected text into one (the selection becomes the default). The side panel calls `preview_template(body, values)` on the unsaved text to list the fields and render the result with test values.

**Calculator** (`calc.rs`): a hand-written recursive-descent parser. A query counts as a calculation only when it contains a binary or postfix operator, a function call, or implicit multiplication, so ordinary searches like `e` or `2024` never show a result. `%` is modulo (`rem_euclid`). `$N` references are substituted as text before parsing (`substitute_clips`).

**Windows.** Three windows load the same page; `src/routes/+page.svelte` picks the UI by window label. They are `"create": false` in `tauri.conf.json` and built in `setup` *after* `app.manage(AppState)`. Tauri would otherwise create them before `setup` runs, and in the release build the page loads fast enough to invoke a command before the state exists (panic → instant exit, since release uses `panic = "abort"`).
- `main` — the popover (`Popover.svelte`). Transparent and undecorated; the cards draw their own shadow inside a `MARGIN` border. Blur hides it. Every hide must go through `hide_launcher` so that `hidden_at` is recorded.
- `chip` — the post-paste strip (`Chip.svelte`). `platform::make_non_activating` turns it into a frameless `WS_POPUP` tool window with `WS_EX_NOACTIVATE` at startup (otherwise Windows draws its hidden caption as a "Stash ×" bar), and it is shown with `platform::show_without_focus`, so the target app keeps the keyboard. Never call `window.show()` / `window.hide()` on it on Windows: Tauri does not know it is visible, so its `hide()` is a no-op. Hide it with `platform::hide_window` (`yank::hide_chip`).
- `manage` — an undecorated window for settings and the snippet editor (`Manage.svelte`); the views' headers are `data-tauri-drag-region` and carry a `CloseButton`. Opened with `open_manage`; the UI also reads the last request with `manage_request` because it may load after the event was sent.

Events are sent with `emit_to(label, …)` and listened to with `getCurrentWebviewWindow().listen`, so each window only sees its own.

**Popover placement** (`placement.rs`, `show_launcher`). By default `center_anchor` puts the window in the upper middle of the monitor under the mouse (`Anchor::centered`), centering the card plus the auto-opened preview. With `config.followCaret`, `find_anchor` asks `platform::caret_rect` for the caret of the target window instead (Win32 caret → MSAA `OBJID_CARET` → UIA `TextPattern2` caret range, or `TextPattern` selection for providers like Windows Terminal; the COM part runs on a worker thread with a 250 ms timeout) and falls back to the mouse pointer. The anchor and the monitor's work area are stored in `AppState::anchor`. The chip has its own `AppState::chip_anchor`, looked up at paste time by `caret_anchor` in the pasted-into window. `launcher-shown` carries `{ keepQuery, anchor, space, targetApp, autoPeek }`; `space` (free room around the anchor, logical px) lets the UI decide to open upwards (`flip`). `keepQuery` is true when the window is reopened within `config.keepQuerySeconds` of the last `hide_launcher`; the popover then calls `resume()` instead of `reset()`.

**Panel layout inside the popover** (`src/lib/layout.ts`, `placePanels`). Panels never push each other around. The card stays fixed, the preview sits beside it (`sideFor`), and the action menu takes the first slot that fits: past the preview in its column, then beyond the preview, then the card's other side, overlaying the preview only as a last resort. Every panel is positioned absolutely from these coordinates; each is drawn hidden until it has been measured. The submenu is absolutely placed beside the main menu (`ActionMenu.svelte`), so it never resizes the menu either. The popover reports the bounding box with `place_popover({ width, height, cardX, margin, flip })`. `Anchor::place` pins the window to the anchor by its top edge (or bottom edge when `flip`) and horizontally by `cardX`. So keep the rule: nothing may start above the card when opening down, or end below it when opening up, otherwise the card moves.

**Swap after paste** (`yank.rs`). `after_paste` records `LastPaste` (target window, the pasted text plus earlier clips as choices) and shows the chip for a few seconds. The swap hotkey (`config.swapHotkey`, default Alt+V) is registered only while the chip is visible; the global-shortcut handler routes it via `is_swap_hotkey`. That handler hands every press to a new thread. The plugin invokes it while holding its shortcut-table lock, and `register`/`unregister` take that same lock, so calling either inside the callback (e.g. `show_launcher` → `hide_chip` dropping Alt+V) deadlocks the main thread. Each press taps an unassigned key (`mask_menu_key`, so releasing Alt does not open the target's menu bar) and advances `pending`; a thread waits for Alt to be released and then erases the current text (Shift+← to select, or Backspace in terminals — see `TERMINALS`) and pastes the chosen one. Terminals only get single-line choices; texts over 2000 characters are never erased.

**Platform layer.** `src-tauri/src/platform/` exposes the same free functions per OS, chosen with `cfg` (only `windows.rs` is real, using `windows-sys` 0.61, plus the `windows` crate for the MSAA/UIA COM interfaces). Keep Win32 code out of the other modules.

**Data locations.** The config and `stash.db` are in `%APPDATA%\io.github.liuchang1437.stash\`. Snippets default to `Documents\Stash Snippets`. `migrate.rs` moves data from the project's former name "Box" (`com.box.app`, `box.db`, `Box Snippets`) on first start, and only when the new location does not exist yet.

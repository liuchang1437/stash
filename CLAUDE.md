# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Stash (随手) is a Windows tray app built with Tauri 2 (Rust) and SvelteKit/Svelte 5 in SPA mode. Alt+Space opens a launcher that searches clipboard history and Markdown snippets together, evaluates calculator expressions, and pastes the chosen result back into the previously focused window. All UI text and the README are written in Chinese.

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

## Architecture

**Rust owns all state; the frontend is a thin view.** `AppState` in `src-tauri/src/lib.rs` holds the config, the SQLite `Db`, the in-memory search `Index`, and the previously focused window handle. The frontend calls commands defined in `commands.rs` through the typed wrappers in `src/lib/api.ts`. A new command must be added in three places: `commands.rs`, the `generate_handler!` list in `lib.rs`, and `api.ts`.

**Search runs entirely in memory.** At startup, `search::Index` is built from the DB's clips and from the snippet files. Each `Entry` precomputes nucleo haystacks: title, body truncated to 1000 characters, and pinyin full/initials when the text contains Chinese. Clips are updated incrementally (`upsert_clip`/`remove`); snippets are replaced as a whole on reload. Ranking is the match score plus a frecency bonus (`rank_bonus`). Calculator rows are not stored in the index; `commands::search` prepends them.

**Items are addressed by string keys** (`search::ItemRef`): `c:<clip id>`, `s:<snippet relative path>`, `=:<calc result>`. Every command takes these keys.

**Activation is two-phase.** `activate(key, mode, values)` with `mode` ∈ `paste | copy | open`:
- If a snippet has input variables and no `values` were passed, it returns `NeedsInput { fields }`.
- The UI then shows `FillForm` and calls `activate` again with the values.
- Usage stats are recorded only after the action succeeds.

**Paste flow** (`commands::deliver`, runs on a spawned thread):
1. Write the text to the clipboard.
2. Re-activate the window that was in front before the launcher opened (`prev_window`). This must happen *before* hiding the launcher, while Stash still owns the foreground.
3. Hide the launcher.
4. Send Ctrl+V, then press ← as many times as needed to land on `{{cursor}}`.
5. Optionally restore the previous clipboard text after 500 ms.

**Stash's own clipboard writes are invisible to its own listener.** `platform::write_clipboard_text` adds the `ExcludeClipboardContentFromMonitorProcessing` and `CanIncludeInClipboardHistory=0` formats, and the WM_CLIPBOARDUPDATE listener skips content carrying those formats. That listener is a message-only window on its own thread. Do not record pasted text through some other path.

**Snippets: files are the source of truth, stats live in the DB.** Each snippet is one `.md` file with front matter containing `title` and `tags` (`snippets.rs`). Its id is the path relative to the snippets directory. A `notify` watcher with a 300 ms debounce reloads the whole snippets directory after any change. Use counts live in the `snippet_usage` table so that using a snippet never modifies (and re-syncs) the synced directory. Keep it that way.

**Clipboard history numbering is shared.** `{{clipboard:N}}` in templates and `$N` in calculator expressions both mean the N-th most recently copied *or used* clip, with 1 = latest. Both resolve through `Index::recent_clips`. Pinning does not affect this order.

**Template engine** (`template.rs`): `parse` → `fields` → `render`. With `url_encode` (open mode), a value is percent-encoded only when the rendered text before it already contains `://`, so `https://x/s?q={{q}}` gets encoded but a bare `{{url}}` does not. If you add a variable, also update the syntax help in `SnippetEditor.svelte` and the README table.

**Calculator** (`calc.rs`): a hand-written recursive-descent parser. A query counts as a calculation only when it contains a binary or postfix operator, a function call, or implicit multiplication, so ordinary searches like `e` or `2024` never show a result. `%` is modulo (`rem_euclid`). `$N` references are substituted as text before parsing (`substitute_clips`).

**Window and UI state.** There is one undecorated window labeled `main`. `src/routes/+page.svelte` switches between the views `search | fill | edit | settings`. Blur hides the window only in `search` and `fill`. `show_launcher` emits `launcher-shown` with a boolean payload `keepQuery`, which is true when the window is reopened within `config.keepQuerySeconds` of the last `hide_launcher`. The launcher then calls `resume()` (keep the query and select it) instead of `reset()`. Every hide must go through `hide_launcher` so that `hidden_at` is recorded.

**Platform layer.** `src-tauri/src/platform/` exposes the same free functions per OS, chosen with `cfg` (only `windows.rs` is real, using `windows-sys` 0.61). Keep Win32 code out of the other modules.

**Data locations.** The config and `stash.db` are in `%APPDATA%\io.github.liuchang1437.stash\`. Snippets default to `Documents\Stash Snippets`. `migrate.rs` moves data from the project's former name "Box" (`com.box.app`, `box.db`, `Box Snippets`) on first start, and only when the new location does not exist yet.

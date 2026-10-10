# Contributing to Stash

Thanks for your interest! Bug reports, ideas and pull requests are all welcome. For anything larger than a small fix, please open an [issue](https://github.com/liuchang1437/stash/issues) first so we can agree on the approach.

## Development setup

You need Node.js, Rust with the MSVC toolchain, and Visual Studio Build Tools (C++ workload).

```bash
npm install
npm run tauri dev -- --no-watch
```

The app runs in the tray; press `Alt+Space` to open the popover.

## Before you open a pull request

```bash
npm run check
```

```bash
cd src-tauri && cargo test
```

- Quit any running `stash.exe` first: Windows locks the executable, so `cargo test` / `cargo build` fail while the app is running.
- `npm run check` (svelte-check / TypeScript) is the only frontend check; there are no frontend tests. Rust tests live in `#[cfg(test)]` modules at the bottom of each source file.
- Keep the user-facing text in Chinese, matching the rest of the UI.
- If you change behavior that the docs describe, update both [docs/guide.md](docs/guide.md) and [docs/guide.zh-CN.md](docs/guide.zh-CN.md) (and both READMEs if it affects them).
- If you add a snippet variable or change the template syntax, update `src-tauri/src/template.rs`, `src/lib/template.ts` and the variable tables in both guides.

## Project layout

```
src-tauri/src/
  lib.rs          startup, tray, hotkeys, windows, clipboard listener, snippets folder watcher
  commands.rs     commands called by the frontend (search, paste, pin, edit, settings)
  placement.rs    popover position: upper middle of the screen, or at the caret / flipped above / clamped to the edge
  yank.rs         post-paste strip, swap and undo
  search.rs       in-memory index: nucleo fuzzy matching + pinyin + frecency, scope / tag filters
  highlight.rs    highlight ranges for search hits (including pinyin mapped back to Chinese characters)
  calc.rs         calculator: expression parser, $N clipboard references
  template.rs     snippet variable parsing and rendering
  snippets.rs     reading and writing .md snippet files
  db.rs           SQLite: clipboard history, snippet usage stats
  config.rs       settings
  migrate.rs      migrating data from the old name "Box"
  platform/       OS-specific code (Windows only for now): clipboard, caret lookup, key input
src/lib/
  Popover.svelte  the popover: list, inline variable fields, calculator, Ctrl K menu
  layout.ts       placement of panels inside the popover (card, preview and menu never push each other)
  Peek.svelte     side preview (laid out / raw text, snippet result / template)
  Rendered.svelte a snippet's rendered result, variables colored by kind (shared by preview and editor)
  ActionMenu.svelte  cascading action menu
  Chip.svelte     post-paste strip
  Manage.svelte   settings and snippet editor window (SettingsView / SnippetEditor)
  editor.ts       snippet editor (CodeMirror): variable highlighting, errors, {{ completion
  template.ts     frontend view of the variable syntax: parsing for highlighting, insertable variables
  kinds.ts        content type detection, table parsing, "paste as…" transformations
```

[CLAUDE.md](CLAUDE.md) has more detailed architecture notes: how state, search, the paste flow, windows and the platform layer fit together.

## License

By contributing, you agree that your contributions will be licensed under the [MIT License](LICENSE).

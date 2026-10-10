# Stash 随手

English | [简体中文](README.zh-CN.md)

[![Release](https://img.shields.io/github/v/release/liuchang1437/stash)](https://github.com/liuchang1437/stash/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
![Platform: Windows](https://img.shields.io/badge/platform-Windows%2010%2F11-0078d4)
[![Built with Tauri](https://img.shields.io/badge/built%20with-Tauri%202-24c8db)](https://tauri.app)

A keyboard launcher for Windows that keeps your clipboard history, your Markdown snippets and a calculator one hotkey away, and pastes the result straight back into the app you were typing in.

![The Stash popover: clipboard history on the left, a preview of the selected entry on the right](docs/images/popover.png)

> [!NOTE]
> The interface is currently in Chinese only. The [user guide](docs/guide.md) gives the Chinese labels next to their English meaning.

## Highlights

- **One search for everything.** `Alt+Space` opens a popover that searches clipboard history and snippets together, with a live preview beside the list.
- **Fuzzy and pinyin matching.** Type `zb` to find 周报. Results are ranked by match quality, then by how often and how recently you used them.
- **Pastes where you were.** Stash switches back to the previous window and pastes for you. It can restore your original clipboard afterwards.
- **Swap after paste.** Pasted the wrong thing? Hold `Alt` and tap `V` to cycle through earlier clips in place, like Emacs yank-pop.
- **Snippets are plain `.md` files** with variables (text, defaults, choices, dates, clipboard history, cursor position). You fill them in inside the popover and see the result as you type. Keep the folder in OneDrive or git to sync it.
- **Calculator in the search box.** Type `(1+2)*3^2`, or `$1 + $2` to add the last two numbers you copied.
- **Knows what you copied.** Tables, commands, identifiers, URLs and Solana addresses get matching previews and actions, such as "paste as JSON string" or "paste as Markdown table".
- **Private by design.** Everything stays on your machine. Stash never connects to the network on its own, and it respects the "don't record" flags that password managers set.

## Install

Requirements: Windows 10 or 11 (64-bit) and the Microsoft Edge WebView2 Runtime, which comes with Windows 11 and is installed automatically if it is missing.

1. Download the latest `.exe` (NSIS) or `.msi` installer from [Releases](https://github.com/liuchang1437/stash/releases).
2. Run it. The installer is not code-signed yet, so SmartScreen may warn you: choose **More info → Run anyway**.

You can also [build it from source](#build-from-source).

## Quick start

1. Start Stash. It lives in the system tray; there is no main window.
2. Copy a few things as usual, then press `Alt+Space` in any app.
3. Type to search, use `↑ ↓` to pick, and press `↵` to paste into the app you came from (`⇧↵` copies only).
4. Press `Ctrl N` to turn the selected clip into a snippet, or type `12*3` to see the calculator.
5. Press `Ctrl ,` (or use the tray menu) to open the settings, where you can change the hotkey and turn on start at login.

The [user guide](docs/guide.md) covers everything else: all shortcuts, the snippet format and variables, the calculator, and URL snippets.

## Privacy & data

- Clipboard history is stored only in a local SQLite file. Stash never connects to the network on its own; it only opens a URL in your browser when you ask it to.
- Copies flagged by other apps as private (`ExcludeClipboardContentFromMonitorProcessing` / `CanIncludeInClipboardHistory`, used by 1Password, KeePassXC and others) are never recorded.
- You can also ignore specific programs by executable name, or pause recording from the tray menu.
- Text that Stash itself puts on the clipboard is flagged the same way, so it does not show up in the Win+V history either.

| What | Where |
|---|---|
| Settings | `%APPDATA%\io.github.liuchang1437.stash\config.json` |
| Clipboard history | `%APPDATA%\io.github.liuchang1437.stash\stash.db` |
| Snippets | `Documents\Stash Snippets` (configurable) |

## FAQ

**`Alt+Space` doesn't open Stash / opens something else.**
Another program (PowerToys Run, for example) is probably using the same hotkey. Open the settings, click the hotkey field and press a new combination.

**The popover appears in the middle of the screen. Can it follow my text cursor?**
Yes: turn on "Open next to the text caret" (在输入光标旁边打开) in the settings. It works in most editors, terminals and browsers. Where Stash can't find the caret, it opens next to the mouse pointer and the title bar says "caret not found" (未找到光标).

**How do I sync snippets between computers?**
Point the snippets folder at a synced location (OneDrive, Nutstore, a git repository…) in the settings. Usage statistics are kept in the local database, so using a snippet never modifies its file or triggers a sync.

**Does it work on macOS or Linux?**
Not yet. The system-specific code is isolated under `src-tauri/src/platform/`, so ports are possible; contributions are welcome.

**How do I uninstall and remove all data?**
Uninstall Stash from Windows Settings → Apps, then delete `%APPDATA%\io.github.liuchang1437.stash\`. Your snippets folder is left alone; delete it too if you no longer need it.

## Build from source

You need Node.js, Rust with the MSVC toolchain, and Visual Studio Build Tools (C++ workload).

```bash
npm install
npm run tauri dev
```

The app starts in the tray; press `Alt+Space` to open it. To build the installers (written to `src-tauri/target/release/bundle/`):

```bash
npm run tauri build
```

## Contributing

Bug reports, ideas and pull requests are welcome. Please open an [issue](https://github.com/liuchang1437/stash/issues) first for larger changes. See [CONTRIBUTING.md](CONTRIBUTING.md) for the development setup, checks and project layout.

## Acknowledgements

Stash is built on [Tauri](https://tauri.app), [Svelte](https://svelte.dev) and [CodeMirror](https://codemirror.net), and relies on [nucleo](https://github.com/helix-editor/nucleo) for fuzzy matching, [rust-pinyin](https://github.com/mozillazg/rust-pinyin) for pinyin search, [rusqlite](https://github.com/rusqlite/rusqlite) / SQLite for storage, [notify](https://github.com/notify-rs/notify) for watching the snippets folder and [windows-rs](https://github.com/microsoft/windows-rs) for the Win32 APIs.

## License

[MIT](LICENSE)

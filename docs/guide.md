# Stash user guide

English | [简体中文](guide.zh-CN.md)

See the [README](../README.md) for installation and a quick start. The interface is in Chinese; where this guide names a label you will see on screen, the Chinese text follows in parentheses.

- [The popover](#the-popover)
  - [After pasting: swap or undo](#after-pasting-swap-or-undo)
- [Calculator](#calculator)
- [Keyboard shortcuts](#keyboard-shortcuts)
- [Snippet format](#snippet-format)
  - [Editing snippets](#editing-snippets)
  - [URL snippets (quick search)](#url-snippets-quick-search)
- [Settings](#settings)
- [Migrating from Box](#migrating-from-box)

## The popover

The popover opens in the upper middle of the screen under the mouse, with the list and the preview centered as a unit. Turn on "Open next to the text caret" (在输入光标旁边打开) in the settings to open it right where you are typing instead; this works in terminals, editors, browsers and chat apps such as Feishu. If Stash can't find the caret, the popover opens next to the mouse pointer and its title bar says "caret not found" (未找到光标). When there is not enough room below the caret, the whole popover flips above it, with the search box still next to the caret.

- `Ctrl Tab` cycles the scope through All → Clipboard → Snippets (全部 → 剪贴板 → Snippets); the current scope is shown to the left of the search box. Typing `#` lists the scopes and your snippet tags (`#clip` `#snip` `#pin` `#work`…); pick one with `↵`, or type the full name followed by a space. `Backspace` in an empty search box returns to All. The Clipboard scope is grouped into Pinned / Today / Yesterday / Earlier (置顶 / 今天 / 昨天 / 更早); the Snippets scope lists every snippet, including ones you have never used.
- Matched characters are highlighted, and pinyin matches light up the Chinese characters they stand for (`zb` → **周报**). When the match is in the body rather than the first line, a clip shows the matching line as its title, and a snippet shows that line under its title when selected. The preview highlights every occurrence and scrolls to the first one.
- The selected row gets an extra summary line. A full preview opens beside the list automatically, on whichever side has more room (the settings can make it open only when you press `→`); `←` closes it. Tables are laid out by default; `Tab` switches to the raw text.
- A snippet's preview shows what will be pasted: dates and clipboard contents are filled in, variables show their defaults, variables without a default appear as `‹name›`, and `{{cursor}}` is a thin bar. Variables you fill in are green; values filled in automatically are blue (hover to see where they come from). `Tab` switches to the template source.
- Press `↵` on a snippet with variables and the fields open inline under the row. Pick choices with `1–9` or `← →`, type text directly, and move to the next field with `Tab`. The side preview shows the text to be pasted as you type, with the field you are editing highlighted.
- Click into the preview text (or press `F2`) to edit what will be pasted. The caret lands where you clicked, and a drag selection stays selected. For a clip you edit the full text; for a snippet you edit the rendered result (pressing `F2` while filling in variables keeps the values entered so far). `Ctrl ↵` pastes, `Ctrl ⇧ ↵` copies, `Esc` discards. The original clip and snippet file are left unchanged, and the edited text is not added to the clipboard history.
- Stash recognizes what kind of content a clip is (table, command, identifier, Solana address, URL…), and the preview and the `Ctrl K` actions adapt to it.
- `Ctrl K` opens the action menu. Submenus open as soon as their item is highlighted; `→` moves into them. "Paste as…" (粘贴为…) converts the text to a single line, a quoted string, a JSON string or a Markdown table. Solana addresses can also be shortened to `3wCv…ijkL` or wrapped as `pubkey!("…")`, and `Ctrl ↵` opens them on Solscan.
- `Alt 1–9` pastes the N-th row directly.
- The input method switches to English mode when the popover opens (search understands pinyin, and `Shift` switches back to Chinese). You can turn this off in the settings.

Panels that open later never push existing ones around: the preview stays next to the list, and the action menu takes the first free slot beside the preview, beyond the preview, or on the other side of the list, covering the preview only as a last resort.

To find the caret, Stash tries the Win32 caret, then MSAA (Chromium / Electron), then UI Automation (for apps like Windows Terminal that only support the basic TextPattern, it uses the selection).

### After pasting: swap or undo

After a paste, a small strip appears next to the pasted text (next to the mouse if the caret can't be found) for a few seconds. It doesn't take focus, so you can keep typing.

- Hold `Alt` and tap `V` repeatedly: the strip steps back through earlier clips, and releasing `Alt` replaces what you just pasted with the chosen one, like Emacs yank-pop.
- Click "Undo" (撤销) on the strip to delete what was just pasted.

The replacement works by selecting the pasted text with Shift+← and pasting over it; in terminals Stash uses Backspace instead. In terminals only single-line clips are offered, and text over 2000 characters is never replaced. The hotkey can be changed or turned off in the settings. It is registered only while the strip is visible, so `Alt+V` stays free the rest of the time.

## Calculator

Type an expression into the search box and the result appears as the first row. `↵` pastes it and `⇧↵` copies it. `Tab` cycles through three formats: the plain number, the number with thousands separators, and `expression = result`.

| Syntax | Examples |
|---|---|
| `+ - * / ^`, parentheses, `1e3`, `1_000`, thousands separators `1,234,567.8` | `(1+2)*3^2` → 27, `1,200*3` → 3600 |
| `%` modulo, `!` factorial | `10 % 3` → 1, `-7 % 3` → 2, `5!` → 120 |
| `$N`: the N-th most recent clipboard entry (1 = latest) | copy 12, then 30: `$1+$2` → 42 |
| Functions `sqrt cbrt abs sin cos tan asin acos atan ln log log2 exp floor ceil round` | `sqrt 2`, `log(1000)` |
| Constants `pi π e tau`, degrees `deg` | `2pi`, `sin(30deg)` → 0.5 |
| Full-width symbols | `（１＋２）×３÷２` → 4.5 |

A query only counts as a calculation when it contains an operator or a function, so ordinary searches such as `e` or `2024` are not affected. Trigonometric functions use radians by default.

Thousands separators (including the full-width `，`) must be in standard positions: 1–3 digits in the first group, then groups of 3. Something like `1,23` is not read as 123; it just shows no result. Results keep about 15 significant digits so floating-point noise doesn't show.

Clipboard entries referenced with `$N` are read as numbers. They may contain thousands separators, spaces, currency symbols (`¥ $ € £`), full-width digits, and a trailing `%` (`15%` reads as 0.15). If a referenced entry isn't a number, no result is shown. The preview shows the expression with the values substituted, so you can check which number each `$N` picked up.

## Keyboard shortcuts

| Key | Action |
|---|---|
| `↵` / `⇧↵` | Paste / copy only |
| `Ctrl ↵` | Open in the default browser (URLs); Solana addresses and transaction signatures open on Solscan |
| `↑ ↓`, `Ctrl J`, `PageUp/PageDown` | Move the selection (scroll the preview when it is open) |
| `Alt 1–9` | Paste the N-th row |
| `→` / `←` | Open / close the side preview (when the search caret is at the end / start) |
| `Tab` | Cycle calculator formats; switch the preview between laid-out and raw text |
| `F2` / click the preview | Edit the text before pasting (`Ctrl ↵` pastes, `Ctrl ⇧ ↵` copies, `Esc` discards) |
| `Ctrl Tab` / `Ctrl ⇧ Tab` | Switch scope: All / Clipboard / Snippets |
| `#` | Filter by scope or snippet tag |
| `Ctrl K` | Action menu |
| `Ctrl P` | Pin / unpin a clip |
| `Ctrl N` | New snippet (starting from the selected clip, if any) |
| `Ctrl E` | Edit snippet |
| `Ctrl D` | Delete (snippets need a second press to confirm) |
| `Ctrl ,` | Settings |
| `Esc` | Clear the search / return to All / go back / hide |

If you reopen the popover within 60 seconds of closing it (configurable; 0 turns this off), the previous search is kept and selected, so typing replaces it. Results are recomputed, so references like `$1` use the latest clipboard.

## Snippet format

Snippets live in `Documents\Stash Snippets` by default, one file per snippet:

```markdown
---
title: Weekly report
tags: [work]
---
Done this week ({{date:MM-dd}}):
- {{cursor}}

Next week: {{plan}}
```

| Variable | Meaning |
|---|---|
| `{{name}}` `{{name=default}}` `{{env:prod\|dev}}` | Filled in in the popover before pasting (text / with a default / choices) |
| `{{date}}` `{{time}}` `{{datetime}}` `{{date:yyyy-MM-dd}}` | Current time; supports `yyyy yy MM dd HH hh mm ss` |
| `{{clipboard}}` | Current clipboard text |
| `{{clipboard:N}}` | The N-th most recent clipboard entry (1 = latest); after copying A, B, C, `{{clipboard:3}}` is A |
| `{{uuid}}` | A random UUID |
| `{{cursor}}` | Where the caret ends up after pasting |

### Editing snippets

Press `Ctrl N` in the popover to create a snippet and `Ctrl E` to edit one. You don't need to remember the syntax above:

![The snippet editor completing a variable after typing {{](images/snippet-editor.png)

- Typing `{{` lists every variable along with its current value (today's date, for example). Pick one to insert it, and use `Tab` to jump between the name and the choices.
- Or write real text first, select the part that should vary (a person's name, say) and press `Ctrl K`. "Fill-in" (填空) turns it into `{{name=Alice}}`, with the selection as the default; just give it a name. You can also turn it into choices, a date, a clipboard reference and so on. With nothing selected, `Ctrl K` inserts at the caret.
- Variables are colored by type. Malformed ones (a variable name containing a space, for example) are marked in red, with the reason on hover; they are pasted as literal text.
- The side panel lists the variables you will be asked for. You can try out values and see the pasted result update below.

### URL snippets (quick search)

A snippet whose content is a URL can be opened in the browser with `Ctrl ↵`. Variables are filled in in the popover first, as usual:

```markdown
---
title: Google search
---
https://www.google.com/search?q={{query}}
```

When opening in the browser, variable values inside the URL are URL-encoded, so non-ASCII text, spaces and `&` come through correctly; when pasting they are left as they are. If the whole URL is a single variable (`{{url}}`), its value is not encoded. Only `http`, `https` and `www.` addresses can be opened.

Usage counts and other statistics are kept in the local database, not in the snippet files, so using a snippet never triggers a sync.

## Settings

Open the settings with `Ctrl ,` or from the tray menu. You can change:

- The launcher hotkey (default `Alt+Space`) and the swap-after-paste hotkey (default `Alt+V`; can be turned off)
- Whether the popover opens next to the text caret, opens the preview automatically, and switches the input method to English
- The snippets folder (default `Documents\Stash Snippets`)
- How many clipboard entries to keep (default 5000; pinned entries don't count and are never removed automatically)
- Programs whose copies are never recorded (1Password, KeePass, KeePassXC and Bitwarden by default)
- How many seconds the last search is kept after closing (default 60)
- Whether to restore the previous clipboard after pasting, and whether to start at login

The tray menu can also pause clipboard recording and open the snippets folder.

## Migrating from Box

This project used to be called Box. The first time Stash starts, it moves `%APPDATA%\com.box.app` and `Documents\Box Snippets` to the new locations, unless those already exist.

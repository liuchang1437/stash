import { invoke } from "@tauri-apps/api/core";

export type Hit = {
  key: string;
  kind: "clip" | "snippet" | "calc";
  title: string;
  preview: string;
  tags: string[];
  source: string | null;
  pinned: boolean;
  useCount: number;
  lastUsedAt: number;
  chars: number;
};

/** paste into the previous window, copy only, or open in the browser */
export type Mode = "paste" | "copy" | "open";

/** Mirrors the backend check: a single http(s) or www. address. */
export function looksLikeUrl(text: string): boolean {
  return /^\s*(https?:\/\/|www\.)\S+\s*$/i.test(text);
}

export type Field = { name: string; default: string; options: string[] };

export type Activation =
  | { status: "done" }
  | { status: "needsInput"; title: string; fields: Field[] };

export type Snippet = { path: string; title: string; tags: string[]; body: string };

export type Config = {
  hotkey: string;
  snippetsDir: string;
  historyLimit: number;
  maxClipChars: number;
  ignoredApps: string[];
  restoreClipboard: boolean;
  keepQuerySeconds: number;
  followCaret: boolean;
  autoPeek: boolean;
  swapHotkey: string;
};

export type Settings = {
  config: Config;
  autostart: boolean;
  defaultSnippetsDir: string;
  hotkeyError: string | null;
};

/** Free space around the caret (or mouse), in CSS pixels. */
export type Space = { above: number; below: number; left: number; right: number };

/** Payload of `launcher-shown`. */
export type Shown = {
  keepQuery: boolean;
  /** "mouse" when the caret of the target app could not be found. */
  anchor: "caret" | "mouse" | "center";
  space: Space;
  /** Executable the paste goes to, e.g. `WindowsTerminal.exe`. */
  targetApp: string | null;
  /** Open the preview panel right away instead of on →. */
  autoPeek: boolean;
};

/** Popover window geometry, CSS pixels (see `placement.rs`). */
export type Layout = { width: number; height: number; cardX: number; margin: number; flip: boolean };

export type CalcRef = { n: number; value: string; source: string | null; lastUsedAt: number };
export type CalcDetail = { result: string; expression: string; refs: CalcRef[] };

/** Piece of a rendered snippet; `field` is the variable it came from. */
export type Segment = { text: string; field: string | null };

export type ManageRequest = { view: "settings" | "edit"; key: string | null; body: string | null };

/** Payload of the post-paste chip. */
export type ChipState = {
  phase: "pasted" | "choosing" | "replaced";
  index: number;
  total: number;
  preview: string;
  canSwap: boolean;
  hotkey: string;
};

export const api = {
  search: (query: string) => invoke<Hit[]>("search", { query }),
  activate: (key: string, mode: Mode, values?: Record<string, string>) =>
    invoke<Activation>("activate", { key, mode, values: values ?? null }),
  activateText: (key: string | null, text: string, mode: Mode) =>
    invoke<void>("activate_text", { key, text, mode }),
  togglePin: (key: string) => invoke<boolean>("toggle_pin", { key }),
  deleteItem: (key: string) => invoke<void>("delete_item", { key }),
  getSnippet: (key: string) => invoke<Snippet>("get_snippet", { key }),
  saveSnippet: (path: string | null, title: string, tags: string[], body: string) =>
    invoke<string>("save_snippet", { path, title, tags, body }),
  previewSnippet: (key: string, values: Record<string, string>) =>
    invoke<Segment[]>("preview_snippet", { key, values }),
  calcDetail: (query: string) => invoke<CalcDetail | null>("calc_detail", { query }),
  openExplorer: (key: string) => invoke<void>("open_explorer", { key }),
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  hide: () => invoke<void>("hide_window"),
  placePopover: (layout: Layout) => invoke<void>("place_popover", { layout }),
  openSnippetsDir: () => invoke<void>("open_snippets_dir"),
  openManage: (view: ManageRequest["view"], key?: string | null, body?: string | null) =>
    invoke<void>("open_manage", { view, key: key ?? null, body: body ?? null }),
  manageRequest: () => invoke<ManageRequest | null>("manage_request"),
  closeManage: () => invoke<void>("close_manage"),
  chipSwap: () => invoke<void>("chip_swap"),
  chipUndo: () => invoke<void>("chip_undo"),
};

/** "1234567.5" -> "1,234,567.5"; scientific notation is left as is. */
export function groupDigits(value: string): string {
  if (/e/i.test(value)) return value;
  const [int, frac] = value.split(".");
  const grouped = int.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
  return frac === undefined ? grouped : `${grouped}.${frac}`;
}

export function relativeTime(unixSeconds: number): string {
  if (!unixSeconds) return "";
  const diff = Date.now() / 1000 - unixSeconds;
  if (diff < 60) return "刚刚";
  if (diff < 3600) return `${Math.floor(diff / 60)} 分钟前`;
  if (diff < 86400) return `${Math.floor(diff / 3600)} 小时前`;
  if (diff < 86400 * 7) return `${Math.floor(diff / 86400)} 天前`;
  const d = new Date(unixSeconds * 1000);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

/** Compact age for list rows: `now`, `7m`, `3h`, `2d`, `09-14`. */
export function shortTime(unixSeconds: number): string {
  if (!unixSeconds) return "";
  const diff = Date.now() / 1000 - unixSeconds;
  if (diff < 60) return "now";
  if (diff < 3600) return `${Math.floor(diff / 60)}m`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h`;
  if (diff < 86400 * 30) return `${Math.floor(diff / 86400)}d`;
  const d = new Date(unixSeconds * 1000);
  return `${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

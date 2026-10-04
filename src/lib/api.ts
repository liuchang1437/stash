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

export type Field ={ name: string; default: string; options: string[] };

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
};

export type Settings = {
  config: Config;
  autostart: boolean;
  defaultSnippetsDir: string;
  hotkeyError: string | null;
};

export const api = {
  search: (query: string) => invoke<Hit[]>("search", { query }),
  activate: (key: string, mode: Mode, values?: Record<string, string>) =>
    invoke<Activation>("activate", { key, mode, values: values ?? null }),
  togglePin: (key: string) => invoke<boolean>("toggle_pin", { key }),
  deleteItem: (key: string) => invoke<void>("delete_item", { key }),
  getSnippet: (key: string) => invoke<Snippet>("get_snippet", { key }),
  saveSnippet: (path: string | null, title: string, tags: string[], body: string) =>
    invoke<string>("save_snippet", { path, title, tags, body }),
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  hide: () => invoke<void>("hide_window"),
  openSnippetsDir: () => invoke<void>("open_snippets_dir"),
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

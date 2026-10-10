// What a clip looks like (table, address, command…), how to show it and
// what it can be turned into. Pure functions; no Tauri calls.

import { looksLikeUrl, type Hit, type Range } from "./api";

export type Kind =
  | "table"
  | "text"
  | "lines"
  | "id"
  | "addr"
  | "sig"
  | "url"
  | "cmd"
  | "num"
  | "path"
  | "snippet"
  | "calc";

/** Short tag shown in the list column. */
export const TAGS: Record<Kind, string> = {
  table: "TBL",
  text: "TXT",
  lines: "TXT",
  id: "ID",
  addr: "ADDR",
  sig: "SIG",
  url: "URL",
  cmd: "CMD",
  num: "NUM",
  path: "PATH",
  snippet: "SNIP",
  calc: "=",
};

export const LABELS: Record<Kind, string> = {
  table: "文本 · 含表格",
  text: "文本",
  lines: "多行文本",
  id: "标识符",
  addr: "Solana 地址",
  sig: "交易签名",
  url: "网址",
  cmd: "命令",
  num: "数字",
  path: "路径",
  snippet: "Snippet",
  calc: "计算",
};

const BASE58 = /^[1-9A-HJ-NP-Za-km-z]+$/;
const COMMAND = /^(\$ |sudo |\.\/|cargo |git |npm |npx |pnpm |yarn |rg |grep |ssh |scp |docker |kubectl |cd |ls |cat |tail |curl |wget |python3? |pip |make |systemctl |journalctl |solana |anchor )/;

export function detectKind(hit: Hit): Kind {
  if (hit.kind === "snippet") return "snippet";
  if (hit.kind === "calc") return "calc";
  return detectTextKind(hit.preview);
}

export function detectTextKind(raw: string): Kind {
  const text = raw.trim();
  if (hasTable(text)) return "table";
  const singleLine = !/\n/.test(text);
  if (singleLine && BASE58.test(text)) {
    if (text.length >= 32 && text.length <= 44) return "addr";
    if (text.length >= 64 && text.length <= 88) return "sig";
  }
  if (looksLikeUrl(text)) return "url";
  if (!singleLine) return "lines";
  if (/^[-+]?(\d[\d,_]*)(\.\d+)?(e[-+]?\d+)?$/i.test(text)) return "num";
  if (/^([A-Za-z]:\\|\\\\|~\/|\/)[^\s]*$/.test(text)) return "path";
  if (COMMAND.test(text) || /\s--?[A-Za-z][\w-]*/.test(text)) return "cmd";
  if (/^[A-Za-z_$][\w$.:\-/]*$/.test(text) && text.length <= 80) return "id";
  return "text";
}

/** Whether a "Solscan" action applies. */
export function isSolana(kind: Kind): boolean {
  return kind === "addr" || kind === "sig";
}

// ---------------------------------------------------------------------------
// Tables in plain text: box drawing (│ ┃ ║), ASCII (+---+ / | a |) and
// Markdown pipe tables.

export type Block = { type: "text"; text: string } | { type: "table"; rows: string[][]; raw: string };

const BORDER_LINE = /^[\s┌┐└┘├┤┬┴┼─━│┃║╔╗╚╝╠╣╦╩╬═╭╮╯╰┏┓┗┛┣┫┳┻╋+\-:|=]+$/;
const TABLE_START = /^[┌├└│┃║╔╠╚╭╰┏┣┗+|]/;
const CELL_SPLIT = /[│┃║|]/;

function isTableLine(line: string): boolean {
  return TABLE_START.test(line.trim());
}

function splitCells(line: string): string[] {
  const cells = line.trim().split(CELL_SPLIT).map((c) => c.trim());
  if (cells.length && cells[0] === "") cells.shift();
  if (cells.length && cells[cells.length - 1] === "") cells.pop();
  return cells;
}

function tableRows(lines: string[]): string[][] | null {
  const rows: string[][] = [];
  for (const line of lines) {
    if (BORDER_LINE.test(line)) continue;
    if (!CELL_SPLIT.test(line)) continue;
    const cells = splitCells(line);
    if (!cells.length) continue;
    const prev = rows[rows.length - 1];
    // A wrapped cell continues on the next line with the first cell empty.
    if (prev && cells[0] === "" && cells.length === prev.length) {
      cells.forEach((c, i) => {
        if (c) prev[i] = prev[i] ? `${prev[i]} ${c}` : c;
      });
      continue;
    }
    rows.push(cells);
  }
  const width = Math.max(0, ...rows.map((r) => r.length));
  if (rows.length < 2 || width < 2) return null;
  return rows.map((r) => [...r, ...Array(width - r.length).fill("")]);
}

export function parseBlocks(text: string): Block[] {
  const lines = text.replace(/\r\n/g, "\n").split("\n");
  const blocks: Block[] = [];
  let prose: string[] = [];
  const flush = () => {
    const t = prose.join("\n").replace(/^\n+|\n+$/g, "");
    if (t) blocks.push({ type: "text", text: t });
    prose = [];
  };
  for (let i = 0; i < lines.length; ) {
    if (isTableLine(lines[i])) {
      let j = i;
      while (j < lines.length && isTableLine(lines[j])) j++;
      const group = lines.slice(i, j);
      const rows = tableRows(group);
      if (rows) {
        flush();
        blocks.push({ type: "table", rows, raw: group.join("\n") });
      } else {
        prose.push(...group);
      }
      i = j;
    } else {
      prose.push(lines[i]);
      i++;
    }
  }
  flush();
  return blocks;
}

export function hasTable(text: string): boolean {
  if (!/[│┃║|]/.test(text)) return false;
  return parseBlocks(text).some((b) => b.type === "table");
}

export function toMarkdown(rows: string[][]): string {
  const esc = (c: string) => c.replace(/\|/g, "\\|");
  const line = (r: string[]) => `| ${r.map(esc).join(" | ")} |`;
  const [head, ...body] = rows;
  return [line(head), `| ${head.map(() => "---").join(" | ")} |`, ...body.map(line)].join("\n");
}

// ---------------------------------------------------------------------------
// "Paste as…" transformations

export type Transform = { id: string; label: string; apply: (text: string) => string };

const abbreviate = (t: string) => {
  const s = t.trim();
  return s.length > 12 ? `${s.slice(0, 4)}…${s.slice(-4)}` : s;
};

export function transformsFor(kind: Kind): Transform[] {
  const list: Transform[] = [];
  if (kind === "addr" || kind === "sig") {
    list.push({ id: "abbr", label: "缩写", apply: abbreviate });
  }
  if (kind === "addr") {
    list.push({ id: "pubkey", label: "Rust 常量", apply: (t) => `pubkey!("${t.trim()}")` });
  }
  if (kind === "table") {
    list.push({
      id: "markdown",
      label: "Markdown 表格",
      apply: (t) =>
        parseBlocks(t)
          .filter((b) => b.type === "table")
          .map((b) => (b.type === "table" ? toMarkdown(b.rows) : ""))
          .join("\n\n"),
    });
  }
  list.push({ id: "quote", label: "加引号", apply: (t) => `"${t.trim().replace(/"/g, '\\"')}"` });
  if (kind === "lines" || kind === "table" || kind === "text" || kind === "cmd") {
    list.push({ id: "oneline", label: "单行", apply: (t) => t.replace(/\s*\r?\n\s*/g, " ").trim() });
    list.push({ id: "json", label: "JSON 字符串", apply: (t) => JSON.stringify(t) });
  }
  list.push({ id: "trim", label: "去掉首尾空白", apply: (t) => t.trim() });
  return list;
}

// ---------------------------------------------------------------------------
// Display helpers

const APP_NAMES: Record<string, [string, string]> = {
  windowsterminal: ["Windows Terminal", "wt"],
  openconsole: ["终端", "term"],
  msedge: ["Edge", "edge"],
  chrome: ["Chrome", "chrome"],
  firefox: ["Firefox", "firefox"],
  feishu: ["飞书", "飞书"],
  lark: ["Lark", "lark"],
  code: ["VS Code", "code"],
  cursor: ["Cursor", "cursor"],
  explorer: ["资源管理器", "explorer"],
  weixin: ["微信", "微信"],
  wechat: ["微信", "微信"],
  notepad: ["记事本", "notepad"],
  winword: ["Word", "word"],
  excel: ["Excel", "excel"],
  powerpnt: ["PowerPoint", "ppt"],
  dingtalk: ["钉钉", "钉钉"],
  telegram: ["Telegram", "tg"],
  slack: ["Slack", "slack"],
  discord: ["Discord", "discord"],
  notion: ["Notion", "notion"],
  obsidian: ["Obsidian", "obsidian"],
  "deskflow-core": ["Deskflow", "deskflow"],
  deskflow: ["Deskflow", "deskflow"],
};

function exeStem(exe: string): string {
  return exe.replace(/\.exe$/i, "");
}

/** Friendly name for an executable, e.g. `msedge.exe` -> `Edge`. */
export function appName(exe: string | null): string {
  if (!exe) return "前一个窗口";
  const stem = exeStem(exe);
  return APP_NAMES[stem.toLowerCase()]?.[0] ?? stem;
}

/** Very short name for list rows, e.g. `WindowsTerminal.exe` -> `wt`. */
export function shortApp(exe: string | null): string {
  if (!exe) return "";
  const stem = exeStem(exe);
  return APP_NAMES[stem.toLowerCase()]?.[1] ?? stem.toLowerCase().slice(0, 10);
}

/** A piece of text; `hit` when it matched the query. */
export type Piece = { text: string; hit: boolean };

/** Splits `text` at the matched ranges the backend computed (sorted). */
export function splitMarks(text: string, marks: Range[]): Piece[] {
  const out: Piece[] = [];
  let at = 0;
  for (const [start, end] of marks) {
    if (start > at) out.push({ text: text.slice(at, start), hit: false });
    out.push({ text: text.slice(start, end), hit: true });
    at = end;
  }
  if (at < text.length || !out.length) out.push({ text: text.slice(at), hit: false });
  return out;
}

/** Splits `text` around every occurrence of `terms`, ignoring case. */
export function markTerms(text: string, terms: string[]): Piece[] {
  if (!terms.length) return [{ text, hit: false }];
  const escaped = [...terms].sort((a, b) => b.length - a.length).map((t) => t.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
  const out: Piece[] = [];
  let at = 0;
  for (const m of text.matchAll(new RegExp(escaped.join("|"), "gi"))) {
    if (m.index! > at) out.push({ text: text.slice(at, m.index), hit: false });
    out.push({ text: m[0], hit: true });
    at = m.index! + m[0].length;
  }
  if (at < text.length || !out.length) out.push({ text: text.slice(at), hit: false });
  return out;
}

export function lineCount(text: string): number {
  return text.replace(/\r\n/g, "\n").replace(/\n+$/, "").split("\n").length;
}

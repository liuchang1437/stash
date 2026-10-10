// Snippet template syntax on the UI side: which `{{…}}` are variables of
// what kind, for highlighting in the editor and the preview, and the
// variables the editor offers to insert. Rendering stays in Rust
// (`template.rs`); `tokens` mirrors its `parse`/`parse_var`, so keep the
// two in step.

import type { Segment } from "./api";

export type TokenKind = "input" | "auto" | "cursor" | "invalid";

/** A `{{…}}` in a template, or an unterminated `{{` (`invalid`, just the braces). */
export type Token = { from: number; to: number; kind: TokenKind; name: string; problem?: string };

const AUTO = new Set(["date", "time", "datetime", "clipboard", "uuid"]);

/** Finds the variables in `src` the way `template::parse` does. */
export function tokens(src: string): Token[] {
  const out: Token[] = [];
  let pos = 0;
  for (;;) {
    const from = src.indexOf("{{", pos);
    if (from < 0) break;
    const end = src.indexOf("}}", from + 2);
    if (end < 0) {
      out.push({ from, to: from + 2, kind: "invalid", name: "", problem: "缺少 }}，会按原文粘贴" });
      break;
    }
    out.push({ from, to: end + 2, ...classify(src.slice(from + 2, end)) });
    pos = end + 2;
  }
  return out;
}

function classify(raw: string): Omit<Token, "from" | "to"> {
  const inner = raw.trim();
  const at = inner.search(/[:=]/);
  const name = (at < 0 ? inner : inner.slice(0, at)).trim();
  const sep = at < 0 ? "" : inner[at];
  const arg = at < 0 ? "" : inner.slice(at + 1).trim();
  if (!name) return { kind: "invalid", name, problem: "缺少变量名，会按原文粘贴" };
  if (/\s/.test(name)) return { kind: "invalid", name, problem: "变量名不能有空格，会按原文粘贴" };
  // Rust's usize parsing also takes a leading `+`.
  if (name === "clipboard" && sep === ":" && !(/^\+?\d+$/.test(arg) && Number(arg) > 0)) {
    return { kind: "invalid", name, problem: "clipboard: 后面要写正整数，1 是最近一条" };
  }
  if (name === "cursor") return { kind: "cursor", name };
  if (AUTO.has(name)) return { kind: "auto", name };
  return { kind: "input", name };
}

/** Splits a template into plain text and its `{{…}}` tokens. */
export function templatePieces(src: string): { text: string; token: Token | null }[] {
  const out: { text: string; token: Token | null }[] = [];
  let at = 0;
  for (const token of tokens(src)) {
    if (token.from > at) out.push({ text: src.slice(at, token.from), token: null });
    out.push({ text: src.slice(token.from, token.to), token });
    at = token.to;
  }
  if (at < src.length) out.push({ text: src.slice(at), token: null });
  return out;
}

/** Tooltip for a rendered variable. */
export function segmentTitle(seg: Segment): string {
  if (seg.kind === "input") return `填写：${seg.name}`;
  if (seg.kind === "cursor") return "粘贴后光标停在这里";
  if (seg.name === "date") return "当前时间，粘贴时取值";
  if (seg.name === "clipboard") return "当前剪贴板，粘贴时取值";
  if (seg.name === "uuid") return "随机 UUID，粘贴时生成";
  const n = seg.name?.match(/^clipboard:(\d+)$/)?.[1];
  return n ? `剪贴板历史第 ${n} 条（1 = 最近）` : "";
}

/** What an empty variable shows in a preview. */
export function segmentPlaceholder(seg: Segment): string {
  if (seg.kind === "input") return `‹${seg.name}›`;
  if (seg.name === "uuid") return "‹UUID›";
  if (seg.name?.startsWith("clipboard")) return "‹剪贴板为空›";
  return "‹空›";
}

// ---------------------------------------------------------------------------
// Variables the editor offers

/**
 * `template` is a CodeMirror snippet: `${name}` marks a field Tab moves
 * through. `selected` is the text selected when inserting (already escaped
 * for the snippet syntax); 填空 and 选项 turn it into the default.
 */
export type Variable = {
  label: string;
  syntax: string;
  info: string;
  /** Shown next to the label: a live example of what it inserts. */
  example?: () => string;
  /** Only offered without a selection. */
  plainOnly?: boolean;
  template: (selected: string) => string;
};

export const VARIABLES: Variable[] = [
  {
    label: "填空",
    syntax: "{{名称}}",
    info: "粘贴前在浮层里填写。选中文字再插入，选中的文字就是默认值",
    template: (s) => (s ? `{{\${名称}=${s}}}` : "{{${名称}}}"),
  },
  {
    label: "带默认值",
    syntax: "{{名称=默认值}}",
    info: "粘贴前填写，不改就用默认值",
    plainOnly: true,
    template: () => "{{${名称}=${默认值}}}",
  },
  {
    label: "选项",
    syntax: "{{名称:A|B}}",
    info: "粘贴前从几个选项里选一个，第一个是默认",
    template: (s) => (s ? `{{\${名称}:${s}|\${选项2}}}` : "{{${名称}:${选项1}|${选项2}}}"),
  },
  {
    label: "日期",
    syntax: "{{date}}",
    info: "粘贴时的日期",
    example: () => formatDate("yyyy-MM-dd"),
    template: () => "{{date}}",
  },
  {
    label: "时间",
    syntax: "{{time}}",
    info: "粘贴时的时间",
    example: () => formatDate("HH:mm"),
    template: () => "{{time}}",
  },
  {
    label: "自定义日期格式",
    syntax: "{{date:格式}}",
    info: "格式里可以用 yyyy yy MM dd HH hh mm ss",
    example: () => formatDate("yyyy年MM月dd日"),
    template: () => "{{date:${yyyy年MM月dd日}}}",
  },
  {
    label: "剪贴板",
    syntax: "{{clipboard}}",
    info: "粘贴时剪贴板里的文字",
    template: () => "{{clipboard}}",
  },
  {
    label: "剪贴板历史",
    syntax: "{{clipboard:N}}",
    info: "剪贴板历史倒数第 N 条，1 是最近一条",
    template: () => "{{clipboard:${2}}}",
  },
  {
    label: "光标位置",
    syntax: "{{cursor}}",
    info: "粘贴后光标停在这里",
    template: () => "{{cursor}}",
  },
  {
    label: "UUID",
    syntax: "{{uuid}}",
    info: "随机 UUID，每次粘贴都不一样",
    template: () => "{{uuid}}",
  },
];

/** Escapes text so a CodeMirror snippet inserts it literally. */
export function escapeSnippet(text: string): string {
  return text.replace(/[{}]/g, "\\$&");
}

/** `yyyy-MM-dd HH:mm` style patterns, as `template.rs` understands them. */
export function formatDate(pattern: string, d = new Date()): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  const values: Record<string, string> = {
    yyyy: String(d.getFullYear()),
    yy: pad(d.getFullYear() % 100),
    MM: pad(d.getMonth() + 1),
    dd: pad(d.getDate()),
    HH: pad(d.getHours()),
    hh: pad(d.getHours() % 12 || 12),
    mm: pad(d.getMinutes()),
    ss: pad(d.getSeconds()),
  };
  return pattern.replace(/yyyy|yy|MM|dd|HH|hh|mm|ss/g, (t) => values[t]);
}

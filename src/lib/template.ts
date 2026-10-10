// Snippet template syntax on the UI side: which `{{…}}` are variables of
// what kind, for highlighting in the editor and the preview, and the
// variables the editor offers to insert. Rendering stays in Rust
// (`template.rs`); `tokens` mirrors its `parse`/`parse_var`, so keep the
// two in step.

import type { Segment } from "./api";
import { t } from "./i18n.svelte";

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
      out.push({ from, to: from + 2, kind: "invalid", name: "", problem: t.template.missingClose });
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
  if (!name) return { kind: "invalid", name, problem: t.template.missingName };
  if (/\s/.test(name)) return { kind: "invalid", name, problem: t.template.spaceInName };
  // Rust's usize parsing also takes a leading `+`.
  if (name === "clipboard" && sep === ":" && !(/^\+?\d+$/.test(arg) && Number(arg) > 0)) {
    return { kind: "invalid", name, problem: t.template.badClipboardIndex };
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
  if (seg.kind === "input") return t.template.fill(seg.name ?? "");
  if (seg.kind === "cursor") return t.template.cursor;
  if (seg.name === "date") return t.template.date;
  if (seg.name === "clipboard") return t.template.clipboard;
  if (seg.name === "uuid") return t.template.uuid;
  const n = seg.name?.match(/^clipboard:(\d+)$/)?.[1];
  return n ? t.template.history(n) : "";
}

/** What an empty variable shows in a preview. */
export function segmentPlaceholder(seg: Segment): string {
  if (seg.kind === "input") return `‹${seg.name}›`;
  if (seg.name === "uuid") return "‹UUID›";
  if (seg.name?.startsWith("clipboard")) return t.template.emptyClipboard;
  return t.template.empty;
}

// ---------------------------------------------------------------------------
// Variables the editor offers

/**
 * `template` is a CodeMirror snippet: `${name}` marks a field Tab moves
 * through. `selected` is the text selected when inserting (already escaped
 * for the snippet syntax); fill-in and choice turn it into the default.
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

/** A CodeMirror snippet field with `text` as its placeholder. */
const field = (text: string) => "${" + text + "}";

/** The variables the editor offers, in the UI language. */
export function variables(): Variable[] {
  const { words, variables: v, dateExample } = t.template;
  const name = field(words.name);
  return [
    {
      ...v.fill,
      syntax: `{{${words.name}}}`,
      template: (s) => (s ? `{{${name}=${s}}}` : `{{${name}}}`),
    },
    {
      ...v.withDefault,
      syntax: `{{${words.name}=${words.default}}}`,
      plainOnly: true,
      template: () => `{{${name}=${field(words.default)}}}`,
    },
    {
      ...v.choice,
      syntax: `{{${words.name}:A|B}}`,
      template: (s) =>
        s
          ? `{{${name}:${s}|${field(words.option + "2")}}}`
          : `{{${name}:${field(words.option + "1")}|${field(words.option + "2")}}}`,
    },
    {
      ...v.date,
      syntax: "{{date}}",
      example: () => formatDate("yyyy-MM-dd"),
      template: () => "{{date}}",
    },
    {
      ...v.time,
      syntax: "{{time}}",
      example: () => formatDate("HH:mm"),
      template: () => "{{time}}",
    },
    {
      ...v.dateFormat,
      syntax: `{{date:${words.format}}}`,
      example: () => formatDate(dateExample),
      template: () => `{{date:${field(dateExample)}}}`,
    },
    {
      ...v.clipboard,
      syntax: "{{clipboard}}",
      template: () => "{{clipboard}}",
    },
    {
      ...v.history,
      syntax: "{{clipboard:N}}",
      template: () => `{{clipboard:${field("2")}}}`,
    },
    {
      ...v.cursor,
      syntax: "{{cursor}}",
      template: () => "{{cursor}}",
    },
    {
      ...v.uuid,
      syntax: "{{uuid}}",
      template: () => "{{uuid}}",
    },
  ];
}

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

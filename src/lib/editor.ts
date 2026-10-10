// CodeMirror for the snippet editor: variables are highlighted, wrong ones
// flagged, `{{` offers the variables, and Ctrl+K opens the variable menu
// (SnippetEditor.svelte). SnippetEditor imports this module dynamically, so
// the popover never loads CodeMirror.

import {
  autocompletion,
  snippet,
  type Completion,
  type CompletionContext,
  type CompletionResult,
} from "@codemirror/autocomplete";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { linter, type Diagnostic } from "@codemirror/lint";
import { EditorState, RangeSetBuilder } from "@codemirror/state";
import {
  Decoration,
  drawSelection,
  EditorView,
  keymap,
  placeholder,
  ViewPlugin,
  type DecorationSet,
  type ViewUpdate,
} from "@codemirror/view";
import { escapeSnippet, tokens, VARIABLES, type TokenKind, type Variable } from "./template";

export type EditorOptions = {
  doc: string;
  onChange: (doc: string) => void;
  /** Ctrl+K: insert a variable, or turn the selection into one. */
  onMenu: () => void;
};

export function createEditor(parent: HTMLElement, options: EditorOptions): EditorView {
  return new EditorView({
    parent,
    state: EditorState.create({
      doc: options.doc,
      extensions: [
        history(),
        drawSelection(),
        EditorView.lineWrapping,
        placeholder("内容。输入 {{ 插入变量"),
        highlightVariables,
        linter(lintVariables, { delay: 300 }),
        autocompletion({ override: [completeVariables], icons: false }),
        keymap.of([
          {
            key: "Mod-k",
            run: () => {
              options.onMenu();
              return true;
            },
          },
          ...historyKeymap,
          ...defaultKeymap,
        ]),
        EditorView.updateListener.of((u) => {
          if (u.docChanged) options.onChange(u.state.doc.toString());
        }),
        theme,
      ],
    }),
  });
}

/** Inserts `variable` at the selection; selected text becomes its default (填空, 选项). */
export function insertVariable(view: EditorView, variable: Variable) {
  const { from, to } = view.state.selection.main;
  const selected = escapeSnippet(view.state.sliceDoc(from, to));
  snippet(variable.template(selected))(view, null, from, to);
  view.focus();
}

// ---------------------------------------------------------------------------
// Highlighting and problems

const MARKS: Record<TokenKind, Decoration> = {
  input: Decoration.mark({ class: "cm-var cm-var-input" }),
  auto: Decoration.mark({ class: "cm-var cm-var-auto" }),
  cursor: Decoration.mark({ class: "cm-var cm-var-cursor" }),
  invalid: Decoration.mark({ class: "cm-var cm-var-invalid" }),
};

function decorate(state: EditorState): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  for (const t of tokens(state.doc.toString())) builder.add(t.from, t.to, MARKS[t.kind]);
  return builder.finish();
}

// Snippets are short, so the whole document is re-scanned on each change.
const highlightVariables = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = decorate(view.state);
    }
    update(u: ViewUpdate) {
      if (u.docChanged) this.decorations = decorate(u.state);
    }
  },
  { decorations: (p) => p.decorations },
);

function lintVariables(view: EditorView): Diagnostic[] {
  return tokens(view.state.doc.toString())
    .filter((t) => t.problem)
    .map((t) => ({ from: t.from, to: t.to, severity: "warning", message: t.problem! }));
}

// ---------------------------------------------------------------------------
// `{{` completion

function completeVariables(ctx: CompletionContext): CompletionResult | null {
  const typed = ctx.matchBefore(/\{\{[^{}]*/);
  // Editing inside an existing `{{…}}` is left alone.
  const inside = /^[^{}\n]*\}\}/.test(ctx.state.sliceDoc(ctx.pos, ctx.pos + 200));
  if (typed && !inside) return { from: typed.from, options: completions(), validFor: /^\{\{[^{}]*$/ };
  // Ctrl+Space anywhere.
  if (ctx.explicit) return { from: ctx.pos, options: completions() };
  return null;
}

function completions(): Completion[] {
  return VARIABLES.map((v, i) => ({
    label: v.syntax,
    detail: v.example ? `${v.label} · ${v.example()}` : v.label,
    info: v.info,
    // Keep the menu in catalogue order.
    boost: -i,
    apply: snippet(v.template("")),
  }));
}

// ---------------------------------------------------------------------------
// Theme: the app's dark terminal look (app.css variables)

const theme = EditorView.theme(
  {
    "&": { height: "100%", color: "var(--text)", backgroundColor: "var(--bg)", fontSize: "13px" },
    "&.cm-focused": { outline: "none" },
    ".cm-scroller": { fontFamily: "var(--mono)", lineHeight: "1.6" },
    ".cm-content": { padding: "12px 0", caretColor: "var(--accent)" },
    ".cm-line": { padding: "0 16px" },
    ".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--accent)" },
    "&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground": {
      backgroundColor: "rgba(108, 182, 255, 0.22)",
    },
    ".cm-placeholder": { color: "var(--text-faint)" },
    ".cm-var": { borderRadius: "3px" },
    ".cm-var-input": { color: "var(--green)", backgroundColor: "rgba(63, 209, 122, 0.1)" },
    ".cm-var-auto": { color: "var(--blue)", backgroundColor: "rgba(108, 182, 255, 0.1)" },
    ".cm-var-cursor": { color: "var(--accent)", backgroundColor: "rgba(245, 165, 36, 0.12)" },
    ".cm-var-invalid": { color: "var(--danger)" },
    ".cm-snippetField": { backgroundColor: "rgba(245, 165, 36, 0.18)" },
    ".cm-tooltip": {
      border: "1px solid var(--border)",
      borderRadius: "6px",
      backgroundColor: "var(--bg-raised)",
      color: "var(--text)",
      fontFamily: "var(--mono)",
      fontSize: "12px",
      overflow: "hidden",
    },
    ".cm-tooltip.cm-tooltip-autocomplete > ul": { fontFamily: "var(--mono)", maxHeight: "18em" },
    ".cm-tooltip.cm-tooltip-autocomplete > ul > li": { padding: "3px 10px", lineHeight: "1.6" },
    ".cm-tooltip.cm-tooltip-autocomplete > ul > li[aria-selected]": {
      backgroundColor: "var(--bg-selected)",
      color: "var(--accent-soft)",
    },
    ".cm-completionLabel": { color: "inherit" },
    ".cm-completionMatchedText": { textDecoration: "none", color: "var(--accent)" },
    ".cm-completionDetail": { marginLeft: "14px", fontStyle: "normal", color: "var(--text-muted)" },
    ".cm-tooltip.cm-completionInfo": { padding: "6px 10px", maxWidth: "240px", color: "var(--text-muted)" },
    ".cm-diagnostic": { padding: "5px 10px" },
    ".cm-diagnostic-warning": { borderLeft: "3px solid var(--danger)" },
  },
  { dark: true },
);

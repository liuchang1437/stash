<script lang="ts">
  import { tick } from "svelte";
  import { relativeTime, type Hit, type Segment } from "./api";
  import { appName, LABELS, lineCount, markTerms, parseBlocks, type Kind } from "./kinds";
  import Rendered from "./Rendered.svelte";
  import { templatePieces } from "./template";

  type Props = {
    hit: Hit;
    kind: Kind;
    /** Tables: the source instead of the table. Snippets: the template instead of the result. */
    raw: boolean;
    onToggle: () => void;
    /** A snippet with variables: what it pastes. */
    rendered?: Segment[] | null;
    /** While its variables are filled in: the field being edited. */
    focus?: string | null;
    /** Being edited before pasting: the text, and the selection it starts with (UTF-16). */
    draft?: { text: string; caret: [number, number] } | null;
    onDraft?: (text: string) => void;
    /**
     * Starts editing; absent where editing is not offered. After a click,
     * `pick` finds what was clicked or selected in the text to edit.
     */
    onEdit?: (pick?: Pick) => void;
  };
  type Pick = (text: string) => [number, number];
  let { hit, kind, raw, onToggle, rendered = null, focus = null, draft = null, onDraft, onEdit }: Props = $props();

  let body: HTMLElement;

  const blocks = $derived(kind === "snippet" ? [] : parseBlocks(hit.preview));
  const hasTable = $derived(blocks.some((b) => b.type === "table"));
  const truncated = $derived(hit.chars > hit.preview.length);
  const filling = $derived(focus !== null);
  const showResult = $derived(!!rendered && (filling || !raw));
  const editing = $derived(draft !== null);

  let editor: HTMLTextAreaElement | undefined = $state();
  // Only the start position, so typing doesn't re-run the effect below.
  const startCaret = $derived(draft?.caret ?? null);

  // Editing starts with the caret where it was clicked, or where pasting
  // would leave it. The panel is drawn hidden until measured, and a hidden
  // textarea can't take focus, so keep trying for a few frames.
  $effect(() => {
    if (!editor || startCaret === null) return;
    const el = editor;
    const [start, end] = startCaret;
    let frames = 0;
    const place = () => {
      // Focusing scrolls the selection into view.
      el.setSelectionRange(start, end);
      el.focus();
      if (document.activeElement !== el && ++frames < 20) requestAnimationFrame(place);
    };
    place();
  });

  // ---------------------------------------------------------------------------
  // Clicking into the preview edits it, with the caret (or the selection made
  // by dragging) at the same place in the text.

  /** Shown but not part of the text: placeholders of empty values, the truncation note. */
  const NOT_TEXT = ".empty, .more";

  /**
   * Whether what's shown can be mapped onto the text to edit. A snippet's
   * template isn't its result, so clicking there puts the caret at the default.
   */
  const mappable = $derived(kind !== "snippet" || !rendered || showResult);

  let pressedInside = false;

  /** The preview's text before a point in it, as shown. */
  function shownBefore(node: Node, offset: number): string {
    const point = document.createRange();
    point.setStart(node, offset);
    const walker = document.createTreeWalker(body, NodeFilter.SHOW_TEXT);
    let out = "";
    for (let t = walker.nextNode(); t; t = walker.nextNode()) {
      if (t === node) return out + (t.textContent ?? "").slice(0, offset);
      // A text node that starts after the point is past it.
      if (point.comparePoint(t, 0) > 0) break;
      if (!t.parentElement?.closest(NOT_TEXT)) out += t.textContent ?? "";
    }
    return out;
  }

  /**
   * Where `shown` ends in `text`. The preview drops blank lines between
   * blocks, lays tables out and leaves UUIDs out, so this matches the shown
   * characters in order, ignoring whitespace, then takes the whitespace the
   * point was after.
   */
  function align(shown: string, text: string): number {
    let at = 0;
    for (const c of shown) {
      if (/\s/.test(c)) continue;
      const i = text.indexOf(c, at);
      if (i >= 0) at = i + c.length;
    }
    const trailing = /\s*$/.exec(shown)![0].replace(/\r/g, "").length;
    for (let n = 0; n < trailing && at < text.length && /\s/.test(text[at]); n++) at++;
    return at;
  }

  function bodyMouseup(e: MouseEvent) {
    const pressed = pressedInside;
    pressedInside = false;
    if (!pressed || !onEdit || editing || e.button !== 0) return;
    // Not on the scrollbars.
    if (e.target === body && (e.offsetX > body.clientWidth || e.offsetY > body.clientHeight)) return;
    if (!mappable) return onEdit();
    // A click leaves a caret selection; a drag a range.
    const sel = window.getSelection();
    let range = sel?.rangeCount ? sel.getRangeAt(0) : null;
    if (!range || !body.contains(range.commonAncestorContainer)) range = document.caretRangeFromPoint(e.clientX, e.clientY);
    if (!range || !body.contains(range.commonAncestorContainer)) return onEdit();
    const before = [shownBefore(range.startContainer, range.startOffset), shownBefore(range.endContainer, range.endOffset)];
    onEdit((text) => [align(before[0], text), align(before[1], text)]);
  }

  /** Tab indents instead of leaving the editor. */
  function editorKeydown(e: KeyboardEvent) {
    if (e.key === "Tab" && !e.ctrlKey && !e.shiftKey && !e.altKey) {
      e.preventDefault();
      document.execCommand("insertText", false, "\t");
    }
  }

  export function scroll(delta: number) {
    body?.scrollBy({ top: delta });
  }

  /** Where `el` is inside the scrolling body. */
  function offsetOf(el: Element): number {
    return el.getBoundingClientRect().top - body.getBoundingClientRect().top + body.scrollTop;
  }

  // A new item (or view) starts at its first match, or at the top.
  $effect(() => {
    hit.key;
    hit.terms;
    raw;
    if (filling || editing) return;
    tick().then(() => {
      if (!body) return;
      const mark = body.querySelector("mark");
      body.scrollTop = mark ? Math.max(0, offsetOf(mark) - 40) : 0;
    });
  });

  // While filling in, keep the field being edited in view.
  $effect(() => {
    if (!focus) return;
    tick().then(() => {
      const el = body?.querySelector(".active");
      if (!el) return;
      const top = offsetOf(el);
      const height = el.getBoundingClientRect().height;
      if (top < body.scrollTop || top + height > body.scrollTop + body.clientHeight) {
        body.scrollTop = Math.max(0, top - 40);
      }
    });
  });
</script>

{#snippet marked(text: string)}{#each markTerms(text, hit.terms) as p}{#if p.hit}<mark>{p.text}</mark>{:else}{p.text}{/if}{/each}{/snippet}

<section class="peek">
  <header>
    <span class="label">{draft ? "修改后粘贴" : filling ? "将粘贴" : LABELS[kind]}</span>
    {#if draft}
      <span class="muted">{draft.text.length.toLocaleString()} 字 · {lineCount(draft.text)} 行 · 原文不变</span>
    {:else if !filling}
      <span class="muted">{hit.chars.toLocaleString()} 字 · {lineCount(hit.preview)} 行</span>
    {/if}
    <span class="spacer"></span>
    {#if !editing && (hasTable || (rendered && !filling))}
      <div class="seg" role="group" aria-label="显示方式">
        <button class:on={!raw} onclick={() => raw && onToggle()}>{hasTable ? "排版" : "结果"}</button>
        <button class:on={raw} onclick={() => !raw && onToggle()}>{hasTable ? "原文" : "模板"}</button>
      </div>
      <kbd>Tab</kbd>
    {/if}
    {#if onEdit && !editing}
      <button class="edit" title="临时修改后再粘贴，原文不变；也可以直接点正文" onclick={() => onEdit()}>修改</button>
      <kbd>F2</kbd>
    {/if}
  </header>

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="body"
    class:editing
    class:editable={!!onEdit}
    bind:this={body}
    onmousedown={() => (pressedInside = true)}
    onmouseup={bodyMouseup}
  >
    {#if draft}
      <textarea
        bind:this={editor}
        value={draft.text}
        oninput={(e) => onDraft?.(e.currentTarget.value)}
        onkeydown={editorKeydown}
        spellcheck="false"
        aria-label="修改后粘贴的文字"
      ></textarea>
    {:else if showResult && rendered}
      <pre class="template"><Rendered segments={rendered} {focus} terms={filling ? [] : hit.terms} /></pre>
    {:else if kind === "snippet"}
      <pre class="template">{#each templatePieces(hit.preview) as piece}{#if piece.token}<span
              class="var {piece.token.kind}"
              title={piece.token.problem ?? ""}>{@render marked(piece.text)}</span
            >{:else}{@render marked(piece.text)}{/if}{/each}</pre>
    {:else}
      {#each blocks as block}
        {#if block.type === "text"}
          <pre class="text">{@render marked(block.text)}</pre>
        {:else if raw}
          <pre class="raw-table">{@render marked(block.raw)}</pre>
        {:else}
          <div class="table-wrap">
            <table>
              <thead>
                <tr>
                  {#each block.rows[0] as cell}<th>{@render marked(cell)}</th>{/each}
                </tr>
              </thead>
              <tbody>
                {#each block.rows.slice(1) as row}
                  <tr>
                    {#each row as cell}<td>{@render marked(cell)}</td>{/each}
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        {/if}
      {/each}
      {#if truncated}<p class="muted more">… 只显示前 {hit.preview.length.toLocaleString()} 字</p>{/if}
    {/if}
  </div>

  <footer>
    {#if kind === "snippet"}
      <span>{hit.key.slice(2)}</span>
      {#if hit.tags.length}<span>{hit.tags.map((t) => "#" + t).join(" ")}</span>{/if}
    {:else}
      <span>src=<b>{appName(hit.source)}</b></span>
      <span>copied=<b>{relativeTime(hit.lastUsedAt) || "?"}</b></span>
    {/if}
    <span>uses=<b>{hit.useCount}</b></span>
    <span class="spacer"></span>
    {#if draft}<span class="muted">Esc 取消修改</span>{:else if !filling}<span class="muted">← 收起</span>{/if}
  </footer>
</section>

<style>
  .peek {
    width: 480px;
    max-height: 460px;
    display: flex;
    flex-direction: column;
    border-radius: 10px;
    border: 1px solid var(--border);
    background: var(--bg);
    box-shadow: var(--shadow);
    overflow: hidden;
  }

  header {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 36px;
    flex: none;
    padding: 0 10px 0 14px;
    border-bottom: 1px solid var(--border-soft);
    font-size: 12px;
  }

  .label {
    color: var(--accent);
  }

  .muted {
    color: var(--text-muted);
  }

  .spacer {
    flex: 1;
  }

  .seg {
    display: flex;
    gap: 2px;
    padding: 2px;
    border-radius: 4px;
    background: var(--bg-subtle);
  }

  .seg button {
    border: none;
    background: transparent;
    padding: 2px 8px;
    border-radius: 3px;
    font-size: 11.5px;
    color: var(--text-muted);
    cursor: pointer;
  }

  .seg button.on {
    background: var(--accent);
    color: var(--accent-text);
  }

  .body {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 10px 14px 12px;
    user-select: text;
  }

  .body.editable {
    cursor: text;
  }

  .edit {
    border: 1px solid var(--border);
    background: transparent;
    padding: 1px 8px;
    border-radius: 4px;
    font-size: 11.5px;
    color: var(--text-muted);
    cursor: pointer;
  }

  .edit:hover {
    color: var(--text);
    border-color: var(--accent);
  }

  .body.editing {
    display: flex;
    flex-direction: column;
    overflow: hidden;
    padding: 0;
  }

  textarea {
    flex: 1 1 auto;
    min-height: 5lh;
    /* As tall as the text up to the panel's max-height, then it scrolls. */
    field-sizing: content;
    resize: none;
    border: none;
    outline: none;
    margin: 0;
    padding: 10px 14px 12px;
    background: transparent;
    color: var(--text);
    caret-color: var(--accent);
    font-family: var(--mono);
    font-size: 12.5px;
    line-height: 1.7;
    tab-size: 4;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  pre {
    margin: 0 0 10px;
    font-family: var(--mono);
    font-size: 12.5px;
    line-height: 1.7;
  }

  pre.text,
  pre.template {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  pre.raw-table {
    white-space: pre;
    overflow-x: auto;
    padding: 8px 10px;
    border-radius: 6px;
    background: var(--bg-subtle);
  }

  /* Template view: variables coloured like the result and the editor. */
  .var {
    border-radius: 3px;
  }

  .var.input {
    color: var(--green);
    background: rgba(63, 209, 122, 0.1);
  }

  .var.auto {
    color: var(--blue);
    background: rgba(108, 182, 255, 0.1);
  }

  .var.cursor {
    color: var(--accent);
    background: rgba(245, 165, 36, 0.12);
  }

  .var.invalid {
    color: var(--danger);
    text-decoration: underline wavy;
    text-underline-offset: 3px;
  }

  .table-wrap {
    margin: 0 0 10px;
    overflow-x: auto;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
  }

  th {
    text-align: left;
    font-weight: 500;
    color: var(--text-muted);
    padding: 5px 8px;
    border-bottom: 1px solid var(--border);
  }

  td {
    padding: 5px 8px;
    border-bottom: 1px solid var(--border-soft);
    vertical-align: top;
  }

  .more {
    margin: 4px 0 0;
    font-size: 11.5px;
  }

  footer {
    display: flex;
    gap: 16px;
    align-items: center;
    height: 30px;
    flex: none;
    padding: 0 14px;
    border-top: 1px solid var(--border-soft);
    font-size: 11px;
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
  }

  footer b {
    font-weight: 400;
    color: var(--text);
  }
</style>

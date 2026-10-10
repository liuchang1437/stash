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
  };
  let { hit, kind, raw, onToggle, rendered = null, focus = null }: Props = $props();

  let body: HTMLElement;

  const blocks = $derived(kind === "snippet" ? [] : parseBlocks(hit.preview));
  const hasTable = $derived(blocks.some((b) => b.type === "table"));
  const truncated = $derived(hit.chars > hit.preview.length);
  const filling = $derived(focus !== null);
  const showResult = $derived(!!rendered && (filling || !raw));

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
    if (filling) return;
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
    <span class="label">{filling ? "将粘贴" : LABELS[kind]}</span>
    {#if !filling}
      <span class="muted">{hit.chars.toLocaleString()} 字 · {lineCount(hit.preview)} 行</span>
    {/if}
    <span class="spacer"></span>
    {#if hasTable || (rendered && !filling)}
      <div class="seg" role="group" aria-label="显示方式">
        <button class:on={!raw} onclick={() => raw && onToggle()}>{hasTable ? "排版" : "结果"}</button>
        <button class:on={raw} onclick={() => !raw && onToggle()}>{hasTable ? "原文" : "模板"}</button>
      </div>
      <kbd>Tab</kbd>
    {/if}
  </header>

  <div class="body" bind:this={body}>
    {#if showResult && rendered}
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
    {#if !filling}<span class="muted">← 收起</span>{/if}
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

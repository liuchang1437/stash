<script lang="ts">
  import { relativeTime, type Hit } from "./api";
  import { appName, LABELS, lineCount, parseBlocks, templatePieces, type Kind } from "./kinds";

  type Props = { hit: Hit; kind: Kind; raw: boolean; onToggle: () => void };
  let { hit, kind, raw, onToggle }: Props = $props();

  let body: HTMLElement;

  const blocks = $derived(kind === "snippet" ? [] : parseBlocks(hit.preview));
  const hasTable = $derived(blocks.some((b) => b.type === "table"));
  const truncated = $derived(hit.chars > hit.preview.length);

  export function scroll(delta: number) {
    body?.scrollBy({ top: delta });
  }
</script>

<section class="peek">
  <header>
    <span class="label">{LABELS[kind]}</span>
    <span class="muted">{hit.chars.toLocaleString()} 字 · {lineCount(hit.preview)} 行</span>
    <span class="spacer"></span>
    {#if hasTable}
      <div class="seg" role="group" aria-label="显示方式">
        <button class:on={!raw} onclick={() => raw && onToggle()}>排版</button>
        <button class:on={raw} onclick={() => !raw && onToggle()}>原文</button>
      </div>
      <kbd>Tab</kbd>
    {/if}
  </header>

  <div class="body" bind:this={body}>
    {#if kind === "snippet"}
      <pre class="template">{#each templatePieces(hit.preview) as piece}{#if piece.variable}<span
              class="var">{piece.text}</span
            >{:else}{piece.text}{/if}{/each}</pre>
    {:else}
      {#each blocks as block}
        {#if block.type === "text"}
          <pre class="text">{block.text}</pre>
        {:else if raw}
          <pre class="raw-table">{block.raw}</pre>
        {:else}
          <div class="table-wrap">
            <table>
              <thead>
                <tr>
                  {#each block.rows[0] as cell}<th>{cell}</th>{/each}
                </tr>
              </thead>
              <tbody>
                {#each block.rows.slice(1) as row}
                  <tr>
                    {#each row as cell}<td>{cell}</td>{/each}
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
    <span class="muted">← 收起</span>
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

  .var {
    padding: 0 3px;
    border-radius: 3px;
    background: rgba(185, 163, 255, 0.16);
    color: var(--violet);
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

<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { onMount, tick } from "svelte";
  import {
    api,
    groupDigits,
    looksLikeUrl,
    relativeTime,
    type Field,
    type Hit,
    type Mode,
  } from "./api";

  const KIND_LABELS: Record<Hit["kind"], string> = {
    clip: "剪贴板",
    snippet: "Snippet",
    calc: "计算器",
  };

  type Props = {
    onFill: (key: string, title: string, fields: Field[], mode: Mode) => void;
    onEdit: (key: string) => void;
    onNewSnippet: (body: string) => void;
    onSettings: () => void;
  };
  let { onFill, onEdit, onNewSnippet, onSettings }: Props = $props();

  let query = $state("");
  let hits = $state<Hit[]>([]);
  let selected = $state(0);
  let status = $state("");
  let confirmDelete = $state<string | null>(null);
  let input: HTMLInputElement;
  let list: HTMLElement;
  let searchSeq = 0;

  const current = $derived(hits[selected] as Hit | undefined);
  // Snippets may render into a URL, so they are always offered.
  const canOpen = $derived(
    current?.kind === "snippet" || (current?.kind === "clip" && looksLikeUrl(current.preview)),
  );

  async function refresh() {
    const seq = ++searchSeq;
    const result = await api.search(query);
    if (seq !== searchSeq) return;
    hits = result;
    if (selected >= hits.length) selected = Math.max(0, hits.length - 1);
  }

  $effect(() => {
    query;
    selected = 0;
    confirmDelete = null;
    refresh();
  });

  export function reset() {
    status = "";
    confirmDelete = null;
    if (query === "") {
      selected = 0;
      refresh();
    } else {
      query = "";
    }
    input?.focus();
  }

  /** Reopened shortly after closing: keep the query, selected so that
   *  typing replaces it, and re-run it since the clipboard may have changed. */
  export function resume() {
    status = "";
    confirmDelete = null;
    selected = 0;
    refresh();
    input?.focus();
    input?.select();
  }

  export function focus() {
    input?.focus();
  }

  onMount(() => {
    input.focus();
    const unlisten = listen("index-changed", refresh);
    return () => {
      unlisten.then((f) => f());
    };
  });

  async function move(delta: number) {
    if (!hits.length) return;
    selected = (selected + delta + hits.length) % hits.length;
    confirmDelete = null;
    await tick();
    list?.querySelector(`[data-index="${selected}"]`)?.scrollIntoView({ block: "nearest" });
  }

  function flash(message: string) {
    status = message;
    setTimeout(() => {
      if (status === message) status = "";
    }, 2500);
  }

  async function run<T>(action: () => Promise<T>): Promise<T | undefined> {
    try {
      return await action();
    } catch (e) {
      flash(String(e));
    }
  }

  async function activate(mode: Mode) {
    const hit = current;
    if (!hit) return;
    const result = await run(() => api.activate(hit.key, mode));
    if (result?.status === "needsInput") onFill(hit.key, result.title, result.fields, mode);
  }

  async function togglePin() {
    if (current?.kind !== "clip") return;
    const pinned = await run(() => api.togglePin(current!.key));
    if (pinned !== undefined) {
      flash(pinned ? "已置顶" : "已取消置顶");
      refresh();
    }
  }

  async function remove() {
    const hit = current;
    if (!hit) return;
    if (hit.kind === "snippet" && confirmDelete !== hit.key) {
      confirmDelete = hit.key;
      return;
    }
    confirmDelete = null;
    await run(() => api.deleteItem(hit.key));
    refresh();
  }

  function onkeydown(e: KeyboardEvent) {
    const ctrl = e.ctrlKey || e.metaKey;
    let handled = true;
    if (e.key === "ArrowDown" || (ctrl && e.key === "j")) move(1);
    else if (e.key === "ArrowUp" || (ctrl && e.key === "k")) move(-1);
    else if (e.key === "PageDown") move(8);
    else if (e.key === "PageUp") move(-8);
    else if (e.key === "Enter") activate(ctrl ? "open" : e.shiftKey ? "copy" : "paste");
    else if (e.key === "Escape") {
      if (confirmDelete) confirmDelete = null;
      else if (query) query = "";
      else api.hide();
    } else if (ctrl && e.key === "p") togglePin();
    else if (ctrl && e.key === "d") remove();
    else if (ctrl && e.key === "n") onNewSnippet(current?.kind === "clip" ? current.preview : "");
    else if (ctrl && e.key === "e" && current?.kind === "snippet") onEdit(current.key);
    else if (ctrl && e.key === ",") onSettings();
    else handled = false;
    if (handled) e.preventDefault();
  }
</script>

<div class="launcher">
  <div class="search">
    <svg viewBox="0 0 24 24" aria-hidden="true"
      ><circle cx="11" cy="11" r="7" /><path d="m20 20-3.5-3.5" /></svg
    >
    <input
      bind:this={input}
      bind:value={query}
      {onkeydown}
      placeholder="搜索剪贴板和 Snippets，或输入算式（$1 = 最近复制的数）"
      spellcheck="false"
      autocomplete="off"
    />
  </div>

  <div class="body">
    <ul class="list" role="listbox" bind:this={list}>
      {#each hits as hit, i (hit.key)}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <li
          role="option"
          aria-selected={i === selected}
          data-index={i}
          class:selected={i === selected}
          onclick={() => (selected = i)}
          ondblclick={() => activate("paste")}
        >
          <span class="kind {hit.kind}" title={KIND_LABELS[hit.kind]}>
            {#if hit.kind === "calc"}
              <svg viewBox="0 0 24 24"
                ><rect x="5" y="3" width="14" height="18" rx="2" /><path
                  d="M8 7h8M8.5 12h.01M12 12h.01M15.5 12h.01M8.5 16h.01M12 16h.01M15.5 16h.01"
                /></svg
              >
            {:else if hit.kind === "clip"}
              <svg viewBox="0 0 24 24"
                ><rect x="8" y="8" width="12" height="12" rx="2" /><path
                  d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2"
                /></svg
              >
            {:else}
              <svg viewBox="0 0 24 24"><path d="m8 7-5 5 5 5M16 7l5 5-5 5" /></svg>
            {/if}
          </span>
          <span class="text">
            <span class="title">
              {#if hit.kind === "calc"}= {groupDigits(hit.title)}{:else}{hit.title || "（空白）"}{/if}
            </span>
            <span class="meta">
              {#if hit.pinned}<span class="pin">已置顶 ·</span>{/if}
              {#if hit.kind === "calc"}
                计算器 · {hit.preview}
              {:else if hit.kind === "snippet"}
                {hit.tags.length ? hit.tags.map((t) => "#" + t).join(" ") : "Snippet"}
              {:else}
                {hit.source ?? "未知来源"} · {relativeTime(hit.lastUsedAt)}
              {/if}
            </span>
          </span>
        </li>
      {:else}
        <li class="empty" role="presentation">{query ? "没有匹配的结果" : "还没有记录，复制点什么试试"}</li>
      {/each}
    </ul>

    <section class="preview">
      {#if current?.kind === "calc"}
        <div class="calc">
          <div class="expression">{current.preview} =</div>
          <div class="result">{groupDigits(current.title)}</div>
          <div class="hint">按 ↵ 粘贴 {current.title}</div>
        </div>
      {:else if current}
        <pre>{current.preview}{current.chars > current.preview.length ? "\n…" : ""}</pre>
        <dl>
          {#if current.kind === "snippet"}
            <dt>Snippet</dt>
            <dd>{current.title}</dd>
          {:else}
            <dt>来源</dt>
            <dd>{current.source ?? "未知"}</dd>
          {/if}
          <dt>{current.kind === "clip" ? "最近复制" : "最近使用"}</dt>
          <dd>{relativeTime(current.lastUsedAt) || "从未"}</dd>
          <dt>字符数</dt>
          <dd>{current.chars}</dd>
          <dt>使用次数</dt>
          <dd>{current.useCount}</dd>
        </dl>
      {/if}
    </section>
  </div>

  <footer>
    {#if status}
      <span class="status">{status}</span>
    {:else if confirmDelete}
      <span class="status danger">再按 <kbd>Ctrl D</kbd> 删除这个 snippet 文件，<kbd>Esc</kbd> 取消</span>
    {:else}
      <span><kbd>↵</kbd> 粘贴</span>
      <span><kbd>⇧ ↵</kbd> 复制</span>
      {#if canOpen}
        <span><kbd>Ctrl ↵</kbd> 浏览器打开</span>
      {/if}
      {#if current?.kind === "clip"}
        <span><kbd>Ctrl P</kbd> 置顶</span>
        <span><kbd>Ctrl N</kbd> 存为 Snippet</span>
      {:else if current?.kind === "snippet"}
        <span><kbd>Ctrl E</kbd> 编辑</span>
        <span><kbd>Ctrl N</kbd> 新建</span>
      {/if}
      {#if current?.kind !== "calc"}
        <span><kbd>Ctrl D</kbd> 删除</span>
      {/if}
      <span class="spacer"></span>
      <button class="link" onclick={onSettings}>设置</button>
    {/if}
  </footer>
</div>

<style>
  .launcher {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }

  .search {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 16px;
    height: 56px;
    border-bottom: 1px solid var(--border);
  }

  .search svg {
    width: 20px;
    height: 20px;
    fill: none;
    stroke: var(--text-muted);
    stroke-width: 2;
    stroke-linecap: round;
    flex: none;
  }

  .search input {
    flex: 1;
    border: none;
    outline: none;
    background: transparent;
    font-size: 18px;
  }

  .search input::placeholder {
    color: var(--text-muted);
  }

  .body {
    flex: 1;
    display: flex;
    min-height: 0;
  }

  .list {
    width: 44%;
    margin: 0;
    padding: 6px;
    list-style: none;
    overflow-y: auto;
    border-right: 1px solid var(--border);
  }

  .list li {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 10px;
    border-radius: 6px;
    cursor: default;
  }

  .list li.selected {
    background: var(--bg-selected);
  }

  .list li.empty {
    color: var(--text-muted);
    justify-content: center;
    padding: 24px 0;
  }

  .kind {
    flex: none;
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: 6px;
    background: var(--bg-subtle);
  }

  .kind svg {
    width: 15px;
    height: 15px;
    fill: none;
    stroke: var(--text-muted);
    stroke-width: 2;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  .kind.snippet svg {
    stroke: var(--snippet);
  }

  .kind.calc svg {
    stroke: var(--accent);
  }

  .calc {
    flex: 1;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 6px;
    min-width: 0;
  }

  .calc .expression {
    color: var(--text-muted);
    font-size: 15px;
    overflow-wrap: anywhere;
  }

  .calc .result {
    font-size: 34px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    overflow-wrap: anywhere;
    user-select: text;
  }

  .calc .hint {
    margin-top: 8px;
    font-size: 12px;
    color: var(--text-muted);
  }

  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .title,
  .meta {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .meta {
    font-size: 12px;
    color: var(--text-muted);
  }

  .pin {
    color: var(--accent);
  }

  .preview {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    padding: 12px 16px;
  }

  .preview pre {
    flex: 1;
    margin: 0;
    overflow: auto;
    white-space: pre-wrap;
    word-break: break-word;
    font-family: "Cascadia Mono", Consolas, "Microsoft YaHei UI", monospace;
    font-size: 13px;
    line-height: 1.5;
    user-select: text;
  }

  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 2px 12px;
    margin: 10px 0 0;
    padding-top: 10px;
    border-top: 1px solid var(--border);
    font-size: 12px;
  }

  dt {
    color: var(--text-muted);
  }

  dd {
    margin: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  footer {
    display: flex;
    align-items: center;
    gap: 14px;
    height: 36px;
    padding: 0 14px;
    border-top: 1px solid var(--border);
    background: var(--bg-subtle);
    font-size: 12px;
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
  }

  .spacer {
    flex: 1;
  }

  .status {
    color: var(--text);
  }

  .status.danger {
    color: var(--danger);
  }

  .link {
    border: none;
    background: none;
    padding: 0;
    color: var(--text-muted);
    cursor: pointer;
  }

  .link:hover {
    color: var(--accent);
  }
</style>

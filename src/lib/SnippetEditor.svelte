<script lang="ts">
  import { onMount } from "svelte";
  import { api, type Snippet } from "./api";

  type Props = {
    snippet: Snippet | null;
    initialBody: string;
    onDone: () => void;
  };
  let { snippet, initialBody, onDone }: Props = $props();

  // The form is seeded once; later prop changes remount the editor.
  // svelte-ignore state_referenced_locally
  let title = $state(snippet?.title ?? "");
  // svelte-ignore state_referenced_locally
  let tags = $state(snippet?.tags.join(", ") ?? "");
  // svelte-ignore state_referenced_locally
  let body = $state(snippet?.body ?? initialBody);
  let error = $state("");
  let titleInput: HTMLInputElement;

  onMount(() => titleInput.focus());

  async function save() {
    error = "";
    const tagList = tags
      .split(/[,，\s]+/)
      .map((t) => t.replace(/^#/, "").trim())
      .filter(Boolean);
    try {
      await api.saveSnippet(snippet?.path ?? null, title, tagList, body);
      onDone();
    } catch (e) {
      error = String(e);
    }
  }

  function onkeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === "s") {
      e.preventDefault();
      save();
    } else if (e.key === "Escape") {
      e.preventDefault();
      onDone();
    }
  }
</script>

<svelte:window {onkeydown} />

<div class="editor">
  <header>
    <input
      bind:this={titleInput}
      bind:value={title}
      class="title"
      placeholder="Snippet 标题"
      spellcheck="false"
    />
    <input bind:value={tags} class="field tags" placeholder="标签，用逗号分隔" spellcheck="false" />
  </header>

  <textarea bind:value={body} spellcheck="false" placeholder="内容"></textarea>

  <details>
    <summary>变量语法</summary>
    <div class="help">
      <code>{"{{name}}"}</code> 粘贴时填写 ·
      <code>{"{{name=默认值}}"}</code> 带默认值 ·
      <code>{"{{env:prod|dev}}"}</code> 下拉选择 ·
      <code>{"{{date:yyyy-MM-dd}}"}</code> <code>{"{{time}}"}</code> 日期时间 ·
      <code>{"{{clipboard}}"}</code> 当前剪贴板 ·
      <code>{"{{clipboard:2}}"}</code> 剪贴板历史倒数第 2 条 ·
      <code>{"{{uuid}}"}</code> ·
      <code>{"{{cursor}}"}</code> 粘贴后光标位置
    </div>
  </details>

  <footer>
    {#if error}
      <span class="error">{error}</span>
    {:else}
      <span>{snippet ? snippet.path : "新建 Snippet"}</span>
    {/if}
    <span class="spacer"></span>
    <span><kbd>Ctrl S</kbd> 保存</span>
    <span><kbd>Esc</kbd> 返回</span>
    <button class="btn" onclick={onDone}>取消</button>
    <button class="btn primary" onclick={save}>保存</button>
  </footer>
</div>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }

  header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
  }

  .title {
    flex: 1;
    border: none;
    outline: none;
    background: transparent;
    font-size: 18px;
    font-weight: 600;
  }

  .tags {
    width: 220px;
  }

  textarea {
    flex: 1;
    resize: none;
    border: none;
    outline: none;
    padding: 12px 16px;
    background: var(--bg);
    font-family: "Cascadia Mono", Consolas, "Microsoft YaHei UI", monospace;
    font-size: 13px;
    line-height: 1.5;
  }

  details {
    padding: 6px 16px;
    border-top: 1px solid var(--border);
    font-size: 12px;
    color: var(--text-muted);
  }

  summary {
    cursor: pointer;
  }

  .help {
    padding: 6px 0 2px;
    line-height: 1.9;
  }

  code {
    padding: 1px 4px;
    border-radius: 4px;
    background: var(--kbd-bg);
    color: var(--text);
    font-family: "Cascadia Mono", Consolas, monospace;
  }

  footer {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 8px 14px;
    border-top: 1px solid var(--border);
    background: var(--bg-subtle);
    font-size: 12px;
    color: var(--text-muted);
    white-space: nowrap;
  }

  .spacer {
    flex: 1;
  }

  .error {
    color: var(--danger);
    white-space: normal;
  }
</style>

<script lang="ts">
  import { onMount } from "svelte";
  import { api, type Settings } from "./api";

  let { onDone }: { onDone: () => void } = $props();

  let settings = $state<Settings | null>(null);
  let ignoredApps = $state("");
  let error = $state("");
  let recording = $state(false);

  onMount(async () => {
    settings = await api.getSettings();
    ignoredApps = settings.config.ignoredApps.join("\n");
    error = settings.hotkeyError ?? "";
  });

  const MODIFIERS = ["Control", "Alt", "Shift", "Meta"];

  function keyName(code: string): string | null {
    if (code.startsWith("Key")) return code.slice(3);
    if (code.startsWith("Digit")) return code.slice(5);
    if (/^F\d+$/.test(code)) return code;
    const named: Record<string, string> = {
      Space: "Space",
      Backquote: "Backquote",
      Semicolon: "Semicolon",
      Quote: "Quote",
      Comma: "Comma",
      Period: "Period",
      Slash: "Slash",
      Backslash: "Backslash",
      BracketLeft: "BracketLeft",
      BracketRight: "BracketRight",
      Minus: "Minus",
      Equal: "Equal",
      Insert: "Insert",
      Home: "Home",
      End: "End",
      PageUp: "PageUp",
      PageDown: "PageDown",
    };
    return named[code] ?? null;
  }

  function recordHotkey(e: KeyboardEvent) {
    if (e.key === "Tab") return;
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Escape") {
      (e.target as HTMLElement).blur();
      return;
    }
    if (MODIFIERS.includes(e.key) || !settings) return;
    const key = keyName(e.code);
    if (!key) return;
    const parts = [];
    if (e.ctrlKey) parts.push("Ctrl");
    if (e.altKey) parts.push("Alt");
    if (e.shiftKey) parts.push("Shift");
    if (e.metaKey) parts.push("Super");
    if (!parts.length && !/^F\d+$/.test(key)) {
      error = "快捷键至少需要一个修饰键（Ctrl / Alt / Shift / Win）";
      return;
    }
    error = "";
    settings.config.hotkey = [...parts, key].join("+");
  }

  async function save() {
    if (!settings) return;
    settings.config.ignoredApps = ignoredApps
      .split(/\r?\n/)
      .map((s) => s.trim())
      .filter(Boolean);
    settings.config.historyLimit = Number(settings.config.historyLimit) || 5000;
    settings.config.keepQuerySeconds = Math.max(0, Math.floor(Number(settings.config.keepQuerySeconds) || 0));
    try {
      await api.saveSettings($state.snapshot(settings));
      onDone();
    } catch (e) {
      error = String(e);
    }
  }

  function onkeydown(e: KeyboardEvent) {
    if (recording) return;
    if (e.key === "Escape") {
      e.preventDefault();
      onDone();
    } else if ((e.ctrlKey || e.metaKey) && e.key === "s") {
      e.preventDefault();
      save();
    }
  }
</script>

<svelte:window {onkeydown} />

<div class="settings">
  <header><h1>设置</h1></header>

  {#if settings}
    <div class="form">
      <label>
        <span>唤起快捷键</span>
        <input
          class="field hotkey"
          class:recording
          readonly
          value={recording ? "请按下新的快捷键…" : settings.config.hotkey}
          onfocus={() => (recording = true)}
          onblur={() => (recording = false)}
          onkeydown={recordHotkey}
        />
        <small>点击后直接按下组合键。默认 Alt+Space。</small>
      </label>

      <label>
        <span>Snippets 目录</span>
        <div class="row">
          <input
            class="field"
            bind:value={settings.config.snippetsDir}
            placeholder={settings.defaultSnippetsDir}
            spellcheck="false"
          />
          <button class="btn" onclick={() => api.openSnippetsDir()}>打开</button>
        </div>
        <small>每个 snippet 是一个 .md 文件。把这个目录放进 OneDrive / 坚果云 / git 即可同步。</small>
      </label>

      <label>
        <span>剪贴板历史保留条数</span>
        <input class="field short" type="number" min="10" bind:value={settings.config.historyLimit} />
        <small>置顶的记录不计入，也不会被自动清理。</small>
      </label>

      <label>
        <span>不记录这些程序的复制（每行一个 exe 名）</span>
        <textarea class="field" rows="3" bind:value={ignoredApps} spellcheck="false"></textarea>
        <small>密码管理器通常会自动标记敏感内容，这里是额外的保险。</small>
      </label>

      <label>
        <span>关闭后保留搜索内容（秒）</span>
        <input class="field short" type="number" min="0" bind:value={settings.config.keepQuerySeconds} />
        <small>在这段时间内重新打开会保留上次的搜索内容，设为 0 表示每次都清空。</small>
      </label>

      <label class="check">
        <input type="checkbox" bind:checked={settings.config.restoreClipboard} />
        <span>粘贴后恢复原来的剪贴板内容</span>
      </label>

      <label class="check">
        <input type="checkbox" bind:checked={settings.autostart} />
        <span>开机自动启动</span>
      </label>
    </div>
  {/if}

  <footer>
    {#if error}<span class="error">{error}</span>{/if}
    <span class="spacer"></span>
    <button class="btn" onclick={onDone}>取消</button>
    <button class="btn primary" onclick={save}>保存</button>
  </footer>
</div>

<style>
  .settings {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }

  header {
    padding: 14px 20px 6px;
  }

  h1 {
    margin: 0;
    font-size: 18px;
    font-weight: 600;
  }

  .form {
    flex: 1;
    overflow-y: auto;
    padding: 6px 20px 16px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  label > span {
    font-weight: 500;
  }

  small {
    color: var(--text-muted);
    font-size: 12px;
  }

  .row {
    display: flex;
    gap: 8px;
  }

  .hotkey {
    width: 240px;
    cursor: pointer;
  }

  .hotkey.recording {
    color: var(--accent);
  }

  .short {
    width: 140px;
  }

  textarea.field {
    resize: none;
    font-family: "Cascadia Mono", Consolas, monospace;
    font-size: 13px;
  }

  .check {
    flex-direction: row;
    align-items: center;
    gap: 8px;
  }

  .check > span {
    font-weight: 400;
  }

  footer {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 14px;
    border-top: 1px solid var(--border);
    background: var(--bg-subtle);
    font-size: 12px;
  }

  .spacer {
    flex: 1;
  }

  .error {
    color: var(--danger);
  }
</style>

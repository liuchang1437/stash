<script lang="ts">
  import { onMount } from "svelte";
  import { api, type Settings } from "./api";
  import CloseButton from "./CloseButton.svelte";

  let { onDone }: { onDone: () => void } = $props();

  let settings = $state<Settings | null>(null);
  let ignoredApps = $state("");
  let error = $state("");
  /** Which hotkey field is waiting for a key combination. */
  let recording = $state<"hotkey" | "swapHotkey" | null>(null);

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

  function recordHotkey(e: KeyboardEvent, which: "hotkey" | "swapHotkey") {
    if (e.key === "Tab") return;
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Escape") {
      (e.target as HTMLElement).blur();
      return;
    }
    if (which === "swapHotkey" && (e.key === "Backspace" || e.key === "Delete") && settings) {
      settings.config.swapHotkey = "";
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
    settings.config[which] = [...parts, key].join("+");
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
  <!-- The window has no title bar: the header drags it. -->
  <header data-tauri-drag-region>
    <h1 data-tauri-drag-region>设置</h1>
    <CloseButton onClose={onDone} />
  </header>

  {#if settings}
    <div class="form">
      <label>
        <span>唤起快捷键</span>
        <input
          class="field hotkey"
          class:recording={recording === "hotkey"}
          readonly
          value={recording === "hotkey" ? "请按下新的快捷键…" : settings.config.hotkey}
          onfocus={() => (recording = "hotkey")}
          onblur={() => (recording = null)}
          onkeydown={(e) => recordHotkey(e, "hotkey")}
        />
        <small>点击后直接按下组合键。默认 Alt+Space。</small>
      </label>

      <label class="check">
        <input type="checkbox" bind:checked={settings.config.followCaret} />
        <span>在输入光标旁边打开（默认在屏幕中上方；找不到光标时跟随鼠标）</span>
      </label>

      <label class="check">
        <input type="checkbox" bind:checked={settings.config.autoPeek} />
        <span>打开时自动展开预览（关闭后按 → 展开）</span>
      </label>

      <label>
        <span>粘贴后「换一条」快捷键</span>
        <input
          class="field hotkey"
          class:recording={recording === "swapHotkey"}
          readonly
          value={recording === "swapHotkey" ? "请按下新的快捷键…" : settings.config.swapHotkey || "（已关闭）"}
          onfocus={() => (recording = "swapHotkey")}
          onblur={() => (recording = null)}
          onkeydown={(e) => recordHotkey(e, "swapHotkey")}
        />
        <small>粘贴后的几秒内按住 Alt 连按 V 选择更早的记录，松开 Alt 原地替换。只在提示条出现时生效；按 Backspace 关闭。</small>
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
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 12px 6px 20px;
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
    align-items: center;
    gap: 8px;
  }

  .row .btn {
    flex: none;
    white-space: nowrap;
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
    font-family: var(--mono);
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

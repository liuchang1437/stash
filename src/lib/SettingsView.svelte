<script lang="ts">
  import { onMount } from "svelte";
  import { api, type Settings } from "./api";
  import CloseButton from "./CloseButton.svelte";
  import { t } from "./i18n.svelte";

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
      error = t.settings.needsModifier;
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
    <h1 data-tauri-drag-region>{t.settings.title}</h1>
    <CloseButton onClose={onDone} />
  </header>

  {#if settings}
    <div class="form">
      <label>
        <span>{t.settings.language}</span>
        <select class="field lang" bind:value={settings.config.language}>
          <option value="">{t.settings.followSystem}</option>
          <!-- Each language is named in itself. -->
          <option value="zh">中文</option>
          <option value="en">English</option>
        </select>
      </label>

      <label>
        <span>{t.settings.hotkey}</span>
        <input
          class="field hotkey"
          class:recording={recording === "hotkey"}
          readonly
          value={recording === "hotkey" ? t.settings.pressKeys : settings.config.hotkey}
          onfocus={() => (recording = "hotkey")}
          onblur={() => (recording = null)}
          onkeydown={(e) => recordHotkey(e, "hotkey")}
        />
        <small>{t.settings.hotkeyHint}</small>
      </label>

      <label class="check">
        <input type="checkbox" bind:checked={settings.config.followCaret} />
        <span>{t.settings.followCaret}</span>
      </label>

      <label class="check">
        <input type="checkbox" bind:checked={settings.config.autoPeek} />
        <span>{t.settings.autoPeek}</span>
      </label>

      <label class="check">
        <input type="checkbox" bind:checked={settings.config.englishInput} />
        <span>{t.settings.englishInput}</span>
      </label>

      <label>
        <span>{t.settings.swapHotkey}</span>
        <input
          class="field hotkey"
          class:recording={recording === "swapHotkey"}
          readonly
          value={recording === "swapHotkey" ? t.settings.pressKeys : settings.config.swapHotkey || t.settings.off}
          onfocus={() => (recording = "swapHotkey")}
          onblur={() => (recording = null)}
          onkeydown={(e) => recordHotkey(e, "swapHotkey")}
        />
        <small>{t.settings.swapHint}</small>
      </label>

      <label>
        <span>{t.settings.snippetsDir}</span>
        <div class="row">
          <input
            class="field"
            bind:value={settings.config.snippetsDir}
            placeholder={settings.defaultSnippetsDir}
            spellcheck="false"
          />
          <button class="btn" onclick={() => api.openSnippetsDir()}>{t.common.open}</button>
        </div>
        <small>{t.settings.snippetsDirHint}</small>
      </label>

      <label>
        <span>{t.settings.historyLimit}</span>
        <input class="field short" type="number" min="10" bind:value={settings.config.historyLimit} />
        <small>{t.settings.historyLimitHint}</small>
      </label>

      <label>
        <span>{t.settings.ignoredApps}</span>
        <textarea class="field" rows="3" bind:value={ignoredApps} spellcheck="false"></textarea>
        <small>{t.settings.ignoredAppsHint}</small>
      </label>

      <label>
        <span>{t.settings.keepQuery}</span>
        <input class="field short" type="number" min="0" bind:value={settings.config.keepQuerySeconds} />
        <small>{t.settings.keepQueryHint}</small>
      </label>

      <label class="check">
        <input type="checkbox" bind:checked={settings.config.restoreClipboard} />
        <span>{t.settings.restoreClipboard}</span>
      </label>

      <label class="check">
        <input type="checkbox" bind:checked={settings.autostart} />
        <span>{t.settings.autostart}</span>
      </label>
    </div>
  {/if}

  <footer>
    {#if error}<span class="error">{error}</span>{/if}
    <span class="spacer"></span>
    <button class="btn" onclick={onDone}>{t.common.cancel}</button>
    <button class="btn primary" onclick={save}>{t.common.save}</button>
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

  .lang {
    width: 240px;
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

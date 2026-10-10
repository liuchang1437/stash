<script lang="ts">
  import type { EditorView } from "@codemirror/view";
  import { onMount, tick } from "svelte";
  import { api, type Field, type Segment, type Snippet } from "./api";
  import CloseButton from "./CloseButton.svelte";
  import Rendered from "./Rendered.svelte";
  import { tokens, VARIABLES, type Variable } from "./template";

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
  let host: HTMLElement;
  let view: EditorView | undefined;
  let editor: typeof import("./editor") | undefined;

  onMount(() => {
    titleInput.focus();
    let gone = false;
    // Loaded here only, so the popover never pays for CodeMirror.
    import("./editor").then((module) => {
      if (gone) return;
      editor = module;
      view = module.createEditor(host, { doc: body, onChange: (doc) => (body = doc), onMenu: openMenu });
    });
    return () => {
      gone = true;
      view?.destroy();
    };
  });

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
    // Already handled: CodeMirror closing its completion or leaving a
    // snippet field, or the variable menu.
    if (e.defaultPrevented) return;
    if ((e.ctrlKey || e.metaKey) && e.key === "s") {
      e.preventDefault();
      save();
    } else if (e.key === "Escape") {
      e.preventDefault();
      onDone();
    }
  }

  // -------------------------------------------------------------------------
  // Preview: the fields the template asks for, and what it pastes with the
  // values tried out here (the defaults until something is typed).

  let fields = $state<Field[]>([]);
  let segments = $state<Segment[]>([]);
  let values = $state<Record<string, string>>({});
  let focus = $state<string | null>(null);
  const problems = $derived(tokens(body).filter((t) => t.problem).length);

  let previewSeq = 0;
  $effect(() => {
    const template = body;
    const tried = $state.snapshot(values);
    const seq = ++previewSeq;
    const timer = setTimeout(async () => {
      const preview = await api.previewTemplate(template, tried).catch(() => null);
      if (seq !== previewSeq || !preview) return;
      fields = preview.fields;
      segments = preview.segments;
    }, 120);
    return () => clearTimeout(timer);
  });

  // -------------------------------------------------------------------------
  // Ctrl+K: insert a variable at the caret, or turn the selection into one.

  type Menu = { x: number; y: number; selection: boolean; index: number };
  let menu = $state<Menu | null>(null);
  let menuEl: HTMLElement | undefined = $state();
  const menuItems = $derived(menu ? VARIABLES.filter((v) => !(menu!.selection && v.plainOnly)) : []);

  async function openMenu() {
    if (!view) return;
    const sel = view.state.selection.main;
    const at = view.coordsAtPos(sel.head);
    const x = Math.max(8, Math.min(at?.left ?? 40, window.innerWidth - 320));
    const below = (at?.bottom ?? 80) + 6;
    menu = { x, y: below, selection: !sel.empty, index: 0 };
    await tick();
    if (!menu || !menuEl) return;
    // No room below the caret: open above it.
    const height = menuEl.offsetHeight;
    if (below + height > window.innerHeight - 8) menu.y = Math.max(8, (at?.top ?? below) - height - 6);
    menuEl.focus();
  }

  function closeMenu() {
    menu = null;
    view?.focus();
  }

  function pick(variable: Variable) {
    menu = null;
    if (view && editor) editor.insertVariable(view, variable);
  }

  function menuKeydown(e: KeyboardEvent) {
    if (!menu) return;
    const n = menuItems.length;
    if (e.key === "ArrowDown") menu.index = (menu.index + 1) % n;
    else if (e.key === "ArrowUp") menu.index = (menu.index - 1 + n) % n;
    else if (e.key === "Enter") pick(menuItems[menu.index]);
    else if (/^[1-9]$/.test(e.key) && Number(e.key) <= n) pick(menuItems[Number(e.key) - 1]);
    else if (e.key === "Escape" || ((e.ctrlKey || e.metaKey) && e.key === "k")) closeMenu();
    else return;
    e.preventDefault();
    e.stopPropagation();
  }
</script>

<svelte:window {onkeydown} />

<div class="editor">
  <!-- The window has no title bar: the header's empty space drags it. -->
  <header data-tauri-drag-region>
    <input
      bind:this={titleInput}
      bind:value={title}
      class="title"
      placeholder="Snippet 标题"
      spellcheck="false"
    />
    <input bind:value={tags} class="field tags" placeholder="标签，用逗号分隔" spellcheck="false" />
    <CloseButton onClose={onDone} />
  </header>

  <div class="main">
    <div class="code">
      <div class="host" bind:this={host}></div>
      <div class="hint">
        <span>输入 <code>{"{{"}</code> 插入变量</span>
        <span>选中文字后 <kbd>Ctrl K</kbd> 把它变成变量</span>
        <span class="spacer"></span>
        {#if problems}<span class="warn">{problems} 处变量写法有误，会按原文粘贴</span>{/if}
      </div>
    </div>

    <aside>
      <section>
        <h3>试填变量 <span class="note">只用于预览，不会保存</span></h3>
        {#each fields as field (field.name)}
          <div class="var-row">
            <span class="vname" title={field.name}>{field.name}</span>
            {#if field.options.length}
              <div class="chips">
                {#each field.options as option}
                  <button
                    class:on={(values[field.name] ?? field.default) === option}
                    onclick={() => (values[field.name] = option)}>{option}</button
                  >
                {/each}
              </div>
            {:else}
              <input
                class="field"
                bind:value={values[field.name]}
                placeholder={field.default || "填一个值试试"}
                spellcheck="false"
                onfocus={() => (focus = field.name)}
                onblur={() => (focus = null)}
              />
            {/if}
          </div>
        {:else}
          <p class="empty">
            没有要填写的变量。输入 <code>{"{{"}</code> 或按 <kbd>Ctrl K</kbd> 插入一个；也可以先选中一段文字，再按
            <kbd>Ctrl K</kbd> 把它变成变量。
          </p>
        {/each}
      </section>

      <section>
        <h3>粘贴结果</h3>
        <pre class="result"><Rendered {segments} {focus} /></pre>
      </section>
    </aside>
  </div>

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

  {#if menu}
    <div
      class="menu"
      role="menu"
      tabindex="-1"
      bind:this={menuEl}
      style:left="{menu.x}px"
      style:top="{menu.y}px"
      onkeydown={menuKeydown}
      onfocusout={(e) => {
        if (!menuEl?.contains(e.relatedTarget as Node)) menu = null;
      }}
    >
      <div class="menu-title">{menu.selection ? "把选中的文字变成…" : "插入变量"}</div>
      {#each menuItems as item, i}
        <button
          role="menuitem"
          tabindex="-1"
          class:sel={i === menu.index}
          onmouseenter={() => menu && (menu.index = i)}
          onclick={() => pick(item)}
        >
          <span class="n">{i < 9 ? i + 1 : ""}</span>
          <span class="mlabel">{item.label}</span>
          <span class="msyntax">{item.syntax}</span>
        </button>
      {/each}
      <div class="menu-info">
        {menuItems[menu.index]?.info}{#if menuItems[menu.index]?.example}
          · 现在是 {menuItems[menu.index].example?.()}{/if}
      </div>
    </div>
  {/if}
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

  .main {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(0, 1fr) 270px;
  }

  .code {
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .host {
    flex: 1;
    min-height: 0;
  }

  .hint {
    display: flex;
    gap: 14px;
    padding: 6px 16px;
    border-top: 1px solid var(--border-soft);
    font-size: 11.5px;
    color: var(--text-faint);
    white-space: nowrap;
    overflow: hidden;
  }

  .warn {
    color: var(--danger);
  }

  aside {
    min-height: 0;
    overflow-y: auto;
    padding: 12px 14px;
    border-left: 1px solid var(--border);
    background: var(--bg-subtle);
    display: flex;
    flex-direction: column;
    gap: 18px;
  }

  h3 {
    margin: 0 0 8px;
    font-size: 11.5px;
    font-weight: 500;
    color: var(--text-muted);
  }

  .note {
    margin-left: 6px;
    font-size: 10.5px;
    color: var(--text-faint);
  }

  .var-row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 6px;
  }

  .vname {
    width: 64px;
    flex: none;
    font-size: 12px;
    color: var(--green);
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .var-row .field {
    flex: 1;
    min-width: 0;
    padding: 3px 8px;
    font-size: 12px;
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .chips button {
    height: 22px;
    padding: 0 7px;
    border: 1px solid var(--border);
    border-radius: 3px;
    background: var(--bg);
    font-size: 11.5px;
    color: var(--text-muted);
    cursor: pointer;
  }

  .chips button.on {
    border-color: var(--accent);
    color: var(--accent);
  }

  .empty {
    margin: 0;
    font-size: 11.5px;
    line-height: 1.8;
    color: var(--text-faint);
  }

  .result {
    margin: 0;
    font-family: var(--mono);
    font-size: 12.5px;
    line-height: 1.7;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    user-select: text;
  }

  code {
    padding: 1px 4px;
    border-radius: 4px;
    background: var(--kbd-bg);
    color: var(--text);
    font-family: var(--mono);
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

  /* Ctrl+K variable menu */
  .menu {
    position: fixed;
    z-index: 10;
    width: 300px;
    padding: 4px 0;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg-raised);
    box-shadow: var(--shadow);
    outline: none;
  }

  .menu-title {
    padding: 4px 10px 6px;
    font-size: 11px;
    color: var(--text-faint);
  }

  .menu button {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 4px 10px;
    border: none;
    background: transparent;
    font-size: 12px;
    text-align: left;
    cursor: pointer;
  }

  .menu button.sel {
    background: var(--bg-selected);
    color: var(--accent-soft);
  }

  .n {
    width: 10px;
    color: var(--text-faint);
  }

  .msyntax {
    margin-left: auto;
    font-size: 11.5px;
    color: var(--text-muted);
  }

  .menu-info {
    margin-top: 4px;
    padding: 6px 10px 4px;
    border-top: 1px solid var(--border-soft);
    font-size: 11px;
    line-height: 1.6;
    color: var(--text-muted);
  }
</style>

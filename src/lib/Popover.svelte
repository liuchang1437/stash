<script lang="ts">
  import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
  import { onMount, tick } from "svelte";
  import ActionMenu, { type MenuItem } from "./ActionMenu.svelte";
  import Peek from "./Peek.svelte";
  import { MARGIN, placePanels } from "./layout";
  import {
    ALL,
    api,
    groupDigits,
    SEARCH_LIMIT,
    shortTime,
    type CalcDetail,
    type Field,
    type Filter,
    type Hit,
    type Mode,
    type Segment,
    type Shown,
    type TagCount,
  } from "./api";
  import {
    appName,
    detectKind,
    isSolana,
    LABELS,
    lineCount,
    shortApp,
    splitMarks,
    TAGS,
    transformsFor,
    type Kind,
    type Piece,
  } from "./kinds";

  /** Height the popover may need; less room than this below the caret flips it up. */
  const NEEDED_BELOW = 380;

  /** Scopes offered by `#` besides snippet tags. */
  const SCOPES: { name: string; label: string; filter: Filter }[] = [
    { name: "clip", label: "剪贴板", filter: { source: "clip", tag: null } },
    { name: "snip", label: "Snippets", filter: { source: "snippet", tag: null } },
    { name: "pin", label: "置顶", filter: { source: "pinned", tag: null } },
  ];
  /** What Ctrl+Tab cycles through. */
  const CYCLE: Filter["source"][] = ["all", "clip", "snippet"];

  /** A `#…` completion: a scope or a snippet tag. */
  type Suggestion = { name: string; note: string; filter: Filter };

  type Phase = "list" | "fill" | "menu";
  type Fill = {
    key: string;
    title: string;
    fields: Field[];
    values: Record<string, string>;
    mode: Mode;
    segments: Segment[];
    focus: string;
  };

  let query = $state("");
  let hits = $state<Hit[]>([]);
  let selected = $state(0);
  let calc = $state<CalcDetail | null>(null);
  let calcFormat = $state(0);
  let phase = $state<Phase>("list");
  let fill = $state<Fill | null>(null);
  let peekOpen = $state(false);
  let peekRaw = $state(false);
  let menuIndex = $state(0);
  let subIndex = $state(-1);
  let status = $state("");
  let confirmDelete = $state<string | null>(null);
  let filter = $state.raw<Filter>(ALL);
  let tags = $state.raw<TagCount[]>([]);
  let suggestIndex = $state(0);

  let shown = $state<Shown>({
    keepQuery: false,
    anchor: "caret",
    space: { above: 1000, below: 1000, left: 1000, right: 2000 },
    targetApp: null,
    autoPeek: true,
  });

  let input: HTMLInputElement;
  let list: HTMLElement;
  let stage: HTMLElement;
  let card: HTMLElement;
  let peek: Peek | undefined = $state();
  let searchSeq = 0;

  const current = $derived(hits[selected] as Hit | undefined);
  const kind = $derived<Kind | undefined>(current && detectKind(current));
  const flip = $derived(shown.space.below < NEEDED_BELOW && shown.space.above > shown.space.below);
  const target = $derived(appName(shown.targetApp));
  const kinds = $derived(hits.map(detectKind));

  const peekShown = $derived(peekOpen && !!current && !!kind && kind !== "calc");

  // -------------------------------------------------------------------------
  // Scope: Ctrl+Tab or `#…` narrows the list to the clipboard, snippets,
  // pinned clips or one snippet tag.

  function scopeLabel(f: Filter): string | null {
    if (f.tag) return "#" + f.tag;
    return SCOPES.find((s) => s.filter.source === f.source)?.label ?? null;
  }

  const scope = $derived(scopeLabel(filter));

  /** `#…` alone in the input lists scopes and tags instead of results. */
  const suggestions = $derived.by<Suggestion[]>(() => {
    const m = /^#(\S*)$/.exec(query);
    if (!m) return [];
    const typed = m[1].toLowerCase();
    const all: Suggestion[] = [
      ...SCOPES.map((s) => ({ name: s.name, note: s.label, filter: s.filter })),
      ...tags.map((t) => ({
        name: t.name,
        note: `标签 · ${t.count} 个 snippet`,
        filter: { source: "snippet", tag: t.name } as Filter,
      })),
    ];
    const starts = (s: Suggestion) => Number(s.name.toLowerCase().startsWith(typed));
    return all.filter((s) => s.name.toLowerCase().includes(typed)).sort((a, b) => starts(b) - starts(a));
  });
  const completing = $derived(suggestions.length > 0);

  /** Browsing the clipboard: rows are grouped by day. */
  const groups = $derived(filter.source === "clip" && !query.trim() ? hits.map(dayGroup) : null);

  function dayGroup(hit: Hit): string {
    if (hit.pinned) return "置顶";
    const midnight = new Date().setHours(0, 0, 0, 0) / 1000;
    if (hit.lastUsedAt >= midnight) return "今天";
    if (hit.lastUsedAt >= midnight - 86400) return "昨天";
    return "更早";
  }

  const placeholder = $derived.by(() => {
    if (!scope) return "搜索，# 筛选，或输入算式 2*$1";
    // A space between Chinese and Latin text.
    return /^[\x00-\x7f]/.test(scope) ? `在 ${scope} 中搜索` : `在${scope}中搜索`;
  });

  const countLabel = $derived.by(() => {
    if (completing) return "筛选";
    if (!query && !scope) return "^Tab 切换类别";
    const n = hits.filter((h) => h.kind !== "calc").length;
    return `${n}${n >= SEARCH_LIMIT ? "+" : ""} 条`;
  });

  const emptyText = $derived.by(() => {
    if (query) return "没有匹配的结果";
    if (filter.tag || filter.source === "snippet") return "没有 snippet，^N 新建一个";
    if (filter.source === "pinned") return "没有置顶的记录，^P 置顶选中的一条";
    return "还没有记录，复制点什么试试";
  });

  function applyFilter(f: Filter) {
    filter = f;
    query = "";
  }

  function cycleScope(delta: number) {
    const at = filter.tag ? CYCLE.indexOf("snippet") : CYCLE.indexOf(filter.source);
    filter = { source: CYCLE[(at + delta + CYCLE.length) % CYCLE.length], tag: null };
  }

  function loadTags() {
    api.snippetTags().then((t) => (tags = t));
  }

  // -------------------------------------------------------------------------
  // Panel placement

  let cardHeight = $state(0);
  let peekHeight = $state(0);
  let menuHeight = $state(0);
  let subOverflow = $state(0);

  // Panel placement lives in layout.ts. Until a panel has been measured its
  // height is a guess (and the panel is drawn hidden), so the first frame
  // already lands in the right place.
  const geometry = $derived(
    placePanels({
      space: shown.space,
      flip,
      cardHeight: cardHeight || 300,
      peekHeight: peekShown ? peekHeight : null,
      menuHeight: phase === "menu" ? menuHeight || 280 : null,
      subOverflow,
    }),
  );

  // A panel is drawn only once measured, so it never flashes at a guess.
  $effect(() => {
    if (!peekShown) peekHeight = 0;
  });
  $effect(() => {
    if (phase !== "menu") menuHeight = 0;
  });

  const calcFormats = $derived.by(() => {
    if (!calc) return [];
    const raw = calc.result;
    const list = [{ label: raw, text: raw }];
    const grouped = groupDigits(raw);
    if (grouped !== raw) list.push({ label: grouped, text: grouped });
    const expr = `${calc.expression} = ${raw}`;
    list.push({ label: "算式 = 结果", text: expr });
    return list;
  });

  // -------------------------------------------------------------------------
  // Window geometry

  let lastLayout = "";

  function reportLayout() {
    const g = geometry;
    const layout = { width: Math.ceil(g.width), height: Math.ceil(g.height), cardX: g.origin.x, margin: MARGIN, flip };
    const key = JSON.stringify(layout);
    if (key === lastLayout) return;
    lastLayout = key;
    api.placePopover(layout);
  }

  $effect(reportLayout);

  // -------------------------------------------------------------------------
  // Search

  async function refresh() {
    const seq = ++searchSeq;
    if (completing) {
      hits = [];
      calc = null;
      return;
    }
    const q = query;
    const result = await api.search(q, filter);
    if (seq !== searchSeq) return;
    const detail = result[0]?.kind === "calc" ? await api.calcDetail(q) : null;
    if (seq !== searchSeq) return;
    hits = result;
    calc = detail;
    if (selected >= hits.length) selected = Math.max(0, hits.length - 1);
  }

  $effect(() => {
    query;
    filter;
    selected = 0;
    suggestIndex = 0;
    calcFormat = 0;
    confirmDelete = null;
    refresh();
  });

  /** Back to the plain list; the preview stays open when it opens by itself. */
  function closePanels() {
    phase = "list";
    fill = null;
    peekOpen = shown.autoPeek;
    subIndex = -1;
  }

  function reset() {
    status = "";
    closePanels();
    confirmDelete = null;
    if (query === "" && filter === ALL) {
      selected = 0;
      refresh();
    } else {
      query = "";
      filter = ALL;
    }
    input?.focus();
  }

  /** Reopened shortly after closing: keep the query and scope, the query selected so typing replaces it. */
  function resume() {
    status = "";
    closePanels();
    selected = 0;
    refresh();
    input?.focus();
    input?.select();
  }

  onMount(() => {
    const win = getCurrentWebviewWindow();
    const unlisteners = [
      win.listen<Shown>("launcher-shown", ({ payload }) => {
        shown = payload;
        lastLayout = "";
        loadTags();
        if (payload.keepQuery) resume();
        else reset();
        reportLayout();
      }),
      win.listen("index-changed", () => {
        loadTags();
        refresh();
      }),
    ];
    loadTags();
    const onBlur = () => api.hide();
    window.addEventListener("blur", onBlur);
    input.focus();
    return () => {
      window.removeEventListener("blur", onBlur);
      unlisteners.forEach((p) => p.then((f) => f()));
    };
  });

  // -------------------------------------------------------------------------
  // Actions

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

  async function move(delta: number) {
    if (!hits.length) return;
    selected = (selected + delta + hits.length) % hits.length;
    calcFormat = 0;
    confirmDelete = null;
    subIndex = -1;
    await tick();
    list?.querySelector(`[data-index="${selected}"]`)?.scrollIntoView({ block: "nearest" });
  }

  async function activate(mode: Mode, index = selected) {
    const hit = hits[index];
    if (!hit) return;
    if (hit.kind === "calc") {
      const format = calcFormats[calcFormat] ?? calcFormats[0];
      if (format) await run(() => api.activateText(null, format.text, mode));
      return;
    }
    const k = detectKind(hit);
    if (mode === "open" && isSolana(k)) {
      await run(() => api.openExplorer(hit.key));
      return;
    }
    if (mode === "open" && hit.kind === "clip" && k !== "url") {
      flash("这条不是网址");
      return;
    }
    const result = await run(() => api.activate(hit.key, mode));
    if (result?.status === "needsInput") startFill(hit, result.title, result.fields, mode);
  }

  async function pasteText(text: string, mode: Mode) {
    await run(() => api.activateText(current?.kind === "calc" ? null : (current?.key ?? null), text, mode));
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
    if (!hit || hit.kind === "calc") return;
    if (hit.kind === "snippet" && confirmDelete !== hit.key) {
      confirmDelete = hit.key;
      return;
    }
    confirmDelete = null;
    await run(() => api.deleteItem(hit.key));
    flash("已删除");
    refresh();
  }

  function newSnippet() {
    const body = current?.kind === "clip" ? current.preview : query;
    api.openManage("edit", null, body);
  }

  // -------------------------------------------------------------------------
  // Inline variables

  async function startFill(hit: Hit, title: string, fields: Field[], mode: Mode) {
    const values = Object.fromEntries(fields.map((f) => [f.name, f.default]));
    fill = { key: hit.key, title, fields, values, mode, segments: [], focus: fields[0]?.name ?? "" };
    phase = "fill";
    peekOpen = false;
    updatePreview();
    await tick();
    focusField(0);
  }

  let previewSeq = 0;
  async function updatePreview() {
    if (!fill) return;
    const seq = ++previewSeq;
    const segments = await run(() => api.previewSnippet(fill!.key, $state.snapshot(fill!.values)));
    if (seq === previewSeq && fill && segments) fill.segments = segments;
  }

  function focusField(i: number) {
    const el = list?.querySelector<HTMLElement>(`[data-field="${i}"]`);
    el?.focus();
    if (el instanceof HTMLInputElement) el.select();
  }

  function chooseOption(field: Field, index: number) {
    if (!fill || index < 0 || index >= field.options.length) return;
    fill.values[field.name] = field.options[index];
    updatePreview();
  }

  async function submitFill(mode?: Mode) {
    if (!fill) return;
    const { key, values } = fill;
    try {
      await api.activate(key, mode ?? fill.mode, $state.snapshot(values));
    } catch (e) {
      flash(String(e));
    }
  }

  function chipsKeydown(e: KeyboardEvent, field: Field) {
    if (!fill) return;
    const current = field.options.indexOf(fill.values[field.name]);
    if (/^[1-9]$/.test(e.key)) {
      chooseOption(field, Number(e.key) - 1);
      e.preventDefault();
    } else if (e.key === "ArrowRight" || e.key === "ArrowDown") {
      chooseOption(field, (current + 1) % field.options.length);
      e.preventDefault();
    } else if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
      chooseOption(field, (current - 1 + field.options.length) % field.options.length);
      e.preventDefault();
    }
  }

  // -------------------------------------------------------------------------
  // Ctrl+K menu

  type Entry = MenuItem & { run: (sub?: number) => void };

  const menu = $derived.by<Entry[]>(() => {
    const hit = current;
    const k = kind;
    if (!hit || !k) return [];
    const items: Entry[] = [];
    if (hit.kind === "calc") {
      items.push({ id: "paste", label: "粘贴", keys: "↵", run: () => activate("paste") });
      items.push({ id: "copy", label: "复制", keys: "⇧↵", run: () => activate("copy") });
      items.push({
        id: "as",
        label: "粘贴为…",
        sub: calcFormats.map((f, i) => ({ id: String(i), label: f.label, preview: f.text })),
        run: (sub) => sub !== undefined && pasteText(calcFormats[sub].text, "paste"),
      });
      return items;
    }
    const snippet = hit.kind === "snippet";
    items.push({ id: "paste", label: snippet ? "填写并粘贴" : "粘贴", keys: "↵", run: () => activate("paste") });
    items.push({ id: "copy", label: "复制", keys: "⇧↵", run: () => activate("copy") });
    if (isSolana(k)) items.push({ id: "solscan", label: "在 Solscan 打开", keys: "^↵", run: () => activate("open") });
    else if (k === "url" || snippet) items.push({ id: "open", label: "在浏览器打开", keys: "^↵", run: () => activate("open") });
    if (!snippet) {
      const transforms = transformsFor(k);
      items.push({
        id: "as",
        label: "粘贴为…",
        sub: transforms.map((t) => ({ id: t.id, label: t.label, preview: t.apply(hit.preview).replace(/\s+/g, " ").slice(0, 60) })),
        run: (sub) => sub !== undefined && pasteText(transforms[sub].apply(hit.preview), "paste"),
      });
    }
    if (!peekShown) {
      items.push({ id: "peek", label: "预览全文", keys: "→", run: () => ((phase = "list"), (peekOpen = true)) });
    }
    if (snippet) {
      items.push({ id: "edit", label: "编辑", keys: "^E", sep: true, run: () => api.openManage("edit", hit.key) });
      items.push({ id: "new", label: "新建 Snippet", keys: "^N", run: newSnippet });
    } else {
      items.push({ id: "pin", label: hit.pinned ? "取消置顶" : "置顶", keys: "^P", sep: true, run: togglePin });
      items.push({ id: "save", label: "存为 Snippet", keys: "^N", run: newSnippet });
    }
    items.push({ id: "delete", label: "删除", keys: "^D", danger: true, sep: true, run: remove });
    return items;
  });

  function openMenu() {
    if (!menu.length) return;
    // The preview stays where it is; the menu is drawn on top of it.
    phase = "menu";
    menuIndex = 0;
    subIndex = -1;
  }

  /** Leaves the menu without touching the preview underneath it. */
  function closeMenu() {
    phase = "list";
    subIndex = -1;
  }

  function pickMenu(index: number, sub?: number) {
    const item = menu[index];
    if (!item) return;
    if (item.sub && sub === undefined) {
      menuIndex = index;
      subIndex = 0;
      return;
    }
    phase = "list";
    subIndex = -1;
    item.run(sub);
  }

  // -------------------------------------------------------------------------
  // Keyboard

  function caretAtEnd(): boolean {
    return !input || (input.selectionStart === input.value.length && input.selectionEnd === input.value.length);
  }

  function caretAtStart(): boolean {
    return !input || (input.selectionStart === 0 && input.selectionEnd === 0);
  }

  /** A key press written the way the menu's shortcut column shows it. */
  function shortcutLabel(e: KeyboardEvent): string | null {
    const ctrl = e.ctrlKey || e.metaKey;
    if (e.key === "Enter") return ctrl ? "^↵" : e.shiftKey ? "⇧↵" : null;
    if (ctrl && /^[a-z]$/i.test(e.key)) return "^" + e.key.toUpperCase();
    return null;
  }

  function menuKeydown(e: KeyboardEvent) {
    const items = menu;
    const sub = subIndex >= 0 ? items[menuIndex]?.sub : undefined;
    // The shortcuts printed next to the items work while the menu is open.
    const label = shortcutLabel(e);
    const shortcut = label ? items.findIndex((item) => item.keys === label) : -1;
    if (shortcut >= 0) {
      e.preventDefault();
      pickMenu(shortcut);
      return;
    }
    if (e.key === "Escape" || (e.ctrlKey && e.key === "k")) {
      if (sub) subIndex = -1;
      else closeMenu();
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      const d = e.key === "ArrowDown" ? 1 : -1;
      if (sub) subIndex = (subIndex + d + sub.length) % sub.length;
      else menuIndex = (menuIndex + d + items.length) % items.length;
    } else if (e.key === "ArrowRight") {
      if (!sub && items[menuIndex]?.sub) subIndex = 0;
    } else if (e.key === "ArrowLeft") {
      if (sub) subIndex = -1;
      else closeMenu();
    } else if (e.key === "Enter") {
      pickMenu(menuIndex, sub ? subIndex : undefined);
    } else {
      return;
    }
    e.preventDefault();
  }

  function fillKeydown(e: KeyboardEvent) {
    if (!fill) return;
    if (e.key === "Escape") {
      closePanels();
      // The input is disabled while filling; focus it once re-enabled.
      tick().then(() => input.focus());
    } else if (e.key === "Enter") {
      submitFill(e.shiftKey ? "copy" : undefined);
    } else if (e.key === "Tab") {
      const i = fill.fields.findIndex((f) => f.name === fill!.focus);
      const n = fill.fields.length;
      focusField((i + (e.shiftKey ? n - 1 : 1)) % n);
    } else {
      return;
    }
    e.preventDefault();
  }

  /** Keys while `#…` suggestions replace the list; true when handled. */
  function suggestKeydown(e: KeyboardEvent): boolean {
    if (e.isComposing) return false;
    const n = suggestions.length;
    const down = flip ? -1 : 1;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      suggestIndex = (suggestIndex + (e.key === "ArrowDown" ? down : -down) + n) % n;
      tick().then(() => list?.querySelector(`[data-suggestion="${suggestIndex}"]`)?.scrollIntoView({ block: "nearest" }));
    } else if (e.key === "Enter" || e.key === "Tab") {
      applyFilter(suggestions[suggestIndex].filter);
    } else if (e.key === " ") {
      // `#work␣` takes an exact match right away; otherwise the space is
      // typed and `#work …` is searched as text.
      const typed = query.slice(1).toLowerCase();
      const exact = suggestions.find((s) => s.name.toLowerCase() === typed);
      if (!exact) return false;
      applyFilter(exact.filter);
    } else {
      return false;
    }
    return true;
  }

  function listKeydown(e: KeyboardEvent) {
    const ctrl = e.ctrlKey || e.metaKey;
    if (ctrl && e.key === "Tab") {
      cycleScope(e.shiftKey ? -1 : 1);
      e.preventDefault();
      return;
    }
    if (completing && suggestKeydown(e)) {
      e.preventDefault();
      return;
    }
    // Opening upwards puts item 1 next to the input, at the bottom.
    const down = flip ? -1 : 1;
    if (e.key === "ArrowDown" || (ctrl && e.key === "j")) move(down);
    else if (e.key === "ArrowUp") move(-down);
    else if (e.key === "PageDown") peekOpen ? peek?.scroll(240) : move(8 * down);
    else if (e.key === "PageUp") peekOpen ? peek?.scroll(-240) : move(-8 * down);
    else if (e.key === "Enter") activate(ctrl ? "open" : e.shiftKey ? "copy" : "paste");
    else if (e.altKey && /^[1-9]$/.test(e.key)) {
      const n = Number(e.key) - 1;
      if (n < hits.length) {
        selected = n;
        activate("paste", n);
      }
    } else if (ctrl && e.key === "k") openMenu();
    else if (e.key === "Tab") {
      if (current?.kind === "calc") calcFormat = (calcFormat + (e.shiftKey ? calcFormats.length - 1 : 1)) % calcFormats.length;
      else if (peekOpen) peekRaw = !peekRaw;
    } else if (e.key === "ArrowRight" && !peekOpen && caretAtEnd()) {
      if (current && current.kind !== "calc") peekOpen = true;
      else return;
    } else if (e.key === "ArrowLeft" && peekOpen && caretAtStart()) {
      // Mirrors →: only once the caret can't move further, so ← still
      // edits the query while the preview is open.
      peekOpen = false;
    } else if (e.key === "Escape") {
      if (confirmDelete) confirmDelete = null;
      // An automatic preview is part of the layout, not something to dismiss
      // first; Esc goes straight to clearing / hiding.
      else if (peekOpen && !shown.autoPeek) peekOpen = false;
      else if (query) query = "";
      else if (scope) filter = ALL;
      else api.hide();
    } else if (e.key === "Backspace" && !query && scope) filter = ALL;
    else if (ctrl && e.key === "p") togglePin();
    else if (ctrl && e.key === "d") remove();
    else if (ctrl && e.key === "n") newSnippet();
    else if (ctrl && e.key === "e" && current?.kind === "snippet") api.openManage("edit", current.key);
    else if (ctrl && e.key === ",") api.openManage("settings");
    else return;
    e.preventDefault();
  }

  function onkeydown(e: KeyboardEvent) {
    if (phase === "menu") menuKeydown(e);
    else if (phase === "fill") fillKeydown(e);
    else listKeydown(e);
  }

  function excerpt(hit: Hit, k: Kind): string {
    if (hit.kind === "snippet") {
      const tags = hit.tags.map((t) => "#" + t).join(" ");
      return [tags, hit.useCount ? `用过 ${hit.useCount} 次` : "", "^E 编辑"].filter(Boolean).join(" · ");
    }
    if (k === "lines") {
      // The title is the first line, or the line that matched the query.
      const lines = hit.preview.split(/\r?\n/).map((l) => l.trim()).filter(Boolean);
      const other = lines[0]?.startsWith(hit.title) ? lines[1] : lines[0];
      return `${lineCount(hit.preview)} 行 · ${other ?? ""}`;
    }
    const parts = [LABELS[k], `${hit.chars.toLocaleString()} 字`];
    if (k === "table") parts.push("→ 看排版");
    if (isSolana(k)) parts.push("^↵ Solscan");
    if (k === "url") parts.push("^↵ 打开");
    return parts.join(" · ");
  }

  function onStageDown(e: MouseEvent) {
    // The transparent border around the cards counts as "outside".
    if (e.target === stage) api.hide();
  }
</script>

<svelte:window {onkeydown} />

{#snippet marked(pieces: Piece[])}{#each pieces as p}{#if p.hit}<mark>{p.text}</mark>{:else}{p.text}{/if}{/each}{/snippet}

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="stage"
  bind:this={stage}
  style:width="{geometry.width}px"
  style:height="{geometry.height}px"
  onmousedown={onStageDown}
>
  <div
    class="card"
    class:flip
    bind:this={card}
    bind:offsetHeight={cardHeight}
    style:left="{geometry.origin.x}px"
    style:top="{geometry.origin.y}px"
  >
    <div class="prompt">
      {#if scope}
        <button
          class="scope"
          tabindex="-1"
          title="Backspace 回到全部"
          onclick={() => {
            filter = ALL;
            input.focus();
          }}>{scope}</button
        >
      {/if}
      <span class="caret-mark">›</span>
      <input
        bind:this={input}
        bind:value={query}
        {placeholder}
        spellcheck="false"
        autocomplete="off"
        disabled={phase === "fill"}
      />
      {#if shown.anchor === "mouse"}<span class="note">未找到光标</span>{/if}
      <span class="count">{countLabel}</span>
    </div>

    <ul class="list" role="listbox" bind:this={list}>
      <!-- While completing `#…` there are no hits, and otherwise no suggestions. -->
      {#each suggestions as s, i (`${s.filter.source}:${s.filter.tag}`)}
        {@const sel = i === suggestIndex}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <li
          class="item"
          class:sel
          role="option"
          aria-selected={sel}
          data-suggestion={i}
          onclick={() => applyFilter(s.filter)}
        >
          <div class="row">
            <span class="num">{sel ? "▸" : ""}</span>
            <span class="title">#{s.name}</span>
            <span class="src">{s.note}</span>
          </div>
        </li>
      {/each}
      {#each hits as hit, i (hit.key)}
        {@const k = kinds[i]}
        {@const sel = i === selected}
        {#if groups && groups[i] !== groups[i - 1]}
          <li class="divider" role="presentation">{groups[i]}</li>
        {/if}
        {#if hit.kind === "calc" && calc}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <li class="calc" class:sel role="option" aria-selected={sel} data-index={i} onclick={() => (selected = i)}>
            <div class="calc-result">= {groupDigits(calc.result)}</div>
            <div class="calc-expr">{calc.expression}</div>
            {#each calc.refs as ref}
              <div class="calc-ref">
                <span class="ref-n">${ref.n}</span>
                <span class="ref-v">{ref.value}</span>
                <span class="ref-src">{shortApp(ref.source)} {shortTime(ref.lastUsedAt)}</span>
              </div>
            {/each}
            {#if sel}
              <div class="formats">
                {#each calcFormats as f, j}
                  <button class:on={j === calcFormat} onclick={() => ((calcFormat = j), activate("paste"))}>{f.label}</button>
                {/each}
                <span class="fmt-hint">Tab 换格式</span>
              </div>
            {/if}
          </li>
        {:else}
          <li class="item" class:sel class:dim={phase === "fill" && !sel} role="option" aria-selected={sel} data-index={i}>
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <div class="row" onclick={() => (selected = i)} ondblclick={() => activate("paste", i)}>
            <span class="num">{sel ? "▸" : i < 9 ? String(i + 1).padStart(2, "0") : "  "}</span>
            <span class="tag {k}">{TAGS[k]}</span>
            {#if hit.pinned}<span class="pin">◆</span>{/if}
            <span class="title">{@render marked(splitMarks(hit.title || "（空白）", hit.titleMarks))}</span>
            <span class="src">{hit.kind === "snippet" ? "snippet" : `${shortApp(hit.source)} ${shortTime(hit.lastUsedAt)}`}</span>
          </div>
          {#if sel && phase === "fill" && fill}
            <div class="fill">
              {#each fill.fields as field, fi (field.name)}
                <div class="frow" class:focus={fill.focus === field.name}>
                  <span class="fname">{field.name}</span>
                  {#if field.options.length}
                    <div
                      class="chips"
                      role="radiogroup"
                      aria-label={field.name}
                      tabindex="0"
                      data-field={fi}
                      onfocus={() => fill && (fill.focus = field.name)}
                      onkeydown={(e) => chipsKeydown(e, field)}
                    >
                      {#each field.options as option, oi}
                        <button
                          tabindex="-1"
                          role="radio"
                          aria-checked={fill.values[field.name] === option}
                          class:on={fill.values[field.name] === option}
                          onclick={() => chooseOption(field, oi)}>{oi < 9 ? `${oi + 1} ` : ""}{option}</button
                        >
                      {/each}
                    </div>
                  {:else}
                    <input
                      class="finput"
                      data-field={fi}
                      aria-label={field.name}
                      bind:value={fill.values[field.name]}
                      oninput={updatePreview}
                      onfocus={() => fill && (fill.focus = field.name)}
                      spellcheck="false"
                    />
                  {/if}
                </div>
              {/each}
              <div class="rendered">
                <span class="r-label">将粘贴 ›</span>
                <span class="r-text"
                  >{#each fill.segments as seg}{#if seg.field === "cursor"}<span class="r-cursor"></span
                      >{:else if seg.field}<span class="r-var" class:active={seg.field === fill.focus}>{seg.text}</span
                      >{:else}{seg.text}{/if}{/each}</span
                >
              </div>
            </div>
          {:else if sel && phase !== "fill"}
            <div class="excerpt">
              {#if hit.context}{@render marked(splitMarks(hit.context, hit.contextMarks))}{:else}{excerpt(hit, k)}{/if}
            </div>
          {/if}
          </li>
        {/if}
      {:else}
        {#if !completing}<li class="empty" role="presentation">{emptyText}</li>{/if}
      {/each}
    </ul>

    <div class="status">
      {#if status}
        <span class="seg msg">{status}</span>
      {:else if confirmDelete}
        <span class="seg danger">再按 ^D 删除这个 snippet 文件 · Esc 取消</span>
      {:else if phase === "fill"}
        <span class="seg primary">↵ 粘贴 → {target}</span>
        <span class="seg">⇧↵ 复制</span>
        <span class="seg">Tab 下一项</span>
        <span class="seg">Esc 返回</span>
      {:else if phase === "menu"}
        <span class="seg primary">↵ 执行</span>
        <span class="seg">↑↓ 选择</span>
        <span class="seg">→ 子菜单</span>
        <span class="seg">Esc 关闭</span>
      {:else if completing}
        <span class="seg primary">↵ 筛选</span>
        <span class="seg">↑↓ 选择</span>
        <span class="seg">Esc 取消</span>
      {:else if current?.kind === "calc"}
        <span class="seg primary">↵ 粘贴 {calcFormats[calcFormat]?.label ?? ""}</span>
        <span class="seg">⇧↵ 复制</span>
        <span class="seg">^K 操作</span>
      {:else if current}
        <span class="seg primary">↵ 粘贴 → {target}</span>
        <span class="seg">⇧↵ 复制</span>
        <span class="seg">{peekShown ? "← 收起" : "→ 预览"}</span>
        <span class="seg">^K 操作</span>
      {:else}
        <span class="seg">^N 新建 Snippet</span>
        <span class="seg">^, 设置</span>
      {/if}
    </div>
  </div>

  <!-- Panels are placed by `geometry`; hidden until measured. -->
  {#if geometry.peek && current && kind}
    <div
      class="float"
      style:left="{geometry.origin.x + geometry.peek.x}px"
      style:top="{geometry.origin.y + geometry.peek.y}px"
      style:visibility={peekHeight ? null : "hidden"}
      bind:offsetHeight={peekHeight}
    >
      <Peek bind:this={peek} hit={current} {kind} raw={peekRaw} onToggle={() => (peekRaw = !peekRaw)} />
    </div>
  {/if}
  {#if geometry.menu && current}
    <div
      class="float menu-float"
      style:left="{geometry.origin.x + geometry.menu.x}px"
      style:top="{geometry.origin.y + geometry.menu.y}px"
      style:visibility={menuHeight ? null : "hidden"}
      bind:offsetHeight={menuHeight}
    >
      <ActionMenu
        items={menu}
        selected={menuIndex}
        subSelected={subIndex}
        leftward={geometry.menu.leftward}
        upward={flip}
        bind:overflow={subOverflow}
        onPick={pickMenu}
        onHover={(i, sub) => {
          if (sub !== undefined) subIndex = sub;
          else if (i !== menuIndex) {
            menuIndex = i;
            subIndex = -1;
          }
        }}
      />
    </div>
  {/if}
</div>

<style>
  .stage {
    /* Everything inside is placed absolutely from `geometry`. */
    position: relative;
  }

  .float {
    position: absolute;
  }

  .menu-float {
    z-index: 1;
  }

  .card {
    position: absolute;
    width: 440px;
    display: flex;
    flex-direction: column;
    border-radius: 10px;
    border: 1px solid var(--border);
    background: var(--bg);
    box-shadow: var(--shadow);
    overflow: hidden;
  }

  /* Opening upwards: input next to the caret (bottom), item 1 right above it. */
  .card.flip {
    flex-direction: column-reverse;
  }

  .card.flip .list {
    flex-direction: column-reverse;
  }

  .card.flip .prompt {
    border-bottom: none;
    border-top: 1px solid var(--border-soft);
  }

  .card.flip .status {
    border-top: none;
    border-bottom: 1px solid var(--border-soft);
  }

  .prompt {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 42px;
    flex: none;
    padding: 0 12px 0 14px;
    border-bottom: 1px solid var(--border-soft);
  }

  .caret-mark {
    color: var(--accent);
    font-size: 16px;
    font-weight: 700;
  }

  .scope {
    flex: none;
    max-width: 140px;
    margin-right: -4px;
    padding: 1px 7px;
    border: 1px solid var(--accent);
    border-radius: 4px;
    background: transparent;
    font-size: 11.5px;
    color: var(--accent);
    cursor: pointer;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .prompt input {
    flex: 1;
    min-width: 0;
    border: none;
    outline: none;
    background: transparent;
    font-size: 14px;
    caret-color: var(--accent);
  }

  .prompt input::placeholder {
    color: var(--text-faint);
  }

  .note {
    font-size: 11px;
    color: var(--accent);
  }

  .count {
    font-size: 11px;
    color: var(--text-faint);
  }

  .list {
    display: flex;
    flex-direction: column;
    max-height: 372px;
    margin: 0;
    padding: 4px 0;
    list-style: none;
    overflow-y: auto;
  }

  .item {
    flex: none;
  }

  .item.sel {
    background: var(--bg-selected);
  }

  .item.dim {
    opacity: 0.45;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 30px;
    padding: 0 14px;
    cursor: default;
  }

  .num {
    width: 16px;
    flex: none;
    font-size: 11px;
    color: var(--text-faint);
    white-space: pre;
  }

  .item.sel .num {
    color: var(--accent);
  }

  .tag {
    width: 34px;
    flex: none;
    font-size: 10.5px;
    font-weight: 700;
    letter-spacing: 0.04em;
    color: var(--text-muted);
  }

  .tag.addr,
  .tag.sig,
  .tag.url {
    color: var(--blue);
  }

  .tag.cmd {
    color: var(--green);
  }

  .tag.snippet {
    color: var(--snippet);
  }

  .item.sel .tag {
    color: var(--accent);
  }

  .pin {
    flex: none;
    font-size: 9px;
    color: var(--accent);
  }

  .title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .item.sel .title {
    color: var(--accent-soft);
  }

  .src {
    flex: none;
    font-size: 11px;
    color: var(--text-faint);
  }

  .excerpt {
    padding: 0 14px 7px 84px;
    font-size: 11.5px;
    color: var(--text-muted);
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .divider {
    flex: none;
    padding: 6px 14px 2px 40px;
    font-size: 10.5px;
    color: var(--text-faint);
  }

  .empty {
    padding: 22px 14px;
    color: var(--text-faint);
    text-align: center;
  }

  /* Calculator */
  .calc {
    flex: none;
    padding: 8px 14px 10px;
    display: flex;
    flex-direction: column;
    gap: 3px;
    cursor: default;
  }

  .calc.sel {
    background: var(--bg-selected);
  }

  .calc-result {
    font-size: 22px;
    font-weight: 700;
    color: var(--accent);
    font-variant-numeric: tabular-nums;
    user-select: text;
  }

  .calc-expr {
    font-size: 11.5px;
    color: var(--text-muted);
    overflow-wrap: anywhere;
  }

  .calc-ref {
    display: flex;
    gap: 8px;
    font-size: 11.5px;
    color: var(--text-muted);
  }

  .ref-n {
    color: var(--blue);
  }

  .ref-v {
    flex: 1;
    min-width: 0;
    color: var(--text);
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .ref-src {
    color: var(--text-faint);
  }

  .formats {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 6px;
  }

  .formats button {
    height: 24px;
    max-width: 180px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: transparent;
    font-size: 11.5px;
    color: var(--text);
    cursor: pointer;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .formats button.on {
    border-color: var(--accent);
    color: var(--accent);
  }

  .fmt-hint {
    font-size: 11px;
    color: var(--text-faint);
  }

  /* Inline variables */
  .fill {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 4px 14px 10px 40px;
  }

  .frow {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 2px 4px;
    margin-left: -4px;
    border-radius: 4px;
  }

  .frow.focus {
    box-shadow: inset 0 0 0 1px var(--accent);
  }

  .fname {
    width: 70px;
    flex: none;
    font-size: 11.5px;
    color: var(--violet);
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    outline: none;
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

  .finput {
    flex: 1;
    min-width: 0;
    height: 22px;
    padding: 0 7px;
    border: 1px solid var(--border);
    border-radius: 3px;
    background: var(--bg);
    font-size: 11.5px;
    outline: none;
  }

  .rendered {
    display: flex;
    gap: 8px;
    margin-top: 4px;
    font-size: 11.5px;
    line-height: 1.6;
  }

  .r-label {
    flex: none;
    color: var(--text-faint);
  }

  .r-text {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    color: var(--text-muted);
    max-height: 80px;
    overflow: hidden;
  }

  .r-var {
    color: var(--green);
  }

  .r-var.active {
    color: var(--accent);
    text-decoration: underline;
    text-underline-offset: 3px;
  }

  .r-cursor {
    display: inline-block;
    width: 1px;
    height: 12px;
    vertical-align: -1px;
    background: var(--accent);
  }

  /* Status line */
  .status {
    display: flex;
    align-items: stretch;
    height: 26px;
    flex: none;
    border-top: 1px solid var(--border-soft);
    background: var(--bg-subtle);
    font-size: 11px;
    white-space: nowrap;
    overflow: hidden;
  }

  .seg {
    display: flex;
    align-items: center;
    padding: 0 10px;
    color: var(--text-muted);
    border-right: 1px solid var(--border-soft);
  }

  .seg.primary {
    background: var(--accent);
    color: var(--accent-text);
    font-weight: 700;
    max-width: 220px;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .seg.msg {
    color: var(--text);
  }

  .seg.danger {
    color: var(--danger);
  }

</style>

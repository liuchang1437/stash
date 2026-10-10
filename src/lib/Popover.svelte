<script lang="ts">
  import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
  import { onMount, tick } from "svelte";
  import ActionMenu, { type MenuItem } from "./ActionMenu.svelte";
  import { t } from "./i18n.svelte";
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
    kindLabel,
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
  const SCOPES: { name: "clip" | "snip" | "pin"; filter: Filter }[] = [
    { name: "clip", filter: { source: "clip", tag: null } },
    { name: "snip", filter: { source: "snippet", tag: null } },
    { name: "pin", filter: { source: "pinned", tag: null } },
  ];
  /** What Ctrl+Tab cycles through. */
  const CYCLE: Filter["source"][] = ["all", "clip", "snippet"];

  /** A `#…` completion: a scope or a snippet tag. */
  type Suggestion = { name: string; note: string; filter: Filter };

  type Phase = "list" | "fill" | "menu" | "edit";
  /** The text about to be pasted, edited in the preview; the item itself stays as it is. */
  type Draft = {
    key: string;
    text: string;
    /** The selection it starts with (UTF-16): what was clicked, or where pasting would leave the caret. */
    caret: [number, number];
    /** The original used CRLF; the textarea only has LF. */
    crlf: boolean;
    /** Where Esc goes back to. */
    back: "list" | "fill";
  };
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
  let draft = $state<Draft | null>(null);
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
    cardWidth: 440,
    peekWidth: 480,
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
    const s = SCOPES.find((s) => s.filter.source === f.source);
    return s ? t.popover.scopes[s.name] : null;
  }

  const scope = $derived(scopeLabel(filter));

  /** `#…` alone in the input lists scopes and tags instead of results. */
  const suggestions = $derived.by<Suggestion[]>(() => {
    const m = /^#(\S*)$/.exec(query);
    if (!m) return [];
    const typed = m[1].toLowerCase();
    const all: Suggestion[] = [
      ...SCOPES.map((s) => ({ name: s.name, note: t.popover.scopes[s.name], filter: s.filter })),
      ...tags.map((tag) => ({
        name: tag.name,
        note: t.popover.tagNote(tag.count),
        filter: { source: "snippet", tag: tag.name } as Filter,
      })),
    ];
    const starts = (s: Suggestion) => Number(s.name.toLowerCase().startsWith(typed));
    return all.filter((s) => s.name.toLowerCase().includes(typed)).sort((a, b) => starts(b) - starts(a));
  });
  const completing = $derived(suggestions.length > 0);

  /** Browsing the clipboard: rows are grouped by day. */
  const groups = $derived(filter.source === "clip" && !query.trim() ? hits.map(dayGroup) : null);

  function dayGroup(hit: Hit): string {
    if (hit.pinned) return t.popover.groups.pinned;
    const midnight = new Date().setHours(0, 0, 0, 0) / 1000;
    if (hit.lastUsedAt >= midnight) return t.popover.groups.today;
    if (hit.lastUsedAt >= midnight - 86400) return t.popover.groups.yesterday;
    return t.popover.groups.earlier;
  }

  const placeholder = $derived.by(() => {
    if (!scope) return t.popover.placeholder;
    return t.popover.searchIn(scope);
  });

  const countLabel = $derived.by(() => {
    if (completing) return t.popover.filtering;
    if (!query && !scope) return t.popover.switchScope;
    const n = hits.filter((h) => h.kind !== "calc").length;
    return t.popover.results(`${n}${n >= SEARCH_LIMIT ? "+" : ""}`);
  });

  const emptyText = $derived.by(() => {
    if (query) return t.popover.noMatch;
    if (filter.tag || filter.source === "snippet") return t.popover.noSnippets;
    if (filter.source === "pinned") return t.popover.noPinned;
    return t.popover.noClips;
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
    api.snippetTags().then((list) => (tags = list));
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
      cardWidth: shown.cardWidth,
      peekWidth: shown.peekWidth,
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
    list.push({ label: t.popover.calcExpression, text: expr });
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
    draft = null;
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
    // The input is disabled while filling in or editing; focus it once re-enabled.
    tick().then(() => input?.focus());
  }

  /** Reopened shortly after closing: keep the query and scope, the query selected so typing replaces it. */
  function resume() {
    status = "";
    closePanels();
    selected = 0;
    refresh();
    tick().then(() => {
      input?.focus();
      input?.select();
    });
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
      flash(t.popover.notUrl);
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
      flash(pinned ? t.popover.pinned : t.popover.unpinned);
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
    flash(t.popover.deleted);
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
    const segments = hit.rendered ?? [];
    fill = { key: hit.key, title, fields, values, mode, segments, focus: fields[0]?.name ?? "" };
    phase = "fill";
    // The preview shows the result while filling in, even if it opens only on →.
    peekOpen = true;
    peekRaw = false;
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
  // Editing before pasting (a click into the preview, or F2): the preview
  // turns into a textarea holding exactly what would be pasted, a snippet with
  // the values filled in so far. Nothing is saved, and the edited text is
  // pasted like a transformation, so it doesn't enter the history either.

  /** `pick` places the caret where the preview was clicked. */
  async function startEdit(pick?: (text: string) => [number, number]) {
    const hit = current;
    if (!hit || hit.kind === "calc" || phase === "edit") return;
    const filling = phase === "fill" && fill?.key === hit.key ? fill : null;
    const values = filling ? $state.snapshot(filling.values) : {};
    const result = await run(() => api.editableText(hit.key, values));
    // The selection may have moved on meanwhile.
    if (!result || current?.key !== hit.key) return;
    const text = result.text.replace(/\r\n/g, "\n");
    // `cursorBack` counts characters after `{{cursor}}`, not UTF-16 units.
    const chars = Array.from(text);
    const end = chars.slice(0, chars.length - result.cursorBack).join("").length;
    const caret = pick ? pick(text) : ([end, end] as [number, number]);
    draft = { key: hit.key, text, caret, crlf: result.text.includes("\r\n"), back: filling ? "fill" : "list" };
    phase = "edit";
    peekOpen = true;
  }

  /** Drops the edits and goes back to the list or the fields. */
  function cancelEdit() {
    phase = draft?.back === "fill" && fill ? "fill" : "list";
    draft = null;
    tick().then(() => {
      if (phase === "fill" && fill) focusField(Math.max(0, fill.fields.findIndex((f) => f.name === fill!.focus)));
      else input.focus();
    });
  }

  async function submitDraft(mode: Mode) {
    if (!draft) return;
    const text = draft.crlf ? draft.text.replace(/\n/g, "\r\n") : draft.text;
    await run(() => api.activateText(draft!.key, text, mode));
  }

  // Selecting another row (a click) leaves the edits behind.
  $effect(() => {
    if (draft && current?.key !== draft.key) closePanels();
  });

  // -------------------------------------------------------------------------
  // Ctrl+K menu

  type Entry = MenuItem & { run: (sub?: number) => void };

  const menu = $derived.by<Entry[]>(() => {
    const hit = current;
    const k = kind;
    if (!hit || !k) return [];
    const items: Entry[] = [];
    if (hit.kind === "calc") {
      items.push({ id: "paste", label: t.action.paste, keys: "↵", run: () => activate("paste") });
      items.push({ id: "copy", label: t.action.copy, keys: "⇧↵", run: () => activate("copy") });
      items.push({
        id: "as",
        label: t.action.pasteAs,
        sub: calcFormats.map((f, i) => ({ id: String(i), label: f.label, preview: f.text })),
        run: (sub) => sub !== undefined && pasteText(calcFormats[sub].text, "paste"),
      });
      return items;
    }
    const snippet = hit.kind === "snippet";
    items.push({ id: "paste", label: snippet ? t.action.fillAndPaste : t.action.paste, keys: "↵", run: () => activate("paste") });
    items.push({ id: "copy", label: t.action.copy, keys: "⇧↵", run: () => activate("copy") });
    if (isSolana(k)) items.push({ id: "solscan", label: t.action.openSolscan, keys: "^↵", run: () => activate("open") });
    else if (k === "url" || snippet) items.push({ id: "open", label: t.action.openBrowser, keys: "^↵", run: () => activate("open") });
    if (!snippet) {
      const transforms = transformsFor(k);
      items.push({
        id: "as",
        label: t.action.pasteAs,
        sub: transforms.map((tf) => ({ id: tf.id, label: tf.label, preview: tf.apply(hit.preview).replace(/\s+/g, " ").slice(0, 60) })),
        run: (sub) => sub !== undefined && pasteText(transforms[sub].apply(hit.preview), "paste"),
      });
    }
    items.push({ id: "draft", label: t.action.editThenPaste, keys: "F2", run: () => startEdit() });
    if (!peekShown) {
      items.push({ id: "peek", label: t.action.preview, keys: "→", run: () => ((phase = "list"), (peekOpen = true)) });
    }
    if (snippet) {
      items.push({ id: "edit", label: t.action.edit, keys: "^E", sep: true, run: () => api.openManage("edit", hit.key) });
      items.push({ id: "new", label: t.action.newSnippet, keys: "^N", run: newSnippet });
    } else {
      items.push({ id: "pin", label: hit.pinned ? t.action.unpin : t.action.pin, keys: "^P", sep: true, run: togglePin });
      items.push({ id: "save", label: t.action.saveAsSnippet, keys: "^N", run: newSnippet });
    }
    items.push({ id: "delete", label: t.action.delete, keys: "^D", danger: true, sep: true, run: remove });
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
    if (e.key === "F2") return "F2";
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
    } else if (e.key === "F2") {
      startEdit();
    } else {
      return;
    }
    e.preventDefault();
  }

  function editKeydown(e: KeyboardEvent) {
    if (e.isComposing) return;
    // Enter alone is a new line.
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) submitDraft(e.shiftKey ? "copy" : "paste");
    else if (e.key === "Escape") cancelEdit();
    else return;
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
    else if (e.key === "F2") startEdit();
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
    else if (phase === "edit") editKeydown(e);
    else listKeydown(e);
  }

  function excerpt(hit: Hit, k: Kind): string {
    if (hit.kind === "snippet") {
      const tags = hit.tags.map((tag) => "#" + tag).join(" ");
      return [tags, hit.useCount ? t.popover.usedTimes(hit.useCount) : "", `^E ${t.action.edit}`].filter(Boolean).join(" · ");
    }
    if (k === "lines") {
      // The title is the first line, or the line that matched the query.
      const lines = hit.preview.split(/\r?\n/).map((l) => l.trim()).filter(Boolean);
      const other = lines[0]?.startsWith(hit.title) ? lines[1] : lines[0];
      return `${t.count.lines(lineCount(hit.preview))} · ${other ?? ""}`;
    }
    const parts = [kindLabel(k), t.count.chars(hit.chars)];
    if (k === "table") parts.push(t.popover.seeLayout);
    if (isSolana(k)) parts.push("^↵ Solscan");
    if (k === "url") parts.push(`^↵ ${t.popover.open}`);
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
    style:width="{shown.cardWidth}px"
  >
    <div class="prompt">
      {#if scope}
        <button
          class="scope"
          tabindex="-1"
          title={t.popover.backToAll}
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
        disabled={phase === "fill" || phase === "edit"}
      />
      {#if shown.anchor === "mouse"}<span class="note">{t.popover.caretNotFound}</span>{/if}
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
                <span class="fmt-hint">{t.popover.nextFormat}</span>
              </div>
            {/if}
          </li>
        {:else}
          <li class="item" class:sel class:dim={(phase === "fill" || phase === "edit") && !sel} role="option" aria-selected={sel} data-index={i}>
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <div class="row" onclick={() => (selected = i)} ondblclick={() => activate("paste", i)}>
            <span class="num">{sel ? "▸" : i < 9 ? String(i + 1).padStart(2, "0") : "  "}</span>
            <span class="tag {k}">{TAGS[k]}</span>
            {#if hit.pinned}<span class="pin">◆</span>{/if}
            <span class="title">{@render marked(splitMarks(hit.title || t.popover.blank, hit.titleMarks))}</span>
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
        <span class="seg danger">{t.popover.confirmDelete}</span>
      {:else if phase === "edit"}
        <span class="seg primary">^↵ {t.action.paste} → {target}</span>
        <span class="seg">^⇧↵ {t.action.copy}</span>
        <span class="seg">Esc {t.action.cancelEdit}</span>
      {:else if phase === "fill"}
        <span class="seg primary">↵ {t.action.paste} → {target}</span>
        <span class="seg">⇧↵ {t.action.copy}</span>
        <span class="seg">Tab {t.action.next}</span>
        <span class="seg">Esc {t.action.back}</span>
      {:else if phase === "menu"}
        <span class="seg primary">↵ {t.action.run}</span>
        <span class="seg">↑↓ {t.action.select}</span>
        <span class="seg">→ {t.action.submenu}</span>
        <span class="seg">Esc {t.action.close}</span>
      {:else if completing}
        <span class="seg primary">↵ {t.action.filter}</span>
        <span class="seg">↑↓ {t.action.select}</span>
        <span class="seg">Esc {t.action.cancel}</span>
      {:else if current?.kind === "calc"}
        <span class="seg primary">↵ {t.action.paste} {calcFormats[calcFormat]?.label ?? ""}</span>
        <span class="seg">⇧↵ {t.action.copy}</span>
        <span class="seg">^K {t.action.actions}</span>
      {:else if current}
        <span class="seg primary">↵ {t.action.paste} → {target}</span>
        <span class="seg">⇧↵ {t.action.copy}</span>
        <span class="seg">{peekShown ? `← ${t.action.collapse}` : `→ ${t.action.peek}`}</span>
        <span class="seg">^K {t.action.actions}</span>
      {:else}
        <span class="seg">^N {t.action.newSnippet}</span>
        <span class="seg">^, {t.action.settings}</span>
      {/if}
    </div>
  </div>

  <!-- Panels are placed by `geometry`; hidden until measured. -->
  {#if geometry.peek && current && kind}
    {@const filling = phase === "fill" && fill?.key === current.key ? fill : null}
    <div
      class="float"
      style:left="{geometry.origin.x + geometry.peek.x}px"
      style:top="{geometry.origin.y + geometry.peek.y}px"
      style:width="{shown.peekWidth}px"
      style:visibility={peekHeight ? null : "hidden"}
      bind:offsetHeight={peekHeight}
    >
      <Peek
        bind:this={peek}
        hit={current}
        {kind}
        raw={peekRaw}
        onToggle={() => (peekRaw = !peekRaw)}
        rendered={filling ? filling.segments : current.rendered}
        focus={filling ? filling.focus : null}
        draft={draft?.key === current.key ? draft : null}
        onDraft={(text) => draft && (draft.text = text)}
        onEdit={startEdit}
      />
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

<script lang="ts" module>
  export type SubItem = { id: string; label: string; preview: string };
  export type MenuItem = {
    id: string;
    label: string;
    keys?: string;
    danger?: boolean;
    /** Draw a separator above this item. */
    sep?: boolean;
    sub?: SubItem[];
  };
</script>

<script lang="ts">
  type Props = {
    items: MenuItem[];
    selected: number;
    /** Index into the submenu, or -1 while the keyboard is in the main menu. */
    subSelected: number;
    /** Open the submenu towards the left (the menu itself is left of the card). */
    leftward: boolean;
    /** Grow a too-tall submenu upwards (the popover opens upwards). */
    upward: boolean;
    /** Out: how far the submenu sticks out past the main menu vertically. */
    overflow?: number;
    onPick: (index: number, sub?: number) => void;
    onHover: (index: number, sub?: number) => void;
  };
  let {
    items,
    selected,
    subSelected,
    leftward,
    upward,
    overflow = $bindable(0),
    onPick,
    onHover,
  }: Props = $props();

  let rows: HTMLElement[] = $state([]);
  let mainHeight = $state(0);
  let subHeight = $state(0);
  // A highlighted item's submenu shows right away; subSelected >= 0 only
  // means the keyboard has moved into it.
  const open = $derived(items[selected]?.sub);
  // The submenu floats beside the main menu without resizing it, so opening
  // it never moves anything. It lines up with its parent row (the sub list's
  // padding + border is 5px) but stays within the main menu's height when it
  // can; a taller one sticks out downwards, or upwards when `upward`.
  const subTop = $derived.by(() => {
    if (!open) return 0;
    const row = rows[selected];
    const room = mainHeight - subHeight;
    if (room < 0) return upward ? room : 0;
    const want = upward
      ? (row ? row.offsetTop + row.offsetHeight : mainHeight) + 5 - subHeight
      : (row?.offsetTop ?? 0) - 5;
    return Math.min(Math.max(want, 0), room);
  });

  $effect(() => {
    overflow = open ? Math.max(0, subHeight - mainHeight) : 0;
  });
</script>

<div class="menus" class:leftward>
  <ul class="menu" role="menu" bind:offsetHeight={mainHeight}>
    {#each items as item, i (item.id)}
      {#if item.sep}<li class="sep" role="separator"></li>{/if}
      <li
        bind:this={rows[i]}
        role="menuitem"
        class:on={i === selected}
        class:danger={item.danger}
        onmouseenter={() => onHover(i)}
        onclick={() => onPick(i)}
        onkeydown={() => {}}
        tabindex="-1"
      >
        <span class="label">{item.label}</span>
        {#if item.sub}<span class="keys">›</span>{:else if item.keys}<span class="keys">{item.keys}</span>{/if}
      </li>
    {/each}
  </ul>

  {#if open}
    <ul class="menu sub" role="menu" style:top="{subTop}px" bind:offsetHeight={subHeight}>
      {#each open as sub, j (sub.id)}
        <li
          role="menuitem"
          class:on={j === subSelected}
          onmouseenter={() => onHover(selected, j)}
          onclick={() => onPick(selected, j)}
          onkeydown={() => {}}
          tabindex="-1"
        >
          <span class="label">{sub.label}</span>
          <span class="preview">{sub.preview}</span>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .menus {
    /* Offset parent for the rows and the absolutely placed submenu. */
    position: relative;
    width: max-content;
  }

  .sub {
    position: absolute;
    left: calc(100% + 6px);
  }

  .leftward .sub {
    left: auto;
    right: calc(100% + 6px);
  }

  .menu {
    width: 220px;
    margin: 0;
    padding: 4px;
    list-style: none;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--bg);
    box-shadow: var(--shadow);
  }

  li[role="menuitem"] {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 28px;
    padding: 0 10px;
    border-radius: 4px;
    cursor: default;
    font-size: 12.5px;
  }

  .sub li[role="menuitem"] {
    flex-direction: column;
    align-items: stretch;
    justify-content: center;
    gap: 1px;
    padding: 5px 10px;
  }

  li.on {
    background: var(--bg-selected);
    color: var(--accent-soft);
  }

  li.danger {
    color: var(--danger);
  }

  .label {
    flex: 1;
  }

  .keys {
    font-size: 11px;
    color: var(--text-muted);
  }

  .preview {
    font-size: 11px;
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .sep {
    height: 1px;
    margin: 4px 6px;
    background: var(--border-soft);
  }
</style>

<script lang="ts">
  import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
  import { onMount } from "svelte";
  import { api, type ChipState } from "./api";

  // The chip window never takes focus, so the app that was pasted into
  // keeps receiving keys. Buttons still react to clicks.
  let chip = $state<ChipState | null>(null);

  onMount(() => {
    const unlisten = getCurrentWebviewWindow().listen<ChipState>("chip", ({ payload }) => (chip = payload));
    return () => {
      unlisten.then((f) => f());
    };
  });

  const hotkey = $derived(chip?.hotkey.replace(/\+/g, " ") ?? "");
</script>

{#if chip}
  <div class="chip" class:choosing={chip.phase === "choosing"}>
    {#if chip.phase === "choosing"}
      <span class="icon">↻</span>
      <span class="lead">松开 Alt 换成 {chip.index + 1}/{chip.total}</span>
    {:else}
      <svg class="icon" width="13" height="13" viewBox="0 0 24 24" aria-hidden="true"
        ><path d="m5 12 5 5 9-10" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round" /></svg
      >
      <span class="lead">{chip.phase === "replaced" ? `已换成 ${chip.index + 1}/${chip.total}` : "已粘贴"}</span>
    {/if}
    <span class="preview">{chip.preview}</span>
    {#if chip.canSwap && chip.phase !== "choosing"}
      <button onclick={() => api.chipSwap()}>{#if hotkey}<kbd>{hotkey}</kbd>{/if} 换一条</button>
    {/if}
    {#if chip.phase !== "choosing"}
      <button onclick={() => api.chipUndo()}>撤销</button>
    {/if}
  </div>
{/if}

<style>
  .chip {
    position: fixed;
    left: 8px;
    top: 8px;
    max-width: calc(100vw - 16px);
    height: 34px;
    display: inline-flex;
    align-items: center;
    gap: 10px;
    padding: 0 6px 0 12px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--bg);
    /* Must fade out within the chip window's 8px margin (CHIP_MARGIN). */
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.4);
    font-size: 12px;
    white-space: nowrap;
  }

  .chip.choosing {
    border-color: var(--accent);
  }

  .icon {
    flex: none;
    color: var(--green);
  }

  .choosing .icon,
  .choosing .lead {
    color: var(--accent);
  }

  .lead {
    flex: none;
  }

  .preview {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--accent-soft);
  }

  button {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 24px;
    padding: 0 8px;
    border: none;
    border-radius: 4px;
    background: var(--bg-raised);
    font-size: 11.5px;
    color: var(--text);
    cursor: pointer;
  }

  button:hover {
    color: var(--accent);
  }
</style>

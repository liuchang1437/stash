<script lang="ts">
  import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
  import Chip from "$lib/Chip.svelte";
  import Manage from "$lib/Manage.svelte";
  import Popover from "$lib/Popover.svelte";

  // All three windows load this page; the window label picks the UI.
  const label = getCurrentWebviewWindow().label;
  document.documentElement.dataset.window = label;
</script>

<svelte:document oncontextmenu={(e) => e.preventDefault()} />

{#if label === "chip"}
  <Chip />
{:else if label === "manage"}
  <Manage />
{:else}
  <Popover />
{/if}

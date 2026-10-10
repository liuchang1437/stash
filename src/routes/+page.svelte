<script lang="ts">
  import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
  import { onMount } from "svelte";
  import { api } from "$lib/api";
  import Chip from "$lib/Chip.svelte";
  import { setLang, type Lang } from "$lib/i18n.svelte";
  import Manage from "$lib/Manage.svelte";
  import Popover from "$lib/Popover.svelte";

  // All three windows load this page; the window label picks the UI.
  const webview = getCurrentWebviewWindow();
  const label = webview.label;
  document.documentElement.dataset.window = label;

  // Render only once the language is known, so no text flashes in the wrong one.
  let ready = $state(false);

  onMount(() => {
    api.language().then((lang) => {
      setLang(lang);
      ready = true;
    });
    const unlisten = webview.listen<Lang>("language", ({ payload }) => setLang(payload));
    return () => {
      unlisten.then((f) => f());
    };
  });
</script>

<svelte:document oncontextmenu={(e) => e.preventDefault()} />

{#if !ready}
  <!-- waiting for the language -->
{:else if label === "chip"}
  <Chip />
{:else if label === "manage"}
  <Manage />
{:else}
  <Popover />
{/if}

<script lang="ts">
  import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
  import { onMount } from "svelte";
  import SettingsView from "./SettingsView.svelte";
  import SnippetEditor from "./SnippetEditor.svelte";
  import { api, type ManageRequest, type Snippet } from "./api";

  // Settings and the snippet editor live in their own regular window, so
  // the popover can stay small and close on blur.
  type View =
    | { name: "settings" }
    | { name: "edit"; snippet: Snippet | null; initialBody: string }
    | { name: "idle" };

  let view = $state<View>({ name: "idle" });
  // Remounts the view when the same kind is opened again.
  let generation = $state(0);

  async function open(request: ManageRequest | null) {
    if (!request) return;
    if (request.view === "settings") {
      view = { name: "settings" };
    } else {
      const snippet = request.key ? await api.getSnippet(request.key).catch(() => null) : null;
      view = { name: "edit", snippet, initialBody: request.body ?? "" };
    }
    generation++;
  }

  onMount(() => {
    api.manageRequest().then(open);
    const unlisten = getCurrentWebviewWindow().listen<ManageRequest>("manage-open", ({ payload }) => open(payload));
    return () => {
      unlisten.then((f) => f());
    };
  });

  const done = () => api.closeManage();
</script>

{#key generation}
  {#if view.name === "settings"}
    <SettingsView onDone={done} />
  {:else if view.name === "edit"}
    <SnippetEditor snippet={view.snippet} initialBody={view.initialBody} onDone={done} />
  {/if}
{/key}

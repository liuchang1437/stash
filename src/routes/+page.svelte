<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { api, type Field, type Mode, type Snippet } from "$lib/api";
  import FillForm from "$lib/FillForm.svelte";
  import Launcher from "$lib/Launcher.svelte";
  import SettingsView from "$lib/SettingsView.svelte";
  import SnippetEditor from "$lib/SnippetEditor.svelte";

  type View =
    | { name: "search" }
    | { name: "fill"; key: string; title: string; fields: Field[]; mode: Mode }
    | { name: "edit"; snippet: Snippet | null; initialBody: string }
    | { name: "settings" };

  let view = $state<View>({ name: "search" });
  let launcher = $state<Launcher>();

  function backToSearch() {
    view = { name: "search" };
    // The launcher remounts; focus it once it exists.
    queueMicrotask(() => launcher?.focus());
  }

  /** Returns an error message for the form to show, if any. */
  async function submitFill(values: Record<string, string>): Promise<string | void> {
    if (view.name !== "fill") return;
    try {
      await api.activate(view.key, view.mode, values);
    } catch (e) {
      return String(e);
    }
    view = { name: "search" };
  }

  async function edit(key: string) {
    view = { name: "edit", snippet: await api.getSnippet(key), initialBody: "" };
  }

  onMount(() => {
    const unlisteners = [
      listen<boolean>("launcher-shown", ({ payload: keepQuery }) => {
        // Reopened within the keep-query window: pick up where the user left
        // off. Otherwise start fresh, except for an unsaved editor or settings.
        if (keepQuery) {
          if (view.name === "search") queueMicrotask(() => launcher?.resume());
          return;
        }
        if (view.name === "fill") view = { name: "search" };
        if (view.name === "search") queueMicrotask(() => launcher?.reset());
      }),
      listen("open-settings", () => (view = { name: "settings" })),
    ];

    api.getSettings().then((s) => {
      if (s.hotkeyError) view = { name: "settings" };
    });

    // Behave like a popup: clicking elsewhere dismisses it, except while
    // editing, where the user may switch away to copy something.
    const onBlur = () => {
      if (view.name === "search" || view.name === "fill") api.hide();
    };
    window.addEventListener("blur", onBlur);

    return () => {
      window.removeEventListener("blur", onBlur);
      unlisteners.forEach((p) => p.then((f) => f()));
    };
  });
</script>

<svelte:document oncontextmenu={(e) => e.preventDefault()} />

{#if view.name === "search"}
  <Launcher
    bind:this={launcher}
    onFill={(key, title, fields, mode) => (view = { name: "fill", key, title, fields, mode })}
    onEdit={edit}
    onNewSnippet={(body) => (view = { name: "edit", snippet: null, initialBody: body })}
    onSettings={() => (view = { name: "settings" })}
  />
{:else if view.name === "fill"}
  <FillForm
    title={view.title}
    fields={view.fields}
    mode={view.mode}
    onSubmit={submitFill}
    onCancel={backToSearch}
  />
{:else if view.name === "edit"}
  <SnippetEditor snippet={view.snippet} initialBody={view.initialBody} onDone={backToSearch} />
{:else}
  <SettingsView onDone={backToSearch} />
{/if}

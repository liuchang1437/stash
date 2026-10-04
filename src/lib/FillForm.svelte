<script lang="ts">
  import { onMount } from "svelte";
  import type { Field, Mode } from "./api";

  type Props = {
    title: string;
    fields: Field[];
    mode: Mode;
    /** Resolves to an error message when the action failed. */
    onSubmit: (values: Record<string, string>) => Promise<string | void>;
    onCancel: () => void;
  };
  let { title, fields, mode, onSubmit, onCancel }: Props = $props();

  const ACTION_LABELS: Record<Mode, string> = { paste: "粘贴", copy: "复制", open: "在浏览器打开" };
  let error = $state("");

  // svelte-ignore state_referenced_locally
  let values = $state<Record<string, string>>(
    Object.fromEntries(fields.map((f) => [f.name, f.default])),
  );
  let form: HTMLFormElement;

  onMount(() => {
    const first = form.querySelector<HTMLElement>("input, select");
    first?.focus();
    if (first instanceof HTMLInputElement) first.select();
  });

  async function submit(e: Event) {
    e.preventDefault();
    error = (await onSubmit($state.snapshot(values))) ?? "";
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      onCancel();
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<form bind:this={form} onsubmit={submit} {onkeydown}>
  <header>
    <span class="label">填写变量</span>
    <h1>{title}</h1>
  </header>

  <div class="fields">
    {#each fields as field (field.name)}
      <label>
        <span>{field.name}</span>
        {#if field.options.length}
          <select class="field" bind:value={values[field.name]}>
            {#each field.options as option}
              <option value={option}>{option}</option>
            {/each}
          </select>
        {:else}
          <input class="field" bind:value={values[field.name]} spellcheck="false" />
        {/if}
      </label>
    {/each}
  </div>

  <footer>
    {#if error}
      <span class="error">{error}</span>
    {:else}
      <span><kbd>Tab</kbd> 下一项</span>
      <span><kbd>↵</kbd> {ACTION_LABELS[mode]}</span>
      <span><kbd>Esc</kbd> 返回</span>
    {/if}
    <span class="spacer"></span>
    <button type="button" class="btn" onclick={onCancel}>取消</button>
    <button type="submit" class="btn primary">{ACTION_LABELS[mode]}</button>
  </footer>
</form>

<style>
  form {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }

  header {
    padding: 16px 20px 8px;
  }

  .label {
    font-size: 12px;
    color: var(--text-muted);
  }

  h1 {
    margin: 2px 0 0;
    font-size: 18px;
    font-weight: 600;
  }

  .fields {
    flex: 1;
    overflow-y: auto;
    padding: 8px 20px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  label > span {
    font-size: 12px;
    color: var(--text-muted);
  }

  footer {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 10px 14px;
    border-top: 1px solid var(--border);
    background: var(--bg-subtle);
    font-size: 12px;
    color: var(--text-muted);
  }

  .spacer {
    flex: 1;
  }

  .error {
    color: var(--danger);
  }
</style>

<script lang="ts">
  // The manage window has no native title bar; this replaces its close box.
  let { onClose }: { onClose: () => void } = $props();
</script>

<button class="close" title="关闭 (Esc)" aria-label="关闭" onclick={onClose}>
  <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true">
    <path d="M2 2l8 8M10 2l-8 8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
  </svg>
</button>

<style>
  .close {
    flex: none;
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
  }

  .close:hover {
    background: var(--danger);
    color: var(--bg);
  }
</style>

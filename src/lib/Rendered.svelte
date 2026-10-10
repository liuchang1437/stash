<script lang="ts">
  // A rendered snippet: what it pastes, with every variable marked by kind.
  // Inline content; the parent wraps it in a <pre>.
  import type { Segment } from "./api";
  import { markTerms } from "./kinds";
  import { segmentPlaceholder, segmentTitle } from "./template";

  type Props = {
    segments: Segment[];
    /** The input being filled in. */
    focus?: string | null;
    /** Search terms to highlight in the template's own text. */
    terms?: string[];
  };
  let { segments, focus = null, terms = [] }: Props = $props();
</script>

{#each segments as seg}{#if seg.kind === "text"}{#each markTerms(seg.text, terms) as p}{#if p.hit}<mark
        >{p.text}</mark
      >{:else}{p.text}{/if}{/each}{:else if seg.kind === "cursor"}<span class="cursor" title={segmentTitle(seg)}
    ></span>{:else}<span
      class="var {seg.kind}"
      class:active={seg.kind === "input" && seg.name === focus}
      class:empty={!seg.text}
      title={segmentTitle(seg)}>{seg.text || segmentPlaceholder(seg)}</span
    >{/if}{/each}

<style>
  .var {
    border-radius: 3px;
  }

  .input {
    color: var(--green);
    background: rgba(63, 209, 122, 0.1);
  }

  .auto {
    color: var(--blue);
    background: rgba(108, 182, 255, 0.1);
  }

  .active {
    color: var(--accent);
    background: rgba(245, 165, 36, 0.14);
    text-decoration: underline;
    text-underline-offset: 3px;
  }

  /* An empty value shows where it goes. */
  .empty {
    background: transparent;
    outline: 1px dashed currentColor;
    outline-offset: -1px;
    opacity: 0.8;
  }

  .cursor {
    display: inline-block;
    width: 2px;
    height: 1.15em;
    margin: 0 1px;
    vertical-align: -0.2em;
    background: var(--accent);
  }
</style>

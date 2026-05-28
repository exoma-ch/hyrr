<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    title: string;
    controls?: Snippet;
    actions?: Snippet;
  }

  let { title, controls, actions }: Props = $props();
</script>

<div class="section-header">
  <span class="section-title">{title}</span>
  {#if controls}
    <div class="section-controls">
      {@render controls()}
    </div>
  {/if}
  <div class="section-spacer"></div>
  {#if actions}
    <div class="section-actions">
      {@render actions()}
    </div>
  {/if}
</div>

<style>
  .section-header {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0 0 0.35rem 0;
    min-height: 1.5rem;
    flex-wrap: wrap;
  }

  .section-title {
    font-size: 0.7rem;
    font-weight: 600;
    color: var(--c-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    white-space: nowrap;
  }

  .section-controls {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    flex-wrap: wrap;
  }

  .section-spacer {
    flex: 1;
  }

  .section-actions {
    display: flex;
    align-items: center;
    gap: 0.3rem;
  }
</style>

<script lang="ts">
  import { parseFormula } from "@hyrr/compute";

  interface Props {
    query: string;
    currentEnrichment?: Record<string, Record<number, number>>;
    onenrichment?: (element: string) => void;
  }

  let { query, currentEnrichment, onenrichment }: Props = $props();

  let queryElements = $derived.by(() => {
    const q = query.trim();
    if (!q) return [];
    try { return Object.keys(parseFormula(q)); } catch { return []; }
  });
</script>

{#if queryElements.length > 0 && onenrichment}
  <div class="enrichment-row">
    <span class="enr-label">Isotopic enrichment:</span>
    {#each queryElements as el}
      <button
        class="el-badge"
        class:enriched={!!currentEnrichment?.[el]}
        onclick={() => onenrichment?.(el)}
      >{el}{#if currentEnrichment?.[el]}<span class="enr-dot"></span>{/if}</button>
    {/each}
  </div>
{/if}

<style>
  .enrichment-row {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    padding: 0.25rem 0.4rem;
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 4px;
  }

  .enr-label {
    font-size: 0.7rem;
    color: var(--c-text-muted);
    margin-right: 0.2rem;
  }

  .el-badge {
    background: var(--c-bg-muted);
    border: 1px solid var(--c-border);
    border-radius: 3px;
    color: var(--c-text-muted);
    font-size: 0.7rem;
    font-weight: 500;
    padding: 0.15rem 0.35rem;
    cursor: pointer;
    line-height: 1;
  }

  .el-badge:hover { border-color: var(--c-accent); color: var(--c-accent); }

  .el-badge.enriched {
    border-color: var(--c-gold);
    color: var(--c-gold);
    background: var(--c-gold-tint-subtle);
  }

  .enr-dot {
    display: inline-block;
    width: 4px;
    height: 4px;
    background: var(--c-gold);
    border-radius: 50%;
    margin-left: 0.2rem;
    vertical-align: middle;
  }
</style>

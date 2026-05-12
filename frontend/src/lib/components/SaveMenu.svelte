<script lang="ts">
  /**
   * Shared save-menu affordance for every plot + table. Click the save icon
   * to open a menu of export options (CSV, Parquet, etc.). Each option
   * triggers a callback passed in by the parent.
   */
  import { Save } from "lucide-svelte";
  import {
    tracesToCsv,
    triggerDownload,
    csvTimestampedName,
    type CsvTrace,
  } from "../plotting/csv-export";
  import { downloadParquet } from "../plotting/parquet-export";

  interface Props {
    /** Filename prefix — will be suffixed with `-<iso-timestamp>`. */
    filenamePrefix: string;
    /** x-axis label for the CSV / Parquet header. */
    xLabel: string;
    /** y-axis label for the CSV / Parquet header. */
    yLabel: string;
    /** Trace data returned by the parent's render function. */
    getTraces: () => CsvTrace[];
    /** Optional additional CSV notes, one per line. */
    notes?: () => string[];
    /** Tooltip for the trigger button. */
    title?: string;
  }

  let {
    filenamePrefix,
    xLabel,
    yLabel,
    getTraces,
    notes,
    title = "Save / download",
  }: Props = $props();

  let open = $state(false);
  let wrapEl = $state<HTMLSpanElement | undefined>();

  function onGlobalClick(e: MouseEvent) {
    if (open && wrapEl && !wrapEl.contains(e.target as Node)) open = false;
  }

  function emitCsv() {
    open = false;
    const traces = getTraces();
    if (traces.length === 0) return;
    const csv = tracesToCsv(xLabel, yLabel, traces, notes?.() ?? []);
    triggerDownload(csvTimestampedName(filenamePrefix), csv);
  }

  async function emitParquet() {
    open = false;
    const traces = getTraces();
    if (traces.length === 0) return;
    await downloadParquet(filenamePrefix, xLabel, yLabel, traces);
  }
</script>

<svelte:window onclick={onGlobalClick} />

<span class="save-menu-wrap" bind:this={wrapEl}>
  <button
    class="save-btn"
    class:active={open}
    onclick={(e: MouseEvent) => { e.stopPropagation(); open = !open; }}
    {title}
    aria-haspopup="menu"
    aria-expanded={open}
  >
    <Save size={13} aria-hidden="true" />
  </button>
  {#if open}
    <div class="menu" role="menu">
      <button class="menu-item" role="menuitem" onclick={emitCsv}>
        CSV <span class="hint">text, spreadsheet-friendly</span>
      </button>
      <button class="menu-item" role="menuitem" onclick={emitParquet}>
        Parquet <span class="hint">typed columns, polars/duckdb/pandas</span>
      </button>
    </div>
  {/if}
</span>

<style>
  .save-menu-wrap {
    position: relative;
    display: inline-flex;
  }
  .save-btn {
    background: var(--c-bg-muted);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text-muted);
    padding: 0.15rem 0.35rem;
    cursor: pointer;
    line-height: 0;
  }
  .save-btn:hover,
  .save-btn.active {
    border-color: var(--c-accent);
    color: var(--c-accent);
  }
  .menu {
    position: absolute;
    top: calc(100% + 4px);
    right: 0;
    min-width: 260px;
    background: var(--c-bg-subtle);
    border: 1px solid var(--c-border);
    border-radius: 6px;
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.35);
    z-index: 1000;
    padding: 4px;
    display: flex;
    flex-direction: column;
  }
  .menu-item {
    text-align: left;
    padding: 6px 10px;
    background: none;
    border: 0;
    border-radius: 4px;
    color: var(--c-text);
    font-size: 0.8rem;
    cursor: pointer;
    display: grid;
    grid-template-columns: 64px 1fr;
    align-items: baseline;
    gap: 0.5rem;
    width: 100%;
  }
  .menu-item:hover {
    background: var(--c-bg-muted);
  }
  .hint {
    color: var(--c-text-muted);
    font-size: 0.7rem;
    line-height: 1.2;
  }
</style>

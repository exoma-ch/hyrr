<script lang="ts" module>
  import type { ElementCell } from "./periodic-table-data";
  export type { ElementCell } from "./periodic-table-data";

  /** Tooltip/inspect-callback payload — narrow on purpose so the parent
   *  can mirror this shape without reaching into PT internals. */
  export interface ElementInfo {
    Z: number;
    symbol: string;
    name: string;
    block: ElementCell["block"];
    period: number;
    group: number | null;
  }
</script>

<script lang="ts">
  import type { Snippet } from "svelte";
  import { PERIODIC_TABLE } from "./periodic-table-data";

  interface Props {
    onselect: (symbol: string) => void;
    /** Symbol of the currently-selected element (renders with the
     *  `.selected` highlight). */
    selected?: string;
    /** Symbols whose cells should be greyed-out (e.g. no TENDL coverage
     *  for the active projectile). Disabled cells stay rendered and
     *  keyboard-reachable; click is suppressed. Wired in later commits. */
    disabled?: Set<string>;
    /** Symbols that should pulse with a secondary highlight (e.g. rows
     *  already present in the define-form). */
    highlighted?: Set<string>;
    /** Selection mode — "multi" is reserved for future work. */
    mode?: "single" | "multi";
    /** When set, controls the "show Z>92" toggle externally. When unset,
     *  the component owns the toggle internally (defaults to false —
     *  i.e. transactinides hidden by default). The label "Z>92" is per
     *  the domain-review correction: Th (Z=90) is a real production
     *  target, not a transuranic. */
    showTransuranics?: boolean;
    /** Optional tooltip body. The PT owns positioning; the parent
     *  renders content. (Not wired in this commit; landed in a later
     *  Phase 1 commit alongside accessibility.) */
    tooltip?: Snippet<[ElementInfo]>;
  }

  // `tooltip` is declared in `Props` but bound only by the accessibility
  // commit; not destructured here.
  let {
    onselect,
    selected,
    disabled,
    highlighted,
    mode = "single",
    showTransuranics,
  }: Props = $props();

  let showTransuranicsInternal = $state(false);
  let effectiveShow = $derived(showTransuranics ?? showTransuranicsInternal);

  let visibleCells = $derived(
    effectiveShow ? PERIODIC_TABLE : PERIODIC_TABLE.filter((c) => c.Z <= 92),
  );

  function handleClick(cell: ElementCell): void {
    if (disabled?.has(cell.symbol)) return;
    onselect(cell.symbol);
  }

  function toggleHighZ(): void {
    if (showTransuranics === undefined) {
      showTransuranicsInternal = !showTransuranicsInternal;
    }
  }
</script>

<div class="pt-wrap">
  <div class="pt-grid" data-mode={mode}>
    {#each visibleCells as cell (cell.Z)}
      <button
        class="pt-cell"
        data-block={cell.block}
        data-z={cell.Z}
        class:selected={selected === cell.symbol}
        class:highlighted={highlighted?.has(cell.symbol)}
        class:disabled={disabled?.has(cell.symbol)}
        style:grid-row={cell.row >= 8 ? cell.row + 1 : cell.row}
        style:grid-column={cell.col}
        type="button"
        onclick={() => handleClick(cell)}
      >
        <span class="cell-z">{cell.Z}</span>
        <span class="cell-sym">{cell.symbol}</span>
        <span class="cell-block" aria-hidden="true">{cell.block}</span>
      </button>
    {/each}
  </div>

  {#if showTransuranics === undefined}
    <button
      class="pt-toggle"
      onclick={toggleHighZ}
      aria-pressed={effectiveShow}
    >{effectiveShow ? "Hide Z>92" : "Show Z>92"}</button>
  {/if}
</div>

<style>
  .pt-wrap {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.4rem;
  }

  .pt-grid {
    display: grid;
    grid-template-columns: repeat(18, minmax(1.4rem, 1fr));
    grid-template-rows: repeat(7, minmax(2rem, auto)) 0.4rem repeat(2, minmax(2rem, auto));
    gap: 2px;
    padding: 0.4rem;
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    width: 100%;
  }

  .pt-cell {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    background: var(--c-bg-subtle);
    border: 1px solid var(--c-border);
    border-radius: 3px;
    padding: 0.2rem 0.1rem;
    cursor: pointer;
    color: var(--c-text);
    aspect-ratio: 1 / 1;
    min-width: 0;
    line-height: 1;
  }

  /* Block colours — kept low-saturation so dark + light themes both work. */
  .pt-cell[data-block="s"] { background: var(--c-block-s, #f6dadf); color: #5a1a25; }
  .pt-cell[data-block="p"] { background: var(--c-block-p, #fdeed1); color: #5a3a10; }
  .pt-cell[data-block="d"] { background: var(--c-block-d, #d6e4f5); color: #1a3756; }
  .pt-cell[data-block="f"] { background: var(--c-block-f, #d8efe1); color: #14442a; }

  .pt-cell:hover { filter: brightness(1.06); }

  .pt-cell.selected {
    outline: 2px solid var(--c-accent);
    outline-offset: 1px;
    z-index: 1;
  }

  .pt-cell.highlighted {
    box-shadow: 0 0 0 2px var(--c-gold) inset;
  }

  .pt-cell.disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }

  .cell-z {
    position: absolute;
    top: 0.15rem;
    left: 0.2rem;
    font-size: 0.55rem;
    color: inherit;
    opacity: 0.7;
  }

  .cell-sym {
    font-size: 0.85rem;
    font-weight: 600;
  }

  /* Block glyph at bottom-left — non-colour channel for the s/p/d/f
     classification (pitfall 12: don't rely on colour alone). */
  .cell-block {
    position: absolute;
    bottom: 0.15rem;
    left: 0.2rem;
    font-size: 0.5rem;
    font-weight: 700;
    color: inherit;
    opacity: 0.55;
    text-transform: uppercase;
    letter-spacing: 0.02em;
  }

  .pt-toggle {
    background: var(--c-bg-subtle);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text-muted);
    padding: 0.25rem 0.6rem;
    font-size: 0.75rem;
    cursor: pointer;
    align-self: flex-start;
  }

  .pt-toggle:hover { border-color: var(--c-accent); color: var(--c-text); }
  .pt-toggle[aria-pressed="true"] { background: var(--c-accent-tint-subtle); color: var(--c-accent); border-color: var(--c-accent); }
</style>

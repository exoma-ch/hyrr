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

  let tooltipIdCounter = 0;
</script>

<script lang="ts">
  import type { Snippet } from "svelte";
  import { tick } from "svelte";
  import { PERIODIC_TABLE, ELEMENT_BY_Z } from "./periodic-table-data";

  interface Props {
    onselect: (symbol: string) => void;
    /** Symbol of the currently-selected element. */
    selected?: string;
    /** Symbols whose cells should be greyed-out (e.g. no TENDL coverage
     *  for the active projectile). Disabled cells stay rendered and
     *  keyboard-reachable; click is suppressed; the accessible name
     *  carries the reason (", no TENDL data"). */
    disabled?: Set<string>;
    /** Symbols that should pulse with a secondary highlight. */
    highlighted?: Set<string>;
    /** Symbols whose isotopic composition can be enriched (i.e. have
     *  ≥ 1 stable isotope on the supplier circuit). Rendered as a
     *  small corner glyph on the cell. The follow-up "use enriched…"
     *  shortcut lives in the parent's inspect panel. */
    enrichableSet?: Set<string>;
    /** Selection mode — "multi" is reserved for future work. */
    mode?: "single" | "multi";
    /** Controlled "show Z>92" toggle — when undefined the component
     *  owns the toggle internally and renders the toggle button. */
    showTransuranics?: boolean;
    /** Optional tooltip body. Parent renders content; PT positions it
     *  below the focused/hovered cell. */
    tooltip?: Snippet<[ElementInfo]>;
  }

  let {
    onselect,
    selected,
    disabled,
    highlighted,
    enrichableSet,
    mode = "single",
    showTransuranics,
    tooltip,
  }: Props = $props();

  let showTransuranicsInternal = $state(false);
  let effectiveShow = $derived(showTransuranics ?? showTransuranicsInternal);

  let visibleCells = $derived(
    effectiveShow ? PERIODIC_TABLE : PERIODIC_TABLE.filter((c) => c.Z <= 92),
  );

  /** Visible cells grouped by data-row, sorted by column — the lookup
   *  used for arrow-key navigation. */
  let cellsByRow = $derived.by(() => {
    const map = new Map<number, ElementCell[]>();
    for (const cell of visibleCells) {
      const arr = map.get(cell.row) ?? [];
      arr.push(cell);
      map.set(cell.row, arr);
    }
    for (const arr of map.values()) arr.sort((a, b) => a.col - b.col);
    return map;
  });

  /**
   * Row traversal order for ArrowUp / ArrowDown. The visible PT
   * organises detached lanthanide (data-row 8, period 6) and actinide
   * (data-row 9, period 7) rows BELOW the main grid for layout
   * reasons, but semantically (and for screen-reader logical order)
   * row 8 sits between periods 6 and 7, and row 9 sits after period 7.
   * Stepping from Ce (row 8) up should land on Hf/La (row 6), not
   * Rf (row 7) — and Hf down should drop into Ce.
   */
  const ROW_ORDER = [1, 2, 3, 4, 5, 6, 8, 7, 9];

  let visibleRows = $derived(ROW_ORDER.filter((r) => cellsByRow.has(r)));

  /** Roving-tabindex anchor — exactly one cell has tabindex=0 at a time.
   *  Also drives the tooltip target so screen readers reading the focused
   *  cell never hear a description that belongs to a different cell. */
  let focusedZ = $state(1);

  let activeCell = $derived(ELEMENT_BY_Z.get(focusedZ) ?? null);

  // Stable per-instance id for the tooltip element (used by aria-describedby).
  let tooltipId = `pt-tooltip-${++tooltipIdCounter}`;

  // If the focused cell becomes hidden (toggling Z>92 off while focused
  // on a high-Z cell), fall back to the highest still-visible Z so the
  // user keeps roughly their place — and re-focus the DOM since the old
  // button has unmounted.
  $effect(() => {
    if (!visibleCells.some((c) => c.Z === focusedZ)) {
      const fallback = visibleCells.reduce(
        (best, c) => (c.Z > best ? c.Z : best),
        1,
      );
      focusedZ = fallback;
      void tick().then(() => {
        const el = document.querySelector<HTMLButtonElement>(
          `[data-pt-tooltip-id="${tooltipId}"] [data-z="${fallback}"]`,
        );
        // Only steal focus if focus was inside the grid before the
        // current cell unmounted, otherwise we'd grab focus from
        // unrelated UI on first mount.
        if (el && document.activeElement && document.activeElement.closest(`[data-pt-tooltip-id="${tooltipId}"]`)) {
          el.focus();
        }
      });
    }
  });

  function gridRowFor(cell: ElementCell): number {
    return cell.row >= 8 ? cell.row + 1 : cell.row;
  }

  function ariaLabelFor(cell: ElementCell): string {
    const parts = [cell.name, String(cell.Z)];
    if (disabled?.has(cell.symbol)) {
      // When the cell is disabled the "enrichable" hint is misleading
      // (you can't enrich a target with no TENDL coverage). Skip it.
      parts.push("no TENDL data");
    } else if (enrichableSet?.has(cell.symbol)) {
      parts.push("enrichable");
    }
    return parts.join(", ");
  }

  /** Polite announcement for disabled-cell activation. Sighted users see
   *  the cell stays greyed; without this, screen-reader users get no
   *  feedback that Enter/click did anything. Cleared after ~1s so a
   *  second attempt on the same cell re-announces. */
  let liveMessage = $state("");
  let liveMessageTimer: number | undefined;
  function announceDisabled(cell: ElementCell): void {
    if (liveMessageTimer !== undefined) clearTimeout(liveMessageTimer);
    liveMessage = `${cell.name}: no TENDL data for the current projectile. Pick another element or switch projectile.`;
    liveMessageTimer = window.setTimeout(() => {
      liveMessage = "";
      liveMessageTimer = undefined;
    }, 1000);
  }

  function handleClick(cell: ElementCell): void {
    focusedZ = cell.Z;
    if (disabled?.has(cell.symbol)) {
      announceDisabled(cell);
      return;
    }
    onselect(cell.symbol);
  }

  async function moveFocus(nextZ: number): Promise<void> {
    if (nextZ === focusedZ) return;
    focusedZ = nextZ;
    await tick();
    const el = document.querySelector<HTMLButtonElement>(
      `[data-pt-tooltip-id="${tooltipId}"] [data-z="${nextZ}"]`,
    );
    el?.focus();
  }

  function neighbourSameRow(currentZ: number, dir: -1 | 1): number {
    const cur = ELEMENT_BY_Z.get(currentZ);
    if (!cur) return currentZ;
    const row = cellsByRow.get(cur.row);
    if (!row) return currentZ;
    const idx = row.findIndex((c) => c.Z === currentZ);
    const next = row[idx + dir];
    return next?.Z ?? currentZ;
  }

  function neighbourCrossRow(currentZ: number, dir: -1 | 1): number {
    const cur = ELEMENT_BY_Z.get(currentZ);
    if (!cur) return currentZ;
    // Walk visible rows in the requested direction, picking the first
    // row that has any cell — this lets up/down skip over the spacer
    // between row 7 and the lanthanide row.
    const rows = visibleRows;
    const idx = rows.indexOf(cur.row);
    const targetRow = rows[idx + dir];
    if (targetRow === undefined) return currentZ;
    const candidates = cellsByRow.get(targetRow);
    if (!candidates || candidates.length === 0) return currentZ;
    let best = candidates[0];
    let bestDelta = Math.abs(best.col - cur.col);
    for (const c of candidates) {
      const d = Math.abs(c.col - cur.col);
      if (d < bestDelta) { best = c; bestDelta = d; }
    }
    return best.Z;
  }

  function handleKeydown(event: KeyboardEvent): void {
    let nextZ: number | null = null;

    switch (event.key) {
      case "ArrowLeft":  nextZ = neighbourSameRow(focusedZ, -1); break;
      case "ArrowRight": nextZ = neighbourSameRow(focusedZ, 1); break;
      case "ArrowUp":    nextZ = neighbourCrossRow(focusedZ, -1); break;
      case "ArrowDown":  nextZ = neighbourCrossRow(focusedZ, 1); break;
      case "Home": {
        if (event.ctrlKey || event.metaKey) {
          nextZ = 1;
        } else {
          const cur = ELEMENT_BY_Z.get(focusedZ);
          const row = cur ? cellsByRow.get(cur.row) : undefined;
          nextZ = row?.[0]?.Z ?? null;
        }
        break;
      }
      case "End": {
        const cur = ELEMENT_BY_Z.get(focusedZ);
        const row = cur ? cellsByRow.get(cur.row) : undefined;
        nextZ = row?.[row.length - 1]?.Z ?? null;
        break;
      }
      case "Enter":
      case " ": {
        const cell = ELEMENT_BY_Z.get(focusedZ);
        if (cell) handleClick(cell);
        event.preventDefault();
        return;
      }
      default: return;
    }

    if (nextZ !== null) {
      event.preventDefault();
      void moveFocus(nextZ);
    }
  }

  function toggleHighZ(): void {
    if (showTransuranics === undefined) {
      showTransuranicsInternal = !showTransuranicsInternal;
    }
  }
</script>

<div class="pt-wrap" data-pt-tooltip-id={tooltipId}>
  <div role="status" aria-live="polite" class="pt-live-region">{liveMessage}</div>
  <div
    role="grid"
    aria-label="Periodic table"
    class="pt-grid"
    data-mode={mode}
    tabindex={-1}
    onkeydown={handleKeydown}
  >
    {#each visibleRows as row (row)}
      <div role="row" aria-rowindex={row} class="pt-row">
        {#each cellsByRow.get(row) ?? [] as cell (cell.Z)}
          <button
            role="gridcell"
            type="button"
            class="pt-cell"
            data-block={cell.block}
            data-z={cell.Z}
            class:selected={selected === cell.symbol}
            class:highlighted={highlighted?.has(cell.symbol)}
            class:disabled={disabled?.has(cell.symbol)}
            style:grid-row={gridRowFor(cell)}
            style:grid-column={cell.col}
            tabindex={focusedZ === cell.Z ? 0 : -1}
            aria-colindex={cell.col}
            aria-label={ariaLabelFor(cell)}
            aria-describedby={tooltip && focusedZ === cell.Z ? tooltipId : undefined}
            aria-selected={selected === cell.symbol ? true : undefined}
            onclick={() => handleClick(cell)}
            onfocus={() => { focusedZ = cell.Z; }}
          >
            <span class="cell-z">{cell.Z}</span>
            <span class="cell-sym">{cell.symbol}</span>
            <span class="cell-block" aria-hidden="true">{cell.block}</span>
            {#if enrichableSet?.has(cell.symbol)}
              <span class="cell-enrich" aria-hidden="true">★</span>
            {/if}
          </button>
        {/each}
      </div>
    {/each}
  </div>

  {#if tooltip && activeCell}
    <div role="tooltip" id={tooltipId} class="pt-tooltip">
      {@render tooltip({
        Z: activeCell.Z,
        symbol: activeCell.symbol,
        name: activeCell.name,
        block: activeCell.block,
        period: activeCell.period,
        group: activeCell.group,
      })}
    </div>
  {/if}

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
    align-items: stretch;
    gap: 0.4rem;
  }

  .pt-live-region {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
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

  /* role="row" wrappers don't participate in CSS grid layout — cells
     are direct grid items via display: contents on the row. */
  .pt-row { display: contents; }

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

  /* Focus ring — black core carries WCAG 2.4.13 (~19:1 against every
     block colour). The white halo is for legibility against dark
     surrounding chrome / future dark-theme support; it is intentionally
     low-contrast (~1.1:1) against light block backgrounds and is not
     the indicator carrying the contrast requirement. */
  .pt-cell:focus-visible {
    outline: 2px solid #000;
    outline-offset: 1px;
    box-shadow: 0 0 0 4px #fff;
    z-index: 2;
  }

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

  .cell-enrich {
    position: absolute;
    top: 0.1rem;
    right: 0.2rem;
    font-size: 0.55rem;
    color: var(--c-gold);
    line-height: 1;
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

  .pt-tooltip {
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    padding: 0.4rem 0.6rem;
    font-size: 0.75rem;
    color: var(--c-text);
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

  @media (prefers-reduced-motion: reduce) {
    .pt-cell { transition: none; }
  }

  /* Compact rendering on narrow viewports — at this width the parent
     should hide the PT behind a toggle (Phase 2), but if it's shown
     anyway the cells stay legible by dropping the Z + corner overlays.
     Block classification is then conveyed via border-style instead of
     the (hidden) glyph so we don't fall back to a colour-only signal
     (WCAG 1.4.1). */
  @media (max-width: 600px) {
    .pt-grid { gap: 1px; padding: 0.25rem; }
    .pt-cell { padding: 0.05rem; }
    .cell-z, .cell-block, .cell-enrich { display: none; }
    .cell-sym { font-size: 0.7rem; }
    .pt-cell[data-block="s"] { border-style: solid; }
    .pt-cell[data-block="p"] { border-style: dashed; }
    .pt-cell[data-block="d"] { border-style: dotted; }
    .pt-cell[data-block="f"] { border-style: double; border-width: 3px; }
  }
</style>

<script lang="ts">
  import { parseFormula } from "@hyrr/compute";
  import {
    SUPPLIER_CATALOG,
    getSuppliersForIsotope,
    resolveSupplierUrl,
    type Supplier,
  } from "../../data/suppliers";

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

  /** Per enriched element: dominant mass + its fraction. */
  type EnrichedTarget = { element: string; mass: number; fraction: number };

  let enrichedTargets = $derived.by<EnrichedTarget[]>(() => {
    const out: EnrichedTarget[] = [];
    if (!currentEnrichment) return out;
    for (const [el, masses] of Object.entries(currentEnrichment)) {
      const entries = Object.entries(masses);
      if (entries.length === 0) continue;
      let bestMass = -1;
      let bestFrac = -1;
      for (const [m, f] of entries) {
        const mass = Number(m);
        // On a tie (e.g. user sets Mo-98 = Mo-100 = 0.5) prefer the
        // heavier isotope — the typical procurement intent for medical
        // targets is the more-neutron-rich mass.
        if (f > bestFrac || (f === bestFrac && mass > bestMass)) {
          bestFrac = f;
          bestMass = mass;
        }
      }
      if (bestMass > 0) out.push({ element: el, mass: bestMass, fraction: bestFrac });
    }
    return out;
  });

  /** Format an integer mass as a Unicode superscript prefix (e.g. 68 → "⁶⁸"). */
  const SUPER = ["⁰", "¹", "²", "³", "⁴", "⁵", "⁶", "⁷", "⁸", "⁹"];
  function superMass(mass: number): string {
    return String(mass).split("").map((d) => SUPER[Number(d)] ?? d).join("");
  }

  function formatFraction(frac: number): string {
    const pct = frac * 100;
    return pct >= 99.95 ? "100" : pct.toFixed(pct >= 10 ? 1 : 2);
  }

  function flagSummary(s: Supplier): string {
    const flags = s.flags ?? [];
    if (flags.length === 0) return "";
    return flags.map((f) => `${f.type}: ${f.detail} (as of ${f.asOf})`).join("\n");
  }

  function chipTitle(s: Supplier): string {
    const parts = [s.country];
    if (s.notes) parts.push(s.notes);
    parts.push(`Last reviewed: ${SUPPLIER_CATALOG.last_reviewed}`);
    const flag = flagSummary(s);
    if (flag) parts.push(flag);
    return parts.join("\n");
  }

  function suppliersFor(t: EnrichedTarget): Supplier[] {
    return getSuppliersForIsotope(t.element, t.mass);
  }
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

{#if enrichedTargets.length > 0}
  <div class="sourcing-block" data-testid="supplier-block">
    {#each enrichedTargets as t (t.element + t.mass)}
      {@const suppliers = suppliersFor(t)}
      <div class="sourcing-row">
        <span class="src-label">
          Where to source <span class="iso-name">{superMass(t.mass)}{t.element}</span>
          @ {formatFraction(t.fraction)}%:
        </span>
        {#if suppliers.length === 0}
          <span class="empty">No curated suppliers in catalog.</span>
        {:else}
          <div class="chips">
            {#each suppliers as s (s.id)}
              <a
                class="supplier-chip"
                class:flagged={(s.flags ?? []).length > 0}
                href={resolveSupplierUrl(s, t.element, t.mass)}
                target="_blank"
                rel="noopener noreferrer"
                title={chipTitle(s)}
                data-testid="supplier-chip"
                data-supplier-id={s.id}
              >
                <span class="chip-name">{s.name}</span>
                {#if (s.flags ?? []).length > 0}
                  <span class="chip-flag" aria-label="restricted supplier">⚑</span>
                {/if}
              </a>
            {/each}
          </div>
        {/if}
      </div>
    {/each}
    <div class="reviewed-footer">
      Supplier catalog last reviewed: {SUPPLIER_CATALOG.last_reviewed}
    </div>
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

  .sourcing-block {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    margin-top: 0.4rem;
    padding: 0.4rem 0.5rem;
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 4px;
  }

  .sourcing-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.4rem;
  }

  .src-label {
    font-size: 0.7rem;
    color: var(--c-text-muted);
  }

  .iso-name {
    color: var(--c-gold);
    font-weight: 600;
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 0.25rem;
  }

  .supplier-chip {
    display: inline-flex;
    align-items: center;
    gap: 0.2rem;
    background: var(--c-bg-muted);
    border: 1px solid var(--c-border);
    border-radius: 999px;
    color: var(--c-text-default);
    font-size: 0.7rem;
    line-height: 1;
    padding: 0.2rem 0.5rem;
    text-decoration: none;
    white-space: nowrap;
  }

  .supplier-chip:hover {
    border-color: var(--c-accent);
    color: var(--c-accent);
    background: var(--c-accent-tint-subtle, var(--c-bg-muted));
  }

  .supplier-chip.flagged {
    border-color: var(--c-warning, #c87a00);
    color: var(--c-warning, #c87a00);
    background: var(--c-warning-tint-subtle, var(--c-bg-muted));
  }

  .chip-flag {
    font-size: 0.65rem;
  }

  .empty {
    font-size: 0.7rem;
    color: var(--c-text-muted);
    font-style: italic;
  }

  .reviewed-footer {
    font-size: 0.62rem;
    color: var(--c-text-muted);
    opacity: 0.75;
    margin-top: 0.1rem;
  }
</style>

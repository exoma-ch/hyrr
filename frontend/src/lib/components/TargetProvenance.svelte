<script lang="ts">
  /**
   * What was actually irradiated (#666).
   *
   * A shared result used to carry its answers in full and its question barely
   * at all: the material survived only as an unresolved name ("havar"), while
   * the composition that name resolved to never left the engine. Two runs whose
   * yields differ by orders of magnitude — natural vs enriched — looked
   * identical in the artifact.
   *
   * Collapsed by default: this is reference material for someone checking a
   * result, not something to push past the numbers they opened the file for.
   */
  import type { SimulationResult } from "@hyrr/compute";

  let { result }: { result: SimulationResult } = $props();

  // The name lives in the config (what was asked for); the composition lives in
  // the layer result (what was used). Pairing them is the point — a mismatch
  // between the two is exactly what this panel makes visible.
  let rows = $derived(
    (result.layers ?? []).map((layer, i) => ({
      index: i,
      name: (result.config?.layers ?? [])[i]?.material ?? `layer ${i + 1}`,
      prov: layer.provenance,
      sources: layer.stopping_power_sources ?? {},
    })),
  );

  let anyProvenance = $derived(rows.some((r) => r.prov != null));

  const fmt = (v: number | undefined, digits = 4) =>
    v == null || !Number.isFinite(v) ? "—" : Number(v.toPrecision(digits)).toString();

  /** "98: 90%, 100: 10%" — the resolved abundance vector, not the symbol alone. */
  function isotopeVector(isotopes: Record<string, number> | undefined): string {
    if (!isotopes) return "—";
    const entries = Object.entries(isotopes).sort((a, b) => Number(a[0]) - Number(b[0]));
    if (entries.length === 0) return "—";
    return entries.map(([a, frac]) => `${a}: ${(frac * 100).toFixed(2)}%`).join(", ");
  }
</script>

<details class="prov">
  <summary>Target &amp; provenance</summary>

  {#if !anyProvenance}
    <!-- Absence is reported, never papered over: a result produced before #666
         carries no composition, and saying so is different from implying the
         target had none. -->
    <p class="none">
      This result predates target provenance, so the composition it was computed
      against was not recorded. The material names above come from the config.
    </p>
  {:else}
    {#each rows as row (row.index)}
      <section>
        <h4>
          {row.name}
          {#if row.prov?.is_monitor}<span class="tag">monitor</span>{/if}
          {#if row.prov?.nist_compound}
            <span class="tag" title="ICRU-measured stopping data replaced Bragg additivity">
              NIST {row.prov.nist_compound}
            </span>
          {/if}
        </h4>

        {#if row.prov}
          <dl>
            <dt>Density</dt>
            <dd>{fmt(row.prov.density_g_cm3)} g/cm³</dd>
            <dt>Thickness used</dt>
            <dd>{fmt(row.prov.thickness_cm)} cm</dd>
            {#if row.prov.areal_density_g_cm2 != null}
              <dt>Areal density</dt>
              <dd>{fmt(row.prov.areal_density_g_cm2)} g/cm²</dd>
            {/if}
          </dl>

          <table>
            <thead>
              <tr><th>Element</th><th>Z</th><th>Atom fraction</th><th>Isotopes</th><th>Stopping</th></tr>
            </thead>
            <tbody>
              {#each row.prov.elements as el (el.z)}
                <tr>
                  <td>{el.symbol}</td>
                  <td>{el.z}</td>
                  <td>{fmt(el.atom_fraction)}</td>
                  <td class="iso">{isotopeVector(el.isotopes)}</td>
                  <td>{row.sources[String(el.z)] ?? "—"}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        {:else}
          <p class="none">No composition recorded for this layer.</p>
        {/if}
      </section>
    {/each}
  {/if}
</details>

<style>
  .prov {
    border: 1px solid var(--border, #d0d7de);
    border-radius: 6px;
    padding: 0.5rem 0.75rem;
    font-size: 0.9rem;
  }
  summary {
    cursor: pointer;
    font-weight: 600;
  }
  section {
    margin-top: 0.75rem;
  }
  h4 {
    margin: 0 0 0.35rem;
    font-size: 0.95rem;
  }
  .tag {
    font-size: 0.75rem;
    font-weight: 400;
    border: 1px solid var(--border, #d0d7de);
    border-radius: 999px;
    padding: 0 0.4rem;
    margin-left: 0.35rem;
  }
  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 0 0.75rem;
    margin: 0 0 0.5rem;
  }
  dt {
    opacity: 0.7;
  }
  dd {
    margin: 0;
  }
  table {
    border-collapse: collapse;
    width: 100%;
  }
  th,
  td {
    text-align: left;
    padding: 0.15rem 0.5rem 0.15rem 0;
    border-bottom: 1px solid var(--border, #eaeef2);
  }
  .iso {
    font-variant-numeric: tabular-nums;
  }
  .none {
    opacity: 0.75;
    margin: 0.35rem 0 0;
  }
</style>

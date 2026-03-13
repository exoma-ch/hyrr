<script lang="ts">
  import { getDepthPreview } from "../stores/depth-preview.svelte";
  import { getResult } from "../stores/results.svelte";
  import { getDoseConstant } from "../utils/dose-constants";
  import { fmtDoseRate } from "../utils/format";

  let preview = $derived(getDepthPreview());
  let result = $derived(getResult());

  /** Sum dose@1m (µSv/h) for all isotopes in a layer */
  function layerDose(layerIndex: number): number | null {
    if (!result) return null;
    const layerResult = result.layers[layerIndex];
    if (!layerResult) return null;
    let total = 0;
    let hasAny = false;
    for (const iso of layerResult.isotopes) {
      const d = getDoseConstant(iso.name, iso.activity_Bq);
      if (d !== null) {
        total += d;
        hasAny = true;
      }
    }
    return hasAny ? total : null;
  }

  function totalDose(): number {
    let sum = 0;
    for (let i = 0; i < preview.length; i++) {
      sum += layerDose(i) ?? 0;
    }
    return sum;
  }
</script>

<div class="layer-table">
  {#if preview.length > 0}
    <div class="table-wrapper">
      <table>
        <thead>
          <tr>
            <th class="col-idx">#</th>
            <th class="col-mat">Material</th>
            <th class="col-num">d (mm)</th>
            <th class="col-num">ρd (g/cm²)</th>
            <th class="col-num">E<sub>in</sub> (MeV)</th>
            <th class="col-num">E<sub>out</sub> (MeV)</th>
            <th class="col-num">ΔE (MeV)</th>
            <th class="col-num">Heat (kW)</th>
            <th class="col-num" title="Total dose rate at 1m from all isotopes (end of cooling)">Dose@1m (EOC)</th>
          </tr>
        </thead>
        <tbody>
          {#each preview as layer, i}
            <tr class:has-error={!!layer.error}>
              <td class="col-idx">{i + 1}</td>
              <td class="col-mat">
                {layer.material}
                {#if layer.error}
                  <span class="layer-error" title={layer.error}>{layer.error}</span>
                {/if}
              </td>
              <td class="col-num" class:user-input={layer.userSpecified === "thickness"} class:computed={layer.userSpecified !== "thickness"}>
                {layer.thickness_mm.toFixed(layer.thickness_mm < 1 ? 3 : 1)}
              </td>
              <td class="col-num" class:user-input={layer.userSpecified === "areal_density"} class:computed={layer.userSpecified !== "areal_density"}>
                {layer.areal_density_g_cm2.toFixed(4)}
              </td>
              <td class="col-num computed">{layer.energy_in_MeV.toFixed(2)}</td>
              <td class="col-num" class:user-input={layer.userSpecified === "energy_out"} class:computed={layer.userSpecified !== "energy_out"}>
                {layer.energy_out_MeV.toFixed(2)}
              </td>
              <td class="col-num computed">{layer.delta_E_MeV.toFixed(2)}</td>
              <td class="col-num computed">{layer.heat_kW.toFixed(4)}</td>
              <td class="col-num computed">{layerDose(i) !== null ? fmtDoseRate(layerDose(i)!) : "—"}</td>
            </tr>
          {/each}
        </tbody>
        <tfoot>
          <tr class="total-row">
            <td></td>
            <td class="col-mat">Total</td>
            <td class="col-num">{preview.reduce((s, l) => s + l.thickness_mm, 0).toFixed(1)}</td>
            <td class="col-num">{preview.reduce((s, l) => s + l.areal_density_g_cm2, 0).toFixed(4)}</td>
            <td class="col-num">{preview.length > 0 ? preview[0].energy_in_MeV.toFixed(2) : "—"}</td>
            <td class="col-num">{preview.length > 0 ? preview[preview.length - 1].energy_out_MeV.toFixed(2) : "—"}</td>
            <td class="col-num">{preview.reduce((s, l) => s + l.delta_E_MeV, 0).toFixed(2)}</td>
            <td class="col-num">{preview.reduce((s, l) => s + l.heat_kW, 0).toFixed(4)}</td>
            <td class="col-num">{totalDose() > 0 ? fmtDoseRate(totalDose()) : "—"}</td>
          </tr>
        </tfoot>
      </table>
    </div>
  {:else}
    <p class="empty">Add layers to see energy loss preview</p>
  {/if}
</div>

<style>
  .layer-table {
    background: var(--c-bg-subtle);
    border: 1px solid var(--c-border);
    border-radius: 3px;
    padding: 0.5rem;
  }

  .table-wrapper {
    overflow-x: auto;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.8rem;
  }

  th, td {
    padding: 0.35rem 0.5rem;
    white-space: nowrap;
  }

  th {
    text-align: right;
    border-bottom: 1px solid var(--c-border);
    color: var(--c-text-muted);
    font-weight: 500;
    font-size: 0.75rem;
  }

  td {
    border-bottom: 1px solid var(--c-bg-hover);
    font-variant-numeric: tabular-nums;
  }

  .col-idx {
    width: 30px;
    text-align: center;
    color: var(--c-accent);
    font-weight: 600;
  }

  .col-mat {
    text-align: left;
    color: var(--c-text);
  }

  .col-num {
    text-align: right;
  }

  .user-input {
    color: var(--c-text);
  }

  .computed {
    color: var(--c-text-subtle);
  }

  tr:hover td {
    background: var(--c-bg-hover);
  }

  .total-row td {
    border-top: 1px solid var(--c-border);
    font-weight: 600;
    color: var(--c-text-muted);
  }

  .has-error td {
    background: var(--c-red-tint-faint);
  }

  .layer-error {
    display: block;
    font-size: 0.65rem;
    color: var(--c-red);
    font-weight: 400;
  }

  .empty {
    color: var(--c-text-faint);
    font-style: italic;
    font-size: 0.8rem;
    margin: 0;
    padding: 0.5rem;
  }
</style>

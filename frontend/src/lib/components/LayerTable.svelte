<script lang="ts">
  import { getDepthPreview } from "../stores/depth-preview.svelte";
  import { getResult } from "../stores/results.svelte";
  import { getDoseConstant } from "../utils/dose-constants";
  import { fmtDoseRate } from "../utils/format";
  import { SYMBOL_TO_Z } from "../utils/formula";

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
      const dc = getDoseConstant(iso.name.replace(/m$/, "").replace(/-\d+.*/, ""), iso.A);
      // Try using the element symbol from the name (e.g. "Tc-99m" -> "Tc", A=99)
      const parts = iso.name.match(/^([A-Z][a-z]?)-(\d+)/);
      if (parts) {
        const c = getDoseConstant(parts[1], Number(parts[2]));
        if (c !== null) {
          total += (iso.activity_Bq / 1e6) * c;
          hasAny = true;
        }
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
            <th class="col-num">E_in (MeV)</th>
            <th class="col-num">E_out (MeV)</th>
            <th class="col-num">ΔE (MeV)</th>
            <th class="col-num">Heat (kW)</th>
            <th class="col-num" title="Total dose rate at 1m from all isotopes">Dose@1m</th>
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
    background: #161b22;
    border: 1px solid #2d333b;
    border-radius: 6px;
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
    border-bottom: 1px solid #2d333b;
    color: #8b949e;
    font-weight: 500;
    font-size: 0.75rem;
  }

  td {
    border-bottom: 1px solid #1c2128;
    font-variant-numeric: tabular-nums;
  }

  .col-idx {
    width: 30px;
    text-align: center;
    color: #58a6ff;
    font-weight: 600;
  }

  .col-mat {
    text-align: left;
    color: #e1e4e8;
  }

  .col-num {
    text-align: right;
  }

  .user-input {
    color: #e1e4e8;
  }

  .computed {
    color: #6e7681;
  }

  tr:hover td {
    background: #1c2128;
  }

  .total-row td {
    border-top: 1px solid #2d333b;
    font-weight: 600;
    color: #8b949e;
  }

  .has-error td {
    background: rgba(248, 81, 73, 0.05);
  }

  .layer-error {
    display: block;
    font-size: 0.65rem;
    color: #f85149;
    font-weight: 400;
  }

  .empty {
    color: #484f58;
    font-style: italic;
    font-size: 0.8rem;
    margin: 0;
    padding: 0.5rem;
  }
</style>

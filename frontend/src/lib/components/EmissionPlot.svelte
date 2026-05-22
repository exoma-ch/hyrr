<script lang="ts">
  /**
   * Stack-level gamma emission Rate vs Energy stick-spectrum plot.
   *
   * For each filtered isotope at a chosen time (EOB or EOC), computes
   * emission rate = activity(t) * totalIntensity for every gamma line,
   * then renders as thin Plotly bars (stick spectrum) colored per isotope.
   *
   * Issue #219 — P0: gamma sticks at EOB/EOC with per-isotope colors.
   */
  import { onMount, onDestroy } from "svelte";
  import type { SimulationResult, IsotopeResultData } from "../types";
  import { darkLayout, PLOTLY_CONFIG, TRACE_COLORS, themeColors } from "../plotting/plotly-helpers";
  import type { CsvTrace } from "../plotting/csv-export";
  import SaveMenu from "./SaveMenu.svelte";
  import { getResolvedTheme } from "../stores/theme.svelte";
  import { nucLabel } from "@hyrr/compute";
  import { getDataStore } from "../scheduler/sim-scheduler.svelte";
  import type { EmissionLine, EmissionRadType } from "@hyrr/compute";
  import { getIsotopeFilter } from "../stores/isotope-filter.svelte";
  import { getSelectedIsotopes } from "../stores/selection.svelte";
  import { aggregateByIsotopeName } from "../plotting/aggregate-isotopes";

  const EMISSION_TABS = [
    { id: "gamma", label: "\u03B3", radTypes: ["gamma", "annihilation"] as EmissionRadType[] },
    { id: "beta-", label: "\u03B2\u207B", radTypes: ["beta-"] as EmissionRadType[] },
    { id: "beta+", label: "\u03B2\u207A", radTypes: ["beta+"] as EmissionRadType[] },
    { id: "CE", label: "CE", radTypes: ["ce"] as EmissionRadType[] },
    { id: "xray", label: "X-ray", radTypes: ["xray"] as EmissionRadType[] },
    { id: "auger", label: "Auger", radTypes: ["auger"] as EmissionRadType[] },
  ] as const;

  type EmTabId = (typeof EMISSION_TABS)[number]["id"];

  interface Props {
    result: SimulationResult;
  }

  let { result }: Props = $props();
  let plotDiv = $state<HTMLDivElement | null>(null);
  let Plotly = $state<any>(null);

  let logY = $state(false);
  let activeEmTab = $state<EmTabId>("gamma");
  /** "eob" or "eoc" — which time to evaluate activity at. */
  let timePoint = $state<"eob" | "eoc">("eoc");

  let lastExport = $state<{ traces: CsvTrace[]; xLabel: string; yLabel: string } | null>(null);

  const INTENSITY_THRESHOLD = 0.001; // 0.1% — same as EmissionsTable

  let selected = $derived(getSelectedIsotopes());
  let sharedFilter = $derived(getIsotopeFilter());

  onMount(async () => {
    Plotly = await import("plotly.js-dist-min");
  });

  onDestroy(() => {
    if (Plotly && plotDiv) Plotly.purge(plotDiv);
  });

  // Single effect for all render dependencies
  $effect(() => {
    const p = Plotly;
    const div = plotDiv;
    const _result = result;
    const _sel = selected;
    const _tp = timePoint;
    const _log = logY;
    const _emTab = activeEmTab;
    const _filter = JSON.stringify(sharedFilter);
    const _theme = getResolvedTheme();
    if (p && div) render();
  });

  /** Get activity at the chosen time point for an isotope. */
  function getActivityAtTime(
    iso: { time_grid_s?: number[]; activity_vs_time_Bq?: number[]; activity_Bq: number },
    irrTime: number,
    tp: "eob" | "eoc",
  ): number {
    if (tp === "eoc") {
      // activity_Bq is EOC value
      return iso.activity_Bq;
    }
    // EOB: find the value at irradiation_s in the time grid
    if (!iso.time_grid_s || !iso.activity_vs_time_Bq) return iso.activity_Bq;
    const grid = iso.time_grid_s;
    const acts = iso.activity_vs_time_Bq;
    // Find closest index to irradiation time
    let bestIdx = 0;
    let bestDist = Math.abs(grid[0] - irrTime);
    for (let i = 1; i < grid.length; i++) {
      const dist = Math.abs(grid[i] - irrTime);
      if (dist < bestDist) {
        bestDist = dist;
        bestIdx = i;
      }
    }
    return acts[bestIdx];
  }

  function render() {
    if (!Plotly || !plotDiv) return;
    const tc = themeColors();
    const db = getDataStore();
    if (!db || !db.emissionDataLoaded) return;

    const irrTime = result.config.irradiation_s;

    // Collect filtered isotopes (same logic as PlotActivityCurve)
    let isosToPlot: { iso: IsotopeResultData; layerIdx: number }[] = [];

    for (const layer of result.layers) {
      if (sharedFilter.layers.size > 0 && !sharedFilter.layers.has(layer.layer_index)) continue;
      for (const iso of layer.isotopes) {
        if (!iso.time_grid_s || !iso.activity_vs_time_Bq) continue;
        if (iso.half_life_s === null || iso.activity_Bq <= 0) continue;

        // Apply shared filter fields
        if (sharedFilter.text && !iso.name.toLowerCase().includes(sharedFilter.text.toLowerCase())) continue;
        const zMin = sharedFilter.zMin ? parseInt(sharedFilter.zMin, 10) : NaN;
        const zMax = sharedFilter.zMax ? parseInt(sharedFilter.zMax, 10) : NaN;
        if (!isNaN(zMin) && iso.Z < zMin) continue;
        if (!isNaN(zMax) && iso.Z > zMax) continue;
        const aMin = sharedFilter.aMin ? parseInt(sharedFilter.aMin, 10) : NaN;
        const aMax = sharedFilter.aMax ? parseInt(sharedFilter.aMax, 10) : NaN;
        if (!isNaN(aMin) && iso.A < aMin) continue;
        if (!isNaN(aMax) && iso.A > aMax) continue;
        const eobMin = sharedFilter.eobMin ? parseFloat(sharedFilter.eobMin) : NaN;
        const eobActivity = iso.activity_vs_time_Bq
          ? Math.max(...iso.activity_vs_time_Bq)
          : iso.activity_Bq;
        if (!isNaN(eobMin) && eobActivity < eobMin) continue;
        const eocMin = sharedFilter.eocMin ? parseFloat(sharedFilter.eocMin) : NaN;
        if (!isNaN(eocMin) && iso.activity_Bq < eocMin) continue;

        if (sharedFilter.reactions.size > 0 && iso.reactions) {
          const isoMechs = iso.reactions.map((r) => {
            const m = r.match(/\(([^)]+)\)/);
            return m ? m[1] : "";
          }).filter(Boolean);
          if (!isoMechs.some((m) => sharedFilter.reactions.has(m))) continue;
        }

        isosToPlot.push({ iso, layerIdx: layer.layer_index });
      }
    }

    // Aggregate by isotope name (sum across layers)
    let aggregated = aggregateByIsotopeName(isosToPlot);

    // Filter by selection or take top 15 by EOC activity
    if (selected.size > 0) {
      aggregated = aggregated.filter((agg) => selected.has(agg.name));
    } else {
      aggregated.sort((a, b) => b.activity_Bq - a.activity_Bq);
      aggregated = aggregated.slice(0, 15);
    }

    // Build stick traces per isotope
    const traces: any[] = [];
    let colorIdx = 0;
    let hasAnyLines = false;

    for (const agg of aggregated) {
      // Get emissions matching the active tab's radiation types
      const tabRadTypes = EMISSION_TABS.find((t) => t.id === activeEmTab)?.radTypes ?? ["gamma"];
      const emissions: EmissionLine[] = db.getEmissions(agg.Z, agg.A)
        .filter((e) => tabRadTypes.includes(e.radType));
      if (emissions.length === 0) continue;

      // Get activity at chosen time
      const activity = getActivityAtTime(
        {
          time_grid_s: agg.time_grid_s,
          activity_vs_time_Bq: agg.activity_vs_time_Bq,
          activity_Bq: agg.activity_Bq,
        },
        irrTime,
        timePoint,
      );
      if (activity <= 0) continue;

      // Compute emission rates, filter by intensity threshold
      const energies: number[] = [];
      const rates: number[] = [];
      for (const line of emissions) {
        if (line.intensity < INTENSITY_THRESHOLD) continue;
        energies.push(line.energyKeV);
        rates.push(activity * line.intensity);
      }

      if (energies.length === 0) continue;
      hasAnyLines = true;

      const color = TRACE_COLORS[colorIdx % TRACE_COLORS.length];
      traces.push({
        x: energies,
        y: rates,
        type: "bar",
        name: nucLabel(agg.name),
        marker: { color },
        hovertemplate: `%{x:.1f} keV<br>%{y:.2e} /s<br>${nucLabel(agg.name)}<extra></extra>`,
      });
      colorIdx++;
    }

    // Sort traces so the smallest max-rate renders first (back) and the
    // largest renders last (front). Plotly hover picks the frontmost bar
    // at a given x, so this prioritizes the dominant emission.
    traces.sort((a, b) => {
      const maxA = Math.max(...(a.y as number[]));
      const maxB = Math.max(...(b.y as number[]));
      return maxA - maxB;
    });

    // Adaptive bar width: ~0.3% of the energy range so sticks are
    // visible at any scale (X-rays ~0.1-100 keV vs gammas ~50-7000 keV).
    const allEnergies = traces.flatMap((t: any) => t.x as number[]);
    const eMin = Math.min(...allEnergies);
    const eMax = Math.max(...allEnergies);
    const barWidth = Math.max(0.3, (eMax - eMin) * 0.003 + 0.5);
    for (const t of traces) {
      (t as any).width = barWidth;
    }

    if (!hasAnyLines) {
      const tabLabel = EMISSION_TABS.find((t) => t.id === activeEmTab)?.label ?? activeEmTab;
      Plotly.react(plotDiv, [], darkLayout({
        xaxis: { title: { text: "Energy (keV)" }, gridcolor: tc.border, range: [0, 2000] },
        yaxis: { title: { text: "Emission rate (/s)" }, gridcolor: tc.border, range: [0, 1] },
        annotations: [{
          text: `No ${tabLabel} emission data for selected isotopes`,
          xref: "paper", yref: "paper",
          x: 0.5, y: 0.5,
          showarrow: false,
          font: { size: 13, color: tc.textMuted ?? "#888" },
        }],
      }), PLOTLY_CONFIG);
      lastExport = null;
      return;
    }

    const layout = darkLayout({
      xaxis: {
        title: { text: "Energy (keV)" },
        gridcolor: tc.border,
      },
      yaxis: {
        title: { text: "Emission rate (/s)" },
        gridcolor: tc.border,
        type: logY ? "log" : "linear",
      },
      barmode: "overlay",
      bargap: 0,
      hovermode: "closest",
      hoverdistance: 30,
      showlegend: traces.length <= 20,
      legend: {
        x: 1,
        xanchor: "right",
        y: 1,
        bgcolor: "rgba(0,0,0,0)",
      },
    });

    lastExport = {
      xLabel: "Energy (keV)",
      yLabel: "Emission rate (/s)",
      traces: traces.map((t: any) => ({ name: t.name, x: [...t.x], y: [...t.y] })),
    };
    Plotly.react(plotDiv, traces, layout, PLOTLY_CONFIG);
  }
</script>

<div class="emission-plot">
  <div class="controls">
    <span class="section-label">Emission spectrum</span>

    {#each EMISSION_TABS as tab}
      <button
        class="ctrl-btn"
        class:active={activeEmTab === tab.id}
        onclick={() => { activeEmTab = tab.id; }}
      >
        {tab.label}
      </button>
    {/each}

    <div class="separator"></div>

    <button
      class="ctrl-btn"
      class:active={timePoint === "eob"}
      onclick={() => { timePoint = "eob"; }}
    >
      EOB
    </button>
    <button
      class="ctrl-btn"
      class:active={timePoint === "eoc"}
      onclick={() => { timePoint = "eoc"; }}
    >
      EOC
    </button>

    <div class="separator"></div>

    <button class="ctrl-btn" class:active={logY} onclick={() => { logY = !logY; }}>
      log Y
    </button>

    <span class="right-actions">
      <SaveMenu
        filenamePrefix="hyrr-{activeEmTab}-emission"
        xLabel={lastExport?.xLabel ?? "Energy (keV)"}
        yLabel={lastExport?.yLabel ?? "Emission rate (/s)"}
        getTraces={() => lastExport?.traces ?? []}
        notes={() => [
          `HYRR ${activeEmTab} emission spectrum (${timePoint === "eob" ? "EOB" : "EOC"})`,
          `generated ${new Date().toISOString()}`,
        ]}
        title="Save / download emission data"
      />
    </span>
  </div>

  <div bind:this={plotDiv} class="plot"></div>
</div>

<style>
  .emission-plot {
    background: var(--c-bg-subtle);
    border: 1px solid var(--c-border);
    border-radius: 3px;
    padding: 0.5rem;
  }

  .controls {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.25rem 0.5rem;
    flex-wrap: wrap;
  }

  .section-label {
    font-size: 0.65rem;
    color: var(--c-text-subtle);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    flex-shrink: 0;
    margin-right: 0.3rem;
  }

  .right-actions {
    margin-left: auto;
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
  }

  .separator {
    width: 1px;
    height: 16px;
    background: var(--c-border);
  }

  .ctrl-btn {
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text-muted);
    padding: 0.2rem 0.5rem;
    font-size: 0.7rem;
    cursor: pointer;
  }

  .ctrl-btn:hover {
    border-color: var(--c-accent);
    color: var(--c-text);
  }

  .ctrl-btn.active {
    background: var(--c-bg-active);
    border-color: var(--c-accent);
    color: var(--c-accent);
  }

  .plot {
    width: 100%;
    min-height: 350px;
  }
</style>

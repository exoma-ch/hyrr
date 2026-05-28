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
    { id: "alpha", label: "\u03B1", radTypes: ["alpha"] as EmissionRadType[] },
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

  let logX = $state(false);
  let logY = $state(false);
  let activeEmTab = $state<EmTabId>("gamma");
  /** "eob" or "eoc" — which time to evaluate activity at. */
  let timePoint = $state<"eob" | "eoc">("eoc");

  let lastExport = $state<{ traces: CsvTrace[]; xLabel: string; yLabel: string } | null>(null);

  const INTENSITY_THRESHOLD = 0.001; // 0.1% — same as EmissionsTable

  let selected = $derived(getSelectedIsotopes());
  let sharedFilter = $derived(getIsotopeFilter());

  /** Which tabs have emission data for the current filtered isotopes. */
  let tabHasData = $derived.by((): Record<string, boolean> => {
    const db = getDataStore();
    if (!db?.emissionDataLoaded) return {};
    const has: Record<string, boolean> = {};
    // Collect all isotopes passing the filter (same logic as render)
    const isos: { Z: number; A: number; state: string }[] = [];
    for (const layer of (result?.layers ?? [])) {
      if (sharedFilter.layers.size > 0 && !sharedFilter.layers.has(layer.layer_index)) continue;
      for (const iso of layer.isotopes) {
        if (iso.half_life_s === null || iso.activity_Bq <= 0) continue;
        if (sharedFilter.text && !iso.name.toLowerCase().includes(sharedFilter.text.toLowerCase())) continue;
        if (selected.size > 0 && !selected.has(iso.name)) continue;
        isos.push({ Z: iso.Z, A: iso.A, state: iso.state ?? "" });
      }
    }
    for (const tab of EMISSION_TABS) {
      has[tab.id] = isos.some((iso) => {
        const emissions = db.getEmissions(iso.Z, iso.A, iso.state);
        return emissions.some((e) => tab.radTypes.includes(e.radType));
      });
    }
    return has;
  });

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
    const _logX = logX;
    const _logY = logY;
    const _emTab = activeEmTab;
    const _filter = JSON.stringify(sharedFilter);
    const _theme = getResolvedTheme();
    if (p && div) render();
  });

  // ---------------------------------------------------------------------------
  // β spectrum shape (Fermi function × phase space)
  // ---------------------------------------------------------------------------

  const ME_KEV = 510.999; // electron rest mass in keV
  const ALPHA_FS = 1 / 137.036; // fine-structure constant

  /**
   * Compute the allowed β spectrum shape N(E) for a single transition.
   * Returns arrays of (energy_keV, N) normalized so ∫N dE = 1.
   *
   * @param E0 - endpoint kinetic energy in keV
   * @param Z  - atomic number of the DAUGHTER nucleus
   * @param isPlus - true for β⁺, false for β⁻
   * @param nPts - number of energy bins
   */
  function betaSpectrum(
    E0: number, Z: number, isPlus: boolean, nPts = 100,
  ): { energies: number[]; shape: number[] } {
    if (E0 <= 0) return { energies: [], shape: [] };
    const energies: number[] = [];
    const shape: number[] = [];
    const dE = E0 / nPts;

    for (let i = 1; i < nPts; i++) {
      const E = i * dE; // kinetic energy
      const Etot = E + ME_KEV; // total energy
      const p = Math.sqrt(Etot * Etot - ME_KEV * ME_KEV); // momentum (keV/c)
      const phasespace = p * Etot * (E0 - E) * (E0 - E);

      // Fermi function (non-relativistic approximation)
      const eta = (isPlus ? -1 : 1) * Z * ALPHA_FS * Etot / p;
      const twoPiEta = 2 * Math.PI * eta;
      const fermi = Math.abs(twoPiEta) < 1e-6
        ? 1.0 // limit for small η
        : twoPiEta / (1 - Math.exp(-twoPiEta));

      energies.push(E);
      shape.push(fermi * phasespace);
    }

    // Normalize to unit area
    let area = 0;
    for (let i = 0; i < shape.length - 1; i++) {
      area += 0.5 * (shape[i] + shape[i + 1]) * dE;
    }
    if (area > 0) {
      for (let i = 0; i < shape.length; i++) {
        shape[i] /= area;
      }
    }

    return { energies, shape };
  }

  const BETA_TABS = new Set(["beta-", "beta+"]);

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

    // Build traces per isotope — bars for discrete, curves for β
    const traces: any[] = [];
    let colorIdx = 0;
    let hasAnyLines = false;
    const isBeta = BETA_TABS.has(activeEmTab);
    const isBetaPlus = activeEmTab === "beta+";


    for (const agg of aggregated) {
      const tabRadTypes = EMISSION_TABS.find((t) => t.id === activeEmTab)?.radTypes ?? ["gamma"];
      const emissions: EmissionLine[] = db.getEmissions(agg.Z, agg.A, agg.state)
        .filter((e) => tabRadTypes.includes(e.radType));
      if (emissions.length === 0) continue;

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

      if (isBeta) {
        // β continuous spectrum: compute Fermi shape for each endpoint
        // Sum multiple transitions (e.g. allowed + first-forbidden)
        const nPts = 100;
        let combinedX: number[] | null = null;
        let combinedY: number[] | null = null;

        for (const line of emissions) {
          if (line.intensity < INTENSITY_THRESHOLD) continue;
          const { energies: ex, shape } = betaSpectrum(
            line.energyKeV, agg.Z, isBetaPlus, nPts,
          );
          if (ex.length === 0) continue;

          // Scale: rate spectrum = activity × branching × shape (already normalized)
          const scaled = shape.map((s) => activity * line.intensity * s);

          if (!combinedX) {
            combinedX = ex;
            combinedY = scaled;
          } else {
            // Add to existing (same grid since nPts is constant — may differ in range)
            // Use the longer range
            if (ex.length > combinedX.length) {
              const padded = new Array(ex.length).fill(0);
              for (let i = 0; i < combinedY.length; i++) padded[i] = combinedY[i];
              combinedX = ex;
              combinedY = padded;
            }
            for (let i = 0; i < Math.min(scaled.length, combinedY!.length); i++) {
              combinedY![i] += scaled[i];
            }
          }
        }

        if (!combinedX || !combinedY) continue;
        hasAnyLines = true;

        const color = TRACE_COLORS[colorIdx % TRACE_COLORS.length];
        traces.push({
          x: combinedX,
          y: combinedY,
          type: "scatter",
          mode: "lines",
          fill: "tozeroy",
          fillcolor: color.replace(")", ", 0.15)").replace("rgb", "rgba"),
          line: { color, width: 1.5 },
          name: nucLabel(agg.name),
          hovertemplate: `%{x:.1f} keV<br>%{y:.2e} /s/keV<br>${nucLabel(agg.name)}<extra></extra>`,
        });
        colorIdx++;
      } else {
        // Discrete lines: bar sticks
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
          marker: { color, opacity: 0.85 },
          hovertemplate: `%{y:.2e} /s<extra></extra>`,
        });
        colorIdx++;
      }
    }

    // β sum envelope: sample all isotope traces on a shared grid
    if (isBeta && traces.length > 1) {
      const betaTraces = traces.filter((t: any) => t.type === "scatter");
      if (betaTraces.length > 1) {
        const allX = betaTraces.flatMap((t: any) => t.x as number[]);
        const eMax = Math.max(...allX);
        const nGrid = 200;
        const grid: number[] = [];
        for (let i = 0; i <= nGrid; i++) grid.push((i / nGrid) * eMax);

        const sumY = new Array(grid.length).fill(0);
        for (const t of betaTraces) {
          const tx = t.x as number[];
          const ty = t.y as number[];
          for (let gi = 0; gi < grid.length; gi++) {
            const e = grid[gi];
            // Linear interpolation into this trace's data
            if (e <= tx[0]) { sumY[gi] += ty[0]; continue; }
            if (e >= tx[tx.length - 1]) continue; // beyond endpoint → 0
            let lo = 0;
            for (let j = 1; j < tx.length; j++) {
              if (tx[j] >= e) { lo = j - 1; break; }
            }
            const frac = (e - tx[lo]) / (tx[lo + 1] - tx[lo]);
            sumY[gi] += ty[lo] + frac * (ty[lo + 1] - ty[lo]);
          }
        }

        traces.push({
          x: grid,
          y: sumY,
          type: "scatter",
          mode: "lines",
          line: { color: "#fff", width: 2, dash: "dash" },
          name: "Sum",
          hoverinfo: "skip",
        });
      }
    }

    // For discrete tabs: sort by max rate descending so the unified tooltip
    // lists contributions from strongest to weakest.
    if (!isBeta) {
      traces.sort((a, b) => {
        const maxA = Math.max(...(a.y as number[]));
        const maxB = Math.max(...(b.y as number[]));
        return maxB - maxA;
      });
    }

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
        type: logX ? "log" : "linear",
      },
      yaxis: {
        title: { text: isBeta ? "Spectral rate (/s/keV)" : "Emission rate (/s)" },
        gridcolor: tc.border,
        type: logY ? "log" : "linear",
      },
      barmode: "stack",
      bargap: 0,
      hovermode: isBeta ? "closest" : "x unified",
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
        disabled={tabHasData[tab.id] === false}
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

    <button class="ctrl-btn" class:active={logX} onclick={() => { logX = !logX; }}>
      log X
    </button>
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

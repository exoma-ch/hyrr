<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import type { SimulationResult } from "../types";
  import { darkLayout, PLOTLY_CONFIG, TRACE_COLORS, themeColors } from "../plotting/plotly-helpers";
  import { getResolvedTheme } from "../stores/theme.svelte";
  import { nucLabel } from "@hyrr/compute";
  import { getSelectedIsotopes } from "../stores/selection.svelte";
  import { getIsotopeFilter } from "../stores/isotope-filter.svelte";
  import type { CsvTrace } from "../plotting/csv-export";
  import SaveMenu from "./SaveMenu.svelte";

  interface Props {
    result: SimulationResult;
  }

  let { result }: Props = $props();
  let plotDiv = $state<HTMLDivElement | null>(null);
  let Plotly = $state<any>(null);
  let logY = $state(false);

  let selected = $derived(getSelectedIsotopes());
  let sharedFilter = $derived(getIsotopeFilter());

  let legendVisibility = new Map<string, boolean | "legendonly">();
  let legendListenersAttached = false;

  let lastExport: { traces: CsvTrace[]; xLabel: string; yLabel: string } | null = null;

  onMount(async () => {
    Plotly = await import("plotly.js-dist-min");
  });

  function attachLegendListeners() {
    if (!plotDiv || legendListenersAttached) return;
    const el = plotDiv as any;
    if (!el.on) return;
    legendListenersAttached = true;
    el.on("plotly_legendclick", (evt: any) => {
      const trace = evt.data[evt.curveNumber];
      if (!trace?.name) return false;
      const name = trace.name as string;
      const currentlyHidden = legendVisibility.get(name) === "legendonly";
      legendVisibility.set(name, currentlyHidden ? true : "legendonly");
      render();
      return false;
    });
    el.on("plotly_legenddoubleclick", (evt: any) => {
      const clickedTrace = evt.data[evt.curveNumber];
      if (!clickedTrace?.name) return false;
      const clickedName = clickedTrace.name as string;
      const allHiddenExceptThis = [...evt.data].every(
        (t: any) =>
          t.name === clickedName ||
          legendVisibility.get(t.name) === "legendonly",
      );
      if (allHiddenExceptThis) {
        legendVisibility.clear();
      } else {
        for (const t of evt.data) {
          if (t.name && t.name !== clickedName) {
            legendVisibility.set(t.name, "legendonly");
          }
        }
        legendVisibility.set(clickedName, true);
      }
      render();
      return false;
    });
  }

  onDestroy(() => {
    if (Plotly && plotDiv) Plotly.purge(plotDiv);
  });

  /** Check if any layer has depth production rates. */
  let hasData = $derived(result.layers.some((l) => l.depth_production_rates && Object.keys(l.depth_production_rates).length > 0));

  $effect(() => {
    const p = Plotly;
    const div = plotDiv;
    const _result = result;
    const _sel = selected;
    const _log = logY;
    const _filter = JSON.stringify(sharedFilter);
    const _theme = getResolvedTheme();
    if (p && div && hasData) render();
  });

  function render() {
    if (!Plotly || !plotDiv) return;
    const tc = themeColors();

    // Build continuous depth axis across all layers
    // For each layer: offset depth by cumulative depth of prior layers
    type IsoDepthEntry = { depths: number[]; rates: number[] };
    const isoData = new Map<string, IsoDepthEntry>();
    const boundaries: { depth: number; label: string }[] = [];
    let cumulativeDepth = 0;

    for (const layer of result.layers) {
      const dp = layer.depth_profile;
      const dpr = layer.depth_production_rates;
      if (!dp || dp.length === 0) {
        // Still accumulate layer thickness for offset
        if (dp && dp.length > 0) {
          cumulativeDepth += dp[dp.length - 1].depth_mm;
        }
        continue;
      }

      const material = result.config.layers[layer.layer_index]?.material ?? "?";
      boundaries.push({ depth: cumulativeDepth, label: material });

      const layerThickness = dp[dp.length - 1].depth_mm;
      const layerStart = cumulativeDepth;
      const layerEnd = cumulativeDepth + layerThickness;
      const presentInLayer = new Set<string>();

      if (dpr) {
        for (const [name, rates] of Object.entries(dpr)) {
          presentInLayer.add(name);
          if (!isoData.has(name)) {
            isoData.set(name, { depths: [], rates: [] });
          }
          const entry = isoData.get(name)!;
          // At every layer boundary where this isotope is present, anchor a
          // zero at layerStart so Plotly draws a true vertical step from the
          // prior layer's last value (which is either 0 if the isotope was
          // absent there, or its real final rate) down to or up from 0. This
          // handles both (a) isotope was in prior layer and goes to 0 here →
          // vertical drop at boundary, and (b) isotope first appears in this
          // layer N>0 → rises from 0 at boundary, not from ambient.
          entry.depths.push(layerStart);
          entry.rates.push(0);
          for (let i = 0; i < Math.min(dp.length, rates.length); i++) {
            entry.depths.push(layerStart + dp[i].depth_mm);
            entry.rates.push(rates[i]);
          }
        }
      }

      // For isotopes NOT produced in this layer but present in prior layers,
      // insert a zero segment spanning this layer (so the line drops to zero)
      for (const [name, entry] of isoData) {
        if (!presentInLayer.has(name) && entry.depths.length > 0) {
          entry.depths.push(layerStart);
          entry.rates.push(0);
          entry.depths.push(layerEnd);
          entry.rates.push(0);
        }
      }

      cumulativeDepth = layerEnd;
    }

    // Lookup: isotope name → any IsotopeResultData that carries its Z/A/reactions.
    // Filters below (Z, A, reactions) reference those fields.
    const nameToIso = new Map<string, (typeof result.layers)[number]["isotopes"][number]>();
    for (const layer of result.layers) {
      for (const iso of layer.isotopes) {
        if (!nameToIso.has(iso.name)) nameToIso.set(iso.name, iso);
      }
    }

    const zMin = sharedFilter.zMin ? parseInt(sharedFilter.zMin, 10) : NaN;
    const zMax = sharedFilter.zMax ? parseInt(sharedFilter.zMax, 10) : NaN;
    const aMin = sharedFilter.aMin ? parseInt(sharedFilter.aMin, 10) : NaN;
    const aMax = sharedFilter.aMax ? parseInt(sharedFilter.aMax, 10) : NaN;
    const textLower = sharedFilter.text.toLowerCase();
    const reactionSet = sharedFilter.reactions;
    function passesFilter(name: string): boolean {
      const iso = nameToIso.get(name);
      if (textLower && !name.toLowerCase().includes(textLower)) return false;
      if (!iso) return true; // name not in any layer's isotope list — keep
      if (!isNaN(zMin) && iso.Z < zMin) return false;
      if (!isNaN(zMax) && iso.Z > zMax) return false;
      if (!isNaN(aMin) && iso.A < aMin) return false;
      if (!isNaN(aMax) && iso.A > aMax) return false;
      if (reactionSet.size > 0 && iso.reactions) {
        const isoMechs = iso.reactions
          .map((r) => {
            const m = r.match(/\(([^)]+)\)/);
            return m ? m[1] : "";
          })
          .filter(Boolean);
        if (!isoMechs.some((m) => reactionSet.has(m))) return false;
      }
      return true;
    }

    // Rank isotopes by total production integrated over depth
    // (trapezoid ∫ rate dx), not the bare sum of samples — a spiky rate in one
    // thin layer should not out-rank a broadly-produced isotope just because
    // depth sampling differs.
    const ranked = [...isoData.entries()]
      .filter(([name]) => passesFilter(name))
      .map(([name, data]) => {
        let total = 0;
        for (let i = 1; i < data.depths.length; i++) {
          total += 0.5 * (data.rates[i - 1] + data.rates[i]) * (data.depths[i] - data.depths[i - 1]);
        }
        return { name, data, total };
      })
      .filter((r) => r.total > 0)
      .sort((a, b) => b.total - a.total);

    // Filter by selection or top 15
    let toPlot = ranked;
    if (selected.size > 0) {
      toPlot = ranked.filter((r) => selected.has(r.name));
    } else {
      toPlot = ranked.slice(0, 15);
    }

    if (toPlot.length === 0) {
      Plotly.purge(plotDiv);
      return;
    }

    // Log-Y clamp: Plotly drops non-positive y values on log scale, which
    // would erase the vertical drops at layer boundaries. Replace zeros with a
    // floor of min-positive / 1000 so drops stay visible.
    let logFloor = 0;
    if (logY) {
      let minPos = Infinity;
      for (const { data } of toPlot) {
        for (const r of data.rates) {
          if (r > 0 && r < minPos) minPos = r;
        }
      }
      logFloor = Number.isFinite(minPos) ? minPos / 1000 : 0;
    }

    const traces: any[] = [];
    let colorIdx = 0;

    for (const { name, data } of toPlot) {
      const traceName = nucLabel(name);
      const yValues = logY && logFloor > 0
        ? data.rates.map((r) => (r > 0 ? r : logFloor))
        : data.rates;
      traces.push({
        uid: name, // stable identity across re-renders so Plotly reuses traces
        x: data.depths,
        y: yValues,
        name: traceName,
        type: "scatter",
        mode: "lines",
        line: { color: TRACE_COLORS[colorIdx % TRACE_COLORS.length], width: 1.5 },
        visible: legendVisibility.get(traceName) ?? true,
      });
      colorIdx++;
    }

    // Layer boundary lines
    const shapes = boundaries.slice(1).map((b) => ({
      type: "line" as const,
      x0: b.depth,
      x1: b.depth,
      y0: 0,
      y1: 1,
      yref: "paper" as const,
      line: { color: tc.textFaint, width: 1, dash: "dash" as const },
    }));

    // Layer labels at top
    const annotations = boundaries.map((b) => ({
      x: b.depth,
      y: 1.02,
      yref: "paper" as const,
      text: b.label,
      showarrow: false,
      font: { color: tc.textMuted, size: 10 },
      xanchor: "left" as const,
    }));

    const layout = darkLayout({
      xaxis: {
        title: "Depth (mm)",
        gridcolor: tc.border,
      },
      yaxis: {
        title: "Production rate (atoms/s/cm)",
        gridcolor: tc.border,
        type: logY ? "log" : "linear",
      },
      shapes,
      annotations,
      showlegend: traces.length <= 20,
      margin: { t: 40, r: 20, b: 50, l: 80 },
    });

    lastExport = {
      xLabel: "Depth (mm)",
      yLabel: "Production rate (atoms/s/cm)",
      traces: traces.map((t: any) => ({ name: t.name, x: [...t.x], y: [...t.y] })),
    };
    Plotly.react(plotDiv, traces, layout, PLOTLY_CONFIG);
    attachLegendListeners();
  }

</script>

{#if hasData}
  <div class="production-depth">
    <div class="controls">
      <span class="plot-title">Production vs Depth</span>
      <button class="ctrl-btn" class:active={logY} onclick={() => { logY = !logY; }}>
        log Y
      </button>
      <SaveMenu
        filenamePrefix="hyrr-depth-production"
        xLabel={lastExport?.xLabel ?? "Depth (mm)"}
        yLabel={lastExport?.yLabel ?? "Production rate (atoms/s/cm)"}
        getTraces={() => lastExport?.traces ?? []}
        notes={() => [`HYRR production-vs-depth export`, `generated ${new Date().toISOString()}`]}
        title="Save / download plot data"
      />
    </div>
    <div class="plot" bind:this={plotDiv}></div>
  </div>
{/if}

<style>
  .production-depth {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }

  .controls {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.2rem 0;
  }

  .plot-title {
    font-size: 0.75rem;
    color: var(--c-text-muted);
    font-weight: 500;
    margin-right: auto;
  }

  .ctrl-btn {
    background: var(--c-bg-muted);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text-muted);
    font-size: 0.7rem;
    padding: 0.15rem 0.4rem;
    cursor: pointer;
    line-height: 1.2;
  }

  .ctrl-btn:hover { border-color: var(--c-accent); color: var(--c-text); }
  .ctrl-btn.active { background: var(--c-accent-tint-subtle); border-color: var(--c-accent); color: var(--c-accent); }

  .plot {
    width: 100%;
    height: 350px;
  }
</style>

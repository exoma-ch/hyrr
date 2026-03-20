<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import type { SimulationResult } from "../types";
  import { darkLayout, PLOTLY_CONFIG, TRACE_COLORS, themeColors } from "../plotting/plotly-helpers";
  import { getResolvedTheme } from "../stores/theme.svelte";
  import { nucLabel } from "@hyrr/compute";
  import { getSelectedIsotopes } from "../stores/selection.svelte";

  interface Props {
    result: SimulationResult;
  }

  let { result }: Props = $props();
  let plotDiv = $state<HTMLDivElement | null>(null);
  let Plotly = $state<any>(null);
  let logY = $state(false);

  let selected = $derived(getSelectedIsotopes());

  let legendVisibility = new Map<string, boolean | "legendonly">();
  let legendListenersAttached = false;

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
          // Insert zero at layer start if this isotope had data in a prior layer
          // (ensures drop to zero at material boundary)
          if (entry.depths.length > 0) {
            entry.depths.push(layerStart);
            entry.rates.push(0);
          }
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

    // Rank isotopes by total integrated production
    const ranked = [...isoData.entries()]
      .map(([name, data]) => {
        const total = data.rates.reduce((s, v) => s + v, 0);
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

    const traces: any[] = [];
    let colorIdx = 0;

    for (const { name, data } of toPlot) {
      const traceName = nucLabel(name);
      traces.push({
        x: data.depths,
        y: data.rates,
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

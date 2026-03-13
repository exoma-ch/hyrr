<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import type { SimulationResult, IsotopeResultData } from "../types";
  import { darkLayout, PLOTLY_CONFIG, TRACE_COLORS, themeColors } from "../plotting/plotly-helpers";
  import { bestActivityUnit, bestTimeUnit, nucLabel } from "../utils/format";
  import { getSelectedIsotopes, clearSelection } from "../stores/selection.svelte";

  interface Props {
    result: SimulationResult;
  }

  let { result }: Props = $props();
  let plotDiv = $state<HTMLDivElement | null>(null);
  let Plotly = $state<any>(null);

  // Track legend visibility state so it persists across Plotly.react() re-renders.
  // Key = trace name, value = Plotly visibility ("true" | "legendonly").
  let legendVisibility = new Map<string, boolean | "legendonly">();

  let logY = $state(false);
  let useEOBTime = $state(false);
  let rnpMode = $state(false);
  let rnpIsotope = $state("");
  let activityFloor = $state(1); // Default 1 Bq floor to filter noise
  let layerFilter = $state<Set<number>>(new Set());

  let selected = $derived(getSelectedIsotopes());

  // Available isotopes for RNP selector
  let allIsotopes = $derived.by(() => {
    const names = new Set<string>();
    for (const layer of result.layers) {
      for (const iso of layer.isotopes) {
        if (iso.activity_Bq > 0) names.add(iso.name);
      }
    }
    return [...names].sort();
  });

  // Available layers
  let layerLabels = $derived(
    result.layers.map((l, i) => ({
      index: l.layer_index,
      label: `L${l.layer_index + 1}: ${result.config.layers[l.layer_index]?.material ?? "?"}`,
    })),
  );

  function toggleLayerFilter(idx: number) {
    const next = new Set(layerFilter);
    if (next.has(idx)) next.delete(idx);
    else next.add(idx);
    layerFilter = next;
  }

  let legendListenersAttached = false;

  onMount(async () => {
    Plotly = await import("plotly.js-dist-min");
  });

  function attachLegendListeners() {
    if (!plotDiv || legendListenersAttached) return;
    const el = plotDiv as any;
    if (!el.on) return; // Plotly hasn't rendered yet
    legendListenersAttached = true;
    el.on("plotly_legendclick", (evt: any) => {
      const trace = evt.data[evt.curveNumber];
      if (!trace?.name) return false; // let Plotly handle it
      const name = trace.name as string;
      // Toggle: if currently visible, hide it; if legendonly, show it
      const currentlyHidden = legendVisibility.get(name) === "legendonly";
      legendVisibility.set(name, currentlyHidden ? true : "legendonly");
      // Return false to prevent Plotly default — we re-render with our state
      render();
      return false;
    });
    el.on("plotly_legenddoubleclick", (evt: any) => {
      const clickedTrace = evt.data[evt.curveNumber];
      if (!clickedTrace?.name) return false;
      const clickedName = clickedTrace.name as string;
      // Double-click: isolate this trace (hide all others) or restore all
      const allHiddenExceptThis = [...evt.data].every(
        (t: any) =>
          t.name === clickedName ||
          legendVisibility.get(t.name) === "legendonly",
      );
      if (allHiddenExceptThis) {
        // Restore all
        legendVisibility.clear();
      } else {
        // Isolate: hide all except clicked
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

  // Single effect for all render dependencies — read all deps eagerly
  $effect(() => {
    const p = Plotly;
    const div = plotDiv;
    const _result = result;
    const _sel = selected;
    const _log = logY;
    const _eob = useEOBTime;
    const _rnp = rnpMode;
    const _rnpIso = rnpIsotope;
    const _floor = activityFloor;
    const _filter = layerFilter;
    if (p && div) render();
  });

  function render() {
    if (!Plotly || !plotDiv) return;
    const tc = themeColors();

    const irrTime = result.config.irradiation_s;
    const coolTime = result.config.cooling_s || 86400;
    const totalTime = irrTime + coolTime;
    const timeOffset = useEOBTime ? irrTime : 0;

    if (rnpMode && rnpIsotope) {
      renderRNP(irrTime, coolTime, totalTime, timeOffset);
      return;
    }

    // Collect isotopes to plot
    let isosToPlot: { iso: IsotopeResultData; layerIdx: number }[] = [];

    for (const layer of result.layers) {
      if (layerFilter.size > 0 && !layerFilter.has(layer.layer_index)) continue;
      for (const iso of layer.isotopes) {
        if (!iso.time_grid_s || !iso.activity_vs_time_Bq) continue;
        if (iso.half_life_s === null || iso.activity_Bq <= 0) continue;
        isosToPlot.push({ iso, layerIdx: layer.layer_index });
      }
    }

    // Filter by selection or top 15
    if (selected.size > 0) {
      isosToPlot = isosToPlot.filter(({ iso }) => selected.has(iso.name));
    } else {
      isosToPlot.sort((a, b) => b.iso.activity_Bq - a.iso.activity_Bq);
      isosToPlot = isosToPlot.slice(0, 15);
    }

    // Apply activity floor
    if (activityFloor > 0) {
      isosToPlot = isosToPlot.filter(({ iso }) => iso.activity_Bq >= activityFloor);
    }

    // Determine units
    let globalMax = 0;
    for (const { iso } of isosToPlot) {
      for (const a of iso.activity_vs_time_Bq!) {
        if (a > globalMax) globalMax = a;
      }
    }
    const { label: actLabel, divisor: actDiv } = bestActivityUnit(globalMax);
    const { label: timeLabel, divisor: timeDiv } = bestTimeUnit(totalTime);

    const traces: any[] = [];
    let colorIdx = 0;

    for (const { iso } of isosToPlot) {
      const times = iso.time_grid_s!.map((t) => (t - timeOffset) / timeDiv);
      const activities = iso.activity_vs_time_Bq!.map((a) => a / actDiv);

      const traceName = nucLabel(iso.name);
      traces.push({
        x: times,
        y: activities,
        name: traceName,
        type: "scatter",
        mode: "lines",
        line: { color: TRACE_COLORS[colorIdx % TRACE_COLORS.length], width: 1.5 },
        visible: legendVisibility.get(traceName) ?? true,
      });
      colorIdx++;
    }

    // EOB marker
    const eobX = (irrTime - timeOffset) / timeDiv;
    const shapes = [
      {
        type: "line" as const,
        x0: eobX, x1: eobX, y0: 0, y1: 1,
        yref: "paper" as const,
        line: { color: tc.orange, width: 1.5, dash: "dash" as const },
      },
    ];
    const annotations = [
      {
        x: eobX, y: 1.02, yref: "paper" as const,
        text: "EOB", showarrow: false,
        font: { color: tc.orange, size: 11 },
        xanchor: "center" as const,
      },
    ];

    const layout = darkLayout({
      xaxis: {
        title: `Time ${useEOBTime ? "from EOB" : ""} (${timeLabel})`,
        gridcolor: tc.border,
      },
      yaxis: {
        title: `Activity (${actLabel})`,
        gridcolor: tc.border,
        type: logY ? "log" : "linear",
      },
      shapes,
      annotations,
      showlegend: traces.length <= 20,
    });

    Plotly.react(plotDiv, traces, layout, PLOTLY_CONFIG);
    attachLegendListeners();
  }

  function renderRNP(irrTime: number, coolTime: number, totalTime: number, timeOffset: number) {
    if (!Plotly || !plotDiv || !rnpIsotope) return;
    const tc = themeColors();

    const { label: timeLabel, divisor: timeDiv } = bestTimeUnit(totalTime);

    // Find the selected isotope and total activity at each time point
    let selectedTimeGrid: number[] | null = null;
    let selectedActivity: number[] | null = null;
    let totalActivity: number[] | null = null;

    for (const layer of result.layers) {
      if (layerFilter.size > 0 && !layerFilter.has(layer.layer_index)) continue;
      for (const iso of layer.isotopes) {
        if (!iso.time_grid_s || !iso.activity_vs_time_Bq) continue;
        if (iso.name === rnpIsotope) {
          selectedTimeGrid = [...iso.time_grid_s];
          selectedActivity = [...iso.activity_vs_time_Bq];
        }
      }
    }

    if (!selectedTimeGrid || !selectedActivity) return;

    // Sum total activity at each time point
    totalActivity = new Array(selectedTimeGrid.length).fill(0);
    for (const layer of result.layers) {
      if (layerFilter.size > 0 && !layerFilter.has(layer.layer_index)) continue;
      for (const iso of layer.isotopes) {
        if (!iso.time_grid_s || !iso.activity_vs_time_Bq) continue;
        for (let i = 0; i < iso.activity_vs_time_Bq.length && i < totalActivity!.length; i++) {
          totalActivity![i] += iso.activity_vs_time_Bq[i];
        }
      }
    }

    // RNP% = selected / total * 100
    const rnpPercent = selectedActivity.map((a, i) =>
      totalActivity![i] > 0 ? (a / totalActivity![i]) * 100 : 0,
    );

    const times = selectedTimeGrid.map((t) => (t - timeOffset) / timeDiv);

    // Find peak purity (only after EOB)
    let peakIdx = 0;
    let peakVal = 0;
    for (let i = 0; i < rnpPercent.length; i++) {
      if (selectedTimeGrid[i] < irrTime) continue; // skip pre-EOB
      if (rnpPercent[i] > peakVal) {
        peakVal = rnpPercent[i];
        peakIdx = i;
      }
    }

    const { label: actLabel, divisor: actDiv } = bestActivityUnit(
      totalActivity!.reduce((m, v) => Math.max(m, v), 0),
    );

    const rnpTraceName = `RNP% (${nucLabel(rnpIsotope)})`;
    const totalTraceName = `Total activity`;
    const traces: any[] = [
      {
        x: times,
        y: rnpPercent,
        name: rnpTraceName,
        type: "scatter",
        mode: "lines",
        line: { color: TRACE_COLORS[0], width: 2 },
        visible: legendVisibility.get(rnpTraceName) ?? true,
      },
      {
        x: times,
        y: totalActivity!.map((a) => a / actDiv),
        name: totalTraceName,
        type: "scatter",
        mode: "lines",
        line: { color: TRACE_COLORS[2], width: 1, dash: "dot" },
        yaxis: "y2",
        visible: legendVisibility.get(totalTraceName) ?? true,
      },
    ];

    const eobX = (irrTime - timeOffset) / timeDiv;

    const annotations: any[] = [
      {
        x: eobX, y: 1.02, yref: "paper" as const,
        text: "EOB", showarrow: false,
        font: { color: tc.orange, size: 11 },
        xanchor: "center" as const,
      },
    ];

    if (peakVal > 0) {
      annotations.push({
        x: times[peakIdx],
        y: peakVal,
        text: `Peak: ${peakVal.toFixed(1)}%`,
        showarrow: true,
        arrowcolor: tc.greenText,
        font: { color: tc.greenText, size: 10 },
      });
    }

    const layout = darkLayout({
      xaxis: {
        title: `Time ${useEOBTime ? "from EOB" : ""} (${timeLabel})`,
        gridcolor: tc.border,
      },
      yaxis: { title: "RNP (%)", gridcolor: tc.border, range: [0, 105] },
      yaxis2: {
        title: `Total Activity (${actLabel})`,
        overlaying: "y",
        side: "right",
        gridcolor: tc.border,
      },
      shapes: [
        {
          type: "line" as const,
          x0: eobX, x1: eobX, y0: 0, y1: 1,
          yref: "paper" as const,
          line: { color: tc.orange, width: 1.5, dash: "dash" as const },
        },
      ],
      annotations,
    });

    Plotly.react(plotDiv, traces, layout, PLOTLY_CONFIG);
    attachLegendListeners();
  }
</script>

<div class="activity-curve">
  <div class="controls">
    <button class="ctrl-btn" class:active={logY} onclick={() => { logY = !logY; }}>
      log Y
    </button>
    <button class="ctrl-btn" class:active={useEOBTime} onclick={() => { useEOBTime = !useEOBTime; }}>
      t from EOB
    </button>

    <div class="separator"></div>

    <button class="ctrl-btn" class:active={rnpMode} onclick={() => { rnpMode = !rnpMode; }}>
      RNP%
    </button>

    {#if rnpMode}
      <select class="rnp-select" bind:value={rnpIsotope}>
        <option value="">select isotope...</option>
        {#each allIsotopes as name}
          <option value={name}>{name}</option>
        {/each}
      </select>
    {/if}

    {#if selected.size > 0}
      <button class="ctrl-btn clear" onclick={clearSelection}>
        Clear ({selected.size})
      </button>
    {/if}
  </div>

  {#if layerLabels.length > 1}
    <div class="layer-chips">
      {#each layerLabels as ll}
        <button
          class="chip"
          class:active={layerFilter.size === 0 || layerFilter.has(ll.index)}
          onclick={() => toggleLayerFilter(ll.index)}
        >
          {ll.label}
        </button>
      {/each}
    </div>
  {/if}

  <div bind:this={plotDiv} class="plot"></div>
</div>

<style>
  .activity-curve {
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

  .ctrl-btn.clear {
    color: var(--c-red);
    border-color: var(--c-red);
  }

  .rnp-select {
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text);
    padding: 0.2rem 0.3rem;
    font-size: 0.7rem;
  }

  .layer-chips {
    display: flex;
    gap: 0.3rem;
    padding: 0.15rem 0.5rem;
    overflow-x: auto;
  }

  .chip {
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 12px;
    color: var(--c-text-muted);
    padding: 0.15rem 0.5rem;
    font-size: 0.65rem;
    cursor: pointer;
    white-space: nowrap;
  }

  .chip.active {
    border-color: var(--c-accent);
    color: var(--c-text);
  }

  .plot {
    width: 100%;
    min-height: 350px;
  }
</style>

<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import type { SimulationResult, IsotopeResultData } from "../types";
  import { darkLayout, PLOTLY_CONFIG, TRACE_COLORS, themeColors } from "../plotting/plotly-helpers";
  import { getResolvedTheme } from "../stores/theme.svelte";
  import { bestActivityUnit, bestTimeUnit, nucLabel } from "@hyrr/compute";
  import { getSelectedIsotopes, clearSelection } from "../stores/selection.svelte";
  import { getIsotopeFilter } from "../stores/isotope-filter.svelte";

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
  let rnpIsotopes = $state<Set<string>>(new Set());
  let rnpPickerOpen = $state(false);
  let rnpPickerRef = $state<HTMLDivElement | null>(null);
  let rnpQuery = $state("");

  function toggleRnpIso(name: string) {
    const next = new Set(rnpIsotopes);
    if (next.has(name)) next.delete(name); else next.add(name);
    rnpIsotopes = next;
  }

  function clearRnpIsos() {
    rnpIsotopes = new Set();
  }

  function onWindowClick(e: MouseEvent) {
    if (!rnpPickerOpen || !rnpPickerRef) return;
    if (!rnpPickerRef.contains(e.target as Node)) {
      rnpPickerOpen = false;
      rnpQuery = "";
    }
  }

  // Focus the search input when the picker opens.
  $effect(() => {
    if (!rnpPickerOpen || !rnpPickerRef) return;
    requestAnimationFrame(() => {
      rnpPickerRef?.querySelector<HTMLInputElement>(".rnp-search")?.focus();
    });
  });

  let selected = $derived(getSelectedIsotopes());
  let sharedFilter = $derived(getIsotopeFilter());

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

  /** Picker list: selected first (sticky, regardless of search), then any
   *  unselected names that match the query. Case-insensitive substring on
   *  both the raw name (e.g. "Tc-99m") and the rendered nuclide label. */
  let rnpPickerList = $derived.by(() => {
    const q = rnpQuery.trim().toLowerCase();
    const selectedFirst = allIsotopes.filter((n) => rnpIsotopes.has(n));
    const restAll = allIsotopes.filter((n) => !rnpIsotopes.has(n));
    const restFiltered = q
      ? restAll.filter((n) =>
          n.toLowerCase().includes(q) || nucLabel(n).toLowerCase().includes(q),
        )
      : restAll;
    return { selected: selectedFirst, rest: restFiltered };
  });

  // Available layers
  let layerLabels = $derived(
    result.layers.map((l, i) => ({
      index: l.layer_index,
      label: `L${l.layer_index + 1}: ${result.config.layers[l.layer_index]?.material ?? "?"}`,
    })),
  );


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
    const _rnpIsos = rnpIsotopes;
    const _filter = JSON.stringify(sharedFilter);
    const _theme = getResolvedTheme();
    if (p && div) render();
  });

  function render() {
    if (!Plotly || !plotDiv) return;
    const tc = themeColors();

    const irrTime = result.config.irradiation_s;
    const coolTime = result.config.cooling_s || 86400;
    const totalTime = irrTime + coolTime;
    const timeOffset = useEOBTime ? irrTime : 0;

    if (rnpMode && rnpIsotopes.size > 0) {
      renderRNP(irrTime, coolTime, totalTime, timeOffset);
      return;
    }

    // Collect isotopes to plot — apply shared filter
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
        // EOB activity = first point after irradiation (approximate: use max)
        const eobActivity = iso.activity_vs_time_Bq
          ? Math.max(...iso.activity_vs_time_Bq)
          : iso.activity_Bq;
        if (!isNaN(eobMin) && eobActivity < eobMin) continue;
        const eocMin = sharedFilter.eocMin ? parseFloat(sharedFilter.eocMin) : NaN;
        if (!isNaN(eocMin) && iso.activity_Bq < eocMin) continue;

        // Reaction mechanism filter
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

    // Filter by selection or top 15
    if (selected.size > 0) {
      isosToPlot = isosToPlot.filter(({ iso }) => selected.has(iso.name));
    } else {
      isosToPlot.sort((a, b) => b.iso.activity_Bq - a.iso.activity_Bq);
      isosToPlot = isosToPlot.slice(0, 15);
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
    if (!Plotly || !plotDiv || rnpIsotopes.size === 0) return;
    const tc = themeColors();

    const { label: timeLabel, divisor: timeDiv } = bestTimeUnit(totalTime);

    // Walk every isotope in the (filtered) result once: collect activity
    // series for the selected ones, and accumulate total activity at each
    // time index. All isotopes share the same time grid by construction.
    let timeGrid: number[] | null = null;
    const selectedSeries: { name: string; activity: number[] }[] = [];
    let totalActivity: number[] | null = null;

    for (const layer of result.layers) {
      if (sharedFilter.layers.size > 0 && !sharedFilter.layers.has(layer.layer_index)) continue;
      for (const iso of layer.isotopes) {
        if (!iso.time_grid_s || !iso.activity_vs_time_Bq) continue;
        if (!timeGrid) {
          timeGrid = [...iso.time_grid_s];
          totalActivity = new Array(timeGrid.length).fill(0);
        }
        for (let i = 0; i < iso.activity_vs_time_Bq.length && i < totalActivity!.length; i++) {
          totalActivity![i] += iso.activity_vs_time_Bq[i];
        }
        if (rnpIsotopes.has(iso.name)) {
          selectedSeries.push({ name: iso.name, activity: [...iso.activity_vs_time_Bq] });
        }
      }
    }

    if (!timeGrid || !totalActivity || selectedSeries.length === 0) return;

    const N = timeGrid.length;
    const times = timeGrid.map((t) => (t - timeOffset) / timeDiv);

    // Per-isotope RNP%: activity_i / total * 100
    const perIsoRnp = selectedSeries.map(({ name, activity }) => ({
      name,
      rnp: activity.map((a, i) => (totalActivity![i] > 0 ? (a / totalActivity![i]) * 100 : 0)),
    }));

    // Aggregate Σ(selected) and Σ(rest) as fractions of total.
    const selectedSum = new Array<number>(N).fill(0);
    for (const { activity } of selectedSeries) {
      for (let i = 0; i < activity.length && i < N; i++) selectedSum[i] += activity[i];
    }
    const selectedRnp = selectedSum.map((s, i) =>
      totalActivity![i] > 0 ? (s / totalActivity![i]) * 100 : 0,
    );
    const restRnp = selectedRnp.map((p) => 100 - p);

    // (peak computation moved below — see peakPostEOB helper)

    const { label: actLabel, divisor: actDiv } = bestActivityUnit(
      totalActivity!.reduce((m, v) => Math.max(m, v), 0),
    );

    const traces: any[] = [];

    /** Find the post-EOB peak of a series and return {idx, val}. */
    const peakPostEOB = (series: number[]) => {
      let pIdx = 0;
      let pVal = 0;
      for (let i = 0; i < series.length; i++) {
        if (timeGrid![i] < irrTime) continue;
        if (series[i] > pVal) {
          pVal = series[i];
          pIdx = i;
        }
      }
      return { idx: pIdx, val: pVal };
    };

    // Per-isotope thin lines + a small marker at each curve's max so the
    // user can read peak purity per isotope without hovering.
    const perIsoPeaks: { name: string; color: string; idx: number; val: number }[] = [];
    perIsoRnp.forEach(({ name, rnp }, idx) => {
      const color = TRACE_COLORS[idx % TRACE_COLORS.length];
      const traceName = `RNP% (${nucLabel(name)})`;
      traces.push({
        x: times,
        y: rnp,
        name: traceName,
        type: "scatter",
        mode: "lines",
        line: { color, width: 1.5 },
        visible: legendVisibility.get(traceName) ?? true,
      });
      const { idx: pIdx, val: pVal } = peakPostEOB(rnp);
      // Suppress markers below 0.5%: a peak of e.g. 0.01% would render as
      // "0.0%" via toFixed(1) and reads as a stuck-at-zero label, not a
      // useful peak. Larger peaks get a smaller decimal under 1%.
      if (pVal >= 0.5) perIsoPeaks.push({ name, color, idx: pIdx, val: pVal });
    });

    // Aggregate traces — bold dashed so they read as summary rather than
    // another sample. Selected vs rest sums to 100 by definition; both shown
    // so the user can read either side directly.
    const selectedTraceName = `Σ selected`;
    const restTraceName = `Σ rest`;
    traces.push(
      {
        x: times,
        y: selectedRnp,
        name: selectedTraceName,
        type: "scatter",
        mode: "lines",
        line: { color: tc.greenText, width: 2.5, dash: "dash" },
        visible: legendVisibility.get(selectedTraceName) ?? true,
      },
      {
        x: times,
        y: restRnp,
        name: restTraceName,
        type: "scatter",
        mode: "lines",
        line: { color: tc.orange, width: 2.5, dash: "dash" },
        visible: legendVisibility.get(restTraceName) ?? "legendonly",
      },
    );

    const totalTraceName = `Total activity`;
    traces.push({
      x: times,
      y: totalActivity!.map((a) => a / actDiv),
      name: totalTraceName,
      type: "scatter",
      mode: "lines",
      line: { color: tc.border, width: 1, dash: "dot" },
      yaxis: "y2",
      visible: legendVisibility.get(totalTraceName) ?? "legendonly",
    });

    const eobX = (irrTime - timeOffset) / timeDiv;

    const annotations: any[] = [
      {
        x: eobX, y: 1.02, yref: "paper" as const,
        text: "EOB", showarrow: false,
        font: { color: tc.orange, size: 11 },
        xanchor: "center" as const,
      },
    ];

    // Per-isotope peak markers (small, color-matched arrow + value).
    for (const { name, color, idx: pIdx, val: pVal } of perIsoPeaks) {
      annotations.push({
        x: times[pIdx],
        y: pVal,
        text: `${nucLabel(name)} ${pVal < 1 ? pVal.toFixed(2) : pVal.toFixed(1)}%`,
        showarrow: true,
        arrowcolor: color,
        ax: 0,
        ay: -16,
        font: { color, size: 9 },
      });
    }

    // Σ-selected peak (the headline "purity" annotation).
    const sumPeak = peakPostEOB(selectedRnp);
    if (sumPeak.val > 0) {
      annotations.push({
        x: times[sumPeak.idx],
        y: sumPeak.val,
        text: `Peak Σ: ${sumPeak.val.toFixed(1)}%`,
        showarrow: true,
        arrowcolor: tc.greenText,
        ax: 0,
        ay: -22,
        font: { color: tc.greenText, size: 10 },
      });
    }
    // Σ-rest peak — only annotate when the rest trace is actually visible
    // (it's legendonly by default, so the annotation would otherwise be a
    // floating number with no curve to anchor it to).
    const restVisible = legendVisibility.get(`Σ rest`) === true;
    if (restVisible) {
      const restPeak = peakPostEOB(restRnp);
      if (restPeak.val > 0) {
        annotations.push({
          x: times[restPeak.idx],
          y: restPeak.val,
          text: `Peak Σ rest: ${restPeak.val.toFixed(1)}%`,
          showarrow: true,
          arrowcolor: tc.orange,
          ax: 0,
          ay: -16,
          font: { color: tc.orange, size: 9 },
        });
      }
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

<svelte:window onclick={onWindowClick} />

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
      <div class="rnp-picker" bind:this={rnpPickerRef}>
        <button
          type="button"
          class="ctrl-btn"
          aria-haspopup="true"
          aria-expanded={rnpPickerOpen}
          onclick={() => { rnpPickerOpen = !rnpPickerOpen; }}
        >
          {rnpIsotopes.size === 0
            ? "select isotopes…"
            : `${rnpIsotopes.size} selected`}
          <span class="caret">▾</span>
        </button>
        {#if rnpPickerOpen}
          <div class="rnp-popover" role="listbox" aria-multiselectable="true">
            <input
              type="text"
              class="rnp-search"
              placeholder="Search isotopes…"
              bind:value={rnpQuery}
              autocomplete="off"
            />
            {#if allIsotopes.length === 0}
              <p class="rnp-empty">No isotopes with activity.</p>
            {:else}
              {#if rnpPickerList.selected.length > 0}
                <div class="rnp-section-label">Selected</div>
                {#each rnpPickerList.selected as name (name)}
                  <label class="rnp-option">
                    <input
                      type="checkbox"
                      checked
                      onchange={() => toggleRnpIso(name)}
                    />
                    <span>{nucLabel(name)}</span>
                  </label>
                {/each}
                <div class="rnp-divider"></div>
              {/if}
              {#if rnpPickerList.rest.length === 0}
                <p class="rnp-empty">No matches.</p>
              {:else}
                {#each rnpPickerList.rest as name (name)}
                  <label class="rnp-option">
                    <input
                      type="checkbox"
                      checked={false}
                      onchange={() => toggleRnpIso(name)}
                    />
                    <span>{nucLabel(name)}</span>
                  </label>
                {/each}
              {/if}
            {/if}
            {#if rnpIsotopes.size > 0}
              <button class="rnp-clear" type="button" onclick={clearRnpIsos}>Clear all</button>
            {/if}
          </div>
        {/if}
      </div>
    {/if}

    {#if selected.size > 0}
      <button class="ctrl-btn clear" onclick={clearSelection}>
        Clear ({selected.size})
      </button>
    {/if}
  </div>

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

  .rnp-picker {
    position: relative;
    display: inline-flex;
  }

  .rnp-picker .caret {
    margin-left: 0.3rem;
    font-size: 0.6rem;
  }

  .rnp-popover {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    z-index: 50;
    min-width: 9rem;
    max-height: 16rem;
    overflow-y: auto;
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    padding: 0.3rem;
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
  }

  .rnp-option {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.15rem 0.25rem;
    border-radius: 3px;
    cursor: pointer;
    color: var(--c-text-muted);
    font-size: 0.7rem;
  }

  .rnp-option:hover {
    background: var(--c-bg-subtle);
    color: var(--c-text);
  }

  .rnp-option input[type="checkbox"] {
    accent-color: var(--c-accent);
    cursor: pointer;
  }

  .rnp-empty {
    margin: 0;
    padding: 0.3rem;
    color: var(--c-text-subtle);
    font-size: 0.7rem;
    font-style: italic;
  }

  .rnp-search {
    background: var(--c-bg-subtle);
    border: 1px solid var(--c-border);
    border-radius: 3px;
    color: var(--c-text);
    padding: 0.25rem 0.4rem;
    font-size: 0.7rem;
    margin-bottom: 0.2rem;
  }

  .rnp-search:focus {
    outline: none;
    border-color: var(--c-accent);
  }

  .rnp-section-label {
    font-size: 0.6rem;
    text-transform: uppercase;
    color: var(--c-text-subtle);
    padding: 0.2rem 0.3rem 0.05rem;
    letter-spacing: 0.05em;
  }

  .rnp-divider {
    height: 1px;
    background: var(--c-border);
    margin: 0.2rem 0;
  }

  .rnp-clear {
    margin-top: 0.2rem;
    background: transparent;
    border: 1px solid var(--c-border);
    border-radius: 3px;
    color: var(--c-text-muted);
    padding: 0.15rem 0.4rem;
    font-size: 0.65rem;
    cursor: pointer;
  }

  .rnp-clear:hover { color: var(--c-red); border-color: var(--c-red); }

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

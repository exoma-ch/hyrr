<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { getDepthPreview } from "../stores/depth-preview.svelte";
  import { darkLayout, PLOTLY_CONFIG, TRACE_COLORS, themeColors } from "../plotting/plotly-helpers";
  import { getResolvedTheme } from "../stores/theme.svelte";
  import type { CsvTrace } from "../plotting/csv-export";
  import SaveMenu from "./SaveMenu.svelte";

  let plotDiv = $state<HTMLDivElement | null>(null);
  let Plotly = $state<any>(null);

  let lastExport: { traces: CsvTrace[] } | null = null;

  let preview = $derived(getDepthPreview());

  onMount(async () => {
    Plotly = await import("plotly.js-dist-min");
  });

  onDestroy(() => {
    if (Plotly && plotDiv) Plotly.purge(plotDiv);
  });

  $effect(() => {
    // Read all deps eagerly to avoid short-circuit tracking issues
    const p = Plotly;
    const div = plotDiv;
    const prev = preview;
    const _theme = getResolvedTheme();
    if (!p || !div) return;
    if (prev && prev.length > 0) {
      render();
    } else {
      p.purge(div);
    }
  });

  function render() {
    if (!Plotly || !plotDiv) return;
    if (preview.length === 0) return;
    const tc = themeColors();

    const allDepths: number[] = [];
    const allEnergies: number[] = [];
    const allHeat: number[] = [];
    let cumulativeDepth = 0;
    const boundaries: { depth: number; label: string }[] = [];

    for (const layer of preview) {
      boundaries.push({ depth: cumulativeDepth, label: layer.material });
      for (const pt of layer.depthPoints) {
        allDepths.push(cumulativeDepth + pt.depth_mm);
        allEnergies.push(pt.energy_MeV);
        allHeat.push(pt.heat_W_cm3 / 10);
      }
      cumulativeDepth += layer.thickness_mm;
    }

    const traces: any[] = [
      {
        x: allDepths,
        y: allEnergies,
        name: "Energy",
        type: "scatter",
        mode: "lines",
        line: { color: TRACE_COLORS[0], width: 2 },
      },
      {
        x: allDepths,
        y: allHeat,
        name: "Heat (W/mm)",
        type: "scatter",
        mode: "lines",
        line: { color: TRACE_COLORS[3], width: 1.5 },
        yaxis: "y2",
      },
    ];

    const shapes = boundaries.slice(1).map((b) => ({
      type: "line" as const,
      x0: b.depth,
      x1: b.depth,
      y0: 0,
      y1: 1,
      yref: "paper" as const,
      line: { color: tc.textFaint, width: 1, dash: "dot" as const },
    }));

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
      xaxis: { title: "Depth (mm)", gridcolor: tc.border, range: [0, cumulativeDepth] },
      yaxis: { title: "Energy (MeV)", gridcolor: tc.border },
      yaxis2: {
        title: "Heat (W/mm)",
        overlaying: "y",
        side: "right",
        gridcolor: tc.border,
      },
      shapes,
      annotations,
      margin: { t: 30, r: 60, b: 50, l: 60 },
    });

    lastExport = {
      traces: traces.map((t: any) => ({ name: t.name, x: [...t.x], y: [...t.y] })),
    };
    Plotly.react(plotDiv, traces, layout, PLOTLY_CONFIG);
  }
</script>

<div class="depth-profile-live">
  <div class="toolbar">
    <span class="label">Depth profile</span>
    <SaveMenu
      filenamePrefix="hyrr-depth-profile"
      xLabel="Depth (mm)"
      yLabel="Energy (MeV) | Heat (W/mm)"
      getTraces={() => lastExport?.traces ?? []}
      notes={() => [`HYRR depth profile live export`, `generated ${new Date().toISOString()}`]}
      title="Save / download depth profile"
    />
  </div>
  <div bind:this={plotDiv} class="plot"></div>
</div>

<style>
  .depth-profile-live {
    background: var(--c-bg-subtle);
    border: 1px solid var(--c-border);
    border-radius: 3px;
    padding: 0.5rem;
  }
  .toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 0.1rem 0.25rem;
  }
  .label {
    font-size: 0.75rem;
    color: var(--c-text-muted);
    font-weight: 500;
  }

  .plot {
    width: 100%;
    min-height: 300px;
  }
</style>

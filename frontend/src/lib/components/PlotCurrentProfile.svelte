<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import type { CurrentProfile } from "@hyrr/compute";
  import { darkLayout, PLOTLY_CONFIG, TRACE_COLORS } from "../plotting/plotly-helpers";
  import { bestTimeUnit } from "../utils/format";
  import { getResolvedTheme } from "../stores/theme.svelte";

  interface Props {
    profile: CurrentProfile;
    /** Enable draggable trim handles for cropping the time range. */
    trimmable?: boolean;
    /** Trim window in seconds (only used when trimmable). */
    trimStartS?: number;
    trimEndS?: number;
    /** Called when a trim handle is dragged. Seconds. */
    ontrim?: (startS: number, endS: number) => void;
  }

  let { profile, trimmable = false, trimStartS = 0, trimEndS = 0, ontrim }: Props = $props();
  let plotDiv = $state<HTMLDivElement | null>(null);
  let Plotly = $state<any>(null);

  let theme = $derived(getResolvedTheme());

  onMount(async () => {
    Plotly = await import("plotly.js-dist-min");
  });

  onDestroy(() => {
    if (Plotly && plotDiv) {
      plotDiv.removeAllListeners?.("plotly_relayout");
      Plotly.purge(plotDiv);
    }
  });

  function render() {
    if (!Plotly || !plotDiv || !profile) return;

    const n = profile.timesS.length;
    const maxT = profile.timesS[n - 1];
    const tu = bestTimeUnit(maxT);

    const x = Array.from(profile.timesS).map((t) => t / tu.divisor);
    const y = Array.from(profile.currentsMA).map((c) => c * 1000); // mA → µA

    const trace = {
      x,
      y,
      type: "scatter" as const,
      mode: "lines" as const,
      line: { shape: "hv" as const, color: TRACE_COLORS[0], width: 1.5 },
      fill: "tozeroy" as const,
      fillcolor: TRACE_COLORS[0] + "20",
      hovertemplate: "%{x:.2f} " + tu.label + "<br>%{y:.1f} µA<extra></extra>",
    };

    const layoutOpts: Record<string, unknown> = {
      xaxis: { title: { text: `Time (${tu.label})` } },
      yaxis: { title: { text: "Current (µA)" }, rangemode: "tozero" },
      margin: { t: 10, r: 10, b: 40, l: 50 },
      showlegend: false,
      height: 150,
    };

    let config = PLOTLY_CONFIG;

    if (trimmable) {
      const x0 = trimStartS / tu.divisor;
      const x1 = trimEndS / tu.divisor;
      const xMax = maxT / tu.divisor;
      // Shaded "removed" regions + two draggable vertical handles
      layoutOpts.shapes = [
        // dimmed left (removed) — not draggable
        { type: "rect", xref: "x", yref: "paper", x0: 0, x1: x0, y0: 0, y1: 1,
          fillcolor: "rgba(128,128,128,0.18)", line: { width: 0 }, layer: "below", editable: false },
        // dimmed right (removed) — not draggable
        { type: "rect", xref: "x", yref: "paper", x0: x1, x1: xMax, y0: 0, y1: 1,
          fillcolor: "rgba(128,128,128,0.18)", line: { width: 0 }, layer: "below", editable: false },
        // start handle (draggable)
        { type: "line", xref: "x", yref: "paper", x0, x1: x0, y0: 0, y1: 1,
          line: { color: TRACE_COLORS[0], width: 2, dash: "solid" }, editable: true },
        // end handle (draggable)
        { type: "line", xref: "x", yref: "paper", x0: x1, x1, y0: 0, y1: 1,
          line: { color: TRACE_COLORS[0], width: 2, dash: "solid" }, editable: true },
      ];
      config = { ...PLOTLY_CONFIG, edits: { shapePosition: true } };
    }

    Plotly.react(plotDiv, [trace], darkLayout(layoutOpts), config);

    if (trimmable) {
      plotDiv.removeAllListeners?.("plotly_relayout");
      plotDiv.on("plotly_relayout", (ev: Record<string, number>) => {
        // Handle shapes are indices 2 (start) and 3 (end)
        let newStart = trimStartS;
        let newEnd = trimEndS;
        let changed = false;
        for (const key of Object.keys(ev)) {
          const m = key.match(/^shapes\[(\d+)\]\.x[01]$/);
          if (!m) continue;
          const idx = Number(m[1]);
          const valS = ev[key] * tu.divisor;
          if (idx === 2) { newStart = valS; changed = true; }
          if (idx === 3) { newEnd = valS; changed = true; }
        }
        if (changed && ontrim) {
          // Clamp + keep ordering
          const lo = Math.max(0, Math.min(newStart, newEnd));
          const hi = Math.min(maxT, Math.max(newStart, newEnd));
          if (hi > lo) ontrim(lo, hi);
        }
      });
    }
  }

  $effect(() => {
    void profile;
    void Plotly;
    void plotDiv;
    void theme;
    void trimmable;
    void trimStartS;
    void trimEndS;
    render();
  });
</script>

<div class="profile-plot" bind:this={plotDiv}></div>

<style>
  .profile-plot {
    width: 100%;
    min-height: 150px;
    border-radius: 4px;
    overflow: hidden;
  }
</style>

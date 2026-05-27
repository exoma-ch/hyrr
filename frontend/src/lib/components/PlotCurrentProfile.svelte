<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import type { CurrentProfile } from "@hyrr/compute";
  import { darkLayout, PLOTLY_CONFIG, TRACE_COLORS } from "../plotting/plotly-helpers";
  import { bestTimeUnit } from "../utils/format";
  import { getResolvedTheme } from "../stores/theme.svelte";

  interface Props {
    profile: CurrentProfile;
  }

  let { profile }: Props = $props();
  let plotDiv = $state<HTMLDivElement | null>(null);
  let Plotly = $state<any>(null);

  let theme = $derived(getResolvedTheme());

  onMount(async () => {
    Plotly = await import("plotly.js-dist-min");
  });

  onDestroy(() => {
    if (Plotly && plotDiv) Plotly.purge(plotDiv);
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

    const layout = darkLayout({
      xaxis: { title: { text: `Time (${tu.label})` } },
      yaxis: { title: { text: "Current (µA)" }, rangemode: "tozero" },
      margin: { t: 10, r: 10, b: 40, l: 50 },
      showlegend: false,
      height: 150,
    });

    Plotly.react(plotDiv, [trace], layout, PLOTLY_CONFIG);
  }

  $effect(() => {
    // Re-render on profile change, Plotly load, or theme change
    void profile;
    void Plotly;
    void plotDiv;
    void theme;
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

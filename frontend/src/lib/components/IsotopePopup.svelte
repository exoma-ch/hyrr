<script lang="ts">
  import { onDestroy } from "svelte";
  import Modal from "./Modal.svelte";
  import { getDataStore } from "../scheduler/sim-scheduler.svelte";
  import { getResult } from "../stores/results.svelte";
  import { getConfig } from "../stores/config.svelte";
  import { formatHalfLife, darkLayout, PLOTLY_CONFIG, TRACE_COLORS, themeColors } from "../plotting/plotly-helpers";
  import { getResolvedTheme } from "../stores/theme.svelte";
  import {
    nudatUrl,
    Z_TO_SYMBOL,
    resolveMaterial,
    PROJECTILE_Z,
    PROJECTILE_A,
    bestActivityUnit,
    bestTimeUnit,
    fmtActivity,
    fmtYield,
    nucHtml,
    nucLabel,
    interp,
  } from "@hyrr/compute";
  import type { DecayMode, CrossSectionData, ProjectileType } from "@hyrr/compute";
  import { getDepthPreview } from "../stores/depth-preview.svelte";

  interface Props {
    open: boolean;
    onclose: () => void;
    name: string;
    Z: number;
    A: number;
    nuclearState?: string;
  }

  let { open, onclose, name, Z, A, nuclearState = "" }: Props = $props();

  let Plotly: any = null;
  let xsPlotDiv = $state<HTMLDivElement | null>(null);
  let depthPlotDiv = $state<HTMLDivElement | null>(null);
  let actPlotDiv = $state<HTMLDivElement | null>(null);

  /** Determine reaction notation, e.g. "(p,n)", "(p,γ)", "(p,2n)" */
  function reactionNotation(proj: string, targetZ: number, targetA: number, residualZ: number, residualA: number): string {
    const pZ = PROJECTILE_Z[proj as ProjectileType] ?? 1;
    const pA = PROJECTILE_A[proj as ProjectileType] ?? 1;
    let eZ = targetZ + pZ - residualZ;
    let eA = targetA + pA - residualA;
    if (eZ === 0 && eA === 0) return `(${proj},γ)`;
    const parts: string[] = [];
    while (eZ >= 2 && eA >= 4) { parts.push("α"); eZ -= 2; eA -= 4; }
    while (eZ >= 2 && eA >= 3) { parts.push("³He"); eZ -= 2; eA -= 3; }
    while (eZ >= 1 && eA >= 3) { parts.push("t"); eZ -= 1; eA -= 3; }
    while (eZ >= 1 && eA >= 2) { parts.push("d"); eZ -= 1; eA -= 2; }
    while (eZ >= 1 && eA >= 1) { parts.push("p"); eZ -= 1; eA -= 1; }
    while (eA >= 1) { parts.push("n"); eA -= 1; }
    // Compact: group duplicates (e.g. ["n","n"] → "2n")
    const counts = new Map<string, number>();
    for (const p of parts) counts.set(p, (counts.get(p) ?? 0) + 1);
    let out = "";
    for (const [particle, count] of counts) out += (count > 1 ? `${count}` : "") + particle;
    return `(${proj},${out})`;
  }

  /** Convert enrichment from config format to Map format expected by resolveMaterial */
  function enrichmentToOverrides(
    enrichment?: Record<string, Record<number, number>>,
  ): Record<string, Map<number, number>> | undefined {
    if (!enrichment) return undefined;
    const result: Record<string, Map<number, number>> = {};
    for (const [symbol, isoMap] of Object.entries(enrichment)) {
      const m = new Map<number, number>();
      for (const [a, frac] of Object.entries(isoMap)) m.set(Number(a), frac);
      result[symbol] = m;
    }
    return result;
  }


  // XS channel: one per target isotope that produces this residual
  interface XsChannel {
    xs: CrossSectionData;
    targetZ: number;
    targetSymbol: string;
    targetA: number;
    abundance: number; // isotopic fraction (0-1), reflecting enrichment
    label: string; // e.g. "⁴⁴Ca(p,n)"
    reaction: string; // e.g. "(p,n)"
  }


  // State
  let decayInfo: { halfLifeS: number | null; decayModes: DecayMode[] } | null = $state(null);
  let parentDecays: { name: string; Z: number; A: number; state: string; mode: string; branching: number }[] = $state([]);
  let xsChannels: XsChannel[] = $state([]);
  let xsScaled = $state(false);
  /** Depth plot mode: false = theory (σ vs depth), true = real (production rate vs depth) */
  let depthReal = $state(false);
  // Compare: additional isotopes with their own channel arrays
  let loading = $state(false);
  let compareIsotopes: { name: string; Z: number; A: number; state: string; channels: XsChannel[] }[] = $state([]);
  let allProducedIsotopes: { name: string; Z: number; A: number; state: string }[] = $state([]);
  let compareFilter = $state("");
  let compareDropdownOpen = $state(false);
  let cachedAllChannels: Map<string, XsChannel[]> = new Map();

  // Only offer isotopes that appear in the simulation result (have activity data)
  let resultIsotopeKeys = $derived.by(() => {
    const result = getResult();
    if (!result) return new Set<string>();
    const keys = new Set<string>();
    for (const layer of result.layers) {
      for (const iso of layer.isotopes) {
        if (iso.time_grid_s && iso.activity_vs_time_Bq) {
          keys.add(`${iso.Z}-${iso.A}-${iso.state || ""}`);
        }
      }
    }
    return keys;
  });

  let sortedCompareList = $derived.by(() => {
    const selectedNames = new Set(compareIsotopes.map((c) => c.name));
    return allProducedIsotopes
      .filter((i) => i.name !== name)
      .filter((i) => resultIsotopeKeys.has(`${i.Z}-${i.A}-${i.state}`))
      .filter((i) => !compareFilter || i.name.toLowerCase().includes(compareFilter.toLowerCase()))
      .sort((a, b) => {
        const aSelected = selectedNames.has(a.name) ? 0 : 1;
        const bSelected = selectedNames.has(b.name) ? 0 : 1;
        return aSelected - bSelected || a.name.localeCompare(b.name);
      });
  });

  function toggleCompare(iso: { name: string; Z: number; A: number; state: string }) {
    if (compareIsotopes.some((c) => c.name === iso.name)) {
      removeCompare(iso.name);
    } else {
      addCompare(iso);
    }
  }

  // Energy range info from depth preview
  let beamEnergy = $state(0);
  let layerEnergies: { material: string; eIn: number; eOut: number }[] = $state([]);

  $effect(() => {
    if (!open) {
      decayInfo = null;
      parentDecays = [];
      xsChannels = [];
      compareIsotopes = [];
      compareFilter = "";
      compareDropdownOpen = false;
      depthReal = false;
      return;
    }
    const db = getDataStore();
    if (!db) return;
    loading = true;
    let cancelled = false;

    (async () => {
    // Yield to let the modal render before heavy work
    await new Promise((r) => setTimeout(r, 0));
    if (cancelled) return;
    const config = getConfig();
    const proj = config.beam.projectile;
    beamEnergy = config.beam.energy_MeV;

    // Layer energies from depth preview
    const preview = getDepthPreview();
    layerEnergies = preview.map((l) => ({
      material: l.material,
      eIn: l.energy_in_MeV,
      eOut: l.energy_out_MeV,
    }));

    // Decay data
    const data = db.getDecayData(Z, A, nuclearState || undefined);
    decayInfo = data ? { halfLifeS: data.halfLifeS, decayModes: data.decayModes } : null;

    // Find parents
    const parents: typeof parentDecays = [];
    for (let dz = -2; dz <= 2; dz++) {
      for (let da = -4; da <= 4; da++) {
        const pZ = Z + dz;
        const pA = A + da;
        if (pZ < 1 || pA < 1) continue;
        for (const pState of ["", "m", "m2"]) {
          const pData = db.getDecayData(pZ, pA, pState || undefined);
          if (!pData) continue;
          for (const mode of pData.decayModes) {
            if (mode.daughterZ === Z && mode.daughterA === A) {
              const sym = Z_TO_SYMBOL[pZ] ?? `Z${pZ}`;
              parents.push({
                name: `${sym}-${pA}${pState}`,
                Z: pZ, A: pA, state: pState,
                mode: mode.mode, branching: mode.branching,
              });
            }
          }
        }
      }
    }
    parentDecays = parents;

    // Ensure cross-section parquet files are loaded for all elements in the stack
    const elementSymbols = new Set<string>();
    for (const layer of config.layers) {
      if (!layer.material) continue;
      try {
        const overrides = enrichmentToOverrides(layer.enrichment);
        const mat = resolveMaterial(db, layer.material, overrides);
        for (const [el] of mat.elements) {
          elementSymbols.add(Z_TO_SYMBOL[el.Z] ?? `Z${el.Z}`);
        }
      } catch { /* skip */ }
    }
    await db.ensureMultipleCrossSections(proj, [...elementSymbols]);
    if (cancelled) return;

    // Cross-section data: build channel arrays for ALL produced isotopes
    const allChanMap = new Map<string, XsChannel[]>();
    const seenChannel = new Set<string>(); // "targetZ-targetA-residualZ-residualA-state"
    const produced: typeof allProducedIsotopes = [];
    const seenIso = new Set<string>();

    for (const layer of config.layers) {
      if (!layer.material) continue;
      try {
        const overrides = enrichmentToOverrides(layer.enrichment);
        const mat = resolveMaterial(db, layer.material, overrides);
        for (const [el, atomFrac] of mat.elements) {
          for (const [targetA, isoAbundance] of el.isotopes) {
            const xsList = db.getCrossSections(proj, el.Z, targetA);
            for (const xs of xsList) {
              const isoKey = `${xs.residualZ}-${xs.residualA}-${xs.state || ""}`;
              // Peak XS for this channel
              let peakXs = 0;
              for (let i = 0; i < xs.xsMb.length; i++) {
                if (xs.xsMb[i] > peakXs) peakXs = xs.xsMb[i];
              }
              // Build channel entry for every isotope (not just main)
              const chanKey = `${el.Z}-${targetA}-${isoKey}`;
              if (!seenChannel.has(chanKey) && peakXs > 1e-6) {
                seenChannel.add(chanKey);
                const tSym = Z_TO_SYMBOL[el.Z] ?? `Z${el.Z}`;
                const rSym = Z_TO_SYMBOL[xs.residualZ] ?? `Z${xs.residualZ}`;
                const rxn = reactionNotation(proj, el.Z, targetA, xs.residualZ, xs.residualA);
                const ch: XsChannel = {
                  xs,
                  targetZ: el.Z,
                  targetSymbol: tSym,
                  targetA,
                  abundance: atomFrac * isoAbundance,
                  label: `<sup>${targetA}</sup>${tSym}${rxn}<sup>${xs.residualA}</sup>${rSym}`,
                  reaction: rxn,
                };
                if (!allChanMap.has(isoKey)) allChanMap.set(isoKey, []);
                allChanMap.get(isoKey)!.push(ch);
              }
              // Collect all produced isotopes for compare selector
              if (!seenIso.has(isoKey)) {
                seenIso.add(isoKey);
                const sym = Z_TO_SYMBOL[xs.residualZ] ?? `Z${xs.residualZ}`;
                produced.push({
                  name: `${sym}-${xs.residualA}${xs.state ? ` (${xs.state})` : ""}`,
                  Z: xs.residualZ, A: xs.residualA, state: xs.state || "",
                });
              }
            }
          }
        }
      } catch { /* skip */ }
    }

    // Sort each isotope's channels by peak XS descending
    function sortChannels(chs: XsChannel[]): XsChannel[] {
      return chs.sort((a, b) => {
        let peakA = 0, peakB = 0;
        for (let i = 0; i < a.xs.xsMb.length; i++) if (a.xs.xsMb[i] > peakA) peakA = a.xs.xsMb[i];
        for (let i = 0; i < b.xs.xsMb.length; i++) if (b.xs.xsMb[i] > peakB) peakB = b.xs.xsMb[i];
        return peakB - peakA;
      });
    }
    for (const [, chs] of allChanMap) sortChannels(chs);

    const mainKey = `${Z}-${A}-${nuclearState || ""}`;
    xsChannels = allChanMap.get(mainKey) ?? [];
    cachedAllChannels = allChanMap;
    allProducedIsotopes = produced.sort((a, b) => a.name.localeCompare(b.name));
    loading = false;
    })();

    return () => { cancelled = true; };
  });

  // Render XS plot — read all deps eagerly (Svelte only tracks synchronous reads)
  $effect(() => {
    const nChannels = xsChannels.length;
    const numCompare = compareIsotopes.length;
    const scaled = xsScaled;
    const div = xsPlotDiv; // eagerly read so Svelte tracks bind:this updates
    const _theme = getResolvedTheme();
    if (!open || (nChannels === 0 && numCompare === 0) || !div) return;
    requestAnimationFrame(() => renderXsPlot());
  });

  async function ensurePlotly() {
    if (!Plotly) Plotly = await import("plotly.js-dist-min");
  }

  function renderXsPlot() {
    ensurePlotly().then(() => {
      if (!Plotly || !xsPlotDiv) return;
      const tc = themeColors();

      const traces: any[] = [];
      const scaled = xsScaled;

      // Energy range: beam energy to final E_out + 10% padding
      const minE = layerEnergies.length > 0
        ? Math.max(0, layerEnergies[layerEnergies.length - 1].eOut * 0.9)
        : 0;
      const maxE = beamEnergy * 1.1;

      // Helper: filter and optionally scale XS data
      function filterXs(energiesMeV: Float64Array, xsMb: Float64Array, scale: number): { e: number[]; xs: number[] } {
        const e: number[] = [];
        const xs: number[] = [];
        for (let i = 0; i < energiesMeV.length; i++) {
          if (energiesMeV[i] >= minE && energiesMeV[i] <= maxE) {
            e.push(energiesMeV[i]);
            xs.push(xsMb[i] * scale);
          }
        }
        return { e, xs };
      }

      // All channels for main isotope
      let colorIdx = 0;
      for (const ch of xsChannels) {
        const scale = scaled ? ch.abundance : 1;
        const { e, xs } = filterXs(ch.xs.energiesMeV, ch.xs.xsMb, scale);
        const pct = scaled ? ` (${(ch.abundance * 100).toFixed(1)}%)` : "";
        traces.push({
          x: e,
          y: xs,
          type: "scatter",
          mode: "lines",
          line: { color: TRACE_COLORS[colorIdx % TRACE_COLORS.length], width: colorIdx === 0 ? 2 : 1.5 },
          name: `${ch.label}${pct}`,
        });
        colorIdx++;
      }

      // Compare traces — all channels per compare isotope
      for (const cmp of compareIsotopes) {
        for (const ch of cmp.channels) {
          const scale = scaled ? ch.abundance : 1;
          const { e, xs } = filterXs(ch.xs.energiesMeV, ch.xs.xsMb, scale);
          const pct = scaled ? ` (${(ch.abundance * 100).toFixed(1)}%)` : "";
          traces.push({
            x: e,
            y: xs,
            type: "scatter",
            mode: "lines",
            line: { color: TRACE_COLORS[colorIdx % TRACE_COLORS.length], width: 1.5 },
            name: `${ch.label}${pct}`,
            legendgroup: cmp.name,
          });
          colorIdx++;
        }
      }

      // Layer boundary shapes — show all layers even if beam stops (eOut = 0)
      const shapes: any[] = [];
      const annotations: any[] = [];
      for (const le of layerEnergies) {
        const eOut = Math.max(0, le.eOut);
        if (le.eIn > 0) {
          shapes.push({
            type: "line", x0: le.eIn, x1: le.eIn, y0: 0, y1: 1,
            yref: "paper",
            line: { color: tc.textFaint, width: 1, dash: "dot" },
          });
        }
        if (eOut !== le.eIn) {
          if (eOut > 0) {
            shapes.push({
              type: "line", x0: eOut, x1: eOut, y0: 0, y1: 1,
              yref: "paper",
              line: { color: tc.textFaint, width: 1, dash: "dot" },
            });
          }
          annotations.push({
            x: (le.eIn + eOut) / 2, y: 1.02, yref: "paper",
            text: le.material, showarrow: false,
            font: { color: tc.textSubtle, size: 9 },
            xanchor: "center",
          });
        }
      }

      const rangeMinE = layerEnergies.length > 0
        ? Math.max(0, layerEnergies[layerEnergies.length - 1].eOut * 0.9)
        : undefined;
      const rangeMaxE = beamEnergy > 0 ? beamEnergy * 1.1 : undefined;

      const yTitle = scaled ? "σ × abundance (mb)" : "Cross section (mb)";
      const layout = darkLayout({
        xaxis: {
          title: "Energy (MeV)",
          gridcolor: tc.border,
          range: rangeMinE !== undefined && rangeMaxE !== undefined ? [rangeMinE, rangeMaxE] : undefined,
        },
        yaxis: { title: yTitle, gridcolor: tc.border },
        margin: { t: 20, r: 20, b: 40, l: 55 },
        height: 220,
        showlegend: traces.length > 1,
        legend: { x: 1, xanchor: "right", y: 1, bgcolor: "rgba(0,0,0,0)" },
        shapes,
        annotations,
      });

      Plotly.react(xsPlotDiv, traces, layout, PLOTLY_CONFIG);
    });
  }

  // Compare: add an isotope with all its XS channels
  function addCompare(iso: { name: string; Z: number; A: number; state: string }) {
    if (compareIsotopes.some((c) => c.name === iso.name)) return;
    const key = `${iso.Z}-${iso.A}-${iso.state}`;
    const channels = cachedAllChannels.get(key) ?? [];
    if (channels.length > 0) {
      compareIsotopes = [...compareIsotopes, { name: iso.name, Z: iso.Z, A: iso.A, state: iso.state, channels }];
    }
  }

  function removeCompare(isoName: string) {
    compareIsotopes = compareIsotopes.filter((c) => c.name !== isoName);
  }

  // Depth plot — toggle between theory σ(E(x)) and real production rate from simulation
  $effect(() => {
    const nChannels = xsChannels.length;
    const numCompare = compareIsotopes.length;
    const _real = depthReal;
    const div = depthPlotDiv;
    const prev = getDepthPreview();
    const _result = getResult();
    const _theme = getResolvedTheme();
    if (!open || !div || (nChannels === 0 && numCompare === 0) || prev.length === 0) return;
    requestAnimationFrame(() => ensurePlotly().then(renderDepthPlot));
  });

  function renderDepthPlot() {
    if (!Plotly || !depthPlotDiv) return;
    const tc = themeColors();
    const preview = getDepthPreview();
    if (preview.length === 0) return;
    const real = depthReal;

    if (real) {
      renderDepthPlotReal(tc);
    } else {
      renderDepthPlotTheory(tc, preview);
    }
  }

  /** Theory mode: raw σ(E(x)) vs depth — material-agnostic */
  function renderDepthPlotTheory(tc: ReturnType<typeof themeColors>, preview: ReturnType<typeof getDepthPreview>) {
    const allDepths: number[] = [];
    const allEnergies: number[] = [];
    let cumulativeDepth = 0;
    const boundaries: { depth: number; label: string }[] = [];

    for (const layer of preview) {
      boundaries.push({ depth: cumulativeDepth, label: layer.material });
      for (const pt of layer.depthPoints) {
        allDepths.push(cumulativeDepth + pt.depth_mm);
        allEnergies.push(pt.energy_MeV);
      }
      cumulativeDepth += layer.thickness_mm;
    }
    if (allDepths.length < 2) return;

    const traces: any[] = [];

    // Energy curve on secondary y-axis
    traces.push({
      x: allDepths, y: allEnergies,
      name: "Energy", type: "scatter", mode: "lines",
      line: { color: tc.textFaint, width: 1.5, dash: "dot" },
      yaxis: "y2",
    });

    // σ(E(x)) for each channel — pure cross-section, no scaling
    let colorIdx = 0;
    function xsAtDepth(xs: CrossSectionData): number[] {
      return Array.from(interp(new Float64Array(allEnergies), xs.energiesMeV, xs.xsMb));
    }

    for (const ch of xsChannels) {
      traces.push({
        x: allDepths, y: xsAtDepth(ch.xs),
        name: ch.label, type: "scatter", mode: "lines",
        fill: colorIdx === 0 ? "tozeroy" : undefined,
        fillcolor: colorIdx === 0 ? TRACE_COLORS[0].replace(")", ", 0.15)").replace("rgb", "rgba") : undefined,
        line: { color: TRACE_COLORS[colorIdx % TRACE_COLORS.length], width: colorIdx === 0 ? 2 : 1.5 },
      });
      colorIdx++;
    }
    for (const cmp of compareIsotopes) {
      for (const ch of cmp.channels) {
        traces.push({
          x: allDepths, y: xsAtDepth(ch.xs),
          name: ch.label, type: "scatter", mode: "lines",
          line: { color: TRACE_COLORS[colorIdx % TRACE_COLORS.length], width: 1.5 },
          legendgroup: cmp.name,
        });
        colorIdx++;
      }
    }

    const { shapes, annotations } = layerMarkers(boundaries, tc);
    const layout = darkLayout({
      xaxis: { title: "Depth (mm)", gridcolor: tc.border, range: [0, cumulativeDepth] },
      yaxis: { title: "σ (mb)", gridcolor: tc.border },
      yaxis2: { title: "Energy (MeV)", overlaying: "y", side: "right", gridcolor: tc.border },
      margin: { t: 20, r: 55, b: 40, l: 55 },
      height: 220, showlegend: true,
      legend: { x: 1, xanchor: "right", y: 0.95, bgcolor: "rgba(0,0,0,0)" },
      shapes, annotations,
    });
    Plotly.react(depthPlotDiv, traces, layout, PLOTLY_CONFIG);
  }

  /**
   * Real mode: production rate vs depth consumed from LayerResult.depth_production_rates.
   * The Rust engine emits rate = σ(E(x)) × abundance × number_density × flux [atoms/s/cm]
   * per depth point; see core/src/production.rs::compute_depth_production_rate. The popup
   * just concatenates per-layer rates onto a cumulative depth axis.
   */
  function renderDepthPlotReal(tc: ReturnType<typeof themeColors>) {
    const result = getResult();
    if (!result) return;
    const layers = result.layers;

    // Walk the simulation result layer-by-layer to build cumulative-depth arrays
    // for a single isotope by name. Inserts (layerStart, 0) at every layer start
    // so Plotly draws clean vertical steps at boundaries (foreign layers → 0).
    function collectForName(isoName: string): { depths: number[]; rates: number[] } {
      const depths: number[] = [];
      const rates: number[] = [];
      let cumulativeDepth = 0;
      for (const layer of layers) {
        const dp = layer.depth_profile;
        if (!dp || dp.length === 0) continue;
        const layerStart = cumulativeDepth;
        const layerThickness = dp[dp.length - 1].depth_mm;
        const dpr = layer.depth_production_rates?.[isoName];

        // Anchor at layerStart=0 so both directions of the boundary (drop-to-zero
        // from prior layer, rise-from-zero in this layer) are visually vertical.
        depths.push(layerStart);
        rates.push(0);

        if (dpr) {
          for (let i = 0; i < Math.min(dp.length, dpr.length); i++) {
            depths.push(layerStart + dp[i].depth_mm);
            rates.push(dpr[i]);
          }
        } else {
          // Foreign layer for this isotope — span with zeros.
          depths.push(layerStart + layerThickness);
          rates.push(0);
        }
        cumulativeDepth = layerStart + layerThickness;
      }
      return { depths, rates };
    }

    const boundaries: { depth: number; label: string }[] = [];
    let cumulativeDepth = 0;
    for (const layer of result.layers) {
      const mat = result.config.layers[layer.layer_index]?.material ?? "?";
      boundaries.push({ depth: cumulativeDepth, label: mat });
      const dp = layer.depth_profile;
      if (dp && dp.length > 0) cumulativeDepth += dp[dp.length - 1].depth_mm;
    }

    const traces: any[] = [];
    let colorIdx = 0;

    // Main isotope
    const main = collectForName(name);
    if (main.rates.some((r) => r > 0)) {
      traces.push({
        x: main.depths, y: main.rates,
        name: nucLabel(name), type: "scatter", mode: "lines",
        fill: "tozeroy",
        fillcolor: TRACE_COLORS[0].replace(")", ", 0.15)").replace("rgb", "rgba"),
        line: { color: TRACE_COLORS[0], width: 2 },
      });
      colorIdx++;
    }

    // Compare isotopes
    for (const cmp of compareIsotopes) {
      const cmpData = collectForName(cmp.name);
      if (cmpData.rates.some((r) => r > 0)) {
        traces.push({
          x: cmpData.depths, y: cmpData.rates,
          name: nucLabel(cmp.name), type: "scatter", mode: "lines",
          line: { color: TRACE_COLORS[colorIdx % TRACE_COLORS.length], width: 1.5 },
        });
        colorIdx++;
      }
    }

    if (traces.length === 0) return;

    const { shapes, annotations } = layerMarkers(boundaries, tc);
    const layout = darkLayout({
      xaxis: { title: "Depth (mm)", gridcolor: tc.border, range: [0, cumulativeDepth] },
      yaxis: { title: "Production rate (atoms/s/cm)", gridcolor: tc.border },
      margin: { t: 20, r: 20, b: 40, l: 70 },
      height: 220, showlegend: traces.length > 1,
      legend: { x: 1, xanchor: "right", y: 0.95, bgcolor: "rgba(0,0,0,0)" },
      shapes, annotations,
    });
    Plotly.react(depthPlotDiv, traces, layout, PLOTLY_CONFIG);
  }

  function layerMarkers(boundaries: { depth: number; label: string }[], tc: ReturnType<typeof themeColors>) {
    const shapes = boundaries.slice(1).map((b) => ({
      type: "line" as const, x0: b.depth, x1: b.depth, y0: 0, y1: 1,
      yref: "paper" as const,
      line: { color: tc.textFaint, width: 1, dash: "dot" as const },
    }));
    const annotations = boundaries.map((b) => ({
      x: b.depth, y: 1.02, yref: "paper" as const,
      text: b.label, showarrow: false,
      font: { color: tc.textMuted, size: 9 },
      xanchor: "left" as const,
    }));
    return { shapes, annotations };
  }

  // Activity data — pulled from simulation result (no recomputation)
  interface ActivityCurve {
    name: string;
    times: number[];
    activities: number[];
    productionRate: number;
    satYield: number;
    eobActivity: number;
    halfLifeS: number | null;
  }

  interface ActivityData {
    main: ActivityCurve | null;
    compare: ActivityCurve[];
  }

  let activityData = $derived.by((): ActivityData | null => {
    if (!open) return null;
    const result = getResult();
    if (!result) return null;

    function findBestIsotope(z: number, a: number, st: string): ActivityCurve | null {
      let best: import("../types").IsotopeResultData | null = null;
      for (const layer of result!.layers) {
        for (const iso of layer.isotopes) {
          if (iso.Z === z && iso.A === a && (iso.state || "") === (st || "")) {
            if (!best || iso.activity_Bq > best.activity_Bq) best = iso;
          }
        }
      }
      if (!best || !best.time_grid_s || !best.activity_vs_time_Bq) return null;
      return {
        name: best.name + (best.state ? ` (${best.state})` : ""),
        times: [...best.time_grid_s],
        activities: [...best.activity_vs_time_Bq],
        productionRate: best.production_rate,
        satYield: best.saturation_yield_Bq_uA,
        eobActivity: best.activity_Bq,
        halfLifeS: best.half_life_s,
      };
    }

    const main = findBestIsotope(Z, A, nuclearState || "");

    const compare: ActivityCurve[] = [];
    for (const cmp of compareIsotopes) {
      const curve = findBestIsotope(cmp.Z, cmp.A, cmp.state);
      if (curve) compare.push(curve);
    }

    return { main, compare };
  });

  let showRnp = $derived(compareIsotopes.length > 0);

  $effect(() => {
    const data = activityData;
    const _cmp = compareIsotopes.length;
    const div = actPlotDiv; // eagerly read so Svelte tracks bind:this updates
    const _theme = getResolvedTheme();
    if (!open || !data || !data.main || !div) return;
    requestAnimationFrame(() => ensurePlotly().then(renderActivityPlot));
  });

  function renderActivityPlot() {
    if (!Plotly || !activityData?.main || !actPlotDiv) return;
    const tc = themeColors();

    const config = getConfig();
    const totalTime = config.irradiation_s + (config.cooling_s || 86400);
    const { label: timeLabel, divisor: timeDiv } = bestTimeUnit(totalTime);
    const eobX = config.irradiation_s / timeDiv;

    const allCurves = [activityData.main, ...activityData.compare];

    // Find global max for unit scaling
    let globalMax = 0;
    for (const curve of allCurves) {
      for (const a of curve.activities) {
        if (a > globalMax) globalMax = a;
      }
    }
    const { label: actLabel, divisor: actDiv } = bestActivityUnit(globalMax);

    const traces: any[] = [];

    if (showRnp) {
      // RNP% mode: show relative contribution of each isotope
      // Compute total activity at each time point (across shown isotopes)
      const mainTimes = activityData.main.times;
      const totalAct = new Float64Array(mainTimes.length);
      for (const curve of allCurves) {
        for (let i = 0; i < Math.min(curve.activities.length, totalAct.length); i++) {
          totalAct[i] += curve.activities[i];
        }
      }

      // RNP% trace for each isotope
      allCurves.forEach((curve, idx) => {
        const rnp = curve.activities.map((a, i) =>
          totalAct[i] > 0 ? (a / totalAct[i]) * 100 : 0,
        );
        traces.push({
          x: curve.times.map((t) => t / timeDiv),
          y: rnp,
          type: "scatter",
          mode: "lines",
          line: { color: TRACE_COLORS[idx % TRACE_COLORS.length], width: idx === 0 ? 2 : 1.5 },
          name: nucLabel(curve.name),
        });
      });

      // Total activity on secondary axis
      traces.push({
        x: mainTimes.map((t) => t / timeDiv),
        y: Array.from(totalAct).map((a) => a / actDiv),
        type: "scatter",
        mode: "lines",
        line: { color: tc.textFaint, width: 1, dash: "dot" },
        yaxis: "y2",
        name: "Total",
      });

      const layout = darkLayout({
        xaxis: { title: `Time (${timeLabel})`, gridcolor: tc.border },
        yaxis: { title: "RNP (%)", gridcolor: tc.border, range: [0, 105] },
        yaxis2: {
          title: `Activity (${actLabel})`,
          overlaying: "y",
          side: "right",
          gridcolor: tc.border,
        },
        margin: { t: 10, r: 55, b: 40, l: 55 },
        height: 220,
        showlegend: true,
        legend: { x: 1, xanchor: "right", y: 1, bgcolor: "rgba(0,0,0,0)" },
        shapes: [{
          type: "line" as const,
          x0: eobX, x1: eobX, y0: 0, y1: 1,
          yref: "paper" as const,
          line: { color: tc.orange, width: 1, dash: "dash" as const },
        }],
        annotations: [{
          x: eobX, y: 1.02, yref: "paper" as const,
          text: "EOB", showarrow: false,
          font: { color: tc.orange, size: 9 },
          xanchor: "center" as const,
        }],
      });

      Plotly.react(actPlotDiv, traces, layout, PLOTLY_CONFIG);
    } else {
      // Single isotope mode: simple activity curve
      traces.push({
        x: activityData.main.times.map((t) => t / timeDiv),
        y: activityData.main.activities.map((a) => a / actDiv),
        type: "scatter",
        mode: "lines",
        line: { color: TRACE_COLORS[0], width: 2 },
        name: nucLabel(name),
      });

      const layout = darkLayout({
        xaxis: { title: `Time (${timeLabel})`, gridcolor: tc.border },
        yaxis: { title: `Activity (${actLabel})`, gridcolor: tc.border },
        margin: { t: 10, r: 20, b: 40, l: 55 },
        height: 200,
        showlegend: false,
        shapes: [{
          type: "line" as const,
          x0: eobX, x1: eobX, y0: 0, y1: 1,
          yref: "paper" as const,
          line: { color: tc.orange, width: 1, dash: "dash" as const },
        }],
        annotations: [{
          x: eobX, y: 1.02, yref: "paper" as const,
          text: "EOB", showarrow: false,
          font: { color: tc.orange, size: 9 },
          xanchor: "center" as const,
        }],
      });

      Plotly.react(actPlotDiv, traces, layout, PLOTLY_CONFIG);
    }
  }

  onDestroy(() => {
    if (Plotly) {
      if (xsPlotDiv) Plotly.purge(xsPlotDiv);
      if (depthPlotDiv) Plotly.purge(depthPlotDiv);
      if (actPlotDiv) Plotly.purge(actPlotDiv);
    }
  });

  function janisUrl(): string {
    const sym = Z_TO_SYMBOL[Z] ?? "";
    return `https://www.oecd-nea.org/janisweb/search/endf?SearchTerm=${sym}${A}`;
  }

  // Nuclear notation helper
  let symbol = $derived(Z_TO_SYMBOL[Z] ?? "");
</script>

<Modal {open} {onclose} wide>
  {#snippet headerChildren()}
    <div class="nuc-title">
      <span class="nuc-notation">{@html nucHtml(name)}</span>
    </div>
  {/snippet}

  <div class="isotope-popup">
    {#if loading}
      <div class="loading-indicator">Loading isotope data...</div>
    {/if}
    <!-- Properties -->
    {#if decayInfo}
      <div class="properties">
        {#if decayInfo.decayModes.length > 0}
          <div class="prop-row">
            <span class="prop-label">Decay</span>
            <span class="prop-value">
              {#each decayInfo.decayModes as mode, i}
                {#if i > 0}<span class="prop-sep">, </span>{/if}
                {mode.mode}{#if mode.branching < 1 && mode.branching > 0}
                  <span class="branching"> ({(mode.branching * 100).toFixed(1)}%)</span>
                {/if}
              {/each}
            </span>
          </div>
        {/if}
      </div>
    {/if}

    <!-- Decay chain -->
    {#if parentDecays.length > 0 || (decayInfo && decayInfo.decayModes.some(m => m.daughterZ !== null))}
      <div class="decay-chain">
        <div class="chain-flow">
          {#if parentDecays.length > 0}
            <div class="chain-group">
              {#each parentDecays as p}
                <span class="chain-nuc parent" title="{p.mode} ({(p.branching * 100).toFixed(1)}%)">{@html nucHtml(p.name)}</span>
              {/each}
            </div>
            <span class="chain-arrow">&rarr;</span>
          {/if}
          <span class="chain-nuc current">{@html nucHtml(name)}</span>
          {#if decayInfo && decayInfo.decayModes.some(m => m.daughterZ !== null)}
            <span class="chain-arrow">&rarr;</span>
            <div class="chain-group">
              {#each decayInfo.decayModes.filter(m => m.daughterZ !== null) as mode}
                {@const dSym = Z_TO_SYMBOL[mode.daughterZ!] ?? `Z${mode.daughterZ}`}
                <span class="chain-nuc daughter" title="{mode.mode} ({(mode.branching * 100).toFixed(1)}%)">
                  {@html nucHtml(`${dSym}-${mode.daughterA}${mode.daughterState || ""}`)}
                </span>
              {/each}
            </div>
          {/if}
        </div>
      </div>
    {/if}

    <!-- Compare table -->
    {#if activityData?.main}
      <div class="compare-table-wrap">
        <table class="compare-table">
          <thead>
            <tr>
              <th class="ct-iso">Isotope</th>
              <th class="ct-hl">t&frac12;</th>
              <th class="ct-act">EOB</th>
              <th class="ct-yield">Sat. yield</th>
              <th class="ct-rate">Prod. rate</th>
              <th class="ct-x"></th>
            </tr>
          </thead>
          <tbody>
            <tr class="ct-main">
              <td class="ct-iso">{@html nucHtml(name)}</td>
              <td class="ct-hl">{formatHalfLife(activityData.main.halfLifeS)}</td>
              <td class="ct-act">{fmtActivity(activityData.main.eobActivity)}</td>
              <td class="ct-yield">{fmtYield(activityData.main.satYield)}</td>
              <td class="ct-rate">{activityData.main.productionRate.toExponential(2)} /s</td>
              <td class="ct-x"></td>
            </tr>
            {#each activityData.compare as cmp}
              <tr>
                <td class="ct-iso">{@html nucHtml(cmp.name)}</td>
                <td class="ct-hl">{formatHalfLife(cmp.halfLifeS)}</td>
                <td class="ct-act">{fmtActivity(cmp.eobActivity)}</td>
                <td class="ct-yield">{fmtYield(cmp.satYield)}</td>
                <td class="ct-rate">{cmp.productionRate.toExponential(2)} /s</td>
                <td class="ct-x">
                  <button class="ct-remove" onclick={() => removeCompare(cmp.name)}>&times;</button>
                </td>
              </tr>
            {/each}
          </tbody>
          <tfoot>
            <tr>
              <td colspan="6" class="ct-add-cell">
                <div class="compare-dropdown-wrapper">
                  <input
                    type="text"
                    class="compare-filter"
                    placeholder="+ compare..."
                    bind:value={compareFilter}
                    onfocus={() => { compareDropdownOpen = true; }}
                    onblur={() => { setTimeout(() => { compareDropdownOpen = false; }, 150); }}
                  />
                  {#if compareDropdownOpen && sortedCompareList.length > 0}
                    <div class="compare-dropdown">
                      {#each sortedCompareList as iso}
                        <button
                          class="compare-option"
                          class:selected={compareIsotopes.some((c) => c.name === iso.name)}
                          onmousedown={(e) => { e.preventDefault(); toggleCompare(iso); }}
                        >
                          {#if compareIsotopes.some((c) => c.name === iso.name)}<span class="check-mark">&#10003;</span>{/if}
                          {@html nucHtml(iso.name)}
                        </button>
                      {/each}
                    </div>
                  {/if}
                </div>
              </td>
            </tr>
          </tfoot>
        </table>
      </div>
    {/if}

    <!-- Cross-section plot -->
    {#if xsChannels.length > 0 || compareIsotopes.length > 0}
      <div class="section">
        <div class="section-bar">
          <span class="section-label">Cross section</span>
          {#if xsChannels.length > 1}
            <button class="scale-toggle" class:active={xsScaled} onclick={() => { xsScaled = !xsScaled; }}>
              Scaled
            </button>
          {/if}
        </div>
        <div bind:this={xsPlotDiv} class="xs-plot"></div>
      </div>
    {/if}

    <!-- Depth production density -->
    {#if (xsChannels.length > 0 || compareIsotopes.length > 0) && getDepthPreview().length > 0}
      <div class="section">
        <div class="section-bar">
          <span class="section-label">{depthReal ? "Production vs depth" : "σ vs depth"}</span>
          <button class="scale-toggle" class:active={depthReal} onclick={() => { depthReal = !depthReal; }}>
            {depthReal ? "Real" : "Theory"}
          </button>
        </div>
        <div bind:this={depthPlotDiv} class="depth-plot"></div>
      </div>
    {/if}

    <!-- Activity curve -->
    {#if activityData?.main}
      <div class="section">
        <div class="section-bar">
          <span class="section-label">Activity{showRnp ? " / RNP%" : ""}</span>
        </div>
        <div bind:this={actPlotDiv} class="act-plot"></div>
      </div>
    {/if}

    <!-- Links -->
    <div class="links">
      <a href={nudatUrl(Z, A, nuclearState)} target="_blank" rel="noopener noreferrer" class="ext-link">
        <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="M3.75 2h3.5a.75.75 0 010 1.5h-3.5a.25.25 0 00-.25.25v8.5c0 .138.112.25.25.25h8.5a.25.25 0 00.25-.25v-3.5a.75.75 0 011.5 0v3.5A1.75 1.75 0 0112.25 14h-8.5A1.75 1.75 0 012 12.25v-8.5C2 2.784 2.784 2 3.75 2zm6.854.22a.75.75 0 011.396-.04L14 5.5a.75.75 0 01-1.5 0V4.56l-3.97 3.97a.75.75 0 01-1.06-1.06L11.44 3.5H10.5a.75.75 0 010-1.5h.104z"></path></svg>
        NuDat 3.0
      </a>
      <a href={janisUrl()} target="_blank" rel="noopener noreferrer" class="ext-link">
        <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="M3.75 2h3.5a.75.75 0 010 1.5h-3.5a.25.25 0 00-.25.25v8.5c0 .138.112.25.25.25h8.5a.25.25 0 00.25-.25v-3.5a.75.75 0 011.5 0v3.5A1.75 1.75 0 0112.25 14h-8.5A1.75 1.75 0 012 12.25v-8.5C2 2.784 2.784 2 3.75 2zm6.854.22a.75.75 0 011.396-.04L14 5.5a.75.75 0 01-1.5 0V4.56l-3.97 3.97a.75.75 0 01-1.06-1.06L11.44 3.5H10.5a.75.75 0 010-1.5h.104z"></path></svg>
        JANIS (NEA)
      </a>
    </div>
  </div>
</Modal>

<style>
  /* Title in header — proper nuclear notation: ᴬ_Z Symbol */
  .nuc-title {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex: 1;
    min-width: 0;
  }

  .nuc-notation {
    font-size: 1.1rem;
    font-weight: 700;
    color: var(--c-accent);
  }



  .loading-indicator {
    text-align: center;
    padding: 1rem;
    color: var(--c-text-muted);
    font-size: 0.8rem;
    font-style: italic;
  }

  /* Body */
  .isotope-popup {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }

  .properties {
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    padding: 0.4rem 0.5rem;
  }

  .prop-row {
    display: flex;
    justify-content: space-between;
    padding: 0.15rem 0;
    font-size: 0.8rem;
  }

  .prop-label { color: var(--c-text-muted); }
  .prop-value { color: var(--c-text); font-variant-numeric: tabular-nums; }
  .prop-sep { color: var(--c-text-faint); }
  .branching { color: var(--c-text-subtle); font-size: 0.7rem; }

  /* Decay chain */
  .decay-chain {
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    padding: 0.4rem 0.5rem;
  }

  .chain-flow {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    flex-wrap: wrap;
  }

  .chain-group {
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
  }

  .chain-nuc {
    font-size: 0.72rem;
    padding: 0.12rem 0.3rem;
    border-radius: 3px;
    white-space: nowrap;
  }

  .chain-nuc.parent { background: var(--c-bg-hover); color: var(--c-text-muted); border: 1px solid var(--c-border); }
  .chain-nuc.current { background: var(--c-bg-active); color: var(--c-accent); border: 1px solid var(--c-accent); font-weight: 600; }
  .chain-nuc.daughter { background: var(--c-bg-hover); color: var(--c-purple); border: 1px solid var(--c-border); }
  .chain-arrow { color: var(--c-text-faint); font-size: 0.85rem; }

  /* Sections */
  .section {
    border: 1px solid var(--c-border);
    border-radius: 4px;
    overflow: hidden;
  }

  .section-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.3rem 0.5rem;
    border-bottom: 1px solid var(--c-border);
    background: var(--c-bg-default);
    gap: 0.5rem;
    flex-wrap: wrap;
  }

  .section-label {
    font-size: 0.65rem;
    color: var(--c-text-subtle);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    flex-shrink: 0;
  }

  .scale-toggle {
    background: var(--c-bg-hover);
    border: 1px solid var(--c-border);
    border-radius: 3px;
    color: var(--c-text-muted);
    padding: 0.1rem 0.4rem;
    font-size: 0.6rem;
    cursor: pointer;
    letter-spacing: 0.02em;
    transition: all 0.15s;
  }

  .scale-toggle:hover {
    color: var(--c-text);
    border-color: var(--c-text-faint);
  }

  .scale-toggle.active {
    color: var(--c-accent);
    border-color: var(--c-accent);
    background: var(--c-bg-active);
  }

  /* Compare table */
  .compare-table-wrap {
    border: 1px solid var(--c-border);
    border-radius: 4px;
    overflow: visible;
  }

  .compare-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.7rem;
    font-variant-numeric: tabular-nums;
  }

  .compare-table thead th {
    background: var(--c-bg-default);
    color: var(--c-text-subtle);
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    font-size: 0.6rem;
    padding: 0.25rem 0.4rem;
    text-align: right;
    border-bottom: 1px solid var(--c-border);
    white-space: nowrap;
  }

  .compare-table thead th.ct-iso { text-align: left; }

  .compare-table td {
    padding: 0.2rem 0.4rem;
    color: var(--c-text-muted);
    text-align: right;
    border-bottom: 1px solid var(--c-bg-hover);
    white-space: nowrap;
  }

  .compare-table td.ct-iso { text-align: left; color: var(--c-text); }

  .compare-table tr.ct-main td {
    color: var(--c-text);
    font-weight: 500;
  }


  .compare-table tr.ct-main td.ct-iso { color: var(--c-accent); }

  .compare-table tfoot td {
    border-bottom: none;
    padding: 0.25rem 0.4rem;
  }

  .ct-x { width: 1.2rem; text-align: center !important; }

  .ct-remove {
    background: none;
    border: none;
    color: var(--c-text-faint);
    cursor: pointer;
    font-size: 0.8rem;
    padding: 0;
    line-height: 1;
  }

  .ct-remove:hover { color: var(--c-red); }

  .ct-add-cell { text-align: left !important; }

  .compare-dropdown-wrapper {
    position: relative;
    display: inline-block;
  }

  .compare-filter {
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 3px;
    color: var(--c-text);
    padding: 0.1rem 0.25rem;
    font-size: 0.65rem;
    width: 7rem;
    outline: none;
  }

  .compare-filter:focus {
    border-color: var(--c-accent);
  }

  .compare-filter::placeholder {
    color: var(--c-text-subtle);
  }

  .compare-dropdown {
    position: absolute;
    top: 100%;
    left: 0;
    z-index: 100;
    background: var(--c-bg-subtle);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    max-height: 200px;
    overflow-y: auto;
    min-width: 10rem;
    margin-top: 2px;
    box-shadow: 0 4px 12px var(--c-overlay);
  }

  .compare-option {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    width: 100%;
    background: none;
    border: none;
    color: var(--c-text);
    padding: 0.25rem 0.4rem;
    font-size: 0.7rem;
    cursor: pointer;
    text-align: left;
  }

  .compare-option:hover {
    background: var(--c-bg-hover);
  }

  .compare-option.selected {
    color: var(--c-accent);
  }

  .check-mark {
    color: var(--c-accent);
    font-size: 0.65rem;
  }

  .xs-plot, .depth-plot, .act-plot {
    width: 100%;
    height: 220px;
  }

  /* Links */
  .links {
    display: flex;
    gap: 1rem;
    border-top: 1px solid var(--c-border);
    padding-top: 0.4rem;
  }

  .ext-link {
    color: var(--c-accent);
    font-size: 0.75rem;
    text-decoration: none;
    display: flex;
    align-items: center;
    gap: 0.25rem;
  }

  .ext-link:hover { text-decoration: underline; }

</style>

<script lang="ts">
  import { onDestroy } from "svelte";
  import Modal from "./Modal.svelte";
  import { getDataStore } from "../scheduler/sim-scheduler.svelte";
  import { getResult } from "../stores/results.svelte";
  import { getConfig } from "../stores/config.svelte";
  import { formatHalfLife, darkLayout, PLOTLY_CONFIG, TRACE_COLORS } from "../plotting/plotly-helpers";
  import { nudatUrl } from "../utils/format";
  import { toggleIsotope, isSelected } from "../stores/selection.svelte";
  import { Z_TO_SYMBOL } from "../utils/formula";
  import { resolveMaterial } from "../compute/materials";
  import { computeProductionRate } from "../compute/production";
  import { dedxMeVPerCm } from "../compute/stopping";
  import { bestActivityUnit, bestTimeUnit, fmtActivity } from "../utils/format";
  import { linspace } from "../compute/interpolation";
  import { PROJECTILE_Z } from "../compute";
  import { getDepthPreview } from "../stores/depth-preview.svelte";
  import type { DecayMode, CrossSectionData } from "../compute/types";

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
  let xsPlotDiv: HTMLDivElement;
  let actPlotDiv: HTMLDivElement;

  // State
  let decayInfo: { halfLifeS: number | null; decayModes: DecayMode[] } | null = $state(null);
  let parentDecays: { name: string; Z: number; A: number; state: string; mode: string; branching: number }[] = $state([]);
  let xsData: CrossSectionData | null = $state(null);
  let selected = $derived(isSelected(name));

  // Compare: additional XS traces to overlay
  let compareIsotopes: { name: string; xs: CrossSectionData }[] = $state([]);
  let allProducedIsotopes: { name: string; Z: number; A: number; state: string }[] = $state([]);

  // Energy range info from depth preview
  let beamEnergy = $state(0);
  let layerEnergies: { material: string; eIn: number; eOut: number }[] = $state([]);

  $effect(() => {
    if (!open) {
      decayInfo = null;
      parentDecays = [];
      xsData = null;
      compareIsotopes = [];
      return;
    }
    const db = getDataStore();
    if (!db) return;

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
                name: `${sym}-${pA}${pState ? ` (${pState})` : ""}`,
                Z: pZ, A: pA, state: pState,
                mode: mode.mode, branching: mode.branching,
              });
            }
          }
        }
      }
    }
    parentDecays = parents;

    // Cross-section data + collect all produced isotopes for compare
    xsData = null;
    const produced: typeof allProducedIsotopes = [];
    const seenIso = new Set<string>();

    for (const layer of config.layers) {
      if (!layer.material) continue;
      try {
        const mat = resolveMaterial(db, layer.material);
        for (const [el] of mat.elements) {
          for (const [targetA] of el.isotopes) {
            const xsList = db.getCrossSections(proj, el.Z, targetA);
            for (const xs of xsList) {
              // Check if this is our isotope
              if (xs.residualZ === Z && xs.residualA === A && (xs.state || "") === (nuclearState || "")) {
                if (!xsData) xsData = xs;
              }
              // Collect all produced isotopes for compare selector
              const isoKey = `${xs.residualZ}-${xs.residualA}-${xs.state || ""}`;
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
    allProducedIsotopes = produced.sort((a, b) => a.name.localeCompare(b.name));
  });

  // Render XS plot
  $effect(() => {
    if (!open || !xsPlotDiv) return;
    if (xsData || compareIsotopes.length > 0) renderXsPlot();
  });

  async function ensurePlotly() {
    if (!Plotly) Plotly = await import("plotly.js-dist-min");
  }

  function renderXsPlot() {
    ensurePlotly().then(() => {
      if (!Plotly || !xsPlotDiv) return;

      const traces: any[] = [];

      // Main isotope XS
      if (xsData) {
        // Determine energy range: beam energy to final E_out + 10% padding
        const minE = layerEnergies.length > 0
          ? Math.max(0, layerEnergies[layerEnergies.length - 1].eOut * 0.9)
          : 0;
        const maxE = beamEnergy * 1.1;

        // Filter to relevant range
        const energies = Array.from(xsData.energiesMeV);
        const xs = Array.from(xsData.xsMb);
        const filteredE: number[] = [];
        const filteredXS: number[] = [];
        for (let i = 0; i < energies.length; i++) {
          if (energies[i] >= minE && energies[i] <= maxE) {
            filteredE.push(energies[i]);
            filteredXS.push(xs[i]);
          }
        }

        traces.push({
          x: filteredE,
          y: filteredXS,
          type: "scatter",
          mode: "lines",
          line: { color: TRACE_COLORS[0], width: 2 },
          name: `${name}`,
        });
      }

      // Compare traces
      compareIsotopes.forEach((cmp, idx) => {
        const minE = layerEnergies.length > 0
          ? Math.max(0, layerEnergies[layerEnergies.length - 1].eOut * 0.9)
          : 0;
        const maxE = beamEnergy * 1.1;

        const energies = Array.from(cmp.xs.energiesMeV);
        const xs = Array.from(cmp.xs.xsMb);
        const filteredE: number[] = [];
        const filteredXS: number[] = [];
        for (let i = 0; i < energies.length; i++) {
          if (energies[i] >= minE && energies[i] <= maxE) {
            filteredE.push(energies[i]);
            filteredXS.push(xs[i]);
          }
        }

        traces.push({
          x: filteredE,
          y: filteredXS,
          type: "scatter",
          mode: "lines",
          line: { color: TRACE_COLORS[(idx + 1) % TRACE_COLORS.length], width: 1.5 },
          name: cmp.name,
        });
      });

      // Layer boundary shapes
      const shapes: any[] = [];
      const annotations: any[] = [];
      for (const le of layerEnergies) {
        if (le.eIn > 0) {
          shapes.push({
            type: "line", x0: le.eIn, x1: le.eIn, y0: 0, y1: 1,
            yref: "paper",
            line: { color: "#484f58", width: 1, dash: "dot" },
          });
        }
        if (le.eOut > 0 && le.eOut !== le.eIn) {
          shapes.push({
            type: "line", x0: le.eOut, x1: le.eOut, y0: 0, y1: 1,
            yref: "paper",
            line: { color: "#484f58", width: 1, dash: "dot" },
          });
          annotations.push({
            x: (le.eIn + le.eOut) / 2, y: 1.02, yref: "paper",
            text: le.material, showarrow: false,
            font: { color: "#6e7681", size: 9 },
            xanchor: "center",
          });
        }
      }

      const minE = layerEnergies.length > 0
        ? Math.max(0, layerEnergies[layerEnergies.length - 1].eOut * 0.9)
        : undefined;
      const maxE = beamEnergy > 0 ? beamEnergy * 1.1 : undefined;

      const layout = darkLayout({
        xaxis: {
          title: "Energy (MeV)",
          gridcolor: "#2d333b",
          range: minE !== undefined && maxE !== undefined ? [minE, maxE] : undefined,
        },
        yaxis: { title: "Cross section (mb)", gridcolor: "#2d333b" },
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

  // Compare: add an isotope XS to the plot
  function addCompare(isoName: string) {
    if (!isoName || compareIsotopes.some((c) => c.name === isoName)) return;
    const db = getDataStore();
    if (!db) return;

    const config = getConfig();
    const proj = config.beam.projectile;

    // Find XS for this isotope
    for (const layer of config.layers) {
      if (!layer.material) continue;
      try {
        const mat = resolveMaterial(db, layer.material);
        for (const [el] of mat.elements) {
          for (const [targetA] of el.isotopes) {
            const xsList = db.getCrossSections(proj, el.Z, targetA);
            const iso = allProducedIsotopes.find((i) => i.name === isoName);
            if (!iso) continue;
            const match = xsList.find(
              (xs) => xs.residualZ === iso.Z && xs.residualA === iso.A && (xs.state || "") === iso.state,
            );
            if (match) {
              compareIsotopes = [...compareIsotopes, { name: isoName, xs: match }];
              return;
            }
          }
        }
      } catch { /* skip */ }
    }
  }

  function removeCompare(isoName: string) {
    compareIsotopes = compareIsotopes.filter((c) => c.name !== isoName);
  }

  // Activity computation
  interface ActivityResult {
    times: number[];
    activities: number[];
    productionRate: number;
    satYield: number;
    eobActivity: number;
  }

  let activityResult: ActivityResult | null = $state(null);

  let activityComputing = $state(false);

  $effect(() => {
    if (!open || !xsData || !decayInfo) {
      activityResult = null;
      return;
    }
    const db = getDataStore();
    if (!db) return;

    const config = getConfig();
    const proj = config.beam.projectile;
    const irrS = config.irradiation_s;
    const coolS = config.cooling_s || 86400;
    const halfLife = decayInfo.halfLifeS;
    if (!halfLife || halfLife <= 0) return;

    // Capture reactive deps, then defer heavy work off the main thread tick
    const preview = getDepthPreview();
    activityComputing = true;

    setTimeout(() => {
      const lambda = Math.LN2 / halfLife;
      let productionRate = 0;

      for (const layer of config.layers) {
        if (!layer.material) continue;
        try {
          const mat = resolveMaterial(db, layer.material);
          const composition: Array<[number, number]> = mat.elements.map(([el, frac]) => [el.Z, frac]);
          const density = mat.density;
          const layerPreview = preview.find((l) => l.material === layer.material);
          if (!layerPreview || layerPreview.energy_in_MeV <= 0) continue;

          const eIn = layerPreview.energy_in_MeV;
          const eOut = layerPreview.energy_out_MeV;
          const thickCm = layerPreview.thickness_mm / 10;

          const dedxFn = (energies: Float64Array) =>
            dedxMeVPerCm(db, proj, composition, density, energies) as Float64Array;

          for (const [el, frac] of mat.elements) {
            for (const [targetA, isoFrac] of el.isotopes) {
              const xsList = db.getCrossSections(proj, el.Z, targetA);
              const match = xsList.find(
                (xs) => xs.residualZ === Z && xs.residualA === A && (xs.state || "") === (nuclearState || ""),
              );
              if (!match) continue;

              const AVOGADRO = 6.022e23;
              const nAtoms = density * thickCm * AVOGADRO / targetA * frac * isoFrac;
              const beamParticlesPerS = config.beam.current_mA * 1e-3 / 1.602e-19;

              const { productionRate: rate } = computeProductionRate(
                match.energiesMeV, match.xsMb,
                dedxFn, eIn, eOut,
                nAtoms, beamParticlesPerS, thickCm,
              );
              productionRate += rate;
            }
          }
        } catch { /* skip */ }
      }

      activityComputing = false;
      if (productionRate <= 0) return;

      const nPts = 100;
      const times: number[] = [];
      const activities: number[] = [];

      const irrTimes = linspace(0, irrS, nPts / 2);
      for (let i = 0; i < irrTimes.length; i++) {
        times.push(irrTimes[i]);
        activities.push((productionRate / lambda) * (1 - Math.exp(-lambda * irrTimes[i])));
      }

      const eobActivity = activities[activities.length - 1];

      const coolTimes = linspace(0, coolS, nPts / 2);
      for (let i = 1; i < coolTimes.length; i++) {
        times.push(irrS + coolTimes[i]);
        activities.push(eobActivity * Math.exp(-lambda * coolTimes[i]));
      }

      const satYield = productionRate / lambda / (config.beam.current_mA * 1000);
      activityResult = { times, activities, productionRate, satYield, eobActivity };
    }, 0);
  });

  $effect(() => {
    if (!open || !activityResult || !actPlotDiv) return;
    ensurePlotly().then(renderActivityPlot);
  });

  function renderActivityPlot() {
    if (!Plotly || !activityResult || !actPlotDiv) return;

    const config = getConfig();
    const totalTime = config.irradiation_s + (config.cooling_s || 86400);
    const { label: timeLabel, divisor: timeDiv } = bestTimeUnit(totalTime);
    const maxAct = activityResult.activities.reduce((m, v) => Math.max(m, v), 0);
    const { label: actLabel, divisor: actDiv } = bestActivityUnit(maxAct);
    const eobX = config.irradiation_s / timeDiv;

    const traces = [{
      x: activityResult.times.map((t) => t / timeDiv),
      y: activityResult.activities.map((a) => a / actDiv),
      type: "scatter",
      mode: "lines",
      line: { color: TRACE_COLORS[0], width: 2 },
      name: name,
    }];

    const layout = darkLayout({
      xaxis: { title: `Time (${timeLabel})`, gridcolor: "#2d333b" },
      yaxis: { title: `Activity (${actLabel})`, gridcolor: "#2d333b" },
      margin: { t: 10, r: 20, b: 40, l: 55 },
      height: 200,
      showlegend: false,
      shapes: [{
        type: "line" as const,
        x0: eobX, x1: eobX, y0: 0, y1: 1,
        yref: "paper" as const,
        line: { color: "#f0883e", width: 1, dash: "dash" as const },
      }],
      annotations: [{
        x: eobX, y: 1.02, yref: "paper" as const,
        text: "EOB", showarrow: false,
        font: { color: "#f0883e", size: 9 },
        xanchor: "center" as const,
      }],
    });

    Plotly.react(actPlotDiv, traces, layout, PLOTLY_CONFIG);
  }

  onDestroy(() => {
    if (Plotly) {
      if (xsPlotDiv) Plotly.purge(xsPlotDiv);
      if (actPlotDiv) Plotly.purge(actPlotDiv);
    }
  });

  function tendlUrl(): string {
    const sym = Z_TO_SYMBOL[Z] ?? "";
    return `https://tendl.web.psi.ch/tendl_2023/proton_html/${sym}${String(A).padStart(3, "0")}/residual.html`;
  }

  // Nuclear notation helper
  let symbol = $derived(Z_TO_SYMBOL[Z] ?? "");
</script>

<Modal {open} {onclose} wide>
  {#snippet headerChildren()}
    <div class="nuc-title">
      <span class="nuc-notation">
        <sup class="nuc-a">{A}</sup><sub class="nuc-z">{Z}</sub>{symbol}{#if nuclearState}<sup class="nuc-state">{nuclearState}</sup>{/if}
      </span>
      <span class="nuc-name">{symbol}-{A}{nuclearState ? ` (${nuclearState})` : ""}</span>
      {#if decayInfo}
        <span class="nuc-hl">{formatHalfLife(decayInfo.halfLifeS)}</span>
      {/if}
      <div class="nuc-actions">
        <button
          class="action-btn"
          class:active={selected}
          onclick={() => toggleIsotope(name)}
        >
          {selected ? "Deselect" : "Compare"}
        </button>
      </div>
    </div>
  {/snippet}

  <div class="isotope-popup">
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
                <span class="chain-nuc parent" title="{p.mode} ({(p.branching * 100).toFixed(1)}%)">{p.name}</span>
              {/each}
            </div>
            <span class="chain-arrow">&rarr;</span>
          {/if}
          <span class="chain-nuc current">{name}{nuclearState ? ` (${nuclearState})` : ""}</span>
          {#if decayInfo && decayInfo.decayModes.some(m => m.daughterZ !== null)}
            <span class="chain-arrow">&rarr;</span>
            <div class="chain-group">
              {#each decayInfo.decayModes.filter(m => m.daughterZ !== null) as mode}
                {@const dSym = Z_TO_SYMBOL[mode.daughterZ!] ?? `Z${mode.daughterZ}`}
                <span class="chain-nuc daughter" title="{mode.mode} ({(mode.branching * 100).toFixed(1)}%)">
                  {dSym}-{mode.daughterA}{mode.daughterState ? ` (${mode.daughterState})` : ""}
                </span>
              {/each}
            </div>
          {/if}
        </div>
      </div>
    {/if}

    <!-- Cross-section plot -->
    {#if xsData || compareIsotopes.length > 0}
      <div class="section">
        <div class="section-bar">
          <span class="section-label">Cross section</span>
          <div class="compare-controls">
            {#each compareIsotopes as cmp}
              <span class="compare-chip">
                {cmp.name}
                <button class="chip-x" onclick={() => removeCompare(cmp.name)}>&times;</button>
              </span>
            {/each}
            <select
              class="compare-select"
              onchange={(e) => { addCompare((e.target as HTMLSelectElement).value); (e.target as HTMLSelectElement).value = ""; }}
            >
              <option value="">+ compare...</option>
              {#each allProducedIsotopes.filter(i => i.name !== name && !compareIsotopes.some(c => c.name === i.name)) as iso}
                <option value={iso.name}>{iso.name}</option>
              {/each}
            </select>
          </div>
        </div>
        <div bind:this={xsPlotDiv} class="xs-plot"></div>
      </div>
    {/if}

    <!-- Activity curve -->
    {#if activityResult}
      <div class="section">
        <div class="section-bar">
          <span class="section-label">Activity</span>
          <div class="activity-stats">
            <span>EOB: {fmtActivity(activityResult.eobActivity)}</span>
            <span>Rate: {activityResult.productionRate.toExponential(2)} /s</span>
          </div>
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
      <a href={tendlUrl()} target="_blank" rel="noopener noreferrer" class="ext-link">
        <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="M3.75 2h3.5a.75.75 0 010 1.5h-3.5a.25.25 0 00-.25.25v8.5c0 .138.112.25.25.25h8.5a.25.25 0 00.25-.25v-3.5a.75.75 0 011.5 0v3.5A1.75 1.75 0 0112.25 14h-8.5A1.75 1.75 0 012 12.25v-8.5C2 2.784 2.784 2 3.75 2zm6.854.22a.75.75 0 011.396-.04L14 5.5a.75.75 0 01-1.5 0V4.56l-3.97 3.97a.75.75 0 01-1.06-1.06L11.44 3.5H10.5a.75.75 0 010-1.5h.104z"></path></svg>
        TENDL 2023
      </a>
    </div>
  </div>
</Modal>

<style>
  /* Title in header */
  .nuc-title {
    display: flex;
    align-items: baseline;
    gap: 0.6rem;
    flex: 1;
    min-width: 0;
  }

  .nuc-notation {
    font-size: 1.4rem;
    font-weight: 700;
    color: #58a6ff;
    line-height: 1;
    position: relative;
  }

  .nuc-a {
    font-size: 0.6em;
    position: relative;
    top: -0.4em;
    margin-right: -0.1em;
    color: #8b949e;
    font-weight: 400;
  }

  .nuc-z {
    font-size: 0.55em;
    position: relative;
    bottom: -0.2em;
    margin-right: -0.05em;
    color: #6e7681;
    font-weight: 400;
  }

  .nuc-state {
    font-size: 0.5em;
    color: #d29922;
  }

  .nuc-name {
    font-size: 0.85rem;
    color: #8b949e;
  }

  .nuc-hl {
    font-size: 0.8rem;
    color: #6e7681;
    font-variant-numeric: tabular-nums;
  }

  .nuc-actions {
    margin-left: auto;
    flex-shrink: 0;
  }

  /* Body */
  .isotope-popup {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }

  .properties {
    background: #0d1117;
    border: 1px solid #2d333b;
    border-radius: 4px;
    padding: 0.4rem 0.5rem;
  }

  .prop-row {
    display: flex;
    justify-content: space-between;
    padding: 0.15rem 0;
    font-size: 0.8rem;
  }

  .prop-label { color: #8b949e; }
  .prop-value { color: #e1e4e8; font-variant-numeric: tabular-nums; }
  .prop-sep { color: #484f58; }
  .branching { color: #6e7681; font-size: 0.7rem; }

  /* Decay chain */
  .decay-chain {
    background: #0d1117;
    border: 1px solid #2d333b;
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

  .chain-nuc.parent { background: #1c2128; color: #8b949e; border: 1px solid #2d333b; }
  .chain-nuc.current { background: #1f3a5f; color: #58a6ff; border: 1px solid #58a6ff; font-weight: 600; }
  .chain-nuc.daughter { background: #1c2128; color: #bc8cff; border: 1px solid #2d333b; }
  .chain-arrow { color: #484f58; font-size: 0.85rem; }

  /* Sections */
  .section {
    border: 1px solid #2d333b;
    border-radius: 4px;
    overflow: hidden;
  }

  .section-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.3rem 0.5rem;
    border-bottom: 1px solid #2d333b;
    background: #0d1117;
    gap: 0.5rem;
    flex-wrap: wrap;
  }

  .section-label {
    font-size: 0.65rem;
    color: #6e7681;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    flex-shrink: 0;
  }

  .compare-controls {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    flex-wrap: wrap;
  }

  .compare-chip {
    display: inline-flex;
    align-items: center;
    gap: 0.15rem;
    background: #1c2128;
    border: 1px solid #2d333b;
    border-radius: 3px;
    padding: 0.1rem 0.3rem;
    font-size: 0.65rem;
    color: #8b949e;
  }

  .chip-x {
    background: none;
    border: none;
    color: inherit;
    font-size: 0.7rem;
    cursor: pointer;
    padding: 0;
    line-height: 1;
    opacity: 0.6;
  }

  .chip-x:hover { opacity: 1; color: #f85149; }

  .compare-select {
    background: #0d1117;
    border: 1px solid #2d333b;
    border-radius: 3px;
    color: #6e7681;
    padding: 0.1rem 0.25rem;
    font-size: 0.65rem;
    cursor: pointer;
  }

  .xs-plot, .act-plot {
    width: 100%;
    height: 220px;
  }

  .activity-stats {
    display: flex;
    gap: 0.75rem;
    font-size: 0.65rem;
    color: #6e7681;
    font-variant-numeric: tabular-nums;
  }

  /* Actions */
  .action-btn {
    background: #21262d;
    border: 1px solid #2d333b;
    border-radius: 4px;
    color: #e1e4e8;
    padding: 0.2rem 0.5rem;
    font-size: 0.75rem;
    cursor: pointer;
  }

  .action-btn:hover { border-color: #58a6ff; }
  .action-btn.active { background: #1f3a5f; border-color: #58a6ff; color: #58a6ff; }

  /* Links */
  .links {
    display: flex;
    gap: 1rem;
    border-top: 1px solid #2d333b;
    padding-top: 0.4rem;
  }

  .ext-link {
    color: #58a6ff;
    font-size: 0.75rem;
    text-decoration: none;
    display: flex;
    align-items: center;
    gap: 0.25rem;
  }

  .ext-link:hover { text-decoration: underline; }
</style>

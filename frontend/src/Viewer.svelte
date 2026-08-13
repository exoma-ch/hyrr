<script lang="ts">
  /**
   * Standalone results viewer (ADR 0006 spike).
   *
   * Renders the *same* result components the live app uses, against a snapshot
   * baked into the HTML. No compute engine, no Parquet, no network — the
   * recipient can filter, sort, select and switch axes, but cannot re-run.
   */
  import { getSnapshot } from "./lib/viewer/snapshot";
  import IsotopeFilterBar from "./lib/components/IsotopeFilterBar.svelte";
  import PlotProductionDepth from "./lib/components/PlotProductionDepth.svelte";
  import PlotActivityCurve from "./lib/components/PlotActivityCurve.svelte";
  import EmissionPlot from "./lib/components/EmissionPlot.svelte";
  import ActivityTableEnhanced from "./lib/components/ActivityTableEnhanced.svelte";

  let error = $state<string | null>(null);
  let snap = $state<ReturnType<typeof getSnapshot> | null>(null);

  try {
    snap = getSnapshot();
  } catch (e) {
    error = String(e);
  }

  let result = $derived(snap?.result ?? null);
  let stack = $derived(
    result ? (result.config.layers ?? []).map((l: { material: string }) => l.material).join(" + ") : "",
  );
  let beam = $derived.by(() => {
    const b = result?.config?.beam;
    if (!b) return "";
    const parts = [b.projectile, b.energy_MeV != null ? `${b.energy_MeV} MeV` : null];
    if (b.current_mA != null) parts.push(`${b.current_mA * 1000} µA`);
    return parts.filter(Boolean).join(" ");
  });
  let timing = $derived.by(() => {
    const c = result?.config;
    if (!c) return "";
    const h = (s: number) => (s % 3600 === 0 ? `${s / 3600} h` : `${s} s`);
    return `${h(c.irradiation_s)} irradiation · ${h(c.cooling_s)} cooling`;
  });
</script>

<main>
  <header>
    <div class="title">
      <strong>HYRR</strong>
      <span class="sub">shared result — view only</span>
    </div>
    {#if result}
      <div class="meta">
        <span>{beam} → {stack}</span>
        <span class="timing">{timing}</span>
      </div>
    {/if}
  </header>

  {#if error}
    <p class="err">Could not read the embedded result: <code>{error}</code></p>
  {:else if result}
    <div class="flow">
      <IsotopeFilterBar {result} />
      <PlotProductionDepth {result} />
      <PlotActivityCurve {result} />
      {#if snap?.tier === "B"}
        <EmissionPlot {result} />
      {/if}
      <ActivityTableEnhanced {result} />
    </div>

    <footer>
      <p>
        This is a static snapshot. Values cannot be recomputed or re-tuned here —
        the physics engine is not included.
        {#if snap?.tier === "A"}
          Emission spectra are omitted from this artifact.
        {/if}
      </p>
      {#if snap?.hyrr_version}
        <p class="ver">Produced by HYRR {snap.hyrr_version}{snap.generated_at ? ` · ${snap.generated_at}` : ""}</p>
      {/if}
    </footer>
  {/if}
</main>

<style>
  main {
    max-width: 1400px;
    margin: 0 auto;
    padding: 1rem;
  }
  header {
    display: flex;
    flex-wrap: wrap;
    gap: 0.75rem;
    align-items: baseline;
    justify-content: space-between;
    padding-bottom: 0.75rem;
    border-bottom: 1px solid var(--c-border, #30363d);
    margin-bottom: 1rem;
  }
  .title strong {
    font-size: 1.1rem;
    letter-spacing: 0.02em;
  }
  .sub {
    margin-left: 0.5rem;
    font-size: 0.8rem;
    color: var(--c-text-muted, #8b949e);
  }
  .meta {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 0.15rem;
    font-size: 0.85rem;
    color: var(--c-text-muted, #8b949e);
  }
  .timing {
    font-size: 0.75rem;
    opacity: 0.8;
  }
  .flow {
    display: flex;
    flex-direction: column;
    gap: 1.5rem;
  }
  footer {
    margin-top: 2rem;
    padding-top: 0.75rem;
    border-top: 1px solid var(--c-border, #30363d);
    font-size: 0.8rem;
    color: var(--c-text-muted, #8b949e);
  }
  .ver {
    opacity: 0.7;
  }
  .err {
    color: var(--c-danger, #f85149);
  }
</style>

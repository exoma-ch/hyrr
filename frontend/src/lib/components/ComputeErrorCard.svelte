<script lang="ts">
  import type { ComputeError } from "../types";
  import { getActiveTraceId } from "../stores/results.svelte";
  import { buildMergedTracePayload } from "../trace/merge-trace";
  import TracePreview from "./TracePreview.svelte";

  type Props = {
    error: ComputeError;
    projectile?: string;
    energyMev?: number;
    onSwitchProjectile?: (suggestion: string) => void;
    onEditBeam?: () => void;
    onReportGap?: () => void;
  };

  let {
    error,
    projectile,
    energyMev,
    onSwitchProjectile,
    onEditBeam,
    onReportGap,
  }: Props = $props();

  // Headline is variant-specific. The previous always-on "No stopping data
  // available" was misleading for `Unknown` errors (e.g. WASM panics — see
  // #211 follow-up #213, where a wasm-bindgen "recursive use of an object"
  // panic was rendered as "No stopping data available / Target —").
  const headline = $derived(
    error.kind !== "StoppingError"
      ? "Compute backend error"
      : error.variant === "NoSourceTable"
        ? "No stopping data for this projectile"
        : error.variant === "NoTargetData"
          ? "No stopping data for this target"
          : "Energy outside tabulated range",
  );

  // Layer attribution from `StoppingError::with_layer_context` (Rust side).
  // Only set when the error surfaced inside the per-layer compute loop.
  const layerLabel = $derived(
    error.kind === "StoppingError" && error.layer_index != null
      ? `L${error.layer_index + 1}${error.layer_material ? ` (${error.layer_material})` : ""}`
      : null,
  );

  const sourceLabel = $derived(
    error.kind === "StoppingError" ? error.source : null,
  );
  const projectileLabel = $derived(
    error.kind === "StoppingError" && error.variant === "NoSourceTable"
      ? error.projectile
      : (projectile ?? null),
  );
  const targetLabel = $derived(
    error.kind === "StoppingError" && error.variant !== "NoSourceTable"
      ? `${error.target_symbol} (Z=${error.target_z})`
      : null,
  );
  const energyLabel = $derived(
    error.kind === "StoppingError" && error.variant === "EnergyOutOfRange"
      ? `${error.energy_mev.toFixed(3)} MeV`
      : energyMev != null
        ? `${energyMev.toFixed(3)} MeV`
        : null,
  );

  // Suggested replacement when StoppingError::NoSourceTable.
  const nearestSuggestion = $derived(
    error.kind === "StoppingError" && error.variant === "NoSourceTable"
      ? pickNearest(error.projectile, error.available)
      : null,
  );

  function pickNearest(_proj: string, available: string[]): string | null {
    const projForms = available
      .map((s) => s.replace(/^catima_/, ""))
      .filter((s) => s.length > 0);
    return projForms[0] ?? null;
  }

  // "View diagnostics" surfaces the SAME trace the bug-report modal would attach
  // (#118/#159) — read-only here, the modal owns the attach checkbox.
  let showDiagnostics = $state(false);
  const tracePayload = $derived(
    showDiagnostics ? buildMergedTracePayload(getActiveTraceId()) : "",
  );
</script>

<div class="error-card" role="alert" aria-live="assertive">
  <header class="error-card-header">
    <span class="badge">{headline}</span>
    <span class="variant-tag">{
      error.kind === "StoppingError" ? error.variant : "Unknown"
    }</span>
  </header>

  <section class="data-points">
    <dl>
      {#if layerLabel}<dt>Layer</dt><dd>{layerLabel}</dd>{/if}
      {#if sourceLabel}<dt>Source</dt><dd>{sourceLabel}</dd>{/if}
      {#if projectileLabel}<dt>Projectile</dt><dd>{projectileLabel}</dd>{/if}
      {#if targetLabel}<dt>Target</dt><dd>{targetLabel}</dd>{/if}
      {#if energyLabel}<dt>Energy</dt><dd>{energyLabel}</dd>{/if}
    </dl>
  </section>

  <section class="explanation">
    <p class="message">{error.message}</p>

    {#if error.kind === "StoppingError" && error.variant === "NoSourceTable"}
      <p class="available-line">
        <strong>Available:</strong> {error.available_pretty}
      </p>
    {:else if error.kind === "StoppingError" && error.variant === "EnergyOutOfRange"}
      <p class="available-line">
        <strong>Tabulated range:</strong>
        {error.min_mev.toFixed(3)} – {error.max_mev.toFixed(3)} MeV
      </p>
    {:else if error.kind === "StoppingError" && error.variant === "NoTargetData"}
      <p class="available-line">
        <strong>Available Z:</strong>
        {error.available_zs.join(", ")}
      </p>
    {/if}

    {#if error.kind === "StoppingError"}
      <p class="consequence">
        Without stopping data, the depth profile, dE/dx curve, residual energy,
        and Bragg peak cannot be computed for this stack.
      </p>
    {:else}
      <p class="consequence">
        The compute backend failed before producing a result. Try refreshing
        the page if this persists — the WASM module state may be poisoned.
      </p>
    {/if}
  </section>

  <footer class="actions">
    {#if nearestSuggestion && onSwitchProjectile}
      <button
        type="button"
        class="primary"
        onclick={() => onSwitchProjectile?.(nearestSuggestion)}
      >
        Switch projectile to {nearestSuggestion}
      </button>
    {/if}
    {#if onEditBeam}
      <button type="button" onclick={onEditBeam}>Edit beam</button>
    {/if}
    {#if onReportGap}
      <button type="button" onclick={onReportGap}>Report this gap</button>
    {/if}
    <button type="button" onclick={() => (showDiagnostics = !showDiagnostics)}>
      {showDiagnostics ? "Hide diagnostics" : "View diagnostics"}
    </button>
  </footer>

  {#if showDiagnostics}
    <section class="diagnostics">
      {#if tracePayload}
        <TracePreview payload={tracePayload} showAttach={false} />
      {:else}
        <p class="no-trace">No trace recorded for this run.</p>
      {/if}
    </section>
  {/if}
</div>

<style>
  /* SSoT theme tokens defined in App.svelte (`--c-*`). The card lives inside
     both light + dark themes and must not invent its own color scale — using
     the project tokens keeps contrast correct without theme-specific overrides. */
  .error-card {
    border: 1px solid var(--c-red);
    border-radius: 6px;
    padding: 1rem 1.25rem;
    background: var(--c-red-tint-subtle);
    margin: 1rem 0;
    font-size: 0.95rem;
    color: var(--c-text);
  }

  .error-card-header {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    margin-bottom: 0.5rem;
  }

  .badge {
    font-weight: 600;
    color: var(--c-red);
    font-size: 1rem;
  }

  .variant-tag {
    font-family: ui-monospace, "JetBrains Mono", Menlo, monospace;
    font-size: 0.8rem;
    background: var(--c-bg-subtle);
    border: 1px solid var(--c-border-muted);
    padding: 0.05rem 0.4rem;
    border-radius: 3px;
    color: var(--c-text-muted);
  }

  .data-points dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 0.15rem 0.75rem;
    margin: 0.5rem 0;
  }
  .data-points dt {
    font-weight: 500;
    color: var(--c-text-muted);
  }
  .data-points dd {
    margin: 0;
    font-family: ui-monospace, "JetBrains Mono", Menlo, monospace;
    color: var(--c-text);
  }

  .explanation .message {
    margin: 0.5rem 0;
    line-height: 1.4;
    color: var(--c-text);
  }
  .available-line {
    margin: 0.4rem 0;
    color: var(--c-text);
  }
  .consequence {
    margin: 0.4rem 0 0;
    font-size: 0.85rem;
    color: var(--c-text-muted);
    font-style: italic;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    margin-top: 0.75rem;
  }

  .actions button {
    padding: 0.35rem 0.85rem;
    border: 1px solid var(--c-border);
    border-radius: 4px;
    background: var(--c-bg-subtle);
    color: var(--c-text);
    cursor: pointer;
    font-size: 0.9rem;
  }
  .actions button.primary {
    background: var(--c-accent);
    color: var(--c-bg-default);
    border-color: transparent;
  }
  .actions button:hover {
    background: var(--c-bg-hover);
  }
  .actions button.primary:hover {
    background: var(--c-accent-hover);
  }
</style>

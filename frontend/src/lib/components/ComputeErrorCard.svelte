<script lang="ts">
  import type { ComputeError } from "../types";

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

  // Pretty fall-throughs derived from the structured payload.
  const sourceLabel = $derived(
    error.kind === "StoppingError" ? error.source : "compute backend",
  );
  const projectileLabel = $derived(
    error.kind === "StoppingError" && error.variant === "NoSourceTable"
      ? error.projectile
      : (projectile ?? "—"),
  );
  const targetLabel = $derived(
    error.kind === "StoppingError" && error.variant !== "NoSourceTable"
      ? `${error.target_symbol} (Z=${error.target_z})`
      : "—",
  );
  const energyLabel = $derived(
    error.kind === "StoppingError" && error.variant === "EnergyOutOfRange"
      ? `${error.energy_mev.toFixed(3)} MeV`
      : energyMev != null
        ? `${energyMev.toFixed(3)} MeV`
        : "—",
  );

  // Suggested replacement when StoppingError::NoSourceTable.
  const nearestSuggestion = $derived(
    error.kind === "StoppingError" && error.variant === "NoSourceTable"
      ? pickNearest(error.projectile, error.available)
      : null,
  );

  function pickNearest(_proj: string, available: string[]): string | null {
    // The Rust side gives us source identifiers like "catima_C-12";
    // strip the prefix for display + dropdown values.
    const projForms = available
      .map((s) => s.replace(/^catima_/, ""))
      .filter((s) => s.length > 0);
    return projForms[0] ?? null;
  }
</script>

<div class="error-card" role="alert" aria-live="assertive">
  <header class="error-card-header">
    <span class="badge">No stopping data available</span>
    <span class="variant-tag">{
      error.kind === "StoppingError" ? error.variant : "Unknown"
    }</span>
  </header>

  <section class="data-points">
    <dl>
      <dt>Source</dt><dd>{sourceLabel}</dd>
      <dt>Projectile</dt><dd>{projectileLabel}</dd>
      <dt>Target</dt><dd>{targetLabel}</dd>
      <dt>Energy</dt><dd>{energyLabel}</dd>
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

    <p class="consequence">
      Without stopping data, the depth profile, dE/dx curve, residual energy,
      and Bragg peak cannot be computed for this stack.
    </p>
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
  </footer>
</div>

<style>
  .error-card {
    border: 1px solid var(--color-error, #d23f3f);
    border-radius: 6px;
    padding: 1rem 1.25rem;
    background: var(--color-error-bg, rgba(210, 63, 63, 0.06));
    margin: 1rem 0;
    font-size: 0.95rem;
    color: var(--color-fg, #1a1a1a);
  }

  .error-card-header {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    margin-bottom: 0.5rem;
  }

  .badge {
    font-weight: 600;
    color: var(--color-error, #d23f3f);
    font-size: 1rem;
  }

  .variant-tag {
    font-family: ui-monospace, "JetBrains Mono", Menlo, monospace;
    font-size: 0.8rem;
    background: var(--color-bg-elevated, #fff);
    border: 1px solid var(--color-border, rgba(0, 0, 0, 0.12));
    padding: 0.05rem 0.4rem;
    border-radius: 3px;
    color: var(--color-fg-muted, #555);
  }

  .data-points dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 0.15rem 0.75rem;
    margin: 0.5rem 0;
  }
  .data-points dt {
    font-weight: 500;
    color: var(--color-fg-muted, #555);
  }
  .data-points dd {
    margin: 0;
    font-family: ui-monospace, "JetBrains Mono", Menlo, monospace;
  }

  .explanation .message {
    margin: 0.5rem 0;
    line-height: 1.4;
  }
  .available-line {
    margin: 0.4rem 0;
  }
  .consequence {
    margin: 0.4rem 0 0;
    font-size: 0.85rem;
    color: var(--color-fg-muted, #555);
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
    border: 1px solid var(--color-border, rgba(0, 0, 0, 0.18));
    border-radius: 4px;
    background: var(--color-bg-elevated, #fff);
    cursor: pointer;
    font-size: 0.9rem;
  }
  .actions button.primary {
    background: var(--color-accent, #2c6ed5);
    color: #fff;
    border-color: transparent;
  }
  .actions button:hover {
    filter: brightness(0.95);
  }
</style>

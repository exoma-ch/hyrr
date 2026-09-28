<script lang="ts">
  /**
   * Non-fatal banner for a data-load failure that happened AFTER the
   * compute succeeded — currently only emissions loads (dose / spectrum
   * data). Rendered alongside the successful result, not in place of it,
   * because activities and yields are unaffected. (#689, PR #715 review)
   *
   * Design contrast with `FetchErrorCard`:
   * - `FetchErrorCard` blocks the app (init-time or compute-time hard
   *   failure — no result to show).
   * - This banner sits between the results and the layer stack. Retry
   *   re-runs the sim; the current result stays visible until then.
   */
  import type { ParsedFetchError } from "../utils/parse-fetch-error";
  import { fetchErrorTitle } from "../utils/parse-fetch-error";

  type Props = {
    warning: ParsedFetchError;
    onretry: () => void;
    ondismiss: () => void;
  };
  let { warning, onretry, ondismiss }: Props = $props();
</script>

<div
  class="post-sim-warning"
  role="status"
  aria-live="polite"
  data-testid="post-sim-data-warning"
>
  <header>
    <span class="badge">Data load warning</span>
    <span class="title">{fetchErrorTitle(warning)}</span>
  </header>
  <p class="message">{warning.message}</p>
  <p class="scope">
    Activities, yields, and depth profiles are unaffected. Dose-rate
    readouts and emission spectra for the affected elements may be
    missing or incomplete.
  </p>
  <footer>
    <button type="button" class="primary" onclick={onretry}>Retry</button>
    <button type="button" onclick={ondismiss}>Dismiss</button>
  </footer>
</div>

<style>
  .post-sim-warning {
    border: 1px solid var(--c-warning, #d29d3f);
    border-radius: 6px;
    padding: 0.75rem 1rem;
    background: var(--c-warning-tint-subtle, rgba(210, 157, 63, 0.08));
    margin: 0.75rem 0;
    font-size: 0.9rem;
    color: var(--c-text);
  }
  header {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    margin-bottom: 0.4rem;
  }
  .badge {
    font-weight: 600;
    color: var(--c-warning, #d29d3f);
    text-transform: uppercase;
    font-size: 0.7rem;
    letter-spacing: 0.05em;
  }
  .title {
    font-weight: 500;
  }
  .message {
    margin: 0.25rem 0;
    line-height: 1.4;
  }
  .scope {
    margin: 0.25rem 0 0.5rem;
    color: var(--c-text-muted);
    font-size: 0.82rem;
  }
  footer {
    display: flex;
    gap: 0.4rem;
  }
  footer button {
    padding: 0.3rem 0.75rem;
    border: 1px solid var(--c-border);
    border-radius: 4px;
    background: var(--c-bg-subtle);
    color: var(--c-text);
    cursor: pointer;
    font-size: 0.85rem;
  }
  footer button.primary {
    background: var(--c-accent);
    color: var(--c-bg-default);
    border-color: transparent;
  }
  footer button:hover {
    background: var(--c-bg-hover);
  }
  footer button.primary:hover {
    background: var(--c-accent-hover);
  }
</style>

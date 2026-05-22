<script lang="ts">
  /**
   * Loading splash for data initialization.
   *
   * With bundled data the init is near-instant (disk read only).
   * On error, mounts FetchErrorCard with retry / open-URL actions.
   */
  import FetchErrorCard from "./FetchErrorCard.svelte";
  import { parseFetchError } from "../utils/parse-fetch-error";

  type Props = {
    loadingState: string;
    fallbackFraction: number;
    loadingError: unknown | null;
    onretry: () => void;
  };
  let {
    loadingState,
    fallbackFraction,
    loadingError,
    onretry,
  }: Props = $props();

  const parsedError = $derived(loadingError ? parseFetchError(loadingError) : null);
</script>

<div class="loading">
  {#if parsedError}
    <FetchErrorCard error={parsedError} {onretry} />
  {:else}
    <p class="stage" data-testid="splash-stage">{loadingState}</p>
    <div class="progress-bar" class:indeterminate={fallbackFraction <= 0}>
      <div
        class="progress-fill"
        style="width: {fallbackFraction > 0 ? fallbackFraction * 100 : 100}%"
      ></div>
    </div>
  {/if}
</div>

<style>
  .loading {
    text-align: center;
    padding: 4rem 1rem;
    color: var(--c-text-muted);
  }
  .stage {
    margin: 0 0 1rem;
  }
  .progress-bar {
    width: 300px;
    height: 6px;
    background: var(--c-border);
    border-radius: 3px;
    margin: 0 auto;
    overflow: hidden;
    position: relative;
  }
  .progress-fill {
    height: 100%;
    background: var(--c-accent);
    border-radius: 3px;
    transition: width 0.3s ease;
  }
  .progress-bar.indeterminate {
    overflow: hidden;
  }
  .progress-bar.indeterminate .progress-fill {
    width: 33% !important;
    animation: slide 1.4s ease-in-out infinite;
  }
  @keyframes slide {
    from { transform: translateX(-100%); }
    to { transform: translateX(300%); }
  }
</style>

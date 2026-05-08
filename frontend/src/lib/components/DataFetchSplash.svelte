<script lang="ts">
  /**
   * Data-fetch splash with progress events (#118).
   *
   * Listens for `hyrr://data-fetch-progress` over Tauri IPC (browser
   * mode silently no-ops — we still render the high-level loading
   * message but no per-stage progress). Renders a determinate
   * `<progress>` bar when `bytes_total` is known, an indeterminate one
   * otherwise.
   *
   * On error, mounts the `FetchErrorCard` (parsed via
   * `parseFetchError`) with retry / open-URL / install / use-bundled
   * actions.
   */
  import { onMount, onDestroy } from "svelte";
  import { isTauri } from "../utils/platform";
  import FetchErrorCard from "./FetchErrorCard.svelte";
  import { parseFetchError } from "../utils/parse-fetch-error";

  type Props = {
    /** High-level loading text (the "Loading nuclear data…" copy from
     *  App.svelte). Independent from the per-stage label coming from
     *  Rust events. */
    loadingState: string;
    /** Coarse fraction (0..1) for the legacy progress channel that
     *  the WASM/TS path still drives via the
     *  `initDataStore(_, onProgress)` callback. The Rust progress
     *  bar takes precedence when its events arrive. */
    fallbackFraction: number;
    /** Raw error from the init flow, if any. Strings (Tauri) and
     *  Errors (network/timeout) both supported. */
    loadingError: unknown | null;
    onretry: () => void;
    onuselimited?: () => void;
  };
  let {
    loadingState,
    fallbackFraction,
    loadingError,
    onretry,
    onuselimited,
  }: Props = $props();

  type RustProgress = {
    stage: "connecting" | "downloading" | "extracting" | "verifying";
    bytes_done: number;
    bytes_total: number | null;
  };
  let rustProgress = $state<RustProgress | null>(null);
  let unsubscribe: (() => void) | null = null;

  onMount(async () => {
    if (!isTauri()) return;
    try {
      const { listen } = await import("@tauri-apps/api/event");
      unsubscribe = await listen<RustProgress>(
        "hyrr://data-fetch-progress",
        (e) => {
          rustProgress = e.payload;
        },
      );
    } catch (e) {
      // No-op — splash falls back to the legacy fraction-based bar.
      console.warn("[splash] failed to subscribe to data-fetch-progress:", e);
    }
  });

  onDestroy(() => {
    unsubscribe?.();
  });

  const stageLabel = $derived(
    rustProgress
      ? rustProgress.stage === "connecting"
        ? "Connecting…"
        : rustProgress.stage === "downloading"
          ? "Downloading nuclear data…"
          : rustProgress.stage === "extracting"
            ? "Extracting…"
            : "Finalising…"
      : loadingState,
  );

  const determinate = $derived(
    rustProgress != null && (rustProgress.bytes_total ?? 0) > 0,
  );
  const fraction = $derived(
    rustProgress && rustProgress.bytes_total
      ? Math.min(1, rustProgress.bytes_done / rustProgress.bytes_total)
      : fallbackFraction,
  );
  const sizeHint = $derived(
    rustProgress && rustProgress.bytes_total
      ? `${formatMib(rustProgress.bytes_done)} / ${formatMib(rustProgress.bytes_total)} MiB`
      : null,
  );

  function formatMib(n: number): string {
    return (n / 1_048_576).toFixed(1);
  }

  const parsedError = $derived(loadingError ? parseFetchError(loadingError) : null);
</script>

<div class="loading">
  {#if parsedError}
    <FetchErrorCard error={parsedError} {onretry} {onuselimited} />
  {:else}
    <p class="stage" data-testid="splash-stage">{stageLabel}</p>
    <div class="progress-bar" class:indeterminate={!determinate}>
      <div
        class="progress-fill"
        style="width: {determinate ? fraction * 100 : 100}%"
      ></div>
    </div>
    {#if sizeHint}
      <p class="size-hint" data-testid="splash-size">{sizeHint}</p>
    {/if}
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
  .size-hint {
    margin: 0.5rem 0 0;
    font-size: 0.8rem;
    color: var(--c-text-faint);
  }
</style>

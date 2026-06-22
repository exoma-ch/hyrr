<script lang="ts">
  /**
   * Recovery card for cold-cache data-fetch failures (#118).
   *
   * Layout per the locked spike: title from variant, "Tried" URL row
   * (when the variant carries one), "Cache at" row, four action
   * buttons (Retry / Open URL / Install from local… / Use bundled),
   * and a CLI fallback hint.
   *
   * The two desktop-only buttons (Install + Use bundled) are hidden
   * in browser mode via `isTauri()`. Browser users can't install
   * local tarballs and don't have a managed cache to "use bundled"
   * against — they fall back to retry / open URL only.
   */
  import { onMount } from "svelte";
  import {
    type ParsedFetchError,
    fetchErrorTitle,
  } from "../utils/parse-fetch-error";
  import { openExternalUrl } from "../utils/open-url";
  import {
    getReleaseUrl,
    getCacheRootPattern,
  } from "../compute/data-fetch-meta";
  import { buildMergedTracePayload } from "../trace/merge-trace";
  import TracePreview from "./TracePreview.svelte";

  type Props = {
    error: ParsedFetchError;
    onretry: () => void;
  };
  let { error, onretry }: Props = $props();

  // The data-load hang (#118) has no compute traceId; its signal lives in the
  // "_init" bucket (backend init failures). Surface the same trace a bug report
  // would carry (#159).
  let showDiagnostics = $state(false);
  const tracePayload = $derived(
    showDiagnostics ? buildMergedTracePayload("_init") : "",
  );

  // SSoT-resolved values (Tauri only). Browser mode falls through to
  // the URL/cache_dir already in the error payload (if any) — both are
  // also synthesised by the Rust side from the same SSoT helper, so
  // they're identical-by-construction.
  let releaseUrl = $state<string | null>(null);
  let cacheRoot = $state<string | null>(null);

  onMount(async () => {
    [releaseUrl, cacheRoot] = await Promise.all([
      getReleaseUrl(),
      getCacheRootPattern(),
    ]);
  });

  const triedUrl = $derived(
    error.kind === "FetchError" &&
      (error.variant === "HttpStatus" || error.variant === "Network")
      ? error.url
      : (releaseUrl ?? ""),
  );
  const cacheDirDisplay = $derived(
    error.kind === "FetchError" && "cache_dir" in error ? error.cache_dir : (cacheRoot ?? ""),
  );

  async function onOpenUrl() {
    if (!triedUrl) return;
    await openExternalUrl(triedUrl);
  }
</script>

<div class="error-card" role="alert" aria-live="assertive">
  <header class="error-card-header">
    <span class="badge">{fetchErrorTitle(error)}</span>
    <span class="variant-tag">{
      error.kind === "FetchError" ? error.variant : "Unknown"
    }</span>
  </header>

  {#if triedUrl || cacheDirDisplay}
    <section class="data-points">
      <dl>
        {#if triedUrl}
          <dt>Tried</dt>
          <dd><code>{triedUrl}</code></dd>
        {/if}
        {#if cacheDirDisplay}
          <dt>Cache at</dt>
          <dd><code>{cacheDirDisplay}</code></dd>
        {/if}
      </dl>
    </section>
  {/if}

  <section class="explanation">
    <p class="message">{error.message || fetchErrorTitle(error)}</p>
  </section>

  <footer class="actions">
    <button type="button" class="primary" onclick={onretry}>
      Retry
    </button>
    {#if triedUrl}
      <button type="button" onclick={onOpenUrl}>
        Open URL in browser
      </button>
    {/if}
    <button type="button" onclick={() => (showDiagnostics = !showDiagnostics)}>
      {showDiagnostics ? "Hide diagnostics" : "View diagnostics"}
    </button>
  </footer>

  {#if showDiagnostics && tracePayload}
    <section class="diagnostics">
      <TracePreview payload={tracePayload} showAttach={false} />
    </section>
  {/if}

  <p class="cli-hint">
    Or from a terminal: <code>hyrr fetch-data</code>
  </p>
</div>

<style>
  .error-card {
    border: 1px solid var(--c-red, #d23f3f);
    border-radius: 6px;
    padding: 1rem 1.25rem;
    background: var(--c-red-tint-subtle, rgba(210, 63, 63, 0.06));
    margin: 1rem auto;
    max-width: 720px;
    font-size: 0.95rem;
    color: var(--c-text);
  }
  .error-card-header {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    flex-wrap: wrap;
    margin-bottom: 0.5rem;
  }
  .badge {
    font-weight: 600;
    color: var(--c-red, #d23f3f);
    font-size: 1rem;
  }
  .variant-tag {
    font-family: ui-monospace, "JetBrains Mono", Menlo, monospace;
    font-size: 0.78rem;
    background: var(--c-bg-default, #fff);
    border: 1px solid var(--c-border);
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
    word-break: break-all;
  }
  .data-points code,
  .cli-hint code {
    font-family: ui-monospace, "JetBrains Mono", Menlo, monospace;
    font-size: 0.85rem;
    color: var(--c-text);
  }
  .explanation .message {
    margin: 0.5rem 0;
    line-height: 1.4;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    margin-top: 0.75rem;
  }
  .actions button {
    padding: 0.4rem 0.9rem;
    border: 1px solid var(--c-border);
    border-radius: 4px;
    background: var(--c-bg-default);
    color: var(--c-text);
    cursor: pointer;
    font-size: 0.9rem;
  }
  .actions button:hover {
    border-color: var(--c-accent);
    background: var(--c-bg-hover);
  }
  .actions button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  .actions button.primary {
    background: var(--c-accent);
    color: #fff;
    border-color: transparent;
  }
  .actions button.primary:hover {
    background: var(--c-accent-hover);
  }
  .cli-hint {
    margin: 0.75rem 0 0;
    font-size: 0.78rem;
    color: var(--c-text-muted);
  }
</style>

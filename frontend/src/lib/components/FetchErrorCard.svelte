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
  import { isTauri } from "../utils/platform";
  import {
    getReleaseUrl,
    getCacheRootPattern,
    getTarballFilename,
  } from "../compute/data-fetch-meta";

  type Props = {
    error: ParsedFetchError;
    onretry: () => void;
    onuselimited?: () => void;
  };
  let { error, onretry, onuselimited }: Props = $props();

  // SSoT-resolved values (Tauri only). Browser mode falls through to
  // the URL/cache_dir already in the error payload (if any) — both are
  // also synthesised by the Rust side from the same SSoT helper, so
  // they're identical-by-construction.
  let releaseUrl = $state<string | null>(null);
  let cacheRoot = $state<string | null>(null);
  let tarballName = $state<string | null>(null);

  onMount(async () => {
    [releaseUrl, cacheRoot, tarballName] = await Promise.all([
      getReleaseUrl(),
      getCacheRootPattern(),
      getTarballFilename(),
    ]);
  });

  const desktop = $derived(isTauri());

  const triedUrl = $derived(
    error.kind === "FetchError" &&
      (error.variant === "HttpStatus" || error.variant === "Network")
      ? error.url
      : (releaseUrl ?? ""),
  );
  const cacheDirDisplay = $derived(
    error.kind === "FetchError" && "cache_dir" in error ? error.cache_dir : (cacheRoot ?? ""),
  );

  let busy = $state(false);
  let installError = $state<string | null>(null);

  async function onOpenUrl() {
    if (!triedUrl) return;
    await openExternalUrl(triedUrl);
  }

  async function onInstallLocal() {
    if (!desktop) return;
    busy = true;
    installError = null;
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const filterName = tarballName ? `Tarball (${tarballName})` : "Tarball";
      const path = await open({
        multiple: false,
        directory: false,
        filters: [
          { name: filterName, extensions: ["zst", "tar.zst"] },
          { name: "All files", extensions: ["*"] },
        ],
      });
      if (typeof path !== "string" || !path) return;

      const { invoke } = await import("@tauri-apps/api/core");
      await invoke<string>("install_from_local_tarball", { path });
      // On success, kick the parent to retry the init flow — the
      // cache is now populated and the next ensure_data short-circuits.
      onretry();
    } catch (e) {
      installError = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  function onUseBundled() {
    onuselimited?.();
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
    {#if installError}
      <p class="install-error">Install failed: <code>{installError}</code></p>
    {/if}
  </section>

  <footer class="actions">
    <button type="button" class="primary" onclick={onretry} disabled={busy}>
      Retry
    </button>
    {#if triedUrl}
      <button type="button" onclick={onOpenUrl} disabled={busy}>
        Open URL in browser
      </button>
    {/if}
    {#if desktop}
      <button type="button" onclick={onInstallLocal} disabled={busy}>
        Install from local tarball…
      </button>
      {#if onuselimited}
        <button type="button" onclick={onUseBundled} disabled={busy}>
          Use bundled data only
        </button>
      {/if}
    {/if}
  </footer>

  {#if tarballName}
    <p class="cli-hint">
      Or from a terminal: <code>hyrr fetch-data --offline-bundle path/to/{tarballName}</code>
    </p>
  {/if}
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
  .install-error {
    color: var(--c-red, #d23f3f);
    margin: 0.4rem 0 0;
    font-size: 0.85rem;
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

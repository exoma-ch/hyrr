<script lang="ts">
  import type { PendingUpdate } from "../updater";

  interface Props {
    update: PendingUpdate;
    ondismiss: () => void;
  }
  let { update, ondismiss }: Props = $props();

  let installing = $state(false);
  let installError = $state("");

  async function install() {
    installing = true;
    installError = "";
    try {
      await update.install();
      // App will relaunch — code below this point is unreachable on success.
    } catch (e) {
      installing = false;
      installError = e instanceof Error ? e.message : String(e);
    }
  }
</script>

<div class="update-overlay" role="dialog" aria-modal="true" aria-labelledby="update-title">
  <div class="update-card">
    <h2 id="update-title">Update available</h2>
    <p class="version-line">
      <span class="version-current">v{update.currentVersion}</span>
      <span class="arrow">→</span>
      <span class="version-new">v{update.version}</span>
    </p>

    {#if update.body}
      <div class="release-notes">
        <h3>Release notes</h3>
        <pre>{update.body}</pre>
      </div>
    {/if}

    {#if installError}
      <p class="install-error" role="alert">Install failed: {installError}</p>
    {/if}

    <div class="actions">
      <button
        type="button"
        class="btn-primary"
        onclick={install}
        disabled={installing}
      >
        {#if installing}<span class="spinner"></span> Installing…{:else}Install now{/if}
      </button>
      <button
        type="button"
        class="btn-secondary"
        onclick={ondismiss}
        disabled={installing}
      >
        Remind me later
      </button>
    </div>
  </div>
</div>

<style>
  .update-overlay {
    position: fixed;
    inset: 0;
    z-index: 1000;
    background: rgba(0, 0, 0, 0.5);
    display: grid;
    place-items: center;
    padding: 1rem;
  }

  .update-card {
    max-width: 28rem;
    width: 100%;
    background: var(--c-bg, #1a1a2e);
    border: 1px solid var(--c-border, #333);
    border-radius: 6px;
    padding: 1.5rem;
    color: var(--c-text, #e0e0e0);
    box-shadow: 0 10px 40px rgba(0, 0, 0, 0.3);
  }

  h2 {
    margin: 0 0 0.5rem;
    font-size: 1.15rem;
  }

  .version-line {
    margin: 0 0 1rem;
    font-family: var(--f-mono, ui-monospace, SFMono-Regular, monospace);
    font-size: 0.95rem;
  }

  .version-current {
    color: var(--c-text-muted);
  }

  .arrow {
    margin: 0 0.4rem;
    color: var(--c-text-muted);
  }

  .version-new {
    color: var(--c-accent);
    font-weight: 600;
  }

  .release-notes {
    margin: 0 0 1rem;
    max-height: 12rem;
    overflow-y: auto;
    border: 1px solid var(--c-border);
    border-radius: 4px;
    background: var(--c-bg-muted);
    padding: 0.5rem 0.75rem;
  }

  .release-notes h3 {
    margin: 0 0 0.25rem;
    font-size: 0.8rem;
    font-weight: 600;
    color: var(--c-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .release-notes pre {
    margin: 0;
    font-family: inherit;
    font-size: 0.85rem;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .install-error {
    margin: 0 0 1rem;
    padding: 0.5rem 0.75rem;
    background: rgba(255, 0, 0, 0.08);
    border: 1px solid var(--c-red, #c00);
    border-radius: 4px;
    color: var(--c-red, #c00);
    font-size: 0.9rem;
  }

  .actions {
    display: flex;
    gap: 0.5rem;
    justify-content: flex-end;
  }

  button {
    padding: 0.5rem 1rem;
    border-radius: 3px;
    cursor: pointer;
    font-size: 0.9rem;
  }

  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .spinner {
    display: inline-block;
    width: 0.85em;
    height: 0.85em;
    border: 2px solid currentColor;
    border-right-color: transparent;
    border-radius: 50%;
    animation: spin 0.7s linear infinite;
    vertical-align: -0.1em;
    margin-right: 0.3em;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .btn-primary {
    background: var(--c-accent);
    color: var(--c-accent-fg, white);
    border: 1px solid var(--c-accent);
  }

  .btn-secondary {
    background: var(--c-bg-muted, #252540);
    color: var(--c-text, #e0e0e0);
    border: 1px solid var(--c-border, #333);
  }
</style>

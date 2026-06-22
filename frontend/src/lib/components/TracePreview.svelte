<script lang="ts">
  /**
   * Literal trace preview (#159). Renders the EXACT JSON payload that will be
   * submitted — not a summary — so what the user sees is what gets sent. Shared by
   * the bug-report modal (with the attach checkbox) and the recovery cards
   * (read-only, `showAttach={false}`).
   */
  interface Props {
    /** The literal merged-trace JSON (already pretty-printed). */
    payload: string;
    /** Whether the trace is attached to the report (bindable). */
    attach?: boolean;
    /** Called when the checkbox toggles. */
    onAttachChange?: (value: boolean) => void;
    /** Show the attach checkbox (modal) vs a read-only preview (recovery card). */
    showAttach?: boolean;
  }
  let { payload, attach = false, onAttachChange, showAttach = true }: Props = $props();

  const eventCount = $derived.by(() => {
    if (!payload) return 0;
    try {
      return (JSON.parse(payload).events ?? []).length;
    } catch {
      return 0;
    }
  });
</script>

{#if payload}
  <div class="trace-preview">
    {#if showAttach}
      <label class="attach">
        <input
          type="checkbox"
          checked={attach}
          onchange={(e) => onAttachChange?.((e.currentTarget as HTMLInputElement).checked)}
        />
        <span>Attach diagnostic trace ({eventCount} events)</span>
      </label>
    {/if}
    <pre class="payload">{payload}</pre>
  </div>
{/if}

<style>
  .trace-preview {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }
  .attach {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.85rem;
    cursor: pointer;
  }
  .payload {
    max-height: 14rem;
    overflow: auto;
    margin: 0;
    padding: 0.5rem 0.6rem;
    font-family: var(--font-mono, ui-monospace, monospace);
    font-size: 0.72rem;
    line-height: 1.35;
    background: var(--bg-subtle, rgba(0, 0, 0, 0.04));
    border: 1px solid var(--border, rgba(0, 0, 0, 0.12));
    border-radius: 6px;
    white-space: pre;
  }
</style>

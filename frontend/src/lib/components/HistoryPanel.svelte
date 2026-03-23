<script lang="ts">
  import { onMount } from "svelte";
  import type { HistoryEntry } from "../types";
  import { getAllRuns, deleteRun, updateLabel, clearHistory } from "../history-db";
  import { setConfig } from "../stores/config.svelte";

  interface Props {
    onrestore?: () => void;
  }

  let { onrestore }: Props = $props();

  let entries = $state<HistoryEntry[]>([]);
  let editingId = $state<number | null>(null);
  let editLabel = $state("");
  let confirmClear = $state(false);

  async function refresh() {
    entries = await getAllRuns();
  }

  onMount(refresh);

  function restore(entry: HistoryEntry) {
    setConfig(entry.config);
    onrestore?.();
  }

  async function remove(id: number | undefined) {
    if (id === undefined) return;
    await deleteRun(id);
    await refresh();
  }

  function startEdit(entry: HistoryEntry) {
    editingId = entry.id ?? null;
    editLabel = entry.label;
  }

  async function saveEdit() {
    if (editingId !== null) {
      await updateLabel(editingId, editLabel);
      editingId = null;
      await refresh();
    }
  }

  function cancelEdit() {
    editingId = null;
  }

  function formatDate(ts: number): string {
    return new Date(ts).toLocaleString(undefined, {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  }
</script>

<div class="history-panel">
  <div class="history-header">
    <button class="refresh-btn" onclick={refresh} title="Refresh">↻</button>
    {#if entries.length > 0}
      {#if confirmClear}
        <span class="confirm-msg">Clear all?
          <button class="confirm-yes" onclick={async () => { await clearHistory(); confirmClear = false; await refresh(); }}>Yes</button>
          <button class="confirm-no" onclick={() => confirmClear = false}>No</button>
        </span>
      {:else}
        <button class="clear-btn" onclick={() => confirmClear = true} title="Clear all history">Clear all</button>
      {/if}
    {/if}
  </div>

  {#if entries.length === 0}
    <p class="empty">No runs saved yet.</p>
  {:else}
    <ul class="history-list">
      {#each entries as entry}
        <li class="history-item">
          {#if editingId === entry.id}
            <div class="edit-row">
              <input
                type="text"
                bind:value={editLabel}
                class="edit-input"
                onkeydown={(e) => e.key === "Enter" && saveEdit()}
              />
              <button class="save-btn" onclick={saveEdit}>✓</button>
              <button class="cancel-btn" onclick={cancelEdit}>✕</button>
            </div>
          {:else}
            <div class="item-content">
              <button class="label-btn" onclick={() => restore(entry)}>
                <span class="label">{entry.label}</span>
                <span class="timestamp">{formatDate(entry.timestamp)}</span>
              </button>
              <div class="item-actions">
                <button
                  class="action-btn"
                  onclick={() => startEdit(entry)}
                  title="Rename"
                >✎</button>
                <button
                  class="action-btn delete"
                  onclick={() => remove(entry.id)}
                  title="Delete"
                >×</button>
              </div>
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .history-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .refresh-btn {
    background: none;
    border: none;
    color: var(--c-text-muted);
    font-size: 1rem;
    cursor: pointer;
  }

  .refresh-btn:hover {
    color: var(--c-accent);
  }

  .clear-btn {
    background: none;
    border: 1px solid var(--c-border);
    border-radius: 3px;
    color: var(--c-text-muted);
    font-size: 0.7rem;
    padding: 0.15rem 0.4rem;
    cursor: pointer;
  }

  .clear-btn:hover {
    color: var(--c-red);
    border-color: var(--c-red);
  }

  .confirm-msg {
    font-size: 0.75rem;
    color: var(--c-red);
    display: flex;
    align-items: center;
    gap: 0.3rem;
  }

  .confirm-yes, .confirm-no {
    background: none;
    border: 1px solid var(--c-border);
    border-radius: 3px;
    font-size: 0.7rem;
    padding: 0.1rem 0.35rem;
    cursor: pointer;
  }

  .confirm-yes {
    color: var(--c-red);
    border-color: var(--c-red);
  }

  .confirm-yes:hover {
    background: var(--c-red-tint-subtle);
  }

  .confirm-no {
    color: var(--c-text-muted);
  }

  .confirm-no:hover {
    color: var(--c-text);
    border-color: var(--c-text-faint);
  }

  .empty {
    color: var(--c-text-faint);
    font-style: italic;
    font-size: 0.8rem;
    margin: 0;
  }

  .history-panel {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .history-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  .history-list::-webkit-scrollbar {
    width: 6px;
  }

  .history-list::-webkit-scrollbar-track {
    background: var(--c-bg-default);
    border-radius: 3px;
  }

  .history-list::-webkit-scrollbar-thumb {
    background: var(--c-border);
    border-radius: 3px;
  }

  .history-list::-webkit-scrollbar-thumb:hover {
    background: var(--c-text-faint);
  }

  .history-item {
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    padding: 0.4rem;
  }

  .item-content {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.3rem;
  }

  .label-btn {
    flex: 1;
    text-align: left;
    background: none;
    border: none;
    color: var(--c-text);
    cursor: pointer;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
  }

  .label-btn:hover .label {
    color: var(--c-accent);
  }

  .label {
    font-size: 0.8rem;
  }

  .timestamp {
    font-size: 0.65rem;
    color: var(--c-text-muted);
  }

  .item-actions {
    display: flex;
    gap: 0.15rem;
    flex-shrink: 0;
  }

  .action-btn {
    background: none;
    border: none;
    color: var(--c-text-muted);
    font-size: 0.8rem;
    cursor: pointer;
    padding: 0.1rem 0.2rem;
  }

  .action-btn:hover {
    color: var(--c-accent);
  }

  .action-btn.delete:hover {
    color: var(--c-red);
  }

  .edit-row {
    display: flex;
    gap: 0.2rem;
  }

  .edit-input {
    flex: 1;
    background: var(--c-bg-subtle);
    border: 1px solid var(--c-accent);
    border-radius: 3px;
    color: var(--c-text);
    padding: 0.2rem 0.3rem;
    font-size: 0.8rem;
  }

  .save-btn,
  .cancel-btn {
    background: none;
    border: 1px solid var(--c-border);
    border-radius: 3px;
    color: var(--c-text-muted);
    cursor: pointer;
    font-size: 0.75rem;
    padding: 0.1rem 0.3rem;
  }

  .save-btn:hover {
    color: var(--c-green-text);
    border-color: var(--c-green-text);
  }

  .cancel-btn:hover {
    color: var(--c-red);
    border-color: var(--c-red);
  }
</style>

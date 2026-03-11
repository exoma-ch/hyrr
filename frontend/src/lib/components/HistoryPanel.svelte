<script lang="ts">
  import { onMount } from "svelte";
  import type { HistoryEntry } from "../types";
  import { getAllRuns, deleteRun, updateLabel } from "../history-db";
  import { setConfig } from "../stores/config.svelte";

  let entries = $state<HistoryEntry[]>([]);
  let editingId = $state<number | null>(null);
  let editLabel = $state("");

  async function refresh() {
    entries = await getAllRuns();
  }

  onMount(refresh);

  function restore(entry: HistoryEntry) {
    setConfig(entry.config);
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
    <h3>History</h3>
    <button class="refresh-btn" onclick={refresh}>↻</button>
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
  .history-panel {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .history-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  h3 {
    margin: 0;
    font-size: 0.85rem;
    color: #8b949e;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .refresh-btn {
    background: none;
    border: none;
    color: #8b949e;
    font-size: 1rem;
    cursor: pointer;
  }

  .refresh-btn:hover {
    color: #58a6ff;
  }

  .empty {
    color: #484f58;
    font-style: italic;
    font-size: 0.8rem;
    margin: 0;
  }

  .history-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    max-height: 300px;
    overflow-y: auto;
  }

  .history-item {
    background: #0d1117;
    border: 1px solid #2d333b;
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
    color: #e1e4e8;
    cursor: pointer;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
  }

  .label-btn:hover .label {
    color: #58a6ff;
  }

  .label {
    font-size: 0.8rem;
  }

  .timestamp {
    font-size: 0.65rem;
    color: #8b949e;
  }

  .item-actions {
    display: flex;
    gap: 0.15rem;
    flex-shrink: 0;
  }

  .action-btn {
    background: none;
    border: none;
    color: #8b949e;
    font-size: 0.8rem;
    cursor: pointer;
    padding: 0.1rem 0.2rem;
  }

  .action-btn:hover {
    color: #58a6ff;
  }

  .action-btn.delete:hover {
    color: #f85149;
  }

  .edit-row {
    display: flex;
    gap: 0.2rem;
  }

  .edit-input {
    flex: 1;
    background: #161b22;
    border: 1px solid #58a6ff;
    border-radius: 3px;
    color: #e1e4e8;
    padding: 0.2rem 0.3rem;
    font-size: 0.8rem;
  }

  .save-btn,
  .cancel-btn {
    background: none;
    border: 1px solid #2d333b;
    border-radius: 3px;
    color: #8b949e;
    cursor: pointer;
    font-size: 0.75rem;
    padding: 0.1rem 0.3rem;
  }

  .save-btn:hover {
    color: #7ee787;
    border-color: #7ee787;
  }

  .cancel-btn:hover {
    color: #f85149;
    border-color: #f85149;
  }
</style>

<script lang="ts">
  import { exportHistory, importHistory } from "../history-db";

  let importCount = $state<number | null>(null);

  async function handleExport() {
    const json = await exportHistory();
    const blob = new Blob([json], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "hyrr-history.json";
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  }

  function handleImportClick() {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".json";
    input.onchange = async () => {
      const file = input.files?.[0];
      if (!file) return;
      const text = await file.text();
      const count = await importHistory(text);
      importCount = count;
      setTimeout(() => (importCount = null), 3000);
    };
    input.click();
  }
</script>

<div class="import-export">
  <button class="ie-btn" onclick={handleExport}>Export History</button>
  <button class="ie-btn" onclick={handleImportClick}>Import History</button>
  {#if importCount !== null}
    <span class="import-msg">Imported {importCount} runs</span>
  {/if}
</div>

<style>
  .import-export {
    display: flex;
    gap: 0.4rem;
    align-items: center;
    flex-wrap: wrap;
  }

  .ie-btn {
    padding: 0.3rem 0.6rem;
    background: #0d1117;
    border: 1px solid #2d333b;
    border-radius: 4px;
    color: #8b949e;
    font-size: 0.75rem;
    cursor: pointer;
  }

  .ie-btn:hover {
    border-color: #58a6ff;
    color: #e1e4e8;
  }

  .import-msg {
    font-size: 0.7rem;
    color: #7ee787;
  }
</style>

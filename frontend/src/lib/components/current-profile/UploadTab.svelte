<script lang="ts">
  import { parseCurrentProfileCSV, type ParseResult, type ParseError } from "@hyrr/compute";
  import type { CurrentProfile } from "@hyrr/compute";
  import ProfileEditor from "./ProfileEditor.svelte";

  interface Props {
    onselect: (profile: CurrentProfile) => void;
  }

  let { onselect }: Props = $props();

  let parsedProfile = $state<CurrentProfile | null>(null);
  let uploadError = $state<string | null>(null);
  let parseWarnings = $state<string[]>([]);
  let rawText = $state<string | null>(null);
  let needsDt = $state(false);
  let dtInput = $state(1);
  let showAllWarnings = $state(false);

  function applyResult(result: ParseResult | ParseError) {
    if ("error" in result) {
      const err = (result as ParseError).error;
      if (err.includes("time step")) {
        needsDt = true;
        uploadError = null;
      } else {
        uploadError = err;
        needsDt = false;
      }
      parsedProfile = null;
      parseWarnings = [];
      return;
    }
    const parsed = result as ParseResult;
    parsedProfile = parsed.profile;
    parseWarnings = parsed.warnings;
    uploadError = null;
    needsDt = false;
  }

  function handleUpload(e: Event) {
    const input = e.target as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;
    uploadError = null;
    const reader = new FileReader();
    reader.onload = () => {
      rawText = reader.result as string;
      applyResult(parseCurrentProfileCSV(rawText));
    };
    reader.readAsText(file);
  }

  async function handlePaste() {
    try {
      const text = await navigator.clipboard.readText();
      if (!text.trim()) { uploadError = "Clipboard is empty"; return; }
      if (text.split(/\r?\n/).filter(l => l.trim()).length > 10_000) { uploadError = "Exceeds 10,000 rows"; return; }
      rawText = text;
      applyResult(parseCurrentProfileCSV(text));
    } catch { uploadError = "Could not read clipboard"; }
  }

  function reparseWithDt() {
    if (!rawText || dtInput <= 0) return;
    applyResult(parseCurrentProfileCSV(rawText, dtInput));
  }

  function clear() {
    parsedProfile = null;
    uploadError = null;
    parseWarnings = [];
    rawText = null;
    needsDt = false;
  }
</script>

<div class="upload-tab">
  {#if !parsedProfile}
    <div class="upload-area">
      <div class="upload-row">
        <label class="file-btn">
          Choose file
          <input type="file" accept=".csv,.tsv,.txt" onchange={handleUpload} />
        </label>
        <button class="paste-btn" onclick={handlePaste}>Paste</button>
      </div>
      <span class="hint">Two columns: time_s, current_mA (header optional). Or single column of current values with a time step.</span>
    </div>

    {#if needsDt}
      <div class="dt-row">
        <label class="dt-label">
          Single-column detected — time step:
          <input type="number" class="dt-input" bind:value={dtInput} min="0.001" step="0.1" />
          <span class="dt-unit">s</span>
        </label>
        <button class="dt-apply" onclick={reparseWithDt}>Apply</button>
      </div>
    {/if}

    {#if uploadError}
      <span class="error">{uploadError}</span>
    {/if}
  {:else}
    <div class="preview">
      <ProfileEditor profile={parsedProfile} onchange={(p) => { parsedProfile = p; }} />
      {#if parseWarnings.length > 0}
        <div class="warnings">
          {#each parseWarnings.slice(0, showAllWarnings ? undefined : 3) as w}
            <span class="warning">{w}</span>
          {/each}
          {#if parseWarnings.length > 3}
            <button class="warnings-toggle" onclick={() => showAllWarnings = !showAllWarnings}>
              {showAllWarnings ? "Show less" : `+${parseWarnings.length - 3} more`}
            </button>
          {/if}
        </div>
      {/if}
      <div class="actions">
        <button class="clear-btn" onclick={clear}>Choose different</button>
        <button class="confirm-btn" onclick={() => onselect(parsedProfile!)}>Use this profile</button>
      </div>
    </div>
  {/if}
</div>

<style>
  .upload-tab { display: flex; flex-direction: column; gap: 0.5rem; }

  .upload-area { display: flex; flex-direction: column; gap: 0.3rem; }
  .upload-row { display: flex; gap: 0.4rem; align-items: center; }

  .file-btn {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text-muted);
    padding: 0.35rem 0.6rem;
    font-size: 0.8rem;
    cursor: pointer;
  }
  .file-btn:hover { border-color: var(--c-accent); color: var(--c-text); }
  .file-btn input { display: none; }

  .paste-btn {
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text-muted);
    padding: 0.35rem 0.6rem;
    font-size: 0.8rem;
    cursor: pointer;
  }
  .paste-btn:hover { border-color: var(--c-accent); color: var(--c-text); }

  .hint { font-size: 0.7rem; color: var(--c-text-subtle); }

  .dt-row {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.4rem;
    background: var(--c-bg-active);
    border-radius: 4px;
    font-size: 0.8rem;
  }
  .dt-label { display: flex; align-items: center; gap: 0.3rem; color: var(--c-text-muted); }
  .dt-input {
    width: 60px;
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 3px;
    color: var(--c-text);
    padding: 0.2rem 0.3rem;
    font-size: 0.8rem;
    text-align: right;
  }
  .dt-unit { font-size: 0.7rem; color: var(--c-text-muted); }
  .dt-apply {
    background: var(--c-accent-tint);
    border: 1px solid var(--c-accent);
    border-radius: 4px;
    color: var(--c-accent);
    padding: 0.2rem 0.5rem;
    font-size: 0.75rem;
    cursor: pointer;
  }

  .error { font-size: 0.75rem; color: var(--c-red); }

  .preview { display: flex; flex-direction: column; gap: 0.4rem; }
  .stats { font-size: 0.75rem; color: var(--c-text-muted); }

  .warnings { display: flex; flex-direction: column; gap: 0.15rem; }
  .warning { font-size: 0.7rem; color: var(--c-orange); }
  .warnings-toggle {
    background: none; border: none; color: var(--c-text-subtle);
    font-size: 0.65rem; cursor: pointer; padding: 0; text-align: left;
  }

  .actions { display: flex; gap: 0.4rem; justify-content: flex-end; margin-top: 0.3rem; }
  .clear-btn {
    background: none;
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text-muted);
    padding: 0.3rem 0.6rem;
    font-size: 0.8rem;
    cursor: pointer;
  }
  .clear-btn:hover { border-color: var(--c-red); color: var(--c-red); }
  .confirm-btn {
    background: var(--c-accent-tint);
    border: 1px solid var(--c-accent);
    border-radius: 4px;
    color: var(--c-accent);
    padding: 0.3rem 0.8rem;
    font-size: 0.8rem;
    font-weight: 600;
    cursor: pointer;
  }
  .confirm-btn:hover { background: var(--c-accent); color: var(--c-bg-default); }
</style>

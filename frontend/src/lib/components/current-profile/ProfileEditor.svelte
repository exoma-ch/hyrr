<script lang="ts">
  import type { CurrentProfile } from "@hyrr/compute";
  import { cropProfile, editProfilePoint, deleteProfilePoint, profileStats } from "@hyrr/compute";
  import PlotCurrentProfile from "../PlotCurrentProfile.svelte";

  interface Props {
    profile: CurrentProfile;
    onchange: (p: CurrentProfile) => void;
  }

  let { profile, onchange }: Props = $props();

  // --- Trim window (transient, drag-to-set) ---
  let durationS = $derived(profile.timesS[profile.timesS.length - 1] ?? 0);
  let trimStartS = $state(0);
  let trimEndS = $state(0);
  let trimDirty = $state(false);

  // Initialize / reset trim window when the profile identity changes.
  let lastProfileRef: CurrentProfile | null = null;
  $effect(() => {
    if (profile !== lastProfileRef) {
      lastProfileRef = profile;
      trimStartS = 0;
      trimEndS = profile.timesS[profile.timesS.length - 1] ?? 0;
      trimDirty = false;
    }
  });

  function onTrim(startS: number, endS: number) {
    trimStartS = startS;
    trimEndS = endS;
    trimDirty = startS > 0 || endS < durationS;
  }

  function applyTrim() {
    if (!trimDirty) return;
    onchange(cropProfile(profile, trimStartS, trimEndS));
  }

  function resetTrim() {
    trimStartS = 0;
    trimEndS = durationS;
    trimDirty = false;
  }

  // --- Editable table ---
  let showTable = $state(false);
  const ROW_H = 26; // px
  const VIEWPORT_H = 220; // px
  const BUFFER = 6;
  let scrollTop = $state(0);

  let nPoints = $derived(profile.timesS.length);
  let virtualize = $derived(nPoints > 50);

  let visibleRange = $derived.by(() => {
    if (!virtualize) return { start: 0, end: nPoints };
    const start = Math.max(0, Math.floor(scrollTop / ROW_H) - BUFFER);
    const end = Math.min(nPoints, start + Math.ceil(VIEWPORT_H / ROW_H) + 2 * BUFFER);
    return { start, end };
  });

  function onScroll(e: Event) {
    scrollTop = (e.target as HTMLElement).scrollTop;
  }

  function fmtTime(s: number): string {
    if (s >= 3600) return `${(s / 3600).toFixed(3)} h`;
    if (s >= 60) return `${(s / 60).toFixed(2)} min`;
    return `${s.toFixed(1)} s`;
  }

  function onPointEdit(i: number, e: Event) {
    const v = parseFloat((e.target as HTMLInputElement).value);
    if (!isNaN(v) && v >= 0) onchange(editProfilePoint(profile, i, v / 1000)); // µA → mA
  }

  function onPointDelete(i: number) {
    onchange(deleteProfilePoint(profile, i));
  }

  let stats = $derived(profileStats(profile));
</script>

<div class="profile-editor">
  <PlotCurrentProfile {profile} trimmable {trimStartS} {trimEndS} ontrim={onTrim} />

  <div class="editor-bar">
    <div class="trim-controls">
      {#if trimDirty}
        <span class="trim-info">Trim → {fmtTime(trimStartS)} – {fmtTime(trimEndS)}</span>
        <button class="mini-btn apply" onclick={applyTrim}>Apply trim</button>
        <button class="mini-btn" onclick={resetTrim}>Reset</button>
      {:else}
        <span class="trim-hint">Drag the handles to crop the time range</span>
      {/if}
    </div>
    <button class="mini-btn" onclick={() => showTable = !showTable}>
      {showTable ? "Hide points" : `Edit points (${nPoints})`}
    </button>
  </div>

  {#if showTable}
    <div class="point-table" onscroll={onScroll} style="max-height: {VIEWPORT_H}px;">
      <div class="table-inner" style="height: {virtualize ? (nPoints + 1) * ROW_H : 'auto'}px; position: {virtualize ? 'relative' : 'static'};">
        <div class="thead" style={virtualize ? 'position: sticky; top: 0; z-index: 1;' : ''}>
          <span class="th-idx">#</span>
          <span class="th-time">TIME</span>
          <span class="th-cur">CURRENT (µA)</span>
          <span class="th-del"></span>
        </div>
        {#each Array.from({ length: visibleRange.end - visibleRange.start }) as _, k}
          {@const i = visibleRange.start + k}
          <div
            class="trow"
            style={virtualize ? `position: absolute; top: ${i * ROW_H + ROW_H}px; left: 0; right: 0; height: ${ROW_H}px;` : ''}
          >
            <span class="td-idx">{i + 1}</span>
            <span class="td-time">{fmtTime(profile.timesS[i])}</span>
            <input
              class="td-cur"
              type="text"
              inputmode="decimal"
              value={(profile.currentsMA[i] * 1000).toFixed(2)}
              onchange={(e) => onPointEdit(i, e)}
            />
            <button class="td-del" onclick={() => onPointDelete(i)} disabled={nPoints <= 2} title="Delete point">×</button>
          </div>
        {/each}
      </div>
    </div>
  {/if}

  <div class="stats">
    {stats.n} pts · {(stats.durationS / 60).toFixed(1)} min · {stats.minCurrentUA.toFixed(0)}–{stats.maxCurrentUA.toFixed(0)} µA · {stats.chargeUAh.toFixed(2)} µAh
  </div>
</div>

<style>
  .profile-editor { display: flex; flex-direction: column; gap: 0.4rem; }

  .editor-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    flex-wrap: wrap;
  }

  .trim-controls { display: flex; align-items: center; gap: 0.4rem; flex-wrap: wrap; }
  .trim-hint { font-size: 0.7rem; color: var(--c-text-subtle); }
  .trim-info { font-size: 0.7rem; color: var(--c-accent); }

  .mini-btn {
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text-muted);
    padding: 0.2rem 0.5rem;
    font-size: 0.7rem;
    cursor: pointer;
  }
  .mini-btn:hover { border-color: var(--c-accent); color: var(--c-text); }
  .mini-btn.apply { border-color: var(--c-accent); color: var(--c-accent); font-weight: 600; }

  .point-table {
    overflow-y: auto;
    border: 1px solid var(--c-border);
    border-radius: 4px;
    background: var(--c-bg-subtle);
    font-size: 0.72rem;
  }

  .thead, .trow {
    display: grid;
    grid-template-columns: 2.5rem 1fr 1fr 1.5rem;
    align-items: center;
    gap: 0.3rem;
    padding: 0 0.4rem;
    box-sizing: border-box;
  }

  .thead {
    background: var(--c-bg-active);
    color: var(--c-text-muted);
    font-size: 0.62rem;
    letter-spacing: 0.04em;
    height: 26px;
  }

  .trow { height: 26px; border-top: 1px solid var(--c-bg-hover); }
  .td-idx, .th-idx { color: var(--c-text-subtle); text-align: right; }
  .td-time, .th-time { color: var(--c-text-muted); }
  .th-cur { text-align: right; }

  .td-cur {
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 3px;
    color: var(--c-text);
    padding: 0.1rem 0.3rem;
    font-size: 0.72rem;
    text-align: right;
    width: 100%;
    box-sizing: border-box;
  }
  .td-cur:focus { outline: none; border-color: var(--c-accent); }

  .td-del {
    background: none;
    border: none;
    color: var(--c-text-subtle);
    cursor: pointer;
    font-size: 0.85rem;
    line-height: 1;
    padding: 0;
  }
  .td-del:hover:not(:disabled) { color: var(--c-red); }
  .td-del:disabled { opacity: 0.3; cursor: not-allowed; }

  .stats { font-size: 0.75rem; color: var(--c-text-muted); }
</style>

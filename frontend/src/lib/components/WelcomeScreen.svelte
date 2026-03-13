<script lang="ts">
  import { setConfig } from "../stores/config.svelte";
  import { PRESETS } from "../presets";

  interface Props {
    onstart?: () => void;
  }

  let { onstart }: Props = $props();

  function loadPreset(idx: number) {
    setConfig({ ...PRESETS[idx].config });
    onstart?.();
  }

  function feelingLucky() {
    const idx = Math.floor(Math.random() * PRESETS.length);
    setConfig({ ...PRESETS[idx].config });
    onstart?.();
  }
</script>

<div class="welcome">
  <img src="/hyrr/logo.svg" alt="HYRR logo" class="hero-logo" />
  <h1 class="hero-title">HYRR</h1>
  <p class="hero-sub">Hierarchical Yield &amp; Radionuclide Rates</p>

  <div class="getting-started">
    <h2>Getting started</h2>
    <div class="steps">
      <div class="step">
        <span class="step-num">1</span>
        <div>
          <strong>Configure beam</strong>
          <p>Choose projectile, energy, and current in the top bar</p>
        </div>
      </div>
      <div class="step">
        <span class="step-num">2</span>
        <div>
          <strong>Build target stack</strong>
          <p>Click <kbd>+ Add layer</kbd> to add materials — set thickness or exit energy</p>
        </div>
      </div>
      <div class="step">
        <span class="step-num">3</span>
        <div>
          <strong>Explore results</strong>
          <p>Depth profile, activity curves, and isotope details update live</p>
        </div>
      </div>
    </div>
  </div>

  <div class="quick-start">
    <h2>Quick start with a preset</h2>
    <div class="preset-grid">
      {#each PRESETS as preset, i}
        <button class="preset-card" onclick={() => loadPreset(i)}>
          <span class="preset-name">{preset.name}</span>
          <span class="preset-desc">{preset.description}</span>
        </button>
      {/each}
    </div>
    <button class="lucky-btn" onclick={feelingLucky}>
      Feeling Lucky
    </button>
  </div>
</div>

<style>
  .welcome {
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 2rem 1rem 4rem;
    max-width: 640px;
    margin: 0 auto;
  }

  .hero-logo {
    width: 120px;
    height: 120px;
    object-fit: contain;
    margin-bottom: 0.5rem;
    opacity: 0.9;
  }

  .hero-title {
    margin: 0;
    font-size: 2.4rem;
    letter-spacing: 0.15em;
    color: #58a6ff;
    line-height: 1;
  }

  .hero-sub {
    margin: 0.4rem 0 0;
    font-size: 0.85rem;
    color: #6e7681;
    letter-spacing: 0.02em;
  }

  .getting-started {
    width: 100%;
    margin-top: 2.5rem;
  }

  .getting-started h2 {
    font-size: 0.85rem;
    color: #8b949e;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    margin: 0 0 0.75rem;
  }

  .steps {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }

  .step {
    display: flex;
    align-items: flex-start;
    gap: 0.75rem;
    padding: 0.6rem 0.75rem;
    background: #161b22;
    border: 1px solid #2d333b;
    border-radius: 3px;
  }

  .step-num {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border-radius: 50%;
    background: #21262d;
    color: #58a6ff;
    font-size: 0.75rem;
    font-weight: 700;
    flex-shrink: 0;
    margin-top: 0.1rem;
  }

  .step strong {
    color: #e1e4e8;
    font-size: 0.8rem;
  }

  .step p {
    margin: 0.15rem 0 0;
    color: #8b949e;
    font-size: 0.75rem;
  }

  kbd {
    background: #21262d;
    border: 1px solid #2d333b;
    border-radius: 3px;
    padding: 0.05rem 0.3rem;
    font-size: 0.7rem;
    color: #c9d1d9;
    font-family: inherit;
  }

  .quick-start {
    width: 100%;
    margin-top: 2rem;
  }

  .quick-start h2 {
    font-size: 0.85rem;
    color: #8b949e;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    margin: 0 0 0.75rem;
  }

  .preset-grid {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }

  .preset-card {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.15rem;
    padding: 0.55rem 0.75rem;
    background: #161b22;
    border: 1px solid #2d333b;
    border-radius: 3px;
    cursor: pointer;
    text-align: left;
    transition: border-color 0.15s, background 0.15s;
  }

  .preset-card:hover {
    border-color: #58a6ff;
    background: #1c2128;
  }

  .preset-name {
    color: #e1e4e8;
    font-size: 0.8rem;
    font-weight: 600;
  }

  .preset-desc {
    color: #6e7681;
    font-size: 0.7rem;
  }

  .lucky-btn {
    margin-top: 0.75rem;
    width: 100%;
    padding: 0.5rem;
    background: #1c2128;
    border: 1px dashed #d29922;
    border-radius: 3px;
    color: #d29922;
    font-size: 0.8rem;
    font-weight: 500;
    cursor: pointer;
    transition: background 0.15s;
  }

  .lucky-btn:hover {
    background: #21262d;
    color: #e3b341;
  }
</style>

<script lang="ts">
  import type { HistoryEntry } from "../types";

  interface Props {
    entryA: HistoryEntry;
    entryB: HistoryEntry;
  }

  let { entryA, entryB }: Props = $props();

  interface Diff {
    field: string;
    a: string;
    b: string;
  }

  let diffs = $derived.by((): Diff[] => {
    const d: Diff[] = [];
    const ca = entryA.config;
    const cb = entryB.config;

    if (ca.beam.projectile !== cb.beam.projectile) {
      d.push({ field: "Projectile", a: ca.beam.projectile, b: cb.beam.projectile });
    }
    if (ca.beam.energy_MeV !== cb.beam.energy_MeV) {
      d.push({ field: "Energy (MeV)", a: String(ca.beam.energy_MeV), b: String(cb.beam.energy_MeV) });
    }
    if (ca.beam.current_mA !== cb.beam.current_mA) {
      d.push({ field: "Current (mA)", a: String(ca.beam.current_mA), b: String(cb.beam.current_mA) });
    }
    if (ca.layers.length !== cb.layers.length) {
      d.push({ field: "# Layers", a: String(ca.layers.length), b: String(cb.layers.length) });
    }
    if (ca.irradiation_s !== cb.irradiation_s) {
      d.push({ field: "Irradiation (s)", a: String(ca.irradiation_s), b: String(cb.irradiation_s) });
    }
    if (ca.cooling_s !== cb.cooling_s) {
      d.push({ field: "Cooling (s)", a: String(ca.cooling_s), b: String(cb.cooling_s) });
    }

    // Compare layer materials
    const maxLayers = Math.max(ca.layers.length, cb.layers.length);
    for (let i = 0; i < maxLayers; i++) {
      const la = ca.layers[i];
      const lb = cb.layers[i];
      if (!la || !lb) continue;
      if (la.material !== lb.material) {
        d.push({ field: `Layer ${i + 1} material`, a: la.material, b: lb.material });
      }
    }

    return d;
  });
</script>

<div class="compare">
  <div class="compare-header">
    <span class="entry-label">{entryA.label}</span>
    <span class="vs">vs</span>
    <span class="entry-label">{entryB.label}</span>
  </div>

  {#if diffs.length === 0}
    <p class="same">Configs are identical.</p>
  {:else}
    <table>
      <thead>
        <tr>
          <th>Field</th>
          <th>A</th>
          <th>B</th>
        </tr>
      </thead>
      <tbody>
        {#each diffs as diff}
          <tr>
            <td>{diff.field}</td>
            <td class="val-a">{diff.a}</td>
            <td class="val-b">{diff.b}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>

<style>
  .compare {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .compare-header {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.8rem;
  }

  .entry-label {
    color: #58a6ff;
    font-weight: 500;
  }

  .vs {
    color: #484f58;
  }

  .same {
    color: #7ee787;
    font-size: 0.8rem;
    margin: 0;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.8rem;
  }

  th {
    text-align: left;
    padding: 0.3rem;
    border-bottom: 1px solid #2d333b;
    color: #8b949e;
    font-weight: 500;
  }

  td {
    padding: 0.3rem;
    border-bottom: 1px solid #1c2128;
  }

  .val-a {
    color: #f85149;
  }

  .val-b {
    color: #7ee787;
  }
</style>

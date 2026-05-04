<script lang="ts">
  import Modal from "./Modal.svelte";
  import DownloadLinks from "./DownloadLinks.svelte";

  interface Props {
    open: boolean;
    onclose: () => void;
  }

  let { open, onclose }: Props = $props();
</script>

<Modal {open} {onclose} title="Help" wide>
  <div class="help">
    <section>
      <h3>What is HYRR?</h3>
      <p>
        <strong>Hierarchical Yield &amp; Radionuclide Rates</strong> — a browser-based tool
        for predicting radio-isotope production in stacked target assemblies. All computation
        runs locally; no server needed.
      </p>
      <p>
        Inspired by <a href="https://github.com/arjankoning1/isotopia" target="_blank" rel="noopener noreferrer">ISOTOPIA</a>,
        the medical isotope production simulator by Arjan Koning (IAEA Nuclear Data Section).
      </p>
      <p>Need offline access? All nuclear data bundled — works air-gapped.</p>
      <DownloadLinks variant="panel" />
    </section>

    <section>
      <h3>Workflow</h3>
      <ol>
        <li><strong>Beam:</strong> Select projectile (p, d, t, <sup>3</sup>He, &alpha;), energy, and current.</li>
        <li><strong>Target stack:</strong> Add layers with <kbd>+ Add layer</kbd>. Click a layer to change its material,
          set thickness (cm) or exit energy (MeV). Drag to reorder.</li>
        <li><strong>Results:</strong> Depth profile and layer table update live. Activity curves and the isotope table
          appear after computation.</li>
        <li><strong>Isotope details:</strong> Click any isotope in the activity table to see cross-section plots,
          decay info, and compare channels.</li>
      </ol>
    </section>

    <section>
      <h3>Layer configuration</h3>
      <ul>
        <li><strong>Thickness vs. exit energy</strong> — Set one; the other is computed from stopping power. Accepts leading decimals: <code>.2mm</code>, <code>.5µm</code>.</li>
        <li><strong>Enrichment</strong> — Click an element badge on a layer card to override natural isotopic abundances.</li>
        <li><strong>Custom materials</strong> — Enter any chemical formula (e.g. <code>CaO+Al</code>) as a layer material.</li>
        <li><strong>Repeating groups</strong> — Click <kbd>⟳</kbd> to wrap layers into a group. Choose <strong>×N</strong> (repeat N times) or <strong>E&lt;</strong> (repeat until beam energy drops below threshold). Drag layers in or out of groups.</li>
      </ul>
    </section>

    <section>
      <h3>Timing</h3>
      <ul>
        <li><strong>Irradiation</strong> — Duration of beam-on-target. Accepts: <code>30min</code>, <code>24h</code>, <code>7d</code>, <code>1y</code>.</li>
        <li><strong>Cooling</strong> — Time after end-of-bombardment. Activity decays during this period.</li>
      </ul>
    </section>

    <section>
      <h3>Sharing &amp; sessions</h3>
      <ul>
        <li><strong>URL sharing</strong> — The URL hash encodes the full config. Copy &amp; share.</li>
        <li><strong>Session tabs</strong> — Click <kbd>+</kbd> to save the current config as a tab. Switch between tabs.</li>
        <li><strong>History</strong> — Every simulation is saved locally. Open with the clock icon (top-right).</li>
        <li><strong>Feeling Lucky</strong> — Loads a random preset.</li>
      </ul>
    </section>

    <section>
      <h3>Nuclear data</h3>
      <p>
        Cross-sections from <strong>TENDL-2025</strong>, stopping powers from <strong>PSTAR/ASTAR</strong> tabulated data
        with velocity scaling for heavier projectiles (more accurate than Bethe-Bloch, especially at low energies relevant to cyclotron targetry).
        Data is loaded as Parquet files and cached in your browser (IndexedDB).
      </p>
    </section>

    <section>
      <h3>Simulation mode</h3>
      <p>
        The <strong>Auto/Manual</strong> toggle next to beam properties controls when simulations run.
        In Auto mode results update live as you edit; in Manual mode click <kbd>Run</kbd> to trigger.
        The status dot shows: grey = idle, gold = busy, green = ready, red = error.
      </p>
    </section>

    <section>
      <h3>Keyboard shortcuts</h3>
      <div class="shortcuts">
        <div><kbd>Esc</kbd> Close any popup</div>
        <div><kbd>Cmd Z</kbd> Undo</div>
        <div><kbd>Cmd Shift Z</kbd> Redo</div>
      </div>
    </section>
  </div>
</Modal>

<style>
  .help {
    display: flex;
    flex-direction: column;
    gap: 1.25rem;
  }

  section h3 {
    margin: 0 0 0.3rem;
    font-size: 0.85rem;
    color: var(--c-text);
  }

  section p,
  section li {
    font-size: 0.8rem;
    color: var(--c-text-muted);
    line-height: 1.5;
    margin: 0;
  }

  section ol,
  section ul {
    margin: 0;
    padding-left: 1.2rem;
  }

  section li {
    margin-bottom: 0.25rem;
  }

  section li strong {
    color: var(--c-text-label);
  }

  code {
    background: var(--c-bg-muted);
    border-radius: 3px;
    padding: 0.1rem 0.3rem;
    font-size: 0.75rem;
    color: var(--c-text-label);
  }

  kbd {
    background: var(--c-bg-muted);
    border: 1px solid var(--c-border);
    border-radius: 3px;
    padding: 0.05rem 0.3rem;
    font-size: 0.7rem;
    color: var(--c-text-label);
    font-family: inherit;
  }

  .shortcuts {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    font-size: 0.8rem;
    color: var(--c-text-muted);
  }
</style>

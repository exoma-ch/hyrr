<script lang="ts">
  import { parseValueUnit } from "../utils/unit-parse";

  interface Props {
    value: number;
    min?: number;
    max?: number;
    label: string;
    unit?: string;
    units?: string[];
    onchange: (value: number) => void;
    onunitchange?: (unit: string) => void;
  }

  let {
    value,
    min = 0,
    max = Infinity,
    label,
    unit = "",
    units = [],
    onchange,
    onunitchange,
  }: Props = $props();

  let editing = $state(false);
  let text = $state("");

  /** All recognized units for parsing: the current unit + any extra units list. */
  let knownUnits = $derived(
    unit
      ? [...new Set([unit, ...units])]
      : [...units],
  );

  function formatDisplay(v: number): string {
    // Use reasonable precision: avoid floating-point noise
    const s = parseFloat(v.toPrecision(10)).toString();
    return s;
  }

  function onFocus(e: Event) {
    editing = true;
    text = formatDisplay(value);
    requestAnimationFrame(() => (e.target as HTMLInputElement).select());
  }

  function onInput(e: Event) {
    text = (e.target as HTMLInputElement).value;
  }

  function commit() {
    editing = false;
    const parsed = parseValueUnit(text, knownUnits);
    if (parsed) {
      const clamped = Math.min(max, Math.max(min, parsed.value));
      onchange(clamped);
      if (parsed.unit && parsed.unit !== unit && onunitchange) {
        onunitchange(parsed.unit);
      }
    }
    // If parse fails, value stays unchanged (input resets to display)
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      commit();
      (e.target as HTMLInputElement).blur();
    } else if (e.key === "Escape") {
      editing = false;
      (e.target as HTMLInputElement).blur();
    }
  }
</script>

<div class="number-input">
  <label>
    <span class="label-text">{label}</span>
    <div class="input-wrapper">
      <input
        type="text"
        inputmode="decimal"
        value={editing ? text : formatDisplay(value)}
        onfocus={onFocus}
        oninput={onInput}
        onblur={commit}
        onkeydown={onKeydown}
        class="text-input"
      />
      {#if unit && !editing}
        <span class="unit-suffix">{unit}</span>
      {/if}
    </div>
  </label>
</div>

<style>
  .number-input {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    font-size: 0.8rem;
  }

  .label-text {
    color: var(--c-text-label);
  }

  .input-wrapper {
    position: relative;
    display: flex;
    align-items: center;
  }

  .text-input {
    width: 100%;
    box-sizing: border-box;
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text);
    padding: 0.3rem 0.4rem;
    font-size: 0.8rem;
    text-align: right;
  }

  .text-input:focus {
    outline: none;
    border-color: var(--c-accent);
  }

  .unit-suffix {
    position: absolute;
    right: 0.4rem;
    color: var(--c-text-faint);
    font-size: 0.7rem;
    pointer-events: none;
  }

  /* When suffix is shown, add right padding so the number doesn't overlap it */
  .input-wrapper:has(.unit-suffix) .text-input {
    padding-right: 2.5rem;
  }

  @media (max-width: 640px) {
    .text-input {
      font-size: 16px; /* prevent iOS auto-zoom */
      padding: 0.4rem 0.5rem;
    }

    .input-wrapper:has(.unit-suffix) .text-input {
      padding-right: 3rem;
    }
  }
</style>

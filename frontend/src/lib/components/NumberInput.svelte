<script lang="ts">
  interface Props {
    value: number;
    min?: number;
    max?: number;
    step?: number;
    label: string;
    unit?: string;
    onchange: (value: number) => void;
  }

  let { value, min = 0, max = 100, step = 1, label, unit = "", onchange }: Props = $props();

  function handleChange(e: Event) {
    const target = e.target as HTMLInputElement;
    const v = parseFloat(target.value);
    if (!isNaN(v)) {
      const clamped = Math.min(max, Math.max(min, v));
      onchange(clamped);
    }
  }
</script>

<div class="number-input">
  <label>
    <span class="label-text">{label}</span>
    {#if unit}<span class="unit">{unit}</span>{/if}
  </label>
  <input
    type="number"
    {min}
    {max}
    {step}
    {value}
    onchange={handleChange}
    class="text-input"
  />
</div>

<style>
  .number-input {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  label {
    display: flex;
    align-items: baseline;
    gap: 0.4rem;
    font-size: 0.8rem;
  }

  .label-text {
    color: var(--c-text-label);
  }

  .unit {
    color: var(--c-text-muted);
    font-size: 0.75rem;
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
</style>

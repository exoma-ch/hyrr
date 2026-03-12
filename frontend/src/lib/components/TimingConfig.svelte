<script lang="ts">
  import {
    getConfig,
    setIrradiation,
    setCooling,
  } from "../stores/config.svelte";
  import {
    toSeconds,
    fromSeconds,
    bestUnit,
    type TimeUnit,
  } from "../utils/time-convert";

  const UNITS: { value: TimeUnit; label: string }[] = [
    { value: "s", label: "s" },
    { value: "min", label: "min" },
    { value: "h", label: "h" },
    { value: "d", label: "d" },
  ];

  let config = $derived(getConfig());

  let irradUnit = $state<TimeUnit>("d");
  let coolUnit = $state<TimeUnit>("d");

  // Track previous raw seconds so we re-detect units on external config changes
  // (e.g. preset load) but not on internal edits (user typing in the input).
  let prevIrradS = -1;
  let prevCoolS = -1;
  $effect(() => {
    if (config.irradiation_s !== prevIrradS) {
      prevIrradS = config.irradiation_s;
      irradUnit = bestUnit(config.irradiation_s);
    }
    if (config.cooling_s !== prevCoolS) {
      prevCoolS = config.cooling_s;
      coolUnit = bestUnit(config.cooling_s);
    }
  });

  let irradValue = $derived(fromSeconds(config.irradiation_s, irradUnit));
  let coolValue = $derived(fromSeconds(config.cooling_s, coolUnit));

  function onIrradChange(e: Event) {
    const v = parseFloat((e.target as HTMLInputElement).value);
    if (!isNaN(v) && v >= 0) {
      const s = toSeconds(v, irradUnit);
      prevIrradS = s; // prevent unit re-detection on user edit
      setIrradiation(s);
    }
  }

  function onCoolChange(e: Event) {
    const v = parseFloat((e.target as HTMLInputElement).value);
    if (!isNaN(v) && v >= 0) {
      const s = toSeconds(v, coolUnit);
      prevCoolS = s; // prevent unit re-detection on user edit
      setCooling(s);
    }
  }

  function onIrradUnitChange(e: Event) {
    const newUnit = (e.target as HTMLSelectElement).value as TimeUnit;
    irradUnit = newUnit;
  }

  function onCoolUnitChange(e: Event) {
    const newUnit = (e.target as HTMLSelectElement).value as TimeUnit;
    coolUnit = newUnit;
  }
</script>

<div class="timing-config">
  <div class="time-row">
    <label>Irradiation</label>
    <div class="time-controls">
      <input
        type="number"
        value={irradValue}
        min={0}
        step={1}
        onchange={onIrradChange}
        class="time-input"
      />
      <select value={irradUnit} onchange={onIrradUnitChange} class="unit-select">
        {#each UNITS as u}
          <option value={u.value}>{u.label}</option>
        {/each}
      </select>
    </div>
  </div>

  <div class="time-row">
    <label>Cooling</label>
    <div class="time-controls">
      <input
        type="number"
        value={coolValue}
        min={0}
        step={1}
        onchange={onCoolChange}
        class="time-input"
      />
      <select value={coolUnit} onchange={onCoolUnitChange} class="unit-select">
        {#each UNITS as u}
          <option value={u.value}>{u.label}</option>
        {/each}
      </select>
    </div>
  </div>
</div>

<style>
  .timing-config {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }

  .time-row {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  label {
    font-size: 0.8rem;
    color: #c9d1d9;
  }

  .time-controls {
    display: flex;
    gap: 0.4rem;
  }

  .time-input {
    flex: 1;
    background: #0d1117;
    border: 1px solid #2d333b;
    border-radius: 4px;
    color: #e1e4e8;
    padding: 0.35rem 0.5rem;
    font-size: 0.85rem;
    text-align: right;
  }

  .time-input:focus {
    outline: none;
    border-color: #58a6ff;
  }

  .unit-select {
    width: 60px;
    background: #0d1117;
    border: 1px solid #2d333b;
    border-radius: 4px;
    color: #e1e4e8;
    padding: 0.35rem;
    font-size: 0.85rem;
    cursor: pointer;
  }

  .unit-select:focus {
    outline: none;
    border-color: #58a6ff;
  }
</style>

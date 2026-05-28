<script lang="ts">
  import type { CurrentProfile } from "@hyrr/compute";
  import Modal from "./Modal.svelte";
  import UploadTab from "./current-profile/UploadTab.svelte";
  import GenerateTab from "./current-profile/GenerateTab.svelte";

  interface Props {
    open: boolean;
    onclose: () => void;
    onselect: (profile: CurrentProfile) => void;
  }

  let { open, onclose, onselect }: Props = $props();

  let view = $state<"upload" | "generate">("upload");

  function handleSelect(profile: CurrentProfile) {
    onselect(profile);
    onclose();
  }
</script>

<Modal {open} {onclose} wide={true}>
  {#snippet headerChildren()}
    <div class="popup-header">
      <h3>Current Profile</h3>
      <div class="view-toggle">
        <button class="view-btn" class:active={view === "upload"} onclick={() => { view = "upload"; }}>Upload</button>
        <button class="view-btn" class:active={view === "generate"} onclick={() => { view = "generate"; }}>Generate</button>
      </div>
    </div>
  {/snippet}

  <div class="popup-body">
    {#if view === "upload"}
      <UploadTab onselect={handleSelect} />
    {:else}
      <GenerateTab onselect={handleSelect} />
    {/if}
  </div>
</Modal>

<style>
  .popup-header {
    display: flex;
    align-items: center;
    gap: 1rem;
    flex: 1;
  }

  .popup-header h3 {
    margin: 0;
    font-size: 0.9rem;
    font-weight: 600;
    color: var(--c-text);
    white-space: nowrap;
  }

  .view-toggle {
    display: flex;
    border: 1px solid var(--c-border);
    border-radius: 4px;
    overflow: hidden;
  }

  .view-btn {
    padding: 0.25rem 0.6rem;
    background: var(--c-bg-default);
    border: none;
    border-right: 1px solid var(--c-border);
    color: var(--c-text-muted);
    font-size: 0.75rem;
    cursor: pointer;
  }

  .view-btn:last-child { border-right: none; }
  .view-btn:hover { color: var(--c-text); }
  .view-btn.active { background: var(--c-bg-active); color: var(--c-accent); font-weight: 600; }

  .popup-body {
    padding: 0.5rem 0;
    min-height: 200px;
  }
</style>

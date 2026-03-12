<script lang="ts">
  import Modal from "./Modal.svelte";
  import { getConfig } from "../stores/config.svelte";
  import { getResult } from "../stores/results.svelte";
  import { getShareableUrl } from "../config-url";

  interface Props {
    open: boolean;
    onclose: () => void;
  }

  let { open, onclose }: Props = $props();

  let name = $state("");
  let email = $state("");
  let description = $state("");

  const REPO = "MorePET/hyrr";

  function buildBody(): string {
    const config = getConfig();
    const result = getResult();
    const configUrl = getShareableUrl(config);

    const sections: string[] = [];

    sections.push(`## Bug Report`);
    sections.push(`**Reporter:** ${name || "Anonymous"}${email ? ` (${email})` : ""}`);
    sections.push(`## Description\n\n${description}`);

    // Debug context
    sections.push(`## Debug Context`);
    sections.push(`**Config URL:** ${configUrl}`);
    sections.push(`**Config JSON:**\n\`\`\`json\n${JSON.stringify(config, null, 2)}\n\`\`\``);

    if (result) {
      const summary = {
        layers: result.layers.map((l: any) => ({
          material: l.material,
          isotopeCount: l.isotopes?.length ?? 0,
        })),
        timestamp: result.timestamp,
      };
      sections.push(`**Result summary:**\n\`\`\`json\n${JSON.stringify(summary, null, 2)}\n\`\`\``);
    } else {
      sections.push(`**Result:** No simulation result available`);
    }

    sections.push(`**Browser:** ${navigator.userAgent}`);
    sections.push(`**Timestamp:** ${new Date().toISOString()}`);

    return sections.join("\n\n");
  }

  function submit() {
    if (!description.trim()) return;

    const title = `[Bug] ${description.slice(0, 70)}`;
    const body = buildBody();

    const url = `https://github.com/${REPO}/issues/new?` + new URLSearchParams({
      title,
      body,
      labels: "bug",
    }).toString();

    window.open(url, "_blank");

    // Reset form
    description = "";
    onclose();
  }
</script>

<Modal {open} {onclose} title="Report a Bug">
  <div class="bug-form">
    <div class="field">
      <label for="bug-name">Name <span class="optional">(optional)</span></label>
      <input id="bug-name" type="text" bind:value={name} placeholder="Your name" />
    </div>

    <div class="field">
      <label for="bug-email">Email <span class="optional">(optional)</span></label>
      <input id="bug-email" type="email" bind:value={email} placeholder="your@email.com" />
    </div>

    <div class="field">
      <label for="bug-desc">Description <span class="required">*</span></label>
      <textarea
        id="bug-desc"
        bind:value={description}
        placeholder="What happened? What did you expect?"
        rows="4"
      ></textarea>
    </div>

    <p class="hint">
      The current configuration and simulation state will be attached automatically.
      You'll review the issue on GitHub before submitting.
    </p>

    <div class="actions">
      <button class="cancel-btn" onclick={onclose}>Cancel</button>
      <button class="submit-btn" onclick={submit} disabled={!description.trim()}>
        Open on GitHub
      </button>
    </div>
  </div>
</Modal>

<style>
  .bug-form {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  label {
    font-size: 0.8rem;
    color: #c9d1d9;
  }

  .optional {
    color: #6e7681;
    font-weight: normal;
  }

  .required {
    color: #f85149;
  }

  input, textarea {
    background: #0d1117;
    border: 1px solid #2d333b;
    border-radius: 4px;
    color: #e1e4e8;
    padding: 0.4rem 0.5rem;
    font-size: 0.85rem;
    font-family: inherit;
    resize: vertical;
  }

  input:focus, textarea:focus {
    outline: none;
    border-color: #58a6ff;
  }

  .hint {
    font-size: 0.75rem;
    color: #6e7681;
    margin: 0;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    margin-top: 0.25rem;
  }

  .cancel-btn {
    background: none;
    border: 1px solid #2d333b;
    border-radius: 4px;
    color: #8b949e;
    padding: 0.4rem 0.75rem;
    font-size: 0.8rem;
    cursor: pointer;
  }

  .cancel-btn:hover {
    color: #e1e4e8;
    border-color: #484f58;
  }

  .submit-btn {
    background: #238636;
    border: 1px solid #2ea043;
    border-radius: 4px;
    color: #fff;
    padding: 0.4rem 0.75rem;
    font-size: 0.8rem;
    cursor: pointer;
    font-weight: 500;
  }

  .submit-btn:hover:not(:disabled) {
    background: #2ea043;
  }

  .submit-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>

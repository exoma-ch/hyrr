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
  let screenshot = $state<File | null>(null);
  let screenshotPreview = $state<string | null>(null);
  let submitting = $state(false);
  let resultMsg = $state<{ ok: boolean; text: string } | null>(null);

  // @ts-ignore – Vite env var, set via .env or build flag
  const WORKER_URL: string = (import.meta as any).env?.VITE_ISSUE_WORKER_URL ?? "";
  const REPO = "exoma-ch/hyrr";
  const MAX_DIM = 1280; // downsample to max 1280px on longest side
  const JPEG_QUALITY = 0.8;

  function handleFileSelect(e: Event) {
    const input = e.target as HTMLInputElement;
    const file = input.files?.[0];
    if (!file || !file.type.startsWith("image/")) return;
    screenshot = file;

    // Generate preview
    const reader = new FileReader();
    reader.onload = () => { screenshotPreview = reader.result as string; };
    reader.readAsDataURL(file);
  }

  function removeScreenshot() {
    screenshot = null;
    screenshotPreview = null;
  }

  /** Downsample image using canvas, return as Blob. */
  async function downsampleImage(file: File): Promise<Blob> {
    const bitmap = await createImageBitmap(file);
    let { width, height } = bitmap;

    if (width > MAX_DIM || height > MAX_DIM) {
      const scale = MAX_DIM / Math.max(width, height);
      width = Math.round(width * scale);
      height = Math.round(height * scale);
    }

    const canvas = new OffscreenCanvas(width, height);
    const ctx = canvas.getContext("2d")!;
    ctx.drawImage(bitmap, 0, 0, width, height);
    bitmap.close();

    return canvas.convertToBlob({ type: "image/jpeg", quality: JPEG_QUALITY });
  }

  /** Upload screenshot to worker, return image URL. */
  async function uploadScreenshot(file: File): Promise<string | null> {
    if (!WORKER_URL) return null;
    try {
      const blob = await downsampleImage(file);
      const res = await fetch(`${WORKER_URL}/upload`, {
        method: "POST",
        headers: { "Content-Type": "image/jpeg" },
        body: blob,
      });
      if (!res.ok) return null;
      const data = await res.json();
      return data.url;
    } catch {
      return null;
    }
  }

  function buildBody(screenshotUrl?: string): string {
    const config = getConfig();
    const result = getResult();
    const configUrl = getShareableUrl(config);

    const sections: string[] = [];

    sections.push(`## Bug Report`);
    sections.push(`**Reporter:** ${name || "Anonymous"}${email ? ` (${email})` : ""}`);
    sections.push(`## Description\n\n${description}`);

    if (screenshotUrl) {
      sections.push(`## Screenshot\n\n![screenshot](${screenshotUrl})`);
    }

    // Debug context
    sections.push(`## Debug Context`);
    sections.push(`**[Reproduce this config](${configUrl})**`);
    sections.push(`**Beam:** ${config.beam?.particle ?? "?"} @ ${config.beam?.energy ?? "?"} MeV, ${config.beam?.current ?? "?"} µA`);
    const layerNames = (config.layers ?? []).map((l: any) => l.material).join(" → ");
    sections.push(`**Stack:** ${layerNames || "empty"} (${config.layers?.length ?? 0} layers)`);

    if (result) {
      const nIso = result.layers.reduce((n: number, l: any) => n + (l.isotopes?.length ?? 0), 0);
      sections.push(`**Result:** ${nIso} isotopes produced`);
    } else {
      sections.push(`**Result:** No simulation result available`);
    }

    sections.push(`**Browser:** ${navigator.userAgent}`);
    sections.push(`**Timestamp:** ${new Date().toISOString()}`);

    return sections.join("\n\n");
  }

  async function submit() {
    if (!description.trim()) return;

    const title = `[Bug] ${description.slice(0, 70)}`;

    // If worker URL is configured, submit directly via API
    if (WORKER_URL) {
      submitting = true;
      resultMsg = null;
      try {
        // Upload screenshot first if present
        let screenshotUrl: string | undefined;
        if (screenshot) {
          const url = await uploadScreenshot(screenshot);
          if (url) screenshotUrl = url;
        }

        const body = buildBody(screenshotUrl);
        const res = await fetch(WORKER_URL, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ title, body, labels: ["bug"] }),
        });
        const data = await res.json();
        if (res.ok) {
          resultMsg = { ok: true, text: `Issue #${data.number} created` };
          description = "";
          screenshot = null;
          screenshotPreview = null;
          setTimeout(() => { resultMsg = null; onclose(); }, 2000);
        } else {
          resultMsg = { ok: false, text: data.error ?? "Failed to create issue" };
        }
      } catch {
        resultMsg = { ok: false, text: "Network error — falling back to GitHub" };
        fallbackToGitHub(title, buildBody());
      } finally {
        submitting = false;
      }
      return;
    }

    // Fallback: open on GitHub (requires account, no screenshot support)
    fallbackToGitHub(title, buildBody());
  }

  function fallbackToGitHub(title: string, body: string) {
    const url = `https://github.com/${REPO}/issues/new?` + new URLSearchParams({
      title,
      body,
      labels: "bug",
    }).toString();

    window.open(url, "_blank");
    description = "";
    screenshot = null;
    screenshotPreview = null;
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

    {#if WORKER_URL}
      <div class="field">
        <label for="bug-screenshot">Screenshot <span class="optional">(optional)</span></label>
        {#if screenshotPreview}
          <div class="preview-wrap">
            <img src={screenshotPreview} alt="Screenshot preview" class="preview-img" />
            <button class="remove-btn" onclick={removeScreenshot} title="Remove">&times;</button>
          </div>
        {:else}
          <label class="file-drop" for="bug-screenshot">
            Click or drop an image
            <input
              id="bug-screenshot"
              type="file"
              accept="image/*"
              onchange={handleFileSelect}
              class="file-input"
            />
          </label>
        {/if}
      </div>
    {/if}

    <p class="hint">
      The current configuration and simulation state will be attached automatically.
      {#if !WORKER_URL}You'll review the issue on GitHub before submitting.{/if}
    </p>

    {#if resultMsg}
      <p class="result-msg" class:success={resultMsg.ok} class:error={!resultMsg.ok}>
        {resultMsg.text}
      </p>
    {/if}

    <div class="actions">
      <button class="cancel-btn" onclick={onclose}>Cancel</button>
      <button
        class="submit-btn"
        onclick={submit}
        disabled={!description.trim() || submitting}
      >
        {#if submitting}Submitting...{:else if WORKER_URL}Submit Bug Report{:else}Open on GitHub{/if}
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

  .file-drop {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 1rem;
    border: 1.5px dashed #2d333b;
    border-radius: 6px;
    color: #6e7681;
    font-size: 0.8rem;
    cursor: pointer;
    transition: border-color 0.15s, color 0.15s;
  }

  .file-drop:hover {
    border-color: #58a6ff;
    color: #8b949e;
  }

  .file-input {
    display: none;
  }

  .preview-wrap {
    position: relative;
    display: inline-block;
  }

  .preview-img {
    max-width: 100%;
    max-height: 150px;
    border-radius: 4px;
    border: 1px solid #2d333b;
  }

  .remove-btn {
    position: absolute;
    top: 4px;
    right: 4px;
    background: rgba(13, 17, 23, 0.8);
    border: 1px solid #2d333b;
    border-radius: 50%;
    color: #f85149;
    width: 20px;
    height: 20px;
    font-size: 0.75rem;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    line-height: 1;
  }

  .hint {
    font-size: 0.75rem;
    color: #6e7681;
    margin: 0;
  }

  .result-msg {
    font-size: 0.8rem;
    margin: 0;
    padding: 0.35rem 0.5rem;
    border-radius: 4px;
  }

  .result-msg.success {
    color: #3fb950;
    background: rgba(63, 185, 80, 0.1);
  }

  .result-msg.error {
    color: #f85149;
    background: rgba(248, 81, 73, 0.1);
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

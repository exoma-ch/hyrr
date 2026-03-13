<script lang="ts">
  import { getConfig } from "../stores/config.svelte";
  import { getResult } from "../stores/results.svelte";
  import { getShareableUrl } from "../config-url";
  import { getBugReportOpen, closeBugReport } from "../stores/bugreport.svelte";

  let open = $derived(getBugReportOpen());

  let name = $state("");
  let email = $state("");
  let description = $state("");
  let screenshot = $state<Blob | null>(null);
  let screenshotPreview = $state<string | null>(null);
  let submitting = $state(false);
  let capturing = $state(false);
  let resultMsg = $state<{ ok: boolean; text: string } | null>(null);

  const WORKER_URL = import.meta.env.VITE_ISSUE_WORKER_URL ?? "";
  const REPO = "exoma-ch/hyrr";
  const MAX_DIM = 1280;
  const JPEG_QUALITY = 0.8;

  /** Capture the page behind this panel using html2canvas. */
  async function captureScreen() {
    capturing = true;
    try {
      const html2canvas = (await import("html2canvas")).default;
      // Hide the bug report panel during capture
      const panel = document.querySelector(".bug-panel") as HTMLElement | null;
      if (panel) panel.style.visibility = "hidden";

      const canvas = await html2canvas(document.body, {
        backgroundColor: getComputedStyle(document.documentElement).getPropertyValue("--c-bg-default").trim() || "#0d1117",
        scale: 1,
        logging: false,
        useCORS: true,
      });

      if (panel) panel.style.visibility = "";

      // Downsample if needed
      const blob = await downsampleCanvas(canvas);
      screenshot = blob;
      screenshotPreview = canvas.toDataURL("image/jpeg", 0.6);
    } catch (e) {
      console.warn("Screenshot capture failed:", e);
    } finally {
      capturing = false;
    }
  }

  function handleFileSelect(e: Event) {
    const input = e.target as HTMLInputElement;
    const file = input.files?.[0];
    if (!file || !file.type.startsWith("image/")) return;

    // Downsample the file
    downsampleFile(file).then((blob) => {
      screenshot = blob;
    });

    // Preview
    const reader = new FileReader();
    reader.onload = () => { screenshotPreview = reader.result as string; };
    reader.readAsDataURL(file);
  }

  function removeScreenshot() {
    screenshot = null;
    screenshotPreview = null;
  }

  async function downsampleCanvas(canvas: HTMLCanvasElement): Promise<Blob> {
    let { width, height } = canvas;
    if (width > MAX_DIM || height > MAX_DIM) {
      const scale = MAX_DIM / Math.max(width, height);
      const w = Math.round(width * scale);
      const h = Math.round(height * scale);
      const oc = new OffscreenCanvas(w, h);
      const ctx = oc.getContext("2d")!;
      ctx.drawImage(canvas, 0, 0, w, h);
      return oc.convertToBlob({ type: "image/jpeg", quality: JPEG_QUALITY });
    }
    return new Promise((r) => canvas.toBlob((b) => r(b!), "image/jpeg", JPEG_QUALITY));
  }

  async function downsampleFile(file: File): Promise<Blob> {
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

  async function uploadScreenshot(blob: Blob): Promise<string | null> {
    if (!WORKER_URL) return null;
    try {
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

    sections.push(`**Version:** ${__APP_VERSION__}`);
    sections.push(`**Browser:** ${navigator.userAgent}`);
    sections.push(`**Timestamp:** ${new Date().toISOString()}`);

    return sections.join("\n\n");
  }

  async function submit() {
    if (!description.trim()) return;

    const title = `[Bug] ${description.slice(0, 70)}`;

    if (WORKER_URL) {
      submitting = true;
      resultMsg = null;
      try {
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
          setTimeout(() => { resultMsg = null; closeBugReport(); }, 2000);
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
    closeBugReport();
  }
</script>

{#if open}
  <div class="bug-panel" role="dialog" aria-label="Report a Bug">
    <div class="panel-header">
      <h3>Report a Bug</h3>
      <button class="close-btn" onclick={closeBugReport}>&times;</button>
    </div>

    <div class="panel-body">
      <p class="tip">
        Tip: Keep the bug visible behind this panel before capturing.
      </p>

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
          rows="3"
        ></textarea>
      </div>

      <div class="field">
        <label>Screenshot <span class="optional">(optional)</span></label>
        {#if screenshotPreview}
          <div class="preview-wrap">
            <img src={screenshotPreview} alt="Screenshot preview" class="preview-img" />
            <button class="remove-btn" onclick={removeScreenshot} title="Remove">&times;</button>
          </div>
        {:else}
          <div class="screenshot-actions">
            <button class="capture-btn" onclick={captureScreen} disabled={capturing}>
              {#if capturing}Capturing...{:else}Capture tab{/if}
            </button>
            <label class="file-drop-inline" for="bug-screenshot">
              or attach file
              <input
                id="bug-screenshot"
                type="file"
                accept="image/*"
                onchange={handleFileSelect}
                class="file-input"
              />
            </label>
          </div>
        {/if}
      </div>

      <p class="hint">
        Config and simulation state attached automatically.
        {#if !WORKER_URL}You'll review the issue on GitHub before submitting.{/if}
      </p>

      {#if resultMsg}
        <p class="result-msg" class:success={resultMsg.ok} class:error={!resultMsg.ok}>
          {resultMsg.text}
        </p>
      {/if}

      <div class="actions">
        <button class="cancel-btn" onclick={closeBugReport}>Cancel</button>
        <button
          class="submit-btn"
          onclick={submit}
          disabled={!description.trim() || submitting}
        >
          {#if submitting}Submitting...{:else if WORKER_URL}Submit{:else}Open on GitHub{/if}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .bug-panel {
    position: fixed;
    bottom: 1rem;
    right: 1rem;
    width: 340px;
    max-height: 80vh;
    background: var(--c-bg-subtle);
    border: 1px solid var(--c-border);
    border-radius: 10px;
    box-shadow: 0 8px 32px var(--c-overlay-heavy);
    z-index: 2000;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.6rem 0.75rem;
    border-bottom: 1px solid var(--c-border);
    cursor: grab;
    flex-shrink: 0;
  }

  .panel-header h3 {
    margin: 0;
    font-size: 0.9rem;
    color: var(--c-text);
  }

  .close-btn {
    background: none;
    border: none;
    color: var(--c-text-muted);
    font-size: 1.2rem;
    cursor: pointer;
    padding: 0.1rem 0.3rem;
    border-radius: 4px;
    line-height: 1;
  }

  .close-btn:hover {
    color: var(--c-text);
    background: var(--c-bg-muted);
  }

  .panel-body {
    padding: 0.75rem;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }

  .tip {
    font-size: 0.7rem;
    color: var(--c-gold);
    margin: 0;
    padding: 0.3rem 0.5rem;
    background: var(--c-gold-tint-faint);
    border-radius: 4px;
    border-left: 2px solid var(--c-gold);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
  }

  label {
    font-size: 0.75rem;
    color: var(--c-text-label);
  }

  .optional {
    color: var(--c-text-subtle);
    font-weight: normal;
  }

  .required {
    color: var(--c-red);
  }

  input, textarea {
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text);
    padding: 0.35rem 0.45rem;
    font-size: 0.8rem;
    font-family: inherit;
    resize: vertical;
  }

  input:focus, textarea:focus {
    outline: none;
    border-color: var(--c-accent);
  }

  .screenshot-actions {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .capture-btn {
    background: var(--c-bg-muted);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text-label);
    padding: 0.35rem 0.6rem;
    font-size: 0.75rem;
    cursor: pointer;
    white-space: nowrap;
  }

  .capture-btn:hover:not(:disabled) {
    background: var(--c-border-muted);
    border-color: var(--c-text-faint);
  }

  .capture-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .file-drop-inline {
    font-size: 0.7rem;
    color: var(--c-accent);
    cursor: pointer;
  }

  .file-drop-inline:hover {
    text-decoration: underline;
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
    max-height: 120px;
    border-radius: 4px;
    border: 1px solid var(--c-border);
  }

  .remove-btn {
    position: absolute;
    top: 4px;
    right: 4px;
    background: var(--c-overlay-heavy);
    border: 1px solid var(--c-border);
    border-radius: 50%;
    color: var(--c-red);
    width: 18px;
    height: 18px;
    font-size: 0.7rem;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    line-height: 1;
  }

  .hint {
    font-size: 0.7rem;
    color: var(--c-text-subtle);
    margin: 0;
  }

  .result-msg {
    font-size: 0.75rem;
    margin: 0;
    padding: 0.3rem 0.45rem;
    border-radius: 4px;
  }

  .result-msg.success {
    color: var(--c-green-bright);
    background: var(--c-green-tint);
  }

  .result-msg.error {
    color: var(--c-red);
    background: var(--c-red-tint-subtle);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.4rem;
  }

  .cancel-btn {
    background: none;
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text-muted);
    padding: 0.35rem 0.6rem;
    font-size: 0.75rem;
    cursor: pointer;
  }

  .cancel-btn:hover {
    color: var(--c-text);
    border-color: var(--c-text-faint);
  }

  .submit-btn {
    background: var(--c-green);
    border: 1px solid var(--c-green-emphasis);
    border-radius: 4px;
    color: #fff;
    padding: 0.35rem 0.6rem;
    font-size: 0.75rem;
    cursor: pointer;
    font-weight: 500;
  }

  .submit-btn:hover:not(:disabled) {
    background: var(--c-green-emphasis);
  }

  .submit-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>

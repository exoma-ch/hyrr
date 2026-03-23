<script lang="ts">
  import { detectOS, type OS } from "../utils/platform";

  interface Props {
    /** "inline" = compact footer style, "panel" = full block with other-platforms list */
    variant?: "inline" | "panel";
  }

  let { variant = "panel" }: Props = $props();

  const RELEASES = "https://github.com/exoma-ch/hyrr/releases/latest";

  type PlatformInfo = { label: string; note: string };

  const PLATFORMS: Record<Exclude<OS, "unknown">, PlatformInfo> = {
    windows: { label: "Windows", note: ".msi / .exe" },
    macos:   { label: "macOS",   note: ".dmg  (Apple Silicon & Intel)" },
    linux:   { label: "Linux",   note: ".deb / .AppImage" },
  };

  const os = detectOS();
  const primary = os !== "unknown" ? PLATFORMS[os] : null;
  const others = (Object.entries(PLATFORMS) as [Exclude<OS, "unknown">, PlatformInfo][])
    .filter(([k]) => k !== os);
</script>

{#if variant === "inline"}
  <a href={RELEASES} target="_blank" rel="noopener noreferrer" class="inline-link">
    Desktop app{primary ? ` (${primary.label})` : ""}
  </a>
{:else}
  <div class="download-panel">
    <a href={RELEASES} target="_blank" rel="noopener noreferrer" class="primary-btn">
      {#if primary}
        ↓ Download for {primary.label}
        <span class="note">{primary.note}</span>
      {:else}
        ↓ Download desktop app
      {/if}
    </a>
    <div class="other-links">
      {#each others as [, info]}
        <a href={RELEASES} target="_blank" rel="noopener noreferrer">{info.label}</a>
      {/each}
      {#if !primary}
        {#each Object.values(PLATFORMS) as info}
          <a href={RELEASES} target="_blank" rel="noopener noreferrer">{info.label}</a>
        {/each}
      {/if}
    </div>
  </div>
{/if}

<style>
  /* ─── inline variant (footer) ─── */
  .inline-link {
    color: var(--c-accent);
    text-decoration: none;
  }
  .inline-link:hover {
    text-decoration: underline;
  }

  /* ─── panel variant (help modal) ─── */
  .download-panel {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.4rem;
  }

  .primary-btn {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    background: var(--c-accent);
    color: var(--c-bg-default);
    text-decoration: none;
    border-radius: 4px;
    padding: 0.35rem 0.75rem;
    font-size: 0.8rem;
    font-weight: 600;
    transition: opacity 0.15s;
  }

  .primary-btn:hover {
    opacity: 0.85;
  }

  .note {
    font-weight: 400;
    font-size: 0.72rem;
    opacity: 0.8;
  }

  .other-links {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.75rem;
    color: var(--c-text-muted);
  }

  .other-links a {
    color: var(--c-text-muted);
    text-decoration: none;
  }

  .other-links a:hover {
    color: var(--c-accent);
    text-decoration: underline;
  }
</style>

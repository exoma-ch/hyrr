<script lang="ts">
  import type { CurrentProfile } from "@hyrr/compute";

  interface Props {
    profile: CurrentProfile;
    width?: number;
    height?: number;
  }

  let { profile, width = 80, height = 24 }: Props = $props();

  /** Downsample to ~40 points and build SVG polyline points string. */
  let points = $derived.by(() => {
    const n = profile.timesS.length;
    if (n < 2) return "";
    const step = Math.max(1, Math.floor(n / 40));
    const tMin = profile.timesS[0];
    const tMax = profile.timesS[n - 1];
    const tRange = tMax - tMin || 1;
    let iMax = 0;
    for (let i = 0; i < n; i++) if (profile.currentsMA[i] > profile.currentsMA[iMax]) iMax = i;
    const cMax = profile.currentsMA[iMax] || 1;

    const pts: string[] = [];
    // Start at bottom-left
    pts.push(`0,${height}`);
    for (let i = 0; i < n; i += step) {
      const x = ((profile.timesS[i] - tMin) / tRange) * width;
      const y = height - (profile.currentsMA[i] / cMax) * (height - 2);
      pts.push(`${x.toFixed(1)},${y.toFixed(1)}`);
    }
    // Always include last point
    const lastX = width;
    const lastY = height - (profile.currentsMA[n - 1] / cMax) * (height - 2);
    pts.push(`${lastX.toFixed(1)},${lastY.toFixed(1)}`);
    // Close at bottom-right
    pts.push(`${width},${height}`);
    return pts.join(" ");
  });
</script>

<svg class="profile-sparkline" viewBox="0 0 {width} {height}" width={width} height={height} aria-label="Current profile sparkline">
  <polygon {points} />
</svg>

<style>
  .profile-sparkline {
    display: block;
    flex-shrink: 0;
  }

  .profile-sparkline polygon {
    fill: var(--c-accent-tint-subtle, rgba(59, 130, 246, 0.2));
    stroke: var(--c-accent, #3b82f6);
    stroke-width: 1;
    vector-effect: non-scaling-stroke;
  }
</style>

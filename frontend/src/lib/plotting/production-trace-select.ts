/**
 * Per-layer top-N trace selection for the production-vs-depth plot.
 *
 * A stack-global top-N ranks every isotope by total production across the whole
 * target. In a stacked assembly that lets the highest-yield layer monopolize the
 * plot and buries a thin/downstream layer's signature radionuclide (e.g. ⁴⁴Sc
 * under a Nb foil's bulk). Instead, rank radionuclides WITHIN each layer by that
 * layer's integrated production (∫rate·dx) and keep the top N per layer, unioned
 * across layers — so every layer's headline products are represented.
 *
 * Stable isotopes are excluded via `isStable`: the target matrix and stable
 * fragments aren't "produced radionuclides" (consistent with the activity plot,
 * which skips half_life_s === null).
 */
export interface ProductionLayer {
  depth_profile?: { depth_mm: number }[] | null;
  /** isotope name → production rate sampled at each depth_profile point. */
  depth_production_rates?: Record<string, number[]> | null;
}

/** Trapezoidal ∫ rate·d(depth) for one isotope over a layer's depth samples. */
export function integrateOverDepth(
  depths: { depth_mm: number }[],
  rates: number[],
): number {
  let total = 0;
  for (let i = 1; i < Math.min(depths.length, rates.length); i++) {
    total += 0.5 * (rates[i - 1] + rates[i]) * (depths[i].depth_mm - depths[i - 1].depth_mm);
  }
  return total;
}

/**
 * Union of each layer's top-N radionuclides by within-layer integrated
 * production. Stable isotopes (per `isStable`) and zero-production entries are
 * excluded.
 */
export function selectPerLayerTopN(
  layers: ProductionLayer[],
  isStable: (name: string) => boolean,
  topNPerLayer: number,
): Set<string> {
  const selected = new Set<string>();
  for (const layer of layers) {
    const dp = layer.depth_profile;
    const dpr = layer.depth_production_rates;
    if (!dp || dp.length === 0 || !dpr) continue;

    const totals: { name: string; total: number }[] = [];
    for (const [name, rates] of Object.entries(dpr)) {
      if (isStable(name)) continue;
      const total = integrateOverDepth(dp, rates);
      if (total > 0) totals.push({ name, total });
    }
    totals.sort((a, b) => b.total - a.total);
    for (const { name } of totals.slice(0, topNPerLayer)) selected.add(name);
  }
  return selected;
}

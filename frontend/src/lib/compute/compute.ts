/**
 * Orchestrator: compute_stack — main entry point for HYRR simulation.
 *
 * Wires stopping power, production rate, chain solver, and depth profile
 * modules into a single pipeline.
 */

import { AVOGADRO, LN2 } from "./constants";
import {
  PROJECTILE_Z,
  type Beam,
  type CurrentProfile,
  type DatabaseProtocol,
  type DepthPoint,
  type IsotopeResult,
  type Layer,
  type LayerResult,
  type StackResult,
  type TargetStack,
  layerAverageAtomicMass,
} from "./types";
import { trapezoid } from "./interpolation";
import {
  computeEnergyOut,
  computeThicknessFromEnergy,
  dedxMeVPerCm,
  getStoppingSources,
} from "./stopping";
import {
  batemanActivity,
  computeProductionRate,
  generateDepthProfile,
  saturationYield,
} from "./production";
import { discoverChains, solveChain } from "./chains";

/**
 * Minimum activity fraction relative to peak EOB activity.
 * Isotopes below this are pruned before the chain solver.
 */
const ACTIVITY_CUTOFF_FRACTION = 1e-6;

/**
 * Half-life threshold (seconds) above which an isotope is considered
 * "geologically long-lived".  For such isotopes the activity from
 * typical accelerator irradiation times (hours-days) should be
 * vanishingly small.  We cap their activity at the physically correct
 * Bateman value R * lambda * t_irr to guard against numerical noise
 * from the matrix-exponential chain solver.
 */
const LONG_HALFLIFE_THRESHOLD_S = 1e10;

/** Convert layer's (Element, atom_fraction) to (Z, mass_fraction). */
function layerComposition(layer: Layer): Array<[number, number]> {
  const raw: Array<[number, number]> = [];
  for (const [elem, atomFrac] of layer.elements) {
    let avgMass = 0;
    for (const [A, ab] of elem.isotopes) {
      avgMass += A * ab;
    }
    raw.push([elem.Z, atomFrac * avgMass]);
  }

  const total = raw.reduce((s, [, w]) => s + w, 0);
  if (total <= 0) throw new Error("Layer composition has zero total mass");
  return raw.map(([Z, w]) => [Z, w / total]);
}

/** Run the full HYRR simulation pipeline for a target stack. */
export function computeStack(
  db: DatabaseProtocol,
  stack: TargetStack,
  enableChains: boolean = true,
): StackResult {
  const beam = stack.beam;
  const irrTime = stack.irradiationTimeS;
  const coolTime = stack.coolingTimeS;
  const area = stack.areaCm2;

  let energyIn = beam.energyMeV;
  const layerResults: LayerResult[] = [];

  for (const layer of stack.layers) {
    const lr = computeLayer(
      db, beam.projectile, beam.currentMA,
      beam.particlesPerSecond, PROJECTILE_Z[beam.projectile],
      layer, energyIn, irrTime, coolTime, area,
      enableChains, stack.currentProfile,
    );
    layerResults.push(lr);
    energyIn = lr.energyOut;
  }

  return {
    layerResults,
    irradiationTimeS: irrTime,
    coolingTimeS: coolTime,
  };
}

function computeLayer(
  db: DatabaseProtocol,
  projectile: string,
  currentMA: number,
  particlesPerS: number,
  projectileZ: number,
  layer: Layer,
  energyIn: number,
  irrTime: number,
  coolTime: number,
  area: number,
  enableChains: boolean = true,
  currentProfile: CurrentProfile | null = null,
): LayerResult {
  const composition = layerComposition(layer);
  const density = layer.densityGCm3;

  // Resolve thickness / energy_out
  let thickness: number;
  let energyOut: number;

  if (layer.energyOutMeV !== null) {
    energyOut = layer.energyOutMeV;
    thickness = computeThicknessFromEnergy(
      db, projectile, composition, density, energyIn, energyOut,
    );
  } else if (layer.thicknessCm !== null) {
    thickness = layer.thicknessCm;
    energyOut = computeEnergyOut(
      db, projectile, composition, density, energyIn, thickness,
    );
  } else {
    thickness = layer.arealDensityGCm2! / density;
    energyOut = computeEnergyOut(
      db, projectile, composition, density, energyIn, thickness,
    );
  }

  // Set computed fields
  layer._energyIn = energyIn;
  layer._energyOut = energyOut;
  layer._thickness = thickness;

  // Stopping power sources
  const spSources = getStoppingSources(db, projectile, composition);

  // dedx closure for production rate
  const dedxFn = (E: Float64Array): Float64Array => {
    return dedxMeVPerCm(db, projectile, composition, density, E) as Float64Array;
  };

  // Target geometry
  const volume = thickness * area;
  const avgA = layerAverageAtomicMass(layer);
  const nAtoms = (density * volume * AVOGADRO) / avgA;

  // Per-element, per-isotope, per-residual production
  let isotopeResults = new Map<string, IsotopeResult>();

  let firstEnergies: Float64Array | null = null;
  let firstDedx: Float64Array | null = null;

  for (const [elem, atomFrac] of layer.elements) {
    for (const [A, isotopeAbundance] of elem.isotopes) {
      const weight = atomFrac * isotopeAbundance;
      if (weight <= 0) continue;

      const xsList = db.getCrossSections(projectile, elem.Z, A);
      for (const xs of xsList) {
        const { productionRate: prate, energies, xsInterp, dedxValues } =
          computeProductionRate(
            xs.energiesMeV, xs.xsMb, dedxFn,
            energyIn, energyOut, nAtoms,
            particlesPerS, volume,
          );

        if (firstEnergies === null) {
          firstEnergies = energies;
          firstDedx = dedxValues;
        }

        const scaledRate = prate * weight;
        if (scaledRate <= 0) continue;

        const decay = db.getDecayData(xs.residualZ, xs.residualA, xs.state);
        const halfLife = decay?.halfLifeS ?? null;

        const symbol = db.getElementSymbol(xs.residualZ);
        const stateSuffix = xs.state || "";
        const name = `${symbol}-${xs.residualA}${stateSuffix}`;

        const { timeGrid, activity } = batemanActivity(
          scaledRate, halfLife, irrTime, coolTime,
        );

        const satYield = saturationYield(scaledRate, halfLife, currentMA);
        const activityFinal = activity.length > 0 ? activity[activity.length - 1] : 0;

        const existing = isotopeResults.get(name);
        if (existing) {
          const combinedRate = existing.productionRate + scaledRate;
          const combinedSat = existing.saturationYieldBqUA + satYield;
          const { timeGrid: combinedTg, activity: combinedAct } =
            batemanActivity(combinedRate, halfLife, irrTime, coolTime);

          isotopeResults.set(name, {
            ...existing,
            productionRate: combinedRate,
            saturationYieldBqUA: combinedSat,
            activityBq: combinedAct.length > 0 ? combinedAct[combinedAct.length - 1] : 0,
            timeGridS: combinedTg,
            activityVsTimeBq: combinedAct,
          });
        } else {
          isotopeResults.set(name, {
            name, Z: xs.residualZ, A: xs.residualA,
            state: xs.state, halfLifeS: halfLife,
            productionRate: scaledRate,
            saturationYieldBqUA: satYield,
            activityBq: activityFinal,
            timeGridS: timeGrid,
            activityVsTimeBq: activity,
            source: "direct",
            activityDirectBq: 0,
            activityIngrowthBq: 0,
            activityDirectVsTimeBq: new Float64Array(0),
            activityIngrowthVsTimeBq: new Float64Array(0),
          });
        }
      }
    }
  }

  // Prune isotopes with negligible activity relative to the peak
  if (isotopeResults.size > 0) {
    let peakActivity = 0;
    for (const iso of isotopeResults.values()) {
      // Use peak EOB activity (max over time series) for comparison
      for (let t = 0; t < iso.activityVsTimeBq.length; t++) {
        if (iso.activityVsTimeBq[t] > peakActivity) {
          peakActivity = iso.activityVsTimeBq[t];
        }
      }
    }
    const cutoff = peakActivity * ACTIVITY_CUTOFF_FRACTION;
    if (cutoff > 0) {
      for (const [name, iso] of isotopeResults) {
        // Keep if any time point exceeds cutoff
        let keep = false;
        for (let t = 0; t < iso.activityVsTimeBq.length; t++) {
          if (iso.activityVsTimeBq[t] > cutoff) { keep = true; break; }
        }
        if (!keep) isotopeResults.delete(name);
      }
    }
  }

  // Coupled chain solver
  if (currentProfile !== null && !enableChains) {
    enableChains = true;
  }
  if (enableChains && isotopeResults.size > 0) {
    isotopeResults = applyChainSolverByComponent(
      db, isotopeResults, irrTime, coolTime, particlesPerS,
      currentProfile, currentMA,
    );
  }

  // Sanity-check: clamp activity for geologically long-lived isotopes.
  // The matrix-exponential chain solver can produce numerically inflated
  // abundances when eigenvalues span many orders of magnitude (e.g.
  // lambda_parent ~ 1e-5 vs lambda_daughter ~ 1e-13).  For such daughters
  // the physically correct EOB activity is bounded by R * lambda * t_irr
  // (the linear regime of 1 - exp(-lambda*t) ~ lambda*t).  We enforce
  // this ceiling on every time point.
  for (const [, iso] of isotopeResults) {
    if (
      iso.halfLifeS !== null &&
      iso.halfLifeS > LONG_HALFLIFE_THRESHOLD_S
    ) {
      const lambda = LN2 / iso.halfLifeS;
      // Upper bound: all parent production feeds this isotope, plus its own direct production.
      const maxActivity = iso.productionRate * lambda * irrTime;
      if (maxActivity > 0) {
        for (let t = 0; t < iso.activityVsTimeBq.length; t++) {
          if (iso.activityVsTimeBq[t] > maxActivity) {
            iso.activityVsTimeBq[t] = maxActivity;
          }
        }
        if (iso.activityBq > maxActivity) {
          iso.activityBq = maxActivity;
        }
      }
      // Also clamp direct/ingrowth components
      if (iso.activityDirectVsTimeBq.length > 0) {
        for (let t = 0; t < iso.activityDirectVsTimeBq.length; t++) {
          if (iso.activityDirectVsTimeBq[t] > maxActivity) {
            iso.activityDirectVsTimeBq[t] = maxActivity;
          }
        }
        if (iso.activityDirectBq > maxActivity) {
          iso.activityDirectBq = maxActivity;
        }
      }
      if (iso.activityIngrowthVsTimeBq.length > 0) {
        for (let t = 0; t < iso.activityIngrowthVsTimeBq.length; t++) {
          if (iso.activityIngrowthVsTimeBq[t] > maxActivity) {
            iso.activityIngrowthVsTimeBq[t] = maxActivity;
          }
        }
        if (iso.activityIngrowthBq > maxActivity) {
          iso.activityIngrowthBq = maxActivity;
        }
      }
    }
  }

  // Depth profile
  const depthProfile: DepthPoint[] = [];
  if (firstEnergies !== null && firstDedx !== null) {
    const { depths, energiesOrdered, heatWCm3 } = generateDepthProfile(
      firstEnergies, firstDedx, currentMA, area, projectileZ,
    );
    for (let i = 0; i < depths.length; i++) {
      depthProfile.push({
        depthCm: depths[i],
        energyMeV: energiesOrdered[i],
        dedxMeVCm: Math.abs(firstDedx[firstDedx.length - 1 - i]),
        heatWCm3: heatWCm3[i],
      });
    }
  }

  const heatKW = depthProfile.length >= 2 ? integrateHeat(depthProfile, area) : 0;
  const deltaE = energyIn - energyOut;

  return {
    layer, energyIn, energyOut, deltaEMeV: deltaE,
    heatKW, depthProfile, isotopeResults,
    stoppingPowerSources: spSources,
  };
}

/** Split discovered chain into connected components via undirected BFS. */
function splitComponents(
  chain: import("./types").ChainIsotope[],
): import("./types").ChainIsotope[][] {
  const key = (iso: import("./types").ChainIsotope) =>
    `${iso.Z}-${iso.A}-${iso.state}`;

  // Build undirected adjacency
  const adj = new Map<string, Set<string>>();
  const isoByKey = new Map<string, import("./types").ChainIsotope>();
  for (const iso of chain) {
    const k = key(iso);
    isoByKey.set(k, iso);
    if (!adj.has(k)) adj.set(k, new Set());
    for (const mode of iso.decayModes) {
      if (mode.daughterZ === null || mode.daughterA === null) continue;
      const dk = `${mode.daughterZ}-${mode.daughterA}-${mode.daughterState}`;
      if (!adj.has(dk)) adj.set(dk, new Set());
      adj.get(k)!.add(dk);
      adj.get(dk)!.add(k);
    }
  }

  const visited = new Set<string>();
  const components: import("./types").ChainIsotope[][] = [];

  for (const iso of chain) {
    const k = key(iso);
    if (visited.has(k)) continue;

    const component: import("./types").ChainIsotope[] = [];
    const queue = [k];
    visited.add(k);

    let qi = 0;
    while (qi < queue.length) {
      const cur = queue[qi++];
      const curIso = isoByKey.get(cur);
      if (curIso) component.push(curIso);
      for (const neighbor of adj.get(cur) ?? []) {
        if (!visited.has(neighbor)) {
          visited.add(neighbor);
          queue.push(neighbor);
        }
      }
    }

    if (component.length > 0) components.push(component);
  }

  return components;
}

/** Max chain size for matrix exponential solver. Larger chains use independent Bateman. */
const MAX_CHAIN_SIZE = 40;

function applyChainSolverByComponent(
  db: DatabaseProtocol,
  isotopeResults: Map<string, IsotopeResult>,
  irrTime: number,
  coolTime: number,
  particlesPerS: number,
  currentProfile: CurrentProfile | null = null,
  nominalCurrentMA: number = 1.0,
): Map<string, IsotopeResult> {
  const directIsotopes: Array<[number, number, string, number]> = [];
  for (const iso of isotopeResults.values()) {
    directIsotopes.push([iso.Z, iso.A, iso.state, iso.productionRate]);
  }

  const fullChain = discoverChains(db, directIsotopes);
  if (fullChain.length <= 1) {
    for (const [name, iso] of isotopeResults) {
      isotopeResults.set(name, {
        ...iso,
        source: "direct",
        activityDirectBq: iso.activityBq,
        activityIngrowthBq: 0,
        activityDirectVsTimeBq: new Float64Array(iso.activityVsTimeBq),
        activityIngrowthVsTimeBq: new Float64Array(iso.activityVsTimeBq.length),
      });
    }
    return isotopeResults;
  }

  const components = splitComponents(fullChain);
  const newResults = new Map<string, IsotopeResult>();

  for (const component of components) {
    if (component.length > MAX_CHAIN_SIZE) {
      // Too large for matrix expm — fall back to independent Bateman
      for (const ciso of component) {
        const symbol = db.getElementSymbol(ciso.Z);
        const stateSuffix = ciso.state || "";
        const name = `${symbol}-${ciso.A}${stateSuffix}`;
        const existing = isotopeResults.get(name);
        if (existing) {
          newResults.set(name, {
            ...existing,
            source: "direct",
            activityDirectBq: existing.activityBq,
            activityIngrowthBq: 0,
            activityDirectVsTimeBq: new Float64Array(existing.activityVsTimeBq),
            activityIngrowthVsTimeBq: new Float64Array(existing.activityVsTimeBq.length),
          });
        }
      }
      continue;
    }

    const solution = solveChain(
      component, irrTime, coolTime, particlesPerS,
      200, currentProfile, nominalCurrentMA,
    );

    for (let i = 0; i < solution.isotopes.length; i++) {
      const ciso = solution.isotopes[i];
      const symbol = db.getElementSymbol(ciso.Z);
      const stateSuffix = ciso.state || "";
      const name = `${symbol}-${ciso.A}${stateSuffix}`;

      const totalActivity = solution.activities[i];
      const directActivity = solution.activitiesDirect[i];
      const ingrowthActivity = solution.activitiesIngrowth[i];

      const activityFinal = totalActivity.length > 0 ? totalActivity[totalActivity.length - 1] : 0;
      const directFinal = directActivity.length > 0 ? directActivity[directActivity.length - 1] : 0;
      const ingrowthFinal = ingrowthActivity.length > 0 ? ingrowthActivity[ingrowthActivity.length - 1] : 0;

      const hasDirect = ciso.productionRate > 0;
      let maxIngrowth = 0;
      for (let t = 0; t < ingrowthActivity.length; t++) {
        if (ingrowthActivity[t] > maxIngrowth) maxIngrowth = ingrowthActivity[t];
      }
      const hasIngrowth = ingrowthFinal > 0 || maxIngrowth > 0;

      let source: string;
      if (hasDirect && hasIngrowth) source = "both";
      else if (hasIngrowth) source = "daughter";
      else source = "direct";

      const existing = isotopeResults.get(name);
      const prodRate = existing ? existing.productionRate : ciso.productionRate;
      const satYield = existing ? existing.saturationYieldBqUA : 0;

      newResults.set(name, {
        name, Z: ciso.Z, A: ciso.A,
        state: ciso.state, halfLifeS: ciso.halfLifeS,
        productionRate: prodRate,
        saturationYieldBqUA: satYield,
        activityBq: activityFinal,
        timeGridS: solution.timeGridS,
        activityVsTimeBq: totalActivity,
        source,
        activityDirectBq: directFinal,
        activityIngrowthBq: ingrowthFinal,
        activityDirectVsTimeBq: directActivity,
        activityIngrowthVsTimeBq: ingrowthActivity,
      });
    }
  }

  return newResults;
}

function integrateHeat(profile: DepthPoint[], areaCm2: number): number {
  if (profile.length < 2) return 0;

  const depths = new Float64Array(profile.length);
  const heat = new Float64Array(profile.length);
  for (let i = 0; i < profile.length; i++) {
    depths[i] = profile[i].depthCm;
    heat[i] = profile[i].heatWCm3;
  }

  const powerW = areaCm2 * trapezoid(heat, depths);
  return powerW * 1e-3; // W -> kW
}

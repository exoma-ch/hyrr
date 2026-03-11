/**
 * Decay chain discovery and coupled ODE solver.
 *
 * BFS chain discovery, topological sort, matrix exponential solution
 * for coupled decay+production equations, piecewise current profiles.
 */

import { LN2 } from "./constants";
import { linspace } from "./interpolation";
import { matrixExp, matVecMul } from "./matrix-exp";
import {
  type ChainIsotope,
  type ChainSolution,
  type CurrentProfile,
  type DatabaseProtocol,
  type DecayMode,
  chainIsotopeKey,
  chainIsotopeIsStable,
  currentProfileIntervals,
} from "./types";

/**
 * Discover full decay chains from directly-produced isotopes via BFS.
 * Returns isotopes in topological order (parents before daughters).
 */
export function discoverChains(
  db: DatabaseProtocol,
  directIsotopes: Array<[number, number, string, number]>,
  maxDepth: number = 10,
): ChainIsotope[] {
  const isotopeMap = new Map<string, ChainIsotope>();
  const queue: Array<[number, number, string, number]> = [];

  // Seed with directly-produced isotopes
  for (const [Z, A, state, rate] of directIsotopes) {
    const key = `${Z}-${A}-${state}`;
    const existing = isotopeMap.get(key);
    if (existing) {
      existing.productionRate += rate;
    } else {
      const decay = db.getDecayData(Z, A, state);
      const halfLife = decay?.halfLifeS ?? null;
      const modes = decay?.decayModes ? [...decay.decayModes] : [];
      isotopeMap.set(key, {
        Z, A, state,
        halfLifeS: halfLife,
        productionRate: rate,
        decayModes: modes,
      });
      queue.push([Z, A, state, 0]);
    }
  }

  // BFS through daughters
  let qi = 0;
  while (qi < queue.length) {
    const [Z, A, state, depth] = queue[qi++];
    if (depth >= maxDepth) continue;

    const parent = isotopeMap.get(`${Z}-${A}-${state}`)!;
    if (chainIsotopeIsStable(parent)) continue;

    for (const mode of parent.decayModes) {
      if (mode.daughterZ === null || mode.daughterA === null) continue;
      if (mode.mode === "stable") continue;

      const dkey = `${mode.daughterZ}-${mode.daughterA}-${mode.daughterState}`;
      if (!isotopeMap.has(dkey)) {
        const decay = db.getDecayData(
          mode.daughterZ, mode.daughterA, mode.daughterState,
        );
        isotopeMap.set(dkey, {
          Z: mode.daughterZ,
          A: mode.daughterA,
          state: mode.daughterState,
          halfLifeS: decay?.halfLifeS ?? null,
          productionRate: 0,
          decayModes: decay?.decayModes ? [...decay.decayModes] : [],
        });
        queue.push([mode.daughterZ, mode.daughterA, mode.daughterState, depth + 1]);
      }
    }
  }

  return topologicalSort(isotopeMap);
}

function topologicalSort(
  isotopeMap: Map<string, ChainIsotope>,
): ChainIsotope[] {
  // Build adjacency
  const children = new Map<string, Set<string>>();
  const inDegree = new Map<string, number>();

  for (const key of isotopeMap.keys()) {
    children.set(key, new Set());
    inDegree.set(key, 0);
  }

  for (const [key, iso] of isotopeMap) {
    for (const mode of iso.decayModes) {
      if (mode.daughterZ === null || mode.daughterA === null) continue;
      const dkey = `${mode.daughterZ}-${mode.daughterA}-${mode.daughterState}`;
      if (isotopeMap.has(dkey)) {
        children.get(key)!.add(dkey);
        inDegree.set(dkey, inDegree.get(dkey)! + 1);
      }
    }
  }

  // Kahn's algorithm
  const queue: string[] = [];
  for (const [key, deg] of inDegree) {
    if (deg === 0) queue.push(key);
  }

  const result: ChainIsotope[] = [];
  let qi = 0;
  while (qi < queue.length) {
    const key = queue[qi++];
    result.push(isotopeMap.get(key)!);
    for (const child of children.get(key)!) {
      const newDeg = inDegree.get(child)! - 1;
      inDegree.set(child, newDeg);
      if (newDeg === 0) queue.push(child);
    }
  }

  // Append remaining (cycle fallback)
  if (result.length < isotopeMap.size) {
    for (const [, iso] of isotopeMap) {
      if (!result.includes(iso)) result.push(iso);
    }
  }

  return result;
}

/**
 * Solve coupled decay chain equations using matrix exponential.
 */
export function solveChain(
  chain: ChainIsotope[],
  irradiationTimeS: number,
  coolingTimeS: number,
  beamParticlesPerS: number,
  nTimePoints: number = 200,
  currentProfile: CurrentProfile | null = null,
  nominalCurrentMA: number = 1.0,
): ChainSolution {
  const n = chain.length;
  if (n === 0) {
    const empty = new Float64Array(0);
    return {
      isotopes: chain,
      timeGridS: empty,
      abundances: [],
      activities: [],
      activitiesDirect: [],
      activitiesIngrowth: [],
    };
  }

  // Build index map
  const idx = new Map<string, number>();
  for (let i = 0; i < n; i++) {
    idx.set(chainIsotopeKey(chain[i]), i);
  }

  // Build decay matrix A (n×n flat row-major)
  const A = new Float64Array(n * n);
  for (let i = 0; i < n; i++) {
    const iso = chain[i];
    if (chainIsotopeIsStable(iso)) continue;
    const lam = LN2 / iso.halfLifeS!;
    A[i * n + i] = -lam;
    for (const mode of iso.decayModes) {
      if (mode.daughterZ === null || mode.daughterA === null) continue;
      const dkey = `${mode.daughterZ}-${mode.daughterA}-${mode.daughterState}`;
      const j = idx.get(dkey);
      if (j !== undefined) {
        A[j * n + i] += lam * mode.branching;
      }
    }
  }

  // Nominal production rate vector
  const rNominal = new Float64Array(n);
  for (let i = 0; i < n; i++) {
    rNominal[i] = chain[i].productionRate;
  }

  // Time grid
  const nIrr = Math.floor(nTimePoints / 2);
  const nCool = nTimePoints - nIrr;
  const tIrr = linspace(0, irradiationTimeS, nIrr);
  const tCoolFull = linspace(irradiationTimeS, irradiationTimeS + coolingTimeS, nCool + 1);
  const tCool = tCoolFull.slice(1);

  const timeGrid = new Float64Array(nIrr + nCool);
  timeGrid.set(tIrr, 0);
  timeGrid.set(tCool, nIrr);
  const nT = timeGrid.length;

  // Abundances: array of Float64Array, one per isotope
  const abundances: Float64Array[] = Array.from({ length: n }, () => new Float64Array(nT));
  const activities: Float64Array[] = Array.from({ length: n }, () => new Float64Array(nT));

  // --- Irradiation phase ---
  let nEoi: Float64Array;

  if (currentProfile !== null) {
    nEoi = solveIrradiationPiecewise(
      A, rNominal, tIrr, n, abundances,
      currentProfile, nominalCurrentMA, irradiationTimeS,
    );
  } else {
    // Constant current: forward-stepping with augmented matrix (n+1)×(n+1)
    const m = n + 1;
    const mIrr = new Float64Array(m * m);
    for (let i = 0; i < n; i++) {
      for (let j = 0; j < n; j++) {
        mIrr[i * m + j] = A[i * n + j];
      }
      mIrr[i * m + n] = rNominal[i];
    }

    // Forward-step through irradiation time points
    let nAug = new Float64Array(m);
    nAug[n] = 1.0;

    for (let ti = 0; ti < nIrr; ti++) {
      if (ti === 0) {
        for (let i = 0; i < n; i++) abundances[i][0] = 0;
      } else {
        const dt = tIrr[ti] - tIrr[ti - 1];
        const eM = matrixExp(scaleFlat(mIrr, dt, m), m);
        nAug = new Float64Array(matVecMul(eM, nAug, m));
        // Clamp negative abundances (numerical noise)
        for (let i = 0; i < n; i++) {
          if (nAug[i] < 0) nAug[i] = 0;
          abundances[i][ti] = nAug[i];
        }
      }
    }

    // Step to exact EOI if last grid point isn't exactly irradiationTimeS
    const lastIrrT = tIrr[nIrr - 1];
    if (lastIrrT < irradiationTimeS) {
      const dtFinal = irradiationTimeS - lastIrrT;
      const eM = matrixExp(scaleFlat(mIrr, dtFinal, m), m);
      nAug = new Float64Array(matVecMul(eM, nAug, m));
      for (let i = 0; i < n; i++) {
        if (nAug[i] < 0) nAug[i] = 0;
      }
    }
    nEoi = nAug.slice(0, n);
  }

  // --- Cooling phase (forward-stepping) ---
  {
    let nState = new Float64Array(nEoi);
    for (let ti = 0; ti < nCool; ti++) {
      const tPrev = ti === 0 ? irradiationTimeS : tCool[ti - 1];
      const dt = tCool[ti] - tPrev;
      if (dt <= 0) {
        for (let i = 0; i < n; i++) abundances[i][nIrr + ti] = nState[i];
      } else {
        const eA = matrixExp(scaleFlat(A, dt, n), n);
        nState = new Float64Array(matVecMul(eA, nState, n));
        for (let i = 0; i < n; i++) {
          if (nState[i] < 0) nState[i] = 0;
          abundances[i][nIrr + ti] = nState[i];
        }
      }
    }
  }

  // Compute activities: A_i = lambda_i * N_i
  for (let i = 0; i < n; i++) {
    if (chainIsotopeIsStable(chain[i])) continue;
    const lam = LN2 / chain[i].halfLifeS!;
    for (let t = 0; t < nT; t++) {
      activities[i][t] = lam * abundances[i][t];
    }
  }

  // --- Direct component ---
  const activitiesDirect = computeDirectComponent(
    chain, timeGrid, irradiationTimeS, nT,
    currentProfile, nominalCurrentMA,
  );

  // Ingrowth = total - direct (clamp >= 0)
  const activitiesIngrowth: Float64Array[] = Array.from({ length: n }, (_, i) => {
    const arr = new Float64Array(nT);
    for (let t = 0; t < nT; t++) {
      arr[t] = Math.max(activities[i][t] - activitiesDirect[i][t], 0);
    }
    return arr;
  });

  return {
    isotopes: chain,
    timeGridS: timeGrid,
    abundances,
    activities,
    activitiesDirect,
    activitiesIngrowth,
  };
}

/** Scale a flat n×n matrix by scalar t. */
function scaleFlat(M: Float64Array, t: number, n: number): Float64Array {
  const result = new Float64Array(n * n);
  for (let i = 0; i < n * n; i++) result[i] = M[i] * t;
  return result;
}

function solveIrradiationPiecewise(
  A: Float64Array,
  rNominal: Float64Array,
  tIrr: Float64Array,
  n: number,
  abundances: Float64Array[],
  currentProfile: CurrentProfile,
  nominalCurrentMA: number,
  irradiationTimeS: number,
): Float64Array {
  const intervals = currentProfileIntervals(currentProfile, irradiationTimeS);

  // Build output index map
  const outputIdx = new Map<number, number[]>();
  for (let ti = 0; ti < tIrr.length; ti++) {
    const t = tIrr[ti];
    const list = outputIdx.get(t) ?? [];
    list.push(ti);
    outputIdx.set(t, list);
  }

  // Merge boundaries + output times
  const boundarySet = new Set<number>([0, irradiationTimeS]);
  for (const [s, e] of intervals) {
    boundarySet.add(s);
    boundarySet.add(e);
  }
  for (let ti = 0; ti < tIrr.length; ti++) {
    boundarySet.add(tIrr[ti]);
  }
  const allTimes = [...boundarySet].sort((a, b) => a - b);

  let nState = new Float64Array(n);

  // Record t=0
  const zeroIdxs = outputIdx.get(0);
  if (zeroIdxs) {
    for (const ti of zeroIdxs) {
      for (let i = 0; i < n; i++) abundances[i][ti] = 0;
    }
  }

  let ivIdx = 0;

  for (const tNext of allTimes) {
    if (tNext <= 0) continue;
    const dt = tNext - (allTimes[allTimes.indexOf(tNext) - 1] ?? 0);
    if (dt <= 0) continue;

    // Find current for this sub-step
    while (ivIdx < intervals.length - 1 && intervals[ivIdx][1] <= tNext - dt) {
      ivIdx++;
    }
    const iCurrent = intervals[ivIdx][2];
    const scale = nominalCurrentMA > 0 ? iCurrent / nominalCurrentMA : 0;

    // Build augmented matrix
    const m = n + 1;
    const M = new Float64Array(m * m);
    for (let i = 0; i < n; i++) {
      for (let j = 0; j < n; j++) {
        M[i * m + j] = A[i * n + j];
      }
      M[i * m + n] = rNominal[i] * scale;
    }

    const nAug = new Float64Array(m);
    nAug.set(nState, 0);
    nAug[n] = 1.0;

    const eM = matrixExp(scaleFlat(M, dt, m), m);
    const nAugNew = matVecMul(eM, nAug, m);
    nState = nAugNew.slice(0, n);

    // Record at output times
    const outIdxs = outputIdx.get(tNext);
    if (outIdxs) {
      for (const ti of outIdxs) {
        for (let i = 0; i < n; i++) abundances[i][ti] = nState[i];
      }
    }
  }

  // Ensure EOI stored
  for (let i = 0; i < n; i++) {
    abundances[i][tIrr.length - 1] = nState[i];
  }

  return new Float64Array(nState);
}

function computeDirectComponent(
  chain: ChainIsotope[],
  timeGrid: Float64Array,
  irradiationTimeS: number,
  nT: number,
  currentProfile: CurrentProfile | null,
  nominalCurrentMA: number,
): Float64Array[] {
  const n = chain.length;
  const activitiesDirect: Float64Array[] = Array.from(
    { length: n },
    () => new Float64Array(nT),
  );

  for (let i = 0; i < n; i++) {
    const iso = chain[i];
    if (iso.productionRate <= 0 || chainIsotopeIsStable(iso)) continue;
    const lam = LN2 / iso.halfLifeS!;

    if (currentProfile === null) {
      // Analytical Bateman (constant current)
      let aEoi = 0;
      for (let t = 0; t < nT; t++) {
        if (timeGrid[t] <= irradiationTimeS) {
          activitiesDirect[i][t] =
            iso.productionRate * (1 - Math.exp(-lam * timeGrid[t]));
          if (timeGrid[t] === irradiationTimeS || t === nT - 1) {
            aEoi = activitiesDirect[i][t];
          }
        }
      }
      aEoi = iso.productionRate * (1 - Math.exp(-lam * irradiationTimeS));
      for (let t = 0; t < nT; t++) {
        if (timeGrid[t] > irradiationTimeS) {
          const dtCool = timeGrid[t] - irradiationTimeS;
          activitiesDirect[i][t] = aEoi * Math.exp(-lam * dtCool);
        }
      }
    } else {
      // Forward-stepping through merged boundaries
      const intervals = currentProfileIntervals(currentProfile, irradiationTimeS);

      // Collect irradiation output times
      const irrOutputs: Array<[number, number]> = [];
      for (let t = 0; t < nT; t++) {
        if (timeGrid[t] > irradiationTimeS) break;
        irrOutputs.push([timeGrid[t], t]);
      }

      // Merge boundaries
      const boundarySet = new Set<number>([0, irradiationTimeS]);
      for (const [s, e] of intervals) {
        boundarySet.add(s);
        boundarySet.add(e);
      }
      for (const [t] of irrOutputs) {
        boundarySet.add(t);
      }
      const allTimes = [...boundarySet].sort((a, b) => a - b);

      const outMap = new Map<number, number[]>();
      for (const [t, ti] of irrOutputs) {
        const list = outMap.get(t) ?? [];
        list.push(ti);
        outMap.set(t, list);
      }

      let nVal = 0;
      let tNow = 0;
      let ivIdx = 0;

      for (const tNext of allTimes) {
        if (tNext <= tNow) continue;
        const dt = tNext - tNow;
        if (dt <= 0) continue;

        while (ivIdx < intervals.length - 1 && intervals[ivIdx][1] <= tNow) {
          ivIdx++;
        }
        const scale = nominalCurrentMA > 0
          ? intervals[ivIdx][2] / nominalCurrentMA
          : 0;
        const rT = iso.productionRate * scale;

        const expLdt = Math.exp(-lam * dt);
        nVal = nVal * expLdt + (rT / lam) * (1 - expLdt);
        tNow = tNext;

        const outIdxs = outMap.get(tNow);
        if (outIdxs) {
          for (const ti of outIdxs) {
            activitiesDirect[i][ti] = lam * nVal;
          }
        }
      }

      // Cooling phase
      const aEoi = lam * nVal;
      for (let t = 0; t < nT; t++) {
        if (timeGrid[t] > irradiationTimeS) {
          const dtCool = timeGrid[t] - irradiationTimeS;
          activitiesDirect[i][t] = aEoi * Math.exp(-lam * dtCool);
        }
      }
    }
  }

  return activitiesDirect;
}

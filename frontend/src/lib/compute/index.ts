/** Barrel export for compute module. */

export { computeStack } from "./compute";
export { DataStore } from "./data-store";
export {
  resolveElement,
  resolveIsotopics,
  resolveFormula,
  resolveMaterial,
  massToAtomFractions,
} from "./materials";
export {
  computeProductionRate,
  batemanActivity,
  daughterIngrowth,
  saturationYield,
  generateDepthProfile,
} from "./production";
export {
  elementalDedx,
  compoundDedx,
  dedxMeVPerCm,
  computeEnergyOut,
  computeThicknessFromEnergy,
  clearStoppingCache,
} from "./stopping";
export { discoverChains, solveChain } from "./chains";
export { matrixExp } from "./matrix-exp";
export type {
  Beam,
  ChainIsotope,
  ChainSolution,
  CrossSectionData,
  CurrentProfile,
  DatabaseProtocol,
  DecayData,
  DecayMode,
  DepthPoint,
  Element,
  IsotopeResult,
  Layer,
  LayerResult,
  ProjectileType,
  StackResult,
  TargetStack,
} from "./types";
export { createBeam, PROJECTILE_A, PROJECTILE_Z } from "./types";

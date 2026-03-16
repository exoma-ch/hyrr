/** Barrel export for @hyrr/compute package. */

// --- Core compute ---
export { computeStack, applyChainSolverByComponent } from "./compute";
export { DataStore } from "./data-store";
export {
  resolveElement,
  resolveIsotopics,
  resolveFormula,
  resolveMaterial,
  massToAtomFractions,
  setCustomDensityLookup,
  setCustomCompositionLookup,
  MATERIAL_CATALOG,
  ELEMENT_DENSITIES,
  COMPOUND_DENSITIES,
} from "./materials";
export type { CatalogEntry } from "./materials";
export {
  computeProductionRate,
  batemanActivity,
  daughterIngrowth,
  saturationYield,
  generateDepthProfile,
  computeDepthProductionRate,
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
export { matrixExp, matVecMul } from "./matrix-exp";
export {
  makeLogLogInterpolator,
  trapezoid,
  interp,
  linspace,
} from "./interpolation";

// --- Types ---
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
export {
  createBeam,
  PROJECTILE_A,
  PROJECTILE_Z,
  layerAverageAtomicMass,
  chainIsotopeKey,
  chainIsotopeIsStable,
  currentProfileIntervals,
} from "./types";

// --- Constants ---
export {
  AVOGADRO,
  LN2,
  BARN_CM2,
  MILLIBARN_CM2,
  ELEMENTARY_CHARGE,
  MEV_TO_JOULE,
} from "./constants";

// --- Formula parsing ---
export {
  parseFormula,
  formulaToMassFractions,
  elementsFromIdentifier,
  SYMBOL_TO_Z,
  STANDARD_ATOMIC_WEIGHT,
  Z_TO_SYMBOL,
} from "./formula";

// --- Config bridge ---
export {
  buildTargetStack,
  convertResult,
  getRequiredElements,
} from "./config-bridge";
export type {
  BeamConfig,
  IsotopeOverride,
  LayerConfig,
  SimulationConfig,
  IsotopeResultData,
  DepthPointData,
  LayerResultData,
  SimulationResult,
} from "./config-bridge";

// --- Formatting ---
export {
  fmtActivity,
  fmtYield,
  bestActivityUnit,
  bestTimeUnit,
  fmtDoseRate,
  nudatUrl,
  toSuperscript,
  nucLabel,
  nucHtml,
  buildReactionNotation,
} from "./format";

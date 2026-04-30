/** Barrel export for @hyrr/compute package.
 *
 * Physics compute (production, stopping, chains, interpolation, matrix-exp)
 * has been removed — Rust (Tauri/WASM) is the single source of truth.
 * This package retains: types, config-bridge, materials, formula, format,
 * data-store, constants, and expand-layers.
 */

// --- Data store ---
export { DataStore } from "./data-store";

// --- Materials ---
export {
  resolveElement,
  resolveIsotopics,
  resolveFormula,
  resolveMaterial,
  resolveMixtureToElements,
  massToAtomFractions,
  setCustomDensityLookup,
  setCustomCompositionLookup,
  MATERIAL_CATALOG,
  ELEMENT_DENSITIES,
  COMPOUND_DENSITIES,
} from "./materials";
export type { CatalogEntry, MixtureMode, MixtureResult, MixtureResolverOpts, ResolverRow } from "./materials";

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
  isHeavyIon,
  heavyIonA,
  heavyIonZ,
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
  parseIsotopicFormula,
  formulaToMassFractions,
  elementsFromIdentifier,
  SYMBOL_TO_Z,
  STANDARD_ATOMIC_WEIGHT,
  Z_TO_SYMBOL,
} from "./formula";
export type { IsotopicFormula } from "./formula";

// --- Config bridge ---
export {
  buildTargetStack,
  convertResult,
  getRequiredElements,
  isGroup,
} from "./config-bridge";
export type {
  BeamConfig,
  IsotopeOverride,
  LayerConfig,
  LayerGroup,
  StackItem,
  StackConfig,
  SimulationConfig,
  IsotopeResultData,
  DepthPointData,
  LayerResultData,
  SimulationResult,
} from "./config-bridge";

// --- Layer expansion ---
export { expandLayers, expandedLayerCount } from "./expand-layers";

// --- Interpolation utility (for XS plotting) ---
export { interp } from "./_interp";

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

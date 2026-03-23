/** Barrel export for frontend compute module.
 *
 * Physics computation is handled by the Rust backend (Tauri/WASM).
 * This module retains: data-store, materials, types, constants.
 */

export { DataStore } from "./data-store";
export {
  resolveElement,
  resolveIsotopics,
  resolveFormula,
  resolveMaterial,
  massToAtomFractions,
} from "./materials";
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

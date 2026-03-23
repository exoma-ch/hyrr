/* tslint:disable */
/* eslint-disable */

export class WasmDataStore {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Depth preview — stopping power only, no cross-section data needed.
     */
    computeDepthPreview(config_json: string): string;
    /**
     * Run full simulation. Input/output as JSON strings matching config-bridge.ts contract.
     */
    computeStack(config_json: string): string;
    /**
     * Load abundance table from JSON: `[{"Z": 1, "A": 1, "abundance": 0.9998, "atomic_mass": 1.008}, ...]`
     */
    loadAbundances(json: string): void;
    /**
     * Load cross-section data from JSON for one projectile+element.
     * `[{"residual_Z": .., "residual_A": .., "state": "", "energy_MeV": .., "xs_mb": ..}, ...]`
     */
    loadCrossSections(projectile: string, element_symbol: string, json: string): void;
    /**
     * Load decay data from JSON: `[{"Z": .., "A": .., "state": "", "half_life_s": .., "decay_mode": .., ...}, ...]`
     */
    loadDecayData(json: string): void;
    /**
     * Load element table from JSON: `[{"Z": 1, "symbol": "H"}, ...]`
     */
    loadElements(json: string): void;
    /**
     * Load stopping power data from JSON: `[{"source": "pstar", "target_Z": 1, "energy_MeV": .., "dedx": ..}, ...]`
     */
    loadStoppingData(json: string): void;
    constructor(library: string);
    /**
     * Parse a chemical formula. Returns JSON `{"H": 2, "O": 1}`.
     */
    static parseFormula(formula: string): string;
    /**
     * Resolve a material identifier. Returns JSON with elements, density, molecular_weight.
     */
    resolveMaterial(identifier: string): string;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_wasmdatastore_free: (a: number, b: number) => void;
    readonly wasmdatastore_computeDepthPreview: (a: number, b: number, c: number) => [number, number, number, number];
    readonly wasmdatastore_computeStack: (a: number, b: number, c: number) => [number, number, number, number];
    readonly wasmdatastore_loadAbundances: (a: number, b: number, c: number) => [number, number];
    readonly wasmdatastore_loadCrossSections: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => [number, number];
    readonly wasmdatastore_loadDecayData: (a: number, b: number, c: number) => [number, number];
    readonly wasmdatastore_loadElements: (a: number, b: number, c: number) => [number, number];
    readonly wasmdatastore_loadStoppingData: (a: number, b: number, c: number) => [number, number];
    readonly wasmdatastore_new: (a: number, b: number) => number;
    readonly wasmdatastore_parseFormula: (a: number, b: number) => [number, number, number, number];
    readonly wasmdatastore_resolveMaterial: (a: number, b: number, c: number) => [number, number, number, number];
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;

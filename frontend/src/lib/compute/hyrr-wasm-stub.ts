/**
 * Stub for hyrr-wasm module — used in dev/browser builds where WASM isn't available.
 * The dynamic import in backend.ts will catch the error and fall back to TS.
 */
throw new Error("hyrr-wasm not available in browser build");

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { initConfigCodec } from "../lib/config-codec";

/**
 * Initialize the WASM config codec for node vitest.
 *
 * The browser init (`config-codec.ts` → `wasm.default()`) fetches the `.wasm`
 * over `import.meta.url`, which node's fetch can't resolve. So instantiate the
 * module *synchronously* from the built `.wasm` bytes first; the subsequent
 * `initConfigCodec()` (which calls `default()`) then short-circuits on the
 * already-initialized module. This drives the *real* Rust codec in-process, so
 * these tests exercise the exact bytes the browser produces.
 */
export async function initWasmCodecForTest(): Promise<void> {
  const mod = (await import("hyrr-wasm")) as unknown as {
    initSync(m: { module: BufferSource }): unknown;
  };
  const wasmPath = fileURLToPath(
    new URL("../lib/compute/hyrr-wasm-pkg/hyrr_wasm_bg.wasm", import.meta.url),
  );
  mod.initSync({ module: readFileSync(wasmPath) });
  await initConfigCodec();
}

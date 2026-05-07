// wasm-bindgen `--target web` builds export a callable `default` that
// loads + instantiates the `.wasm` binary. The auto-generated .d.ts in
// the `hyrr-wasm` package describes named exports but doesn't declare
// `default` with a call signature, so `await wasm.default()` in
// `lib/compute/backend.ts` fails svelte-check. This module augmentation
// patches the missing signature without touching the runtime path or
// the wasm-bindgen build config. Tracking issue: #133.
declare module "hyrr-wasm" {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  export default function init(input?: unknown): Promise<any>;
}

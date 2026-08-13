/**
 * Browser bridge to the ONE canonical Rust config codec (#539 increment 3b).
 *
 * The share-URL / `.hyrr.json` encode+decode is now the *same Rust code*
 * (`hyrr-core::config_codec`, compiled to WASM) that the MCP server and desktop
 * app run natively — so a browser-produced `#config=1:…` hash is byte-identical
 * to the MCP link, and there is no hand-mirrored TS codec left to drift (#531).
 *
 * The WASM functions (`encodeConfig`/`decodeConfig`) are *synchronous* once the
 * module is initialized; the only async step is loading+initializing the module.
 * We init it once at app boot (`initConfigCodec`, awaited before the cold-load
 * decode and before `ready`), so every runtime encode/decode is a plain sync
 * call. `isCodecReady()` lets callers stay defensive if a code path runs before
 * boot completes — and it reads a `$state` signal, so a Svelte `$derived` that
 * gates on it re-runs the moment the codec flips ready (this file is a
 * `.svelte.ts` module for exactly that reason).
 *
 * Panic discipline (ADR/memory "wasm-panic-poisons-store"): the WASM `decode`
 * never unwraps on untrusted input — a hostile `#config=` throws a catchable JS
 * `Error`, which the decode callers turn into `null`.
 */

import type {
  CodecConfig,
  CodecSizePolicy,
  CodecEncodeOutcome,
} from "@hyrr/compute";

/** The loaded + initialized hyrr-wasm module (its codec free functions). */
interface CodecModule {
  encodeConfig(config: unknown, policy: unknown): CodecEncodeOutcome;
  decodeConfig(hash: string): CodecConfig;
}

let mod: CodecModule | null = null;
let initPromise: Promise<void> | null = null;
// Reactive ready signal. `mod` holds the (non-proxied) WASM module; this plain
// boolean is the `$state` a `$derived` can subscribe to — so encode paths gated
// on `isCodecReady()` recompute when the codec finishes loading, not only when
// their config input changes (the HeaderBar share-URL `$derived`, #546 nit).
let codecReady = $state(false);

/** Default whole-hash byte budget for a share/MCP link, mirroring the Rust
 *  `config_codec::DEFAULT_URL_BUDGET_BYTES`. Transport/CDN/QR limits bind well
 *  before browsers do. */
export const URL_BUDGET_BYTES = 2000;

/** Base a copied share URL is prefixed with (`SHARE_BASE + hash`). Mirrors the
 *  Rust `config_url::FRONTEND_BASE`. The share budget is measured on the WHOLE
 *  URL, so the encoder must subtract this base's length before deciding what
 *  fits — otherwise the browser link silently runs over budget by |SHARE_BASE|
 *  (the bug #542 nit 2 fixed on the Rust/MCP side but not yet the browser one).
 *  Single source of truth: HeaderBar imports this rather than redeclaring it. */
export const SHARE_BASE = "https://exoma-ch.github.io/hyrr/";

/**
 * Load + initialize the WASM codec module once (idempotent, concurrency-safe).
 *
 * In the browser the WASM binary is already instantiated by `initBackend`
 * (ES-module caching means `import("hyrr-wasm")` returns the same module and
 * `default()` is a no-op after the first init). In Tauri the compute backend is
 * native and never loads WASM, so this is where the codec module gets
 * instantiated — cheap: the codec needs no `DataStore`.
 */
export async function initConfigCodec(): Promise<void> {
  if (mod) return;
  if (!initPromise) {
    initPromise = (async () => {
      const wasm = (await import("hyrr-wasm")) as unknown as CodecModule & {
        default?: () => Promise<unknown>;
      };
      // `--target web`: `default()` (a.k.a. __wbg_init) instantiates the binary.
      // Guarded internally (`if (wasm !== undefined) return`), so this is a
      // no-op when `initBackend` already ran it.
      if (typeof wasm.default === "function") await wasm.default();
      mod = wasm;
      codecReady = true;
    })();
  }
  await initPromise;
}

/** Whether the codec is ready for a synchronous encode/decode. Reactive: reading
 *  this inside a Svelte `$derived`/`$effect` subscribes to the ready flip, so the
 *  derived recomputes when the codec finishes initializing. */
export function isCodecReady(): boolean {
  return codecReady;
}

/**
 * Encode a canonical `CodecConfig` under a size policy. Synchronous; the module
 * must be initialized (`initConfigCodec`). Returns the Rust `EncodeOutcome`
 * (`{ hash, dropped, warnings, link_unusable }`) so callers can warn the user
 * instead of ever losing state silently.
 */
export function encodeCodec(
  config: CodecConfig,
  policy: CodecSizePolicy,
): CodecEncodeOutcome {
  if (!mod) throw new Error("config codec not initialized");
  return mod.encodeConfig(config, policy);
}

/**
 * Decode a `#config=1:…` hash (full URL, bare hash, or raw `1:` payload) back
 * to a canonical `CodecConfig`. Throws a catchable `Error` on any malformed /
 * hostile input (never panics the WASM store). Synchronous post-init.
 */
export function decodeCodec(hash: string): CodecConfig {
  if (!mod) throw new Error("config codec not initialized");
  return mod.decodeConfig(hash);
}

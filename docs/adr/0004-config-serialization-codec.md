# ADR 0004 — Config serialization: one Rust-owned codec (codec-only B′)

- **Status**: accepted (2026-07-29) — implemented across the #539 PR stack
- **Date**: 2026-07-29
- **Relates to**: #539 (the spike), #531 (MCP links dropped custom-alloy
  composition — the bug that motivated this), #96 (URL custom-material
  embedding), #251 (WASM as the live compute source of truth), #542/#544
  (deferred hardening follow-ups)
- **Supersedes**: the ad-hoc split between `config-url-v2.ts` and the
  hand-mirrored `core/src/config_url.rs`

## Context

HYRR had **three independent config serializers**, each embedding a *different*
subset of state, so **no single export round-tripped full state**:

| State | `.hyrr.json` file (`session-io.ts`) | `#config=` URL (`config-url-v2.ts`) | MCP link (Rust `config_url.rs`) |
|---|---|---|---|
| `currentProfile` | ✅ full | ❌ stripped (URL "too large") | ❌ |
| custom-alloy composition | ❌ name-only → silent cross-machine break | ✅ embedded (`cl.x`, #96) | ❌ dropped (#531) |
| density override | ✅ | ✅ | ❌ |
| `secondary_neutron` / `neutron_flux` | ✅ | ✅ | ❌ |

The forks were not hypothetical: `core/src/config_url.rs` was a **hand-mirrored
reimplementation** of `config-url-v2.ts` (its own doc comment said "compatible
with the frontend's `config-url-v2.ts` decoder"), and #531 was that mirror
drifting out of sync. Every field added to the TS codec had to be hand-copied
into the Rust one or data was silently dropped. Two adjacent forks compounded
it: material resolution (`resolveMaterial`/`MATERIAL_CATALOG` in TS vs Rust's
own) and the config type itself (TS `SimulationConfig` mirrored by Rust serde
structs).

The failure mode that mattered most (per a domain-user review): custom-alloy
loss is **silent AND physics-altering** — a custom alloy sets stopping power and
target atom density, so a silently-substituted material yields a plausible-but-
wrong result in someone's logbook.

## Decision

Make **`hyrr-core` the single owner of the config type + serialization codec**,
compiled to WASM for the browser and used natively for the MCP link — the
"codec-only B′" verdict from the #539 review panel (5 fresh-context experts:
systems architect, Rust/WASM interop, Svelte state, serialization/security,
domain user; all recommended B).

**The domain/view boundary** (the crux — drawn deliberately so GUI concerns do
not leak into core):

- **→ Rust (`core/src/config_codec.rs`, canonical, Rust-owned):** the
  `CodecConfig` type, the encode/decode codec (compact-key + raw-DEFLATE +
  base64url), inline custom-material composition, the `SizePolicy` measure-and-
  keep rule, and all decode-time security caps.
- **→ TS (view + browser-local persistence):** the reactive editor model
  (`InternalState` — groups, undo/redo, expansion, UI-validation), the IndexedDB
  custom-material *library*, display prefs, session tabs, and the shared
  embed/hydrate seam (`shared-custom-materials.ts`).

The codec is invoked **only at the two chokepoints** (`getSerializableConfig` /
`restoreSerializableConfig` + the share-URL encode/decode) — never per-keystroke.
Reading B literally ("all state in Rust") was explicitly rejected: it would
marshal the whole editable tree across WASM on every edit, a sync-fork worse than
the drift it replaces.

**Mechanisms:**

- **One compressor, not two agreeing.** Deleting the TS `fflate` encoder leaves a
  single deterministic `miniz_oxide` codec; wasm output == native output, so the
  browser `#config=` link and the MCP link are byte-identical by construction.
  (DEFLATE is self-describing, so *decode*-identity is the real contract.)
- **ts-rs generates the TS types from the Rust structs**, gated by a CI
  `ts-bindings-sync` job (`git diff --exit-code` on the generated file). This
  runs **outside** the hermetic `nix flake check` (crane's `rust-test` uses
  default features, so the generator never compiles there). This is what kills
  the drift class structurally, not by discipline.
- **Measure-and-keep `SizePolicy`.** A realistic 200-sample current profile
  compresses to ~1.4 KB and *fits* the ~2 KB URL budget, so it is kept, not
  blanket-stripped. Only genuinely oversized profiles are dropped — and **never
  silently**: the encoder returns structured `dropped`/`warnings`/`link_unusable`
  that the share UI surfaces, with a `.hyrr.json` lossless fallback. A stack of
  >30 layers withholds the browser link entirely rather than emitting a dead one.
- **Security caps in one auditable place** (the panel required these; the old
  Rust path had none): compressed-input cap (8 KiB), streaming decompression cap
  (1 MiB, bounds allocation *before* the length check — the real bomb guard),
  item/map/formula/profile-length bounds, finite-float validation, and
  `deny_unknown_fields` on the security-sensitive `InlineComposition` only.
- **`serde_json/float_roundtrip`** is enabled so Rust decode equals JS
  `JSON.parse` bit-for-bit (default serde is off by 1 ULP — a latent drift on
  `current_profile` physics values).

## Consequences

- **#531 is closed**; the MCP/URL link carries every piece of state it can
  express. The `.hyrr.json` file is now self-contained (custom-material defs
  embedded, hydrated on load with embedded-def-wins collision).
- **The drift class is gone** — one encoder (Rust, via WASM for the browser +
  native for MCP), TS types generated not mirrored, `fflate` + the ~150-line
  hand-rolled TS codec deleted (small JS-bundle win).
- **Back-compat preserved**: v1 (plain base64url) + v2 links and v1/v2/v3
  `.hyrr.json` files still decode.
- **New seam to respect**: a `#config=` link / dropped file on first paint must
  await WASM-ready before decoding (a cold-load ready-gate at boot). Decode is no
  longer a sync TS call.
- **Deferred (own follow-ups)**: material-catalog unification (the `havar` dup
  across `materials.ts`/`materials.rs`) is intentionally *not* bundled here —
  it touches compute hot paths, a different risk profile (referenced from #539).
  Plus #542 (compound-backed customs travel density-only; URL-base byte count)
  and #544 (embedded-def shadowing has no "forget" path; embedded enrichment not
  yet applied at compute time).

## Alternatives considered

- **Option A — shared spec, two implementations.** Keep separate TS + Rust
  codecs held to a written spec. Rejected: a spec cannot prevent the mirror from
  drifting (that *is* #531); it is "the status quo with extra ceremony." Only
  shared code or a codegen gate structurally removes the drift.
- **Literal "all state in Rust".** Rejected — drags the reactive editor model +
  view state into core and forces per-keystroke WASM marshalling.
- **Adopt an MCP/serialization SDK / rewrite to the MCP 2026-07-28 stateless
  model.** Out of scope — HYRR's codec is a stdio/local concern; no SDK needed.

## Implementation (the #539 PR stack)

- **#541** — production `config_codec.rs`; MCP/URL links emit full state;
  measure-and-keep; TS decoder reads `cp`. **Closes #531.**
- **#543** — `.hyrr.json` self-containment via the shared embed/hydrate seam
  (schema v3, additive).
- **#545** — WASM `encodeConfig`/`decodeConfig` bindings + ts-rs generated types
  + `ts-bindings-sync` CI drift gate.
- **#546** — frontend cutover: browser encode+decode routed through the WASM
  codec, hand-rolled TS encoder + `fflate` deleted, cold-load ready-gate;
  verified live (share-link + cold-load + file round-trips).

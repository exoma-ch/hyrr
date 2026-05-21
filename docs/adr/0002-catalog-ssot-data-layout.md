# ADR 0002 — catalog.json as SSoT for nuclear data file discovery

- **Status**: accepted
- **Date**: 2026-05-21
- **Implements**: #257

## Context

HYRR has three runtime clients consuming nuclear data from parquet files:

- **Rust** (`core/src/db.rs`) — Tauri desktop + WASM + MCP
- **TypeScript** (`packages/compute/src/data-store.ts`, `node-data-store.ts`) — browser frontend + Node CLI
- **Python** (`src/hyrr/db.py`) — library + notebooks + CLI

All three hardcoded paths like `meta/decay.parquet`, `stopping/stopping.parquet`,
`{library}/xs/{proj}_{El}.parquet`. When nucl-parquet shipped per-source stopping
files (data-2026.5.1) and library-prefixed xs (data-2026.5.4), each client needed
manual patching. Python lacked dose_constants and emission data entirely. Adding a
new meta file required editing 3+ codebases.

Meanwhile, `catalog.json` already existed in nucl-parquet with a `shared.meta.files`
manifest and `shared.stopping.sources` list — but no client read it.

## Decision

**Make `catalog.json` the single surface all clients read for discovering data file
paths.** One hardcoded path (the catalog location), everything else resolved from it.

### Schema extension (catalog version 2)

Add `dose_constants`, `decay_detailed`, and `nudex_level_gammas` to `shared.meta.files`.
The existing `shared.stopping.sources` and `shared.stopping.path` are already usable.

### Client changes

- **Rust**: `CatalogPaths` struct reads catalog once, provides `meta_file(name)`,
  `stopping_file(source)`, `stopping_compounds_dir()`. Falls back to hardcoded
  defaults if catalog absent.
- **TypeScript**: `metaUrl(name)`, `stoppingUrl(source)`, `emissionsPath()` resolve
  from `this.catalog` loaded at init. Stopping source list read from catalog.
- **Python**: `_resolve_meta(name)` and `_resolve_stopping()` read from catalog.
  Lazy-load all DataFrames via properties. New `get_dose_constant()` method.

### Backward compatibility

All three clients fall back to hardcoded paths if `catalog.json` is absent or
missing fields. Existing data directories without a catalog continue to work.

## Alternatives considered

### Column-level schema introspection
Over-engineering for 20-30 files. The file-level manifest grouped by role is the
right granularity.

### Directory-convention-only (no manifest)
What we had. Already drifting — Python needed a concatenated `stopping.parquet`
shim that Rust didn't, TS loaded emissions from a path Python didn't know about.

### Strict schema validation (JSON Schema enforcement on load)
The catalog is an internal contract, not a public API. Version check + fallback
is sufficient. Adding JSON Schema validation would add a dependency for no gain.

## Consequences

- Adding a new meta file = one catalog entry in nucl-parquet, zero code changes in hyrr
- Python notebooks now have `db.get_dose_constant(Z, A, state)` for the first time
- Phase 2 (follow-up): remove vendored `data/parquet/` from git, all clients download
  on demand like Rust already does — saves ~117MB per version from git history

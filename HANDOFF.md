# Handoff — 2026-05-20 session

## What's done

### #242 — Absolute emission intensities (PR #244, merged)
- Migrated from old nudex_level_gammas + decay_detailed to unified emissions/{Symbol}.parquet
- NuDat-validated absolute per-decay intensities (Co-60 1173keV: 99.86%)
- All 6 emission tabs wired (γ, β⁻, β⁺/511, CE, X-ray, Auger)
- vitest.config.ts excludes nucl-parquet submodule tests
- Deployed to /tst, Playwright-verified

### #195 — nucl-parquet SSoT migration (in progress, not committed)
- nucl-parquet submodule: added raw table accessors (nist_table, catima_table, reaction_keys)
- core/Cargo.toml: swapped parquet/arrow deps for nucl-parquet path dep
- core/src/db.rs: ~560-line ParquetDataStore replaced with ~150-line NpDataStore adapter
- All 74 tests pass (alpha stopping canary + projectile matrix = bit-identical)
- hyrr-mcp + hyrr-wasm compile clean

### Issues filed
- #245 — hyrr-mcp auto-fetch (SSoT migration fixes this)
- #246 — stale PyPI wheel (SSoT migration fixes this)
- #247 — CalVer cache orphan (SSoT migration fixes this)
- #248 — consumer-path integration tests
- nucl-parquet#199 — compound stopping accessor
- nucl-parquet#200 — dose constant source string

## What's NOT committed yet (in worktree)
- NpDataStore adapter + nucl-parquet accessors
- Need: upstream PR to nucl-parquet, Tauri build check, commit + PR

## Key files
- `core/src/db.rs` — NpDataStore adapter
- `core/Cargo.toml` — dep swap
- `nucl-parquet/clients/rs/nucl-parquet/src/stopping.rs` — raw table accessors
- `nucl-parquet/clients/rs/nucl-parquet/src/xs.rs` — reaction_keys()

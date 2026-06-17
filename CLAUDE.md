# Project: HYRR

**Hierarchical Yield and Radionuclide Rates**

Predicts radio-isotope production in stacked target assemblies. The physics
engine is **Rust** (`hyrr-core`); Python, the browser, the desktop app, and the
MCP server are all thin bindings over it.

## Architecture

The compute engine lives once, in Rust, and is exposed to every surface through
bindings. `src/hyrr/*.py` is a thin Python wrapper — **not** the physics
implementation (the pure-Python compute modules are pre-Rust legacy; see below).

### Rust core + bindings

- `core/` (`hyrr-core`) — physics engine: ∫σ/dEdx integration, Bateman chains,
  PSTAR/ASTAR stopping + Bragg additivity, depth profiles, `ParquetDataStore`,
  material resolution. Simulation bugs live here (`core/src/compute.rs`,
  `core/src/production.rs`).
- `py/` (`hyrr-py`) — PyO3 extension imported as `hyrr._native`. `crate-type =
  cdylib`, lib name `_native`, **excluded from the cargo workspace**. Build with
  `just build-native` (→ `scripts/build-native.sh`, drops `src/hyrr/_native.so`,
  gitignored). Needed for `tests/integration`.
- `wasm/` (`hyrr-wasm`) — wasm-bindgen browser compute backend.
- `hyrr-mcp/` (`hyrr-mcp`) — stdio JSON-RPC MCP server; `py-mcp/` (`hyrr-mcp-py`)
  wraps it for `uvx` distribution.
- `desktop/src-tauri/` (`hyrr-desktop`) — Tauri desktop app (native Rust compute,
  data bundled in the installer).

### Python surface (`src/hyrr/`)

- `api.py` — JSON-in/JSON-out marshaller; `run_simulation` / `run_simulation_from_json`
  route through `hyrr._native` and **require** it (no Python compute fallback).
- `cli.py` — CLI entry point (data download, simulation → Rust).
- `db.py` — Polars/Parquet data + catalog utilities (Python data access for the
  CLI and the legacy modules).
- `models.py`, `materials.py` (py-materials bridge), `plotting.py`,
  `geometry.py` / `compute3d.py` (optional 3D mesh: build123d, tetgen).

### Legacy (pre-Rust — not the live path)

- `production.py` — old pure-Python ∫σ/dEdx + Bateman; imported only by tests.
- `neutrons.py` — Python-only neutron activation; still re-exported, but it is
  not in the Rust core and `tendl-2023-iso` ships no neutron cross-sections.
- the Python branches of `stopping.py`.

## Key Design Decisions

- **Rust core, thin bindings** — `hyrr-core` is the one physics implementation;
  Python (`_native`/PyO3), browser (WASM), desktop (Tauri), and MCP all bind to
  it. JSON-in/JSON-out at every boundary.
- **Parquet for all nuclear data** — columnar, fast indexed lookups. Read by
  `ParquetDataStore` (Rust) and hyparquet (browser).
- **PSTAR/ASTAR tables** for stopping power — replaces bare Bethe-Bloch; velocity
  scaling for d/t/³He.
- **py-materials** (MorePET) for materials — density + elemental composition.
- **Local-first, serverless** — no backend; browser app + IndexedDB history,
  GitHub Pages deploy.

## Data

- **nucl-parquet** — git submodule at `nucl-parquet/` (data under
  `nucl-parquet/data/`); evaluated nuclear data libraries (TENDL, ENDF/B, JENDL,
  JEFF, EXFOR, etc.).
- Default library: `tendl-2023-iso` (configurable via `DataStore(data_dir,
  library="...")`, `--library` CLI flag, or `HYRR_LIBRARY` env var). Note:
  `tendl-2023-iso` ships **charged-particle** cross-sections only (p/d/t/h/a) —
  no neutron sublibrary.
- Data resolution order: `--data-dir` arg > `HYRR_DATA` env > `nucl-parquet/data`
  submodule > `../nucl-parquet` sibling > `~/.hyrr/nucl-parquet`.
- `frontend/public/data/parquet/` — Parquet served as static assets to the
  browser (hyparquet).
- Stopping power source: PSTAR/ASTAR from libdEdx (APTG/libdedx), shared across
  all libraries.

## Frontend (`frontend/`)

- Svelte 5 + TypeScript + Vite.
- Compute backend priority: **Tauri (native Rust) → WASM (`hyrr-wasm`, Rust
  compiled)**. `@hyrr/compute` (`packages/compute`) provides the TS data layer
  (hyparquet `DataStore`) and TS physics fallbacks, but registers WASM-backed
  implementations so the live source of truth is Rust (#251). The TS port also
  serves as the third engine in cross-engine validation.
- Nuclear data: lazy-loaded Parquet via hyparquet, cached in IndexedDB.
- History: IndexedDB (no backend, no auth). Sharing: URL hash config
  (`#config=base64...`).

## Release & CI

- `release-please` cuts releases from conventional commits; merging its PR tags
  + creates the GitHub Release. It also auto-syncs `uv.lock` + `wasm/Cargo.lock`
  on the release PR (`sync-release-lockfiles` job), but a maintainer must
  close→reopen the release PR once to run required CI (GITHUB_TOKEN caveat).
- Every push to `main` deploys to `/hyrr/tst/` (staging). Promotion to prod
  (`/hyrr/`) is manual `workflow_dispatch` (`promote-to-prod.yml`).

## Conventions

- Follow devcontainer project conventions (see `.cursor/rules/` if present).
- Commit format: `type(scope): description` with `Refs: #issue`.
- Use `uv` for package management. Tests in `tests/`, run with `uv run pytest`
  (CI ignores `tests/integration`, which needs the native build).

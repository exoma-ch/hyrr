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

- `release-please` cuts releases from conventional commits. Merging its PR
  pushes **two** tags: `v<ver>` (GitHub Release, desktop artifacts) and
  `hyrr-mcp-v<ver>` — the PyPI wheels publish off the *second*, so a missing
  second tag ships nothing while everything reports green.
- Auth is a dedicated GitHub App token (`hyrr-release-bot`), **not**
  `GITHUB_TOKEN`, so its commits and tags do trigger downstream workflows. The
  old "close→reopen the release PR once to run required CI" caveat no longer
  applies.
- The App installation has `contents:write` + `pull_requests:write` +
  `metadata:read` and **no `workflows`** — so it may only point a ref at a
  commit whose `.github/workflows/**` matches the default branch tip's. That
  makes the `hyrr-mcp-v*` tag a race: if a workflow-touching PR merges between
  the Release publishing and the tag being created, every route (`git push`,
  `POST /git/refs`, `POST /releases`, force-`PATCH`) is refused with a 403 and
  no wheel publishes. It happened on 0.21.0. `scripts/tag-mcp-release.sh` owns
  the create + read-back + trigger assertion; `preflight` warns on the release
  PR; `assert --latest` runs on every push to main; recovery is
  `gh workflow run release-please.yml -f retag=v<ver>`. See ADR 0002 and #676.
- `sync-release-lockfiles` syncs **all eight** lockfiles on the release PR —
  `uv.lock`, `package-lock.json`, and every crate's `Cargo.lock` (core, hyrr-mcp,
  py, py-mcp, wasm, desktop/src-tauri). It also re-resolves some transitive deps
  as a side effect; see #636.
- Post-release, `scripts/verify-release.sh <ver>` asserts the published world
  (both tags, `latest.json`, the full PyPI wheel set incl. aarch64, the
  release-notes entry) — the pre-publish gates cannot see any of that (#401).
- **Deploys go to ETH webhosting**, not GitHub Pages: `deploy-eth.yml` drives the
  `ent → tst → prd` ladder (`hyrrent`/`hyrrtst`/`hyrr.ethz.ch`). Push to `main`
  targets `ent`; `workflow_dispatch` picks the env; `prd` has required-reviewer
  protection. The job is gated on `vars.CI_DEPLOY_ENABLED == 'true'`, which is
  currently **unset**, so CI deploys do not run at all today.
- GitHub Pages serves **only the public landing page** (`deploy-frontend.yml` →
  `gh-pages` root). The old `/hyrr/tst/` + `/hyrr/` app slots and
  `promote-to-prod.yml` were removed with the ETH ladder (#489); the mkdocs site
  is currently published nowhere (#637), though `docs.yml` still validates it on
  every PR.

## Conventions

- Follow devcontainer project conventions (see `.cursor/rules/` if present).
- Commit format: `type(scope): description` with `Refs: #issue`.
- Use `uv` for package management. Tests in `tests/`, run with `uv run pytest`
  (CI ignores `tests/integration`, which needs the native build).

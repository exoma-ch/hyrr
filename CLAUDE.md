# Project: HYRR

**Hierarchical Yield and Radionuclide Rates**

Pure Python package for predicting radio-isotope production in stacked target assemblies.

## Architecture

- `src/hyrr/db.py` — Parquet/Polars data store (cross-sections, stopping powers, abundances, decay data)
- `src/hyrr/stopping.py` — PSTAR/ASTAR table lookup + Bragg additivity + velocity scaling for d/t/³He
- `src/hyrr/production.py` — ∫σ/dEdx integration + Bateman equations + depth profiles
- `src/hyrr/models.py` — Beam, BeamProfile, Element, Layer, TargetStack, result dataclasses
- `src/hyrr/materials.py` — Bridge to py-mat + isotopic composition resolution
- `src/hyrr/plotting.py` — Energy scans, depth profiles, cooling curves, straggling, beam profiles, mesh cross-sections
- `src/hyrr/geometry.py` — STEP import, tetrahedral meshing, ray casting, mesh slicing (optional: build123d, tetgen)
- `src/hyrr/compute3d.py` — 3D compute orchestrator for mesh-based simulations
- `src/hyrr/cli.py` — CLI entry point (data download, simulation)
- `data/build_parquet.py` — Build parquet data from raw TENDL/PSTAR/decay sources

## Key Design Decisions

- **Python only** — no Fortran, no subprocess calls. NumPy/SciPy for numerics.
- **Parquet for all nuclear data** — columnar, fast indexed lookups via Polars. Single code path for pip and WASM.
- **NumPy core, polars/pandas optional** — core computation uses plain numpy arrays. Polars and pandas are lazy-imported for export only (`to_polars()`, `to_pandas()`).
- **py-mat** (MorePET/py-mat) for materials — provides density + elemental composition.
- **PSTAR/ASTAR tables** for stopping power — replaces bare Bethe-Bloch. Velocity scaling for d/t/³He.
- **No parallelism needed** — single simulation < 20 ms.

## Data

- **nucl-parquet** (`../nucl-parquet/`) — standalone repo with 12 evaluated nuclear data libraries (TENDL, ENDF/B, JENDL, JEFF, EXFOR, etc.)
- Default library: `tendl-2024` (configurable via `DataStore(data_dir, library="...")`, `--library` CLI flag, or `HYRR_LIBRARY` env var)
- Data resolution order: `--data-dir` arg > `HYRR_DATA` env > `../nucl-parquet` sibling > `~/.hyrr/nucl-parquet`
- `frontend/public/data/parquet/` — Parquet files served as static assets for the browser frontend (hyparquet)
- Stopping power source: PSTAR/ASTAR from libdEdx (APTG/libdedx), shared across all libraries

## Development Plan

See `development-plan.md` for the full 9-phase implementation plan (Phases 1-8: Python library, Phase 9: serverless WASM frontend on GitHub Pages).

## Frontend (`frontend/`)

- Svelte 5 + TypeScript + Vite
- Pure TypeScript compute — physics ported from Python, no Pyodide/WASM
- Nuclear data: lazy-loaded Parquet files via hyparquet, cached in IndexedDB
- History: IndexedDB (no backend, no auth)
- Sharing: URL hash config encoding (#config=base64...)

## Conventions

- Follow devcontainer project conventions (see `.cursor/rules/` if present)
- Commit format: `type(scope): description` with `Refs: #issue`
- Use `uv` for package management
- Tests in `tests/`, run with `uv run pytest`

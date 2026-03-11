# Project: HYRR

**Hierarchical Yield and Radionuclide Rates**

Pure Python package for predicting radio-isotope production in stacked target assemblies.

## Architecture

- `src/hyrr/db.py` — SQLite access layer (cross-sections, stopping powers, abundances, decay data)
- `src/hyrr/stopping.py` — PSTAR/ASTAR table lookup + Bragg additivity + velocity scaling for d/t/³He
- `src/hyrr/production.py` — ∫σ/dEdx integration + Bateman equations + depth profiles
- `src/hyrr/models.py` — Beam, Element, Layer, TargetStack, result dataclasses
- `src/hyrr/materials.py` — Bridge to py-mat + isotopic composition resolution
- `src/hyrr/plotting.py` — Energy scans, depth profiles, cooling curves
- `src/hyrr/cli.py` — CLI entry point (data download, simulation)
- `data/build_db.py` — One-shot script to build hyrr.sqlite from raw TENDL/PSTAR/decay data

## Key Design Decisions

- **Python only** — no Fortran, no subprocess calls. NumPy/SciPy for numerics.
- **SQLite for all nuclear data** — stdlib, single file, indexed queries. No HDF5.
- **NumPy core, polars/pandas optional** — core computation uses plain numpy arrays. Polars and pandas are lazy-imported for export only (`to_polars()`, `to_pandas()`). Enables single code path for pip and WASM.
- **py-mat** (MorePET/py-mat) for materials — provides density + elemental composition.
- **PSTAR/ASTAR tables** for stopping power — replaces bare Bethe-Bloch. Velocity scaling for d/t/³He.
- **No parallelism needed** — single simulation < 20 ms.

## Data

- `data/hyrr.sqlite` is NOT in git (too large). Built by `data/build_db.py` from raw sources.
- Cross-section source: TENDL-2023 via `isotopia.libs/` in the curie project (`../curie/isotopia.libs/`).
- Stopping power source: PSTAR/ASTAR from libdEdx (APTG/libdedx).
- Validation reference: ISOTOPIA sample outputs in `../curie/samples/`.

## Development Plan

See `development-plan.md` for the full 9-phase implementation plan (Phases 1-8: Python library, Phase 9: serverless WASM frontend on GitHub Pages).

## Frontend (`frontend/`)

- Svelte + TypeScript + Vite
- Runs hyrr in-browser via Pyodide (WASM)
- Nuclear data: lazy-loaded SQL INSERT chunks merged into single in-memory SQLite via sql.js
- History: IndexedDB (no backend, no auth)
- Sharing: URL hash config encoding (#config=base64...)

## Conventions

- Follow devcontainer project conventions (see `.cursor/rules/` if present)
- Commit format: `type(scope): description` with `Refs: #issue`
- Use `uv` for package management
- Tests in `tests/`, run with `uv run pytest`

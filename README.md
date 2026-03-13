# HYRR

**Hierarchical Yield and Radionuclide Rates**

A pure Python package for predicting radio-isotope production in stacked
target assemblies, using TENDL cross-section data and NIST stopping power
tables.

## Live Demo

**[exoma-ch.github.io/hyrr](https://exoma-ch.github.io/hyrr/)** — runs entirely in the browser, no install needed.

## What it does

- Stopping power via PSTAR/ASTAR table lookup (replaces Bethe-Bloch)
- Energy-integrated production rates for any projectile (p, d, t, ³He, α)
- Bateman equations for activity, yield, and decay chains
- Compound materials with natural or enriched isotopic composition
- Stacked layer geometries (windows, targets, degraders, backings)
- Depth-resolved heat and activity profiles

## Installation

```bash
uv add git+https://github.com/exoma-ch/hyrr.git
```

## Quick start

```python
from hyrr import TargetStack, Layer, Beam

stack = TargetStack(
    beam=Beam(projectile="p", energy_MeV=30.0, current_mA=0.15),
    layers=[
        Layer(material=havar, thickness_cm=0.0025),
        Layer(material=enriched_mo100, energy_out_MeV=12.0),
        Layer(material=copper, thickness_cm=0.5),
    ],
)

result = stack.run(irradiation_time_s=86400, cooling_time_s=86400)
result.summary()
```

## Frontend

The browser frontend (`frontend/`) is a standalone Svelte 5 + TypeScript app with pure-TS physics compute (no Python/WASM). Nuclear data is lazy-loaded from Parquet files via hyparquet. All computation runs locally — no server, no data upload.

## Development

```bash
git clone --recurse-submodules https://github.com/exoma-ch/hyrr.git
cd hyrr
uv sync --all-extras
uv run pytest
```

Frontend:

```bash
cd frontend
npm ci
npm run dev
```

## Contributing

1. Fork and create a feature branch
2. `uv sync --all-extras` to install all dependencies
3. `uv run pytest` — all tests must pass
4. `uv run ruff check src/` — no lint errors
5. Commit format: `type(scope): description`
6. Open a PR against `main`

## Dependencies

- numpy, scipy — numerics
- polars — data access (Parquet backend)
- matplotlib — plotting
- py-mat — material definitions
- nucl-parquet — evaluated nuclear data (TENDL, ENDF/B, JENDL, JEFF, EXFOR)

## License

MIT

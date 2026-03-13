# HYRR

**Hierarchical Yield and Radionuclide Rates**

A pure Python package for predicting radio-isotope production in stacked
target assemblies, using TENDL cross-section data and NIST stopping power
tables.

**Try it now: [exoma-ch.github.io/hyrr](https://exoma-ch.github.io/hyrr/)** — full simulation runs in the browser, no install, no data leaves your machine.

## What it does

- Stopping power via PSTAR/ASTAR table lookup (replaces Bethe-Bloch)
- Energy-integrated production rates for any projectile (p, d, t, ³He, α)
- Bateman equations for activity, yield, and decay chains
- Compound materials with natural or enriched isotopic composition
- Stacked layer geometries (windows, targets, degraders, backings)
- Depth-resolved heat and activity profiles

## Performance

HYRR is designed for interactive use — simulations are fast enough for real-time parameter sweeps:

| Operation | Time |
|---|---|
| Single isotope production rate | ~56 µs |
| Full layer simulation (all isotopes) | ~1.6 ms |

Compared to tools like Isotopia, HYRR is significantly faster and lighter: pure NumPy/SciPy with Parquet-backed nuclear data (no heavy ORM, no database server). The browser frontend achieves similar performance with a pure TypeScript compute engine.

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

**[exoma-ch.github.io/hyrr](https://exoma-ch.github.io/hyrr/)** — hosted on GitHub Pages, zero backend.

The browser frontend (`frontend/`) is a standalone Svelte 5 + TypeScript app with a pure-TS physics engine (no Python/WASM). Nuclear data is lazy-loaded from Parquet files via hyparquet. All computation runs locally — no server, no data upload.

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
npm test          # vitest
npm run check     # svelte-check (TypeScript)
```

## Contributing

1. Fork and create a feature branch
2. **Python:** `uv sync --all-extras`, then `uv run pytest` and `uv run ruff check src/`
3. **Frontend:** `cd frontend && npm ci`, then `npm test` and `npm run check`
4. Commit format: `type(scope): description`
5. Open a PR against `main`

## Dependencies

- numpy, scipy — numerics
- polars — data access (Parquet backend)
- matplotlib — plotting
- py-mat — material definitions
- nucl-parquet — evaluated nuclear data (TENDL, ENDF/B, JENDL, JEFF, EXFOR)

## License

MIT

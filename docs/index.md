# HYRR

**Hierarchical Yield and Radionuclide Rates**

A pure Python package and browser app for predicting radio-isotope production in stacked target assemblies.

**[Launch the web app](https://exoma-ch.github.io/hyrr/)** — runs entirely in the browser, no install needed. | **[Desktop app](https://github.com/exoma-ch/hyrr/releases)** — Windows, macOS, Linux, works offline.

## Features

- **Table-based stopping powers** — NIST PSTAR/ASTAR reference data instead of bare Bethe-Bloch
- **Compound materials** — Bragg additivity for any material composition via py-mat
- **Stacked layer geometries** — beam propagation through windows, targets, degraders, backings
- **Repeating layer groups** — repeat a layer set N times or until beam energy drops below a threshold
- **Depth profiles** — spatially resolved heat deposition and activity distributions
- **Parquet data store** — fast columnar lookups via Polars, replaces 547,000 text files
- **Pure Python** — no Fortran compiler, no container, `uv add hyrr`
- **Browser frontend** — Svelte 5 + TypeScript with a pure-TS physics engine, zero backend
- **Desktop app** — Tauri v2 native wrapper, all data bundled, fully offline-capable
- **MCP server** — agent-driven analysis via the Model Context Protocol (`mcp/`)

## Performance

| Operation | Time |
|---|---|
| Single isotope production rate | ~56 µs |
| Full layer simulation (all isotopes) | ~1.6 ms |

Significantly faster and lighter than Isotopia: pure NumPy/SciPy with Parquet-backed nuclear data, no heavy ORM or database server.

## Quick Example

```python
from hyrr.models import Beam, Element, Layer, TargetStack

beam = Beam(projectile="p", energy_MeV=16.0, current_mA=0.15)
mo100 = Element(symbol="Mo", Z=42, isotopes={100: 1.0})
target = Layer(
    density_g_cm3=10.22,
    elements=[(mo100, 1.0)],
    energy_out_MeV=12.0,
)
stack = TargetStack(beam=beam, layers=[target])
```

Or just use the **[web app](https://exoma-ch.github.io/hyrr/)** — configure beams, layers, and enrichments in the UI and get results instantly.

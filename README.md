# HYRR

**Hierarchical Yield and Radionuclide Rates**

A pure Python package for predicting radio-isotope production in stacked
target assemblies, using TENDL cross-section data and NIST stopping power
tables.

## Status

Pre-alpha. See `development-plan.md` for the implementation roadmap.

## What it does

- Stopping power via PSTAR/ASTAR table lookup (replaces Bethe-Bloch)
- Energy-integrated production rates for any projectile (p, d, t, ³He, α)
- Bateman equations for activity, yield, and decay chains
- Compound materials with natural or enriched isotopic composition
- Stacked layer geometries (windows, targets, degraders, backings)
- Depth-resolved heat and activity profiles

## Installation

```bash
uv add git+https://github.com/MorePET/hyrr.git
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

## Dependencies

- numpy, scipy — numerics
- polars — DataFrames
- matplotlib — plotting
- pymat — material definitions
- sqlite3 — data access (stdlib)

## License

MIT

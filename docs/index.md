# HYRR

**Hierarchical Yield and Radionuclide Rates**

A pure Python package for predicting radio-isotope production in stacked target assemblies, replacing the Fortran-based ISOTOPIA with better stopping powers, compound/multilayer support, and depth-resolved output.

## Features

- **Table-based stopping powers** — NIST PSTAR/ASTAR reference data instead of bare Bethe-Bloch
- **Compound materials** — Bragg additivity for any material composition via py-mat
- **Stacked layer geometries** — beam propagation through windows, targets, degraders, backings
- **Depth profiles** — spatially resolved heat deposition and activity distributions
- **Single SQLite database** — replaces 547,000 text files with one indexed file
- **Pure Python** — no Fortran compiler, no container, `pip install hyrr`

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

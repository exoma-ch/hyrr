# Quick Start

## Single-layer isotope production

The most common use case: predict isotope production for a single target layer.

```python
from hyrr.models import Beam, Element, Layer, TargetStack
from hyrr.db import HyrrDatabase

# Open database
db = HyrrDatabase("data/hyrr.sqlite")

# Define beam: 16 MeV protons at 0.15 mA
beam = Beam(projectile="p", energy_MeV=16.0, current_mA=0.15)

# Define target: enriched Mo-100, exit energy 12 MeV
mo100 = Element(symbol="Mo", Z=42, isotopes={100: 1.0})
target = Layer(
    density_g_cm3=10.22,
    elements=[(mo100, 1.0)],
    energy_out_MeV=12.0,
)

# Create stack and run
stack = TargetStack(
    beam=beam,
    layers=[target],
    irradiation_time_s=86400,  # 1 day
    cooling_time_s=86400,      # 1 day
)
```

## Viewing results

```python
from hyrr.output import result_summary, result_to_polars

# Text summary (ISOTOPIA-like format)
print(result_summary(result))

# Polars DataFrame for analysis
df = result_to_polars(result)
print(df)
```

## Plotting

```python
from hyrr.plotting import plot_activity_vs_time, plot_depth_profile

# Activity curves for all isotopes
fig = plot_activity_vs_time(layer_result.isotope_results)
fig.savefig("activity.png")

# Depth profile
fig = plot_depth_profile(layer_result)
fig.savefig("depth_profile.png")
```

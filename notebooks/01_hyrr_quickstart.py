# ---
 # jupyter:
#   jupytext:
#     text_representation:
#       extension: .py
#       format_name: hydrogen
#       format_version: '1.3'
#       jupytext_version: 1.18.1
#   kernelspec:
#     display_name: Python 3 (ipykernel)
#     language: python
#     name: python3
# ---

# %% [markdown]
# # HYRR Quickstart — p + Mo-100 → Tc-99m
#
# This notebook demonstrates the full HYRR pipeline for the standard medical isotope production case:
#
# - **Beam**: 16 MeV protons, 0.15 mA
# - **Target**: Enriched Mo-100, exit energy 12 MeV
# - **Irradiation**: 24 hours
# - **Cooling**: 24 hours
#
# We'll compute production rates, activities, depth profiles, and power density — all from first principles using PSTAR stopping powers and TENDL cross-sections.

# %% [markdown]
# ## 1. Setup

# %%
from hyrr import (
    Beam, Element, DataStore, Layer, TargetStack,
    compute_stack, result_summary, result_to_polars,
)
from hyrr.materials import resolve_element
from hyrr.plotting import (
    plot_activity_vs_time, plot_cooling_curve,
    plot_depth_profile, plot_purity_vs_cooling,
)

db = DataStore("data/parquet")

# %% [markdown]
# ## 2. Define beam and target
#
# 16 MeV proton beam at 0.15 mA on an enriched Mo-100 target. The target thickness is determined by the exit energy (12 MeV).

# %%
beam = Beam(projectile="p", energy_MeV=16.0, current_mA=0.15)

mo100 = resolve_element(db, "Mo", enrichment={100: 1.0})

target = Layer(
    density_g_cm3=10.22,
    elements=[(mo100, 1.0)],
    energy_out_MeV=12.0,
)

stack = TargetStack(
    beam=beam,
    layers=[target],
    irradiation_time_s=86400.0,   # 24 h
    cooling_time_s=86400.0,       # 24 h
    area_cm2=1.0,
)

print(f"Beam: {beam.projectile} at {beam.energy_MeV} MeV, {beam.current_mA} mA")
print(f"Particles/s: {beam.particles_per_second:.4E}")

# %% [markdown]
# ## 3. Run the simulation
#
# A single call computes everything: stopping powers, production rates, Bateman equations, depth profiles, and heat.

# %%
result = compute_stack(db, stack)

# %% [markdown]
# ## 4. Text summary

# %%
print(result_summary(result))

# %% [markdown]
# ## 5. DataFrame view

# %%
df = result_to_polars(result)
df.sort("activity_Bq", descending=True).head(15)

# %% [markdown]
# ## 6. Activity vs time

# %%
lr = result.layer_results[0]
plot_activity_vs_time(lr.isotope_results, top_n=8)

# %% [markdown]
# ## 7. Cooling curve

# %%
plot_cooling_curve(
    lr.isotope_results,
    irradiation_time_s=stack.irradiation_time_s,
    top_n=8,
)

# %% [markdown]
# ## 8. Power density depth profile

# %%
plot_depth_profile(lr, quantity="heat")

# %% [markdown]
# ## 9. Radionuclidic purity

# %%
plot_purity_vs_cooling(
    lr.isotope_results,
    target_isotope="Tc-99m",
    irradiation_time_s=stack.irradiation_time_s,
)

# %% [markdown]
# ## 10. Stopping power sources
#
# Shows which data source (PSTAR, ASTAR, or pycatima/SRIM fallback) was used for each element's stopping power.

# %%
for Z, source in lr.stopping_power_sources.items():
    symbol = db.get_element_symbol(Z)
    print(f"  {symbol} (Z={Z}): {source}")

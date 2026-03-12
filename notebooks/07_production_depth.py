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
# # Production & Depth Profiles
#
# Spatial distribution of isotope production within a target layer:
#
# 1. **Production rate vs depth** — where each isotope is being made
# 2. **Cumulative yield** — how quickly production saturates with depth
# 3. **Excitation function** — cross-section with the beam's energy window highlighted
#
# These plots help optimise target thickness: thicker targets produce more
# activity but also more impurities from secondary reactions at lower energies.

# %% [markdown]
# ## 1. Setup

# %%
from hyrr import Beam, DataStore, Layer, TargetStack, compute_stack
from hyrr.materials import resolve_element
from hyrr.plotting import (
    plot_cumulative_yield,
    plot_excitation_function,
    plot_production_vs_depth,
)
import numpy as np

db = DataStore("data/parquet")

# %% [markdown]
# ## 2. Simulation
#
# Enriched Mo-100 target, 16 -> 12 MeV protons.

# %%
beam = Beam(projectile="p", energy_MeV=16.0, current_mA=0.15)
mo100 = resolve_element(db, "Mo", enrichment={100: 1.0})

stack = TargetStack(
    beam=beam,
    layers=[
        Layer(
            density_g_cm3=10.22,
            elements=[(mo100, 1.0)],
            energy_out_MeV=12.0,
        ),
    ],
    irradiation_time_s=86400.0,
    cooling_time_s=86400.0,
)

result = compute_stack(db, stack)
lr = result.layer_results[0]

# %% [markdown]
# ## 3. Production Rate vs Depth
#
# Local production rate $R(z)$ [s$^{-1}$/cm] for the top 5 isotopes.
# Higher rates near the entrance where the beam energy intersects the
# peak cross-section.

# %%
plot_production_vs_depth(lr, top_n=5)

# %% [markdown]
# ## 4. Cumulative Yield vs Depth
#
# Fraction of total production accumulated from entrance to depth $z$.
# If the curve plateaus early, the target is thicker than necessary for
# that isotope.

# %%
plot_cumulative_yield(lr, top_n=5)

# %% [markdown]
# ## 5. Excitation Function with Energy Window
#
# The Mo-100(p,2n)Tc-99m cross-section with the beam's energy range
# (E_in to E_out) highlighted as a shaded region.

# %%
xs_data = db.getCrossSections(42, 100, 1, 1)
for xs in xs_data:
    if xs.residual_Z == 43 and xs.residual_A == 99 and xs.state == "m":
        energies = np.array(xs.energies_MeV)
        sigmas = np.array(xs.cross_sections_mb)
        break

plot_excitation_function(
    energies,
    sigmas,
    energy_in=lr.energy_in,
    energy_out=lr.energy_out,
    reaction_label="Mo-100(p,2n)Tc-99m",
)

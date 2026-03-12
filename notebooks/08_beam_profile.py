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
# # Beam Profile Diagnostics
#
# Visualise the transverse beam shape and phase-space distribution:
#
# 1. **Beam spot** — 2D Gaussian intensity contour at the target surface
# 2. **Phase-space ellipse** — Twiss ellipse in ($x$, $\theta_x$) space
#
# These are useful for verifying beam optics settings and understanding
# how beam divergence and emittance affect the irradiation footprint.

# %% [markdown]
# ## 1. Setup

# %%
from hyrr.models import BeamProfile
from hyrr.plotting import plot_beam_spot, plot_phase_space

# %% [markdown]
# ## 2. Define beam profiles
#
# Three examples: a small circular spot, an elliptical spot with divergence,
# and a full phase-space definition with Twiss parameters.

# %%
# Circular Gaussian spot (1 mm sigma)
circular = BeamProfile(sigma_x_cm=0.1)

# Elliptical spot with divergence
elliptical = BeamProfile(
    sigma_x_cm=0.15,        # 1.5 mm sigma_x
    sigma_y_cm=0.08,        # 0.8 mm sigma_y
    divergence_x_mrad=2.0,
    divergence_y_mrad=3.0,
)

# Full Twiss parametrisation (e.g., from beam transport code)
twiss = BeamProfile(
    sigma_x_cm=0.12,
    sigma_y_cm=0.10,
    divergence_x_mrad=1.5,
    divergence_y_mrad=2.0,
    emittance_x_mm_mrad=1.8,
    emittance_y_mm_mrad=2.0,
    alpha_x=-0.5,   # converging beam
    alpha_y=0.3,     # slightly diverging
)

# %% [markdown]
# ## 3. Beam Spot — Circular

# %%
plot_beam_spot(circular)

# %% [markdown]
# ## 4. Beam Spot — Elliptical

# %%
plot_beam_spot(elliptical)

# %% [markdown]
# ## 5. Phase-Space Ellipse — x-plane
#
# The Twiss ellipse in ($x$, $\theta_x$) space. The tilt of the ellipse
# encodes the $\alpha$ parameter (correlation between position and angle).
# A negative $\alpha$ means the beam is converging (waist downstream).

# %%
plot_phase_space(twiss, plane="x")

# %% [markdown]
# ## 6. Phase-Space Ellipse — y-plane

# %%
plot_phase_space(twiss, plane="y")

# %% [markdown]
# ## 7. Comparison
#
# Side-by-side spots for the three profiles.

# %%
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

fig, axes = plt.subplots(1, 3, figsize=(15, 4))
for ax, (prof, label) in zip(axes, [
    (circular, "Circular"),
    (elliptical, "Elliptical"),
    (twiss, "Twiss"),
]):
    sx = prof.sigma_x_cm * 10.0
    sy = prof.effective_sigma_y_cm * 10.0
    ax.set_title(f"{label}\n$\\sigma_x$={sx:.1f} mm, $\\sigma_y$={sy:.1f} mm")
    ax.set_aspect("equal")

    import numpy as np
    t = np.linspace(0, 2 * np.pi, 100)
    for n in [1, 2, 3]:
        ax.plot(n * sx * np.cos(t), n * sy * np.sin(t), label=f"{n}$\\sigma$")
    ax.legend(fontsize=8)
    ax.set_xlabel("x [mm]")
    ax.set_ylabel("y [mm]")

fig.tight_layout()
fig

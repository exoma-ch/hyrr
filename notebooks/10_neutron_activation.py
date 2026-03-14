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
# # Neutron Activation
#
# Three scenarios for neutron-induced isotope production:
#
# 1. **Reactor irradiation** — Mo-98(n,γ)Mo-99, the standard medical isotope route
# 2. **Fast neutron activation** — monoenergetic 14 MeV (D-T) neutrons on Cu
# 3. **Secondary neutrons** — neutrons from p + Mo-100 (p,xn) channels activating the target
#
# Unlike charged particles, neutrons have no stopping power — they attenuate
# exponentially with material-dependent macroscopic cross-sections.

# %% [markdown]
# ## 1. Setup

# %%
import numpy as np
from hyrr import DataStore, compute_stack, Beam, Layer, TargetStack
from hyrr.materials import resolve_element
from hyrr.neutrons import (
    ThermalFlux,
    EpithermalFlux,
    WeisskopfFlux,
    MonoenergeticFlux,
    CompositeFlux,
    flux_averaged_xs,
    compute_neutron_activation,
    compute_neutron_source,
    compute_secondary_neutron_activation,
    neutron_multiplicity,
)

# Primary database (proton XS)
db = DataStore("../nucl-parquet")

# Neutron database (TENDL-2025 has n_*.parquet files)
db_n = DataStore("../nucl-parquet", library="tendl-2025")

# %% [markdown]
# ## 2. Flux Spectra
#
# Visualize the different neutron energy spectra available.

# %%
E = np.geomspace(1e-3, 20, 1000)  # MeV

thermal = ThermalFlux(total_flux=1e12, kT_eV=25.3)
evaporation = WeisskopfFlux(total_flux=1e10, temperature_MeV=1.5)
mono_14 = MonoenergeticFlux(total_flux=1e10, energy_MeV=14.0)

print("Flux spectra (normalized to total_flux):")
print(f"  Thermal (kT=25.3 eV):    peak at ~{25.3/2:.0f} eV")
print(f"  Weisskopf (T=1.5 MeV):   peak at T = 1.5 MeV")
print(f"  Monoenergetic:            delta at 14 MeV (D-T)")

# %% [markdown]
# ## 3. Reactor Irradiation — Mo-98(n,γ)Mo-99
#
# The standard route for Mo-99 production: thermal/fast neutrons on a Mo target.
# Natural Mo contains 24.13% Mo-98.
#
# **Setup:**
# - Natural Mo target, 1 cm thick, 10.22 g/cm³
# - Monoenergetic 1 MeV neutrons, φ = 10¹² n/cm²/s
# - 7 days irradiation, 1 day cooling

# %%
mo = resolve_element(db_n, "Mo")
mo_layer = Layer(
    density_g_cm3=10.22,
    elements=[(mo, 1.0)],
    thickness_cm=1.0,
)

flux_1MeV = MonoenergeticFlux(total_flux=1e12, energy_MeV=1.0)

result_mo = compute_neutron_activation(
    db_n,
    mo_layer,
    flux_1MeV,
    irradiation_time_s=7 * 86400,
    cooling_time_s=86400,
    thickness_cm=1.0,
)

print(f"Macroscopic XS: Σ_t = {result_mo.sigma_t:.4f} /cm")
print(f"Transmission:   T = {result_mo.transmission:.4f}")
if result_mo.sigma_t > 0:
    print(f"Mean free path: λ = {1/result_mo.sigma_t:.2f} cm")
print(f"\nProduced isotopes: {len(result_mo.isotope_results)}")
print()

# Top isotopes by activity
sorted_isos = sorted(
    result_mo.isotope_results.values(),
    key=lambda x: x.activity_Bq,
    reverse=True,
)
print(f"{'Isotope':<12} {'Prod. rate [/s]':>16} {'Activity [Bq]':>16} {'t½':>12}")
print("-" * 60)
for iso in sorted_isos[:15]:
    if iso.activity_Bq > 0:
        t_half_str = f"{iso.half_life_s:.0f} s" if iso.half_life_s else "stable"
        print(f"{iso.name:<12} {iso.production_rate:>16.3e} {iso.activity_Bq:>16.3e} {t_half_str:>12}")

# %% [markdown]
# ### Hand-calculation check
#
# Mo-98(n,γ)Mo-99:
# - σ(1 MeV) ≈ 5762 mb
# - N_Mo98 = 10.22 × 6.022×10²³ / 95.95 × 0.2413 ≈ 1.55×10²² /cm³
# - R = N × σ × φ_eff ≈ 1.55×10²² × 5.76×10⁻²⁴ × φ_eff

# %%
if "Mo-99" in result_mo.isotope_results:
    mo99 = result_mo.isotope_results["Mo-99"]
    print(f"Mo-99 production rate: {mo99.production_rate:.3e} /s")
    print(f"Mo-99 activity (7d irr + 1d cool): {mo99.activity_Bq:.3e} Bq")
    print(f"  = {mo99.activity_Bq / 3.7e10:.3f} Ci")

    # Compare with hand calculation
    xs_data = db_n.get_cross_sections("n", 42, 98)
    for xs in xs_data:
        if xs.residual_Z == 42 and xs.residual_A == 99:
            sigma_1MeV = np.interp(1.0, xs.energies_MeV, xs.xs_mb)
            print(f"\nσ(1 MeV) from data: {sigma_1MeV:.1f} mb")
            N_98 = 10.22 * 6.022e23 / 95.95 * 0.2413
            R_hand = N_98 * sigma_1MeV * 1e-27 * 1e12 * 1.0
            print(f"Hand calc R (no attenuation): {R_hand:.3e} /s")
            print(f"HYRR R (with attenuation):    {mo99.production_rate:.3e} /s")
            ratio = mo99.production_rate / R_hand if R_hand > 0 else 0
            print(f"Ratio: {ratio:.3f} (< 1.0 due to exponential attenuation)")
            break

# %% [markdown]
# ## 4. Fast Neutron Activation — D-T Neutrons on Cu
#
# 14 MeV neutrons (from D-T fusion) on a Cu target.
# This activates Cu-63(n,2n)Cu-62, Cu-65(n,2n)Cu-64, etc.

# %%
cu = resolve_element(db_n, "Cu")
cu_layer = Layer(
    density_g_cm3=8.96,
    elements=[(cu, 1.0)],
    thickness_cm=0.5,
)

flux_dt = MonoenergeticFlux(total_flux=1e11, energy_MeV=14.0)

result_cu = compute_neutron_activation(
    db_n,
    cu_layer,
    flux_dt,
    irradiation_time_s=3600,   # 1 hour
    cooling_time_s=3600,       # 1 hour
    thickness_cm=0.5,
)

print(f"Macroscopic XS: Σ_t = {result_cu.sigma_t:.4f} /cm")
print(f"Transmission:   T = {result_cu.transmission:.4f}")
print(f"\nProduced isotopes: {len(result_cu.isotope_results)}")
print()
sorted_cu = sorted(
    result_cu.isotope_results.values(),
    key=lambda x: x.activity_Bq,
    reverse=True,
)
print(f"{'Isotope':<12} {'Prod. rate [/s]':>16} {'Activity [Bq]':>16}")
print("-" * 48)
for iso in sorted_cu[:10]:
    if iso.activity_Bq > 0:
        print(f"{iso.name:<12} {iso.production_rate:>16.3e} {iso.activity_Bq:>16.3e}")

# %% [markdown]
# ## 5. Flux-Averaged Cross-Sections
#
# Compare σ(E) with ⟨σ⟩ for different flux spectra.
# This is the neutron analogue of the Gauss-Hermite straggling convolution.

# %%
# Mo-98(n,gamma)Mo-99 cross-section
xs_data = db_n.get_cross_sections("n", 42, 98)
for xs in xs_data:
    if xs.residual_Z == 42 and xs.residual_A == 99:
        E_xs = xs.energies_MeV
        sigma = xs.xs_mb
        break

print("Mo-98(n,γ)Mo-99 flux-averaged cross-sections:")
print(f"  Energy range: {E_xs.min():.2e} — {E_xs.max():.1f} MeV")
print()

spectra = {
    "Weisskopf T=1.0 MeV": WeisskopfFlux(total_flux=1e10, temperature_MeV=1.0),
    "Weisskopf T=1.5 MeV": WeisskopfFlux(total_flux=1e10, temperature_MeV=1.5),
    "Weisskopf T=2.0 MeV": WeisskopfFlux(total_flux=1e10, temperature_MeV=2.0),
    "Mono 1 MeV": MonoenergeticFlux(total_flux=1e10, energy_MeV=1.0),
    "Mono 5 MeV": MonoenergeticFlux(total_flux=1e10, energy_MeV=5.0),
    "Mono 14 MeV": MonoenergeticFlux(total_flux=1e10, energy_MeV=14.0),
}

for name, flux in spectra.items():
    avg = flux_averaged_xs(E_xs, sigma, flux, n_points=1000)
    print(f"  {name:<25s} ⟨σ⟩ = {avg:>10.1f} mb")

# %% [markdown]
# ## 6. Secondary Neutron Activation
#
# Two-pass calculation:
# 1. p + Mo-100 → charged-particle products + neutrons
# 2. Secondary neutrons activate the Mo target
#
# The dominant channel Mo-100(p,2n)Tc-99m emits 2 neutrons per reaction.

# %%
# Step 1: Charged-particle simulation
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

# Step 2: Neutron source from (p,xn) channels
n_source = compute_neutron_source(lr, projectile_Z=1, projectile_A=1)

tc99m = lr.isotope_results.get("Tc-99m")
if tc99m:
    print(f"Primary charged-particle production:")
    print(f"  Tc-99m rate: {tc99m.production_rate:.3e} /s")
print()
print(f"Secondary neutron source:")
print(f"  Total neutron rate: {n_source.total_neutrons_per_s:.3e} n/s")
print(f"  Spectrum: Weisskopf T={n_source.spectrum.temperature_MeV} MeV")

# %%
# Step 3: Secondary neutron activation
n_result = compute_secondary_neutron_activation(
    db_n,
    lr,
    projectile_Z=1,
    projectile_A=1,
    irradiation_time_s=86400.0,
    cooling_time_s=86400.0,
    neutron_library="tendl-2025",
)

if n_result is not None and len(n_result.isotope_results) > 0:
    print(f"\nSecondary neutron activation products: {len(n_result.isotope_results)}")
    print(f"Effective flux: {n_source.total_neutrons_per_s / 2:.2e} n/cm²/s")
    print()
    sorted_n = sorted(
        n_result.isotope_results.values(),
        key=lambda x: x.activity_Bq,
        reverse=True,
    )
    print(f"{'Isotope':<12} {'Activity [Bq]':>16} {'vs primary':>12}")
    print("-" * 44)
    for iso in sorted_n[:10]:
        if iso.activity_Bq > 0:
            primary = lr.isotope_results.get(iso.name)
            ratio_str = ""
            if primary and primary.activity_Bq > 0:
                ratio = iso.activity_Bq / primary.activity_Bq
                ratio_str = f"{ratio:.2e}"
            print(f"{iso.name:<12} {iso.activity_Bq:>16.3e} {ratio_str:>12}")
else:
    print("\nNo secondary neutron activation products (flux too low or no data)")

# %% [markdown]
# ## 7. Neutron Multiplicity
#
# Number of neutrons emitted per reaction channel, computed from
# conservation of A and Z.

# %%
print("Neutron multiplicity for p + Mo-100 channels:")
channels = [
    ("Mo-100(p,γ)Tc-101",  43, 101),
    ("Mo-100(p,n)Tc-100",  43, 100),
    ("Mo-100(p,2n)Tc-99",  43, 99),
    ("Mo-100(p,3n)Tc-98",  43, 98),
    ("Mo-100(p,pn)Mo-99",  42, 99),
    ("Mo-100(p,p2n)Mo-98", 42, 98),
]
for label, res_Z, res_A in channels:
    n = neutron_multiplicity(42, 100, 1, 1, res_Z, res_A)
    print(f"  {label:<25s}  {n} neutron{'s' if n != 1 else ''}")

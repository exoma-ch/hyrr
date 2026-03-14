# ---
# jupyter:
#   jupytext:
#     formats: py:hydrogen,ipynb
#     text_representation:
#       extension: .py
#       format_name: hydrogen
#       format_version: '1.3'
#       jupytext_version: 1.19.1
#   kernelspec:
#     display_name: HYRR (Python 3)
#     language: python
#     name: hyrr
# ---

# %% [markdown]
# # Target Capsule Assembly — 3D Simulation
#
# Build a realistic target capsule in **build123d** and simulate proton
# beam transport through it using HYRR's 3D ray-casting engine.
#
# ## Assembly layout (beam enters from left → right along Z)
#
# ```
#  ┌────────────────────────────────────────────────┐
#  │                                                │
#  │  [Degrader]  [Retainer+Hole]  [Target]  [Dump] │
#  │   Nb disk     Al ring          Mo-100    Cu    │
#  │   0.5 mm      1.0 mm           0.2 mm   5 mm  │
#  │                                                │
#  └────────────────────────────────────────────────┘
#        ←──── beam direction (Z) ────→
# ```
#
# - **Degrader**: Nb foil — reduces beam energy from 18 → ~16 MeV
# - **Retainer**: Al ring with central 8 mm hole — holds the target foil
# - **Target**: Enriched Mo-100 foil — where Tc-99m is produced
# - **Beam dump**: Cu backing — absorbs remaining beam energy

# %% [markdown]
# ## 1. Setup

# %%
%matplotlib inline

import numpy as np
import matplotlib.pyplot as plt
from matplotlib.patches import Polygon
from matplotlib.collections import PatchCollection

import build123d as b3d
from hyrr import Beam, DataStore
from hyrr.geometry import (
    MaterialInfo,
    TetrahedralMesh,
    cast_ray,
    cut_mesh_with_plane,
    longitudinal_slice,
    axial_slice,
)
from hyrr.compute3d import compute_3d

db = DataStore("../nucl-parquet")

# %% [markdown]
# ## 2. Build the capsule in build123d
#
# All dimensions in cm (HYRR convention). The assembly is centered on
# the Z-axis, beam travels in +Z direction.

# %%
# Dimensions [cm]
CAPSULE_RADIUS = 1.2     # outer capsule radius
HOLE_RADIUS = 0.4        # beam aperture in retainer

DEGRADER_T = 0.05        # Nb degrader thickness
RETAINER_T = 0.10        # Al retainer thickness
TARGET_T = 0.02          # Mo-100 target thickness
DUMP_T = 0.50            # Cu beam dump thickness

# Z positions (stacked from z=0)
z0 = 0.0
z1 = z0 + DEGRADER_T
z2 = z1 + RETAINER_T
z3 = z2 + TARGET_T
z4 = z3 + DUMP_T

print("Assembly layout:")
print(f"  Degrader (Nb):   z = {z0:.3f} — {z1:.3f} cm  ({DEGRADER_T*10:.1f} mm)")
print(f"  Retainer (Al):   z = {z1:.3f} — {z2:.3f} cm  ({RETAINER_T*10:.1f} mm)")
print(f"  Target (Mo-100): z = {z2:.3f} — {z3:.3f} cm  ({TARGET_T*10:.1f} mm)")
print(f"  Dump (Cu):       z = {z3:.3f} — {z4:.3f} cm  ({DUMP_T*10:.1f} mm)")
print(f"  Total length:    {z4:.3f} cm")

# %%
# Build each solid as a cylinder along Z

def make_disk(radius, thickness, z_start, name):
    """Make a cylindrical disk centered on Z-axis."""
    with b3d.BuildPart() as part:
        with b3d.BuildSketch():
            b3d.Circle(radius)
        b3d.extrude(amount=thickness)
    solid = part.part.moved(b3d.Location((0, 0, z_start)))
    solid.label = name
    return solid

def make_ring(outer_r, inner_r, thickness, z_start, name):
    """Make an annular ring (disk with central hole)."""
    with b3d.BuildPart() as part:
        with b3d.BuildSketch():
            b3d.Circle(outer_r)
            b3d.Circle(inner_r, mode=b3d.Mode.SUBTRACT)
        b3d.extrude(amount=thickness)
    solid = part.part.moved(b3d.Location((0, 0, z_start)))
    solid.label = name
    return solid

degrader = make_disk(CAPSULE_RADIUS, DEGRADER_T, z0, "degrader")
retainer = make_ring(CAPSULE_RADIUS, HOLE_RADIUS, RETAINER_T, z1, "retainer")
target = make_disk(HOLE_RADIUS, TARGET_T, z2, "target")  # fits in the hole
dump = make_disk(CAPSULE_RADIUS, DUMP_T, z3, "dump")

print("Built 4 solids:")
for s in [degrader, retainer, target, dump]:
    print(f"  {s.label}")

# %% [markdown]
# ## 3. Define materials

# %%
mat_nb = MaterialInfo(
    name="Nb",
    composition=[(41, 1.0)],
    density_g_cm3=8.57,
    atomic_masses={41: 92.906},
)

mat_al = MaterialInfo(
    name="Al",
    composition=[(13, 1.0)],
    density_g_cm3=2.70,
    atomic_masses={13: 26.982},
)

mat_mo100 = MaterialInfo(
    name="Mo-100",
    composition=[(42, 1.0)],
    density_g_cm3=10.22,
    atomic_masses={42: 100.0},  # enriched Mo-100
)

mat_cu = MaterialInfo(
    name="Cu",
    composition=[(29, 1.0)],
    density_g_cm3=8.96,
    atomic_masses={29: 63.546},
)

material_map = {
    "degrader": mat_nb,
    "retainer": mat_al,
    "target": mat_mo100,
    "dump": mat_cu,
}

print("Materials:")
for name, mat in material_map.items():
    print(f"  {name}: {mat.name}, ρ = {mat.density_g_cm3} g/cm³")

# %% [markdown]
# ## 4. Tessellate → tetrahedral mesh
#
# Convert each solid to a surface mesh, then fill with tetrahedra using TetGen.

# %%
def tessellate_assembly(solids, material_map, max_volume=0.0005):
    """Tessellate multiple build123d solids into one TetrahedralMesh."""
    all_nodes = []
    all_elements = []
    all_material_ids = []
    materials = {}
    node_offset = 0

    import tetgen

    for mat_id, solid in enumerate(solids):
        mat_info = material_map[solid.label]
        materials[mat_id] = mat_info

        # Surface tessellation
        mesh_data = solid.tessellate(tolerance=0.005)
        verts = np.array([[v.X, v.Y, v.Z] for v in mesh_data[0]], dtype=np.float64)
        faces = np.array(mesh_data[1], dtype=np.int64)

        # Volume meshing
        tet = tetgen.TetGen(verts, faces)
        tet.tetrahedralize(order=1, mindihedral=10, maxvolume=max_volume, verbose=0)

        all_nodes.append(tet.node)
        all_elements.append(tet.elem + node_offset)
        all_material_ids.append(np.full(len(tet.elem), mat_id, dtype=np.int32))
        node_offset += len(tet.node)

        print(f"  {solid.label}: {len(tet.node)} nodes, {len(tet.elem)} tets")

    return TetrahedralMesh(
        nodes=np.vstack(all_nodes),
        elements=np.vstack(all_elements),
        material_ids=np.concatenate(all_material_ids),
        materials=materials,
    )

print("Tessellating assembly...")
mesh = tessellate_assembly(
    [degrader, retainer, target, dump],
    material_map,
    max_volume=0.001,
)
print(f"\nTotal mesh: {len(mesh.nodes)} nodes, {len(mesh.elements)} tets")

# %% [markdown]
# ## 5. Visualize the assembly
#
# Longitudinal cross-section (cut along beam axis) colored by material.

# %%
beam_dir = np.array([0.0, 0.0, 1.0])
beam_pos = np.array([0.0, 0.0, -0.1])  # start just before degrader

# Cut along beam axis
_, _, polys = longitudinal_slice(mesh, beam_dir, beam_pos)

# Material colors
mat_colors = {
    0: "#4a90d9",   # Nb - blue
    1: "#7cc47c",   # Al - green
    2: "#d94a4a",   # Mo-100 - red
    3: "#d9a54a",   # Cu - orange
}
mat_names = {i: m.name for i, m in mesh.materials.items()}

fig, ax = plt.subplots(1, 1, figsize=(12, 4))

patches = []
colors = []
for poly in polys:
    patches.append(Polygon(poly.vertices_2d, closed=True))
    colors.append(mat_colors.get(poly.material_id, "#888888"))

pc = PatchCollection(patches, facecolors=colors, edgecolors="none", alpha=0.8)
ax.add_collection(pc)

# Legend
for mat_id, color in mat_colors.items():
    if mat_id in mat_names:
        ax.plot([], [], 's', color=color, markersize=10, label=mat_names[mat_id])

ax.set_xlabel("Lateral [cm]")
ax.set_ylabel("Beam axis [cm]")
ax.set_title("Target Capsule — Longitudinal Cross-Section")
ax.set_aspect("equal")
ax.autoscale()
ax.legend(loc="upper right")
plt.tight_layout()

# %% [markdown]
# ## 6. Ray casting — single ray through center

# %%
origin = np.array([0.0, 0.0, -0.1])
direction = np.array([0.0, 0.0, 1.0])

segments = cast_ray(mesh, origin, direction)

print(f"Central ray: {len(segments)} segments")
print(f"{'Material':<12} {'Entry Z [cm]':>14} {'Exit Z [cm]':>14} {'Path [mm]':>10}")
print("-" * 54)

total_path = 0
for seg in segments:
    mat_name = mesh.materials[seg.material_id].name
    z_in = seg.entry_point[2]
    z_out = seg.exit_point[2]
    path_mm = seg.path_length_cm * 10
    total_path += seg.path_length_cm
    print(f"{mat_name:<12} {z_in:>14.4f} {z_out:>14.4f} {path_mm:>10.2f}")

print(f"\nTotal path through material: {total_path*10:.2f} mm")

# %% [markdown]
# ## 7. 3D simulation
#
# Run the full HYRR 3D pipeline: pencil beam → ray casting → production rates → Bateman.

# %%
from hyrr.models import BeamProfile

beam = Beam(
    projectile="p",
    energy_MeV=18.0,       # higher energy, degrader brings it down
    current_mA=0.15,
    energy_spread_MeV=0.3,
    profile=BeamProfile(sigma_x_cm=0.15),  # 1.5 mm sigma beam spot
    position=(0.0, 0.0, -0.1),
    direction=(0.0, 0.0, 1.0),
)

print("Running 3D simulation...")
result_3d = compute_3d(
    db,
    mesh,
    beam,
    irradiation_time_s=86400.0,   # 24 h
    cooling_time_s=86400.0,       # 24 h
    n_rays=19,                    # hexagonal pattern
)
print("Done.\n")

# Results per solid
for mat_id, sr in result_3d.solid_results.items():
    if not sr.isotope_results:
        continue
    print(f"=== {sr.material_name} (solid {mat_id}) ===")
    sorted_iso = sorted(
        sr.isotope_results.values(),
        key=lambda x: x.activity_Bq,
        reverse=True,
    )
    print(f"  {'Isotope':<12} {'Prod. rate [/s]':>16} {'Activity [Bq]':>16}")
    print(f"  {'-'*48}")
    for iso in sorted_iso[:8]:
        if iso.activity_Bq > 0:
            print(f"  {iso.name:<12} {iso.production_rate:>16.3e} {iso.activity_Bq:>16.3e}")
    print()

# %% [markdown]
# ## 8. Axial slice — production at target depth
#
# Cut perpendicular to beam at the target foil location, showing which
# tetrahedra the beam activates.

# %%
z_target_mid = z2 + TARGET_T / 2
_, _, ax_polys = axial_slice(mesh, beam_dir, beam_pos, depth_cm=z_target_mid + 0.1)

fig, ax = plt.subplots(1, 1, figsize=(6, 6))

patches = []
colors = []
for poly in ax_polys:
    patches.append(Polygon(poly.vertices_2d, closed=True))
    colors.append(mat_colors.get(poly.material_id, "#888888"))

pc = PatchCollection(patches, facecolors=colors, edgecolors="gray",
                     linewidths=0.3, alpha=0.8)
ax.add_collection(pc)

# Mark beam spot
circle = plt.Circle((0, 0), beam.profile.sigma_x_cm * 2,
                     fill=False, color="white", linewidth=2, linestyle="--",
                     label=f"Beam 2σ ({beam.profile.sigma_x_cm*20:.1f} mm)")
ax.add_patch(circle)

for mat_id, color in mat_colors.items():
    if mat_id in mat_names:
        ax.plot([], [], 's', color=color, markersize=10, label=mat_names[mat_id])

ax.set_xlabel("X [cm]")
ax.set_ylabel("Y [cm]")
ax.set_title(f"Axial Slice at z = {z_target_mid:.3f} cm (target midplane)")
ax.set_aspect("equal")
ax.autoscale()
ax.legend()
plt.tight_layout()

# %% [markdown]
# ## 9. Summary
#
# Key results from the target capsule simulation.

# %%
# Find Tc-99m in the Mo-100 target solid
mo_solid_id = None
for mat_id, mat in mesh.materials.items():
    if mat.name == "Mo-100":
        mo_solid_id = mat_id
        break

if mo_solid_id is not None and mo_solid_id in result_3d.solid_results:
    sr = result_3d.solid_results[mo_solid_id]
    tc99m = sr.isotope_results.get("Tc-99m")
    if tc99m:
        print("=== Target (Mo-100) — Tc-99m Production ===")
        print(f"  Production rate:     {tc99m.production_rate:.3e} /s")
        print(f"  Activity (24h + 24h): {tc99m.activity_Bq:.3e} Bq")
        print(f"                      = {tc99m.activity_Bq / 3.7e10:.3f} Ci")
        print(f"                      = {tc99m.activity_Bq * 1e-9:.2f} GBq")
        print(f"  Sat. yield:          {tc99m.saturation_yield_Bq_uA:.3e} Bq/µA")
    else:
        print("Tc-99m not found in Mo-100 solid results")
        print("Available isotopes:", list(sr.isotope_results.keys())[:10])

print("\n=== Beam dump (Cu) — Activation ===")
cu_solid_id = None
for mat_id, mat in mesh.materials.items():
    if mat.name == "Cu":
        cu_solid_id = mat_id
        break
if cu_solid_id is not None and cu_solid_id in result_3d.solid_results:
    sr = result_3d.solid_results[cu_solid_id]
    sorted_iso = sorted(sr.isotope_results.values(), key=lambda x: x.activity_Bq, reverse=True)
    for iso in sorted_iso[:5]:
        if iso.activity_Bq > 0:
            print(f"  {iso.name:<12} {iso.activity_Bq:.3e} Bq")

"""3D STEP geometry support for HYRR.

Provides STEP file import (build123d), tetrahedral meshing (tetgen),
and ray casting for pencil-beam transport through non-planar target
assemblies.

This module requires optional dependencies::

    pip install hyrr[geometry]

All heavy imports (build123d, tetgen, meshio) are lazy-loaded.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path

import numpy as np
import numpy.typing as npt


# ---------------------------------------------------------------------------
# Data structures
# ---------------------------------------------------------------------------


@dataclass
class MaterialInfo:
    """Material description for a mesh region."""

    name: str
    composition: list[tuple[int, float]]  # (Z, mass_fraction)
    density_g_cm3: float
    atomic_masses: dict[int, float]  # Z → avg A (for straggling)


@dataclass
class TetrahedralMesh:
    """Tetrahedral mesh with per-element material assignment."""

    nodes: npt.NDArray[np.float64]  # (N, 3) [cm]
    elements: npt.NDArray[np.int64]  # (M, 4) vertex indices
    material_ids: npt.NDArray[np.int32]  # (M,) per-tet
    materials: dict[int, MaterialInfo]


@dataclass
class RaySegment:
    """One segment of a ray through a single tetrahedron."""

    tet_index: int
    entry_point: npt.NDArray[np.float64]
    exit_point: npt.NDArray[np.float64]
    path_length_cm: float
    material_id: int


# ---------------------------------------------------------------------------
# Lazy imports
# ---------------------------------------------------------------------------

def _require_build123d():
    try:
        import build123d  # noqa: F811
        return build123d
    except ImportError:
        raise ImportError(
            "build123d is required for STEP geometry support. "
            "Install it with: pip install hyrr[geometry]"
        ) from None


def _require_tetgen():
    try:
        import tetgen  # noqa: F811
        return tetgen
    except ImportError:
        raise ImportError(
            "tetgen is required for mesh generation. "
            "Install it with: pip install hyrr[geometry]"
        ) from None


# ---------------------------------------------------------------------------
# STEP import
# ---------------------------------------------------------------------------

def import_step(path: str | Path) -> list[tuple[object, str]]:
    """Import solids from a STEP file.

    Args:
        path: Path to .step or .stp file.

    Returns:
        List of (solid, name) tuples. Each solid is a build123d Shape.
    """
    b3d = _require_build123d()
    path = Path(path)
    if not path.exists():
        raise FileNotFoundError(f"STEP file not found: {path}")

    importer = b3d.importers.import_step(str(path))
    solids = importer.solids()
    result = []
    for i, solid in enumerate(solids):
        name = getattr(solid, "label", None) or f"solid_{i}"
        result.append((solid, name))
    return result


def assign_materials(
    solids: list[tuple[object, str]],
    material_map: dict[str, MaterialInfo],
) -> list[tuple[object, MaterialInfo]]:
    """Assign materials to solids by name.

    Args:
        solids: List of (solid, name) from import_step.
        material_map: Mapping solid_name → MaterialInfo.

    Returns:
        List of (solid, MaterialInfo) pairs.

    Raises:
        KeyError: If a solid name is not found in material_map.
    """
    result = []
    for solid, name in solids:
        if name not in material_map:
            raise KeyError(
                f"No material assigned for solid '{name}'. "
                f"Available: {list(material_map.keys())}"
            )
        result.append((solid, material_map[name]))
    return result


# ---------------------------------------------------------------------------
# Tessellation
# ---------------------------------------------------------------------------

def tessellate(
    solids_with_materials: list[tuple[object, MaterialInfo]],
    max_volume: float = 0.001,
) -> TetrahedralMesh:
    """Generate a tetrahedral mesh from solids using TetGen.

    Args:
        solids_with_materials: List of (solid, MaterialInfo) from assign_materials.
        max_volume: Maximum tetrahedron volume [cm³].

    Returns:
        TetrahedralMesh with material IDs assigned per tet.
    """
    b3d = _require_build123d()
    tg = _require_tetgen()

    all_nodes = []
    all_elements = []
    all_material_ids = []
    materials: dict[int, MaterialInfo] = {}
    node_offset = 0

    for mat_id, (solid, mat_info) in enumerate(solids_with_materials):
        materials[mat_id] = mat_info

        # Tessellate surface
        mesh_data = solid.tessellate(tolerance=0.01)
        vertices = np.array(mesh_data[0], dtype=np.float64)
        faces = np.array(mesh_data[1], dtype=np.int64)

        # Use TetGen for volumetric meshing
        tet = tg.TetGen(vertices, faces)
        tet.tetrahedralize(
            order=1,
            mindihedral=10,
            maxvolume=max_volume,
            verbose=0,
        )

        nodes = tet.node
        elements = tet.elem

        all_nodes.append(nodes)
        all_elements.append(elements + node_offset)
        all_material_ids.append(np.full(len(elements), mat_id, dtype=np.int32))
        node_offset += len(nodes)

    return TetrahedralMesh(
        nodes=np.vstack(all_nodes),
        elements=np.vstack(all_elements),
        material_ids=np.concatenate(all_material_ids),
        materials=materials,
    )


# ---------------------------------------------------------------------------
# Ray casting (Möller-Trumbore)
# ---------------------------------------------------------------------------

def _ray_triangle_intersect(
    origin: npt.NDArray[np.float64],
    direction: npt.NDArray[np.float64],
    v0: npt.NDArray[np.float64],
    v1: npt.NDArray[np.float64],
    v2: npt.NDArray[np.float64],
    eps: float = 1e-12,
) -> float | None:
    """Möller-Trumbore ray-triangle intersection.

    Returns:
        Parameter t >= 0 at intersection, or None if no hit.
    """
    edge1 = v1 - v0
    edge2 = v2 - v0
    pvec = np.cross(direction, edge2)
    det = np.dot(edge1, pvec)

    if abs(det) < eps:
        return None

    inv_det = 1.0 / det
    tvec = origin - v0
    u = np.dot(tvec, pvec) * inv_det
    if u < -eps or u > 1.0 + eps:
        return None

    qvec = np.cross(tvec, edge1)
    v = np.dot(direction, qvec) * inv_det
    if v < -eps or u + v > 1.0 + eps:
        return None

    t = np.dot(edge2, qvec) * inv_det
    if t < -eps:
        return None

    return float(t)


def _tet_faces(element: npt.NDArray[np.int64]) -> list[tuple[int, int, int]]:
    """Return the 4 triangular face index triples of a tetrahedron."""
    a, b, c, d = element
    return [(a, b, c), (a, b, d), (a, c, d), (b, c, d)]


def cast_ray(
    mesh: TetrahedralMesh,
    origin: npt.NDArray[np.float64],
    direction: npt.NDArray[np.float64],
) -> list[RaySegment]:
    """Cast a ray through a tetrahedral mesh.

    Uses brute-force Möller-Trumbore on all tet faces.
    For large meshes, an AABB tree should be used instead.

    Args:
        mesh: The tetrahedral mesh.
        origin: Ray origin [cm], shape (3,).
        direction: Ray direction (unit vector), shape (3,).

    Returns:
        Sorted list of RaySegments along the ray.
    """
    direction = direction / np.linalg.norm(direction)

    # Collect all intersection t-values per tet
    tet_hits: dict[int, list[float]] = {}

    for tet_idx in range(len(mesh.elements)):
        elem = mesh.elements[tet_idx]
        faces = _tet_faces(elem)
        hits = []
        for f in faces:
            v0 = mesh.nodes[f[0]]
            v1 = mesh.nodes[f[1]]
            v2 = mesh.nodes[f[2]]
            t = _ray_triangle_intersect(origin, direction, v0, v1, v2)
            if t is not None and t >= 0:
                # Deduplicate close hits (shared faces)
                if not any(abs(t - h) < 1e-10 for h in hits):
                    hits.append(t)
        if len(hits) >= 2:
            hits.sort()
            tet_hits[tet_idx] = hits

    # Build segments
    segments: list[RaySegment] = []
    for tet_idx, hits in tet_hits.items():
        t_in = hits[0]
        t_out = hits[-1]
        entry = origin + t_in * direction
        exit_ = origin + t_out * direction
        path_length = float(np.linalg.norm(exit_ - entry))
        if path_length > 1e-12:
            segments.append(RaySegment(
                tet_index=tet_idx,
                entry_point=entry,
                exit_point=exit_,
                path_length_cm=path_length,
                material_id=int(mesh.material_ids[tet_idx]),
            ))

    # Sort by entry distance
    segments.sort(key=lambda s: float(np.dot(s.entry_point - origin, direction)))
    return segments


def cast_pencil_beam(
    mesh: TetrahedralMesh,
    center: npt.NDArray[np.float64],
    direction: npt.NDArray[np.float64],
    radius_cm: float,
    n_rays: int = 19,
) -> list[list[RaySegment]]:
    """Cast a pencil beam (hexagonal ray pattern) through a mesh.

    Args:
        mesh: The tetrahedral mesh.
        center: Beam center point [cm], shape (3,).
        direction: Beam direction (unit vector), shape (3,).
        radius_cm: Beam radius [cm].
        n_rays: Number of rays (including center). Arranged in hexagonal rings.

    Returns:
        List of ray results, each a list of RaySegments.
    """
    direction = direction / np.linalg.norm(direction)

    # Build orthonormal basis perpendicular to direction
    if abs(direction[0]) < 0.9:
        perp = np.cross(direction, np.array([1.0, 0.0, 0.0]))
    else:
        perp = np.cross(direction, np.array([0.0, 1.0, 0.0]))
    perp = perp / np.linalg.norm(perp)
    perp2 = np.cross(direction, perp)

    # Generate hex pattern offsets
    offsets = [np.zeros(3)]  # center ray
    remaining = n_rays - 1
    ring = 1
    while remaining > 0:
        n_in_ring = min(6 * ring, remaining)
        for i in range(n_in_ring):
            angle = 2.0 * np.pi * i / (6 * ring)
            r = radius_cm * ring / max(1, int(np.ceil(np.sqrt(n_rays / np.pi))))
            r = min(r, radius_cm)
            offset = r * (np.cos(angle) * perp + np.sin(angle) * perp2)
            offsets.append(offset)
        remaining -= n_in_ring
        ring += 1

    results = []
    for offset in offsets[:n_rays]:
        ray_origin = center + offset
        segments = cast_ray(mesh, ray_origin, direction)
        results.append(segments)

    return results


# ---------------------------------------------------------------------------
# Convenience pipeline
# ---------------------------------------------------------------------------

def step_to_mesh(
    path: str | Path,
    material_map: dict[str, MaterialInfo],
    max_volume: float = 0.001,
) -> TetrahedralMesh:
    """Import a STEP file and generate a tetrahedral mesh.

    Convenience function combining import_step → assign_materials → tessellate.

    Args:
        path: Path to .step/.stp file.
        material_map: Mapping solid_name → MaterialInfo.
        max_volume: Maximum tet volume [cm³].

    Returns:
        TetrahedralMesh ready for ray casting.
    """
    solids = import_step(path)
    assigned = assign_materials(solids, material_map)
    return tessellate(assigned, max_volume)


# ---------------------------------------------------------------------------
# Mesh cross-section cutting
# ---------------------------------------------------------------------------


@dataclass
class MeshSlicePolygon:
    """Polygon from cutting a tetrahedron with a plane."""

    vertices_2d: npt.NDArray[np.float64]  # (N, 2) projected onto plane coords
    vertices_3d: npt.NDArray[np.float64]  # (N, 3) in original coords
    tet_index: int
    material_id: int


def _order_polygon_vertices(
    vertices_2d: npt.NDArray[np.float64],
) -> npt.NDArray[np.int64]:
    """Order 2D polygon vertices counterclockwise by angle from centroid.

    Args:
        vertices_2d: (N, 2) array of 2D vertex coordinates.

    Returns:
        Index array that sorts vertices counterclockwise.
    """
    centroid = vertices_2d.mean(axis=0)
    dx = vertices_2d[:, 0] - centroid[0]
    dy = vertices_2d[:, 1] - centroid[1]
    angles = np.arctan2(dy, dx)
    return np.argsort(angles)


def _build_plane_basis(
    normal: npt.NDArray[np.float64],
) -> tuple[npt.NDArray[np.float64], npt.NDArray[np.float64]]:
    """Build an orthonormal basis (u, v) for a plane given its normal.

    Args:
        normal: Unit normal vector, shape (3,).

    Returns:
        Tuple (u, v) of orthonormal vectors spanning the plane.
    """
    # Pick a vector not parallel to normal
    if abs(normal[0]) < 0.9:
        ref = np.array([1.0, 0.0, 0.0])
    else:
        ref = np.array([0.0, 1.0, 0.0])
    u = np.cross(normal, ref)
    u = u / np.linalg.norm(u)
    v = np.cross(normal, u)
    v = v / np.linalg.norm(v)
    return u, v


def _project_to_plane(
    points_3d: npt.NDArray[np.float64],
    point_on_plane: npt.NDArray[np.float64],
    u: npt.NDArray[np.float64],
    v: npt.NDArray[np.float64],
) -> npt.NDArray[np.float64]:
    """Project 3D points onto 2D plane coordinates.

    Args:
        points_3d: (N, 3) array of 3D points.
        point_on_plane: A point on the plane, shape (3,).
        u: First basis vector of the plane, shape (3,).
        v: Second basis vector of the plane, shape (3,).

    Returns:
        (N, 2) array of 2D coordinates in the plane.
    """
    rel = points_3d - point_on_plane
    coords_2d = np.column_stack([rel @ u, rel @ v])
    return coords_2d


def cut_mesh_with_plane(
    mesh: TetrahedralMesh,
    point: npt.NDArray[np.float64],
    normal: npt.NDArray[np.float64],
) -> list[MeshSlicePolygon]:
    """Cut a tetrahedral mesh with a plane, returning intersection polygons.

    For each tetrahedron, classifies its 4 vertices as above or below the
    cutting plane (by signed distance). If all vertices are on the same side,
    the tet is skipped. Otherwise, edge crossings are found by linear
    interpolation and the resulting 3- or 4-sided polygon is returned.

    Args:
        mesh: The tetrahedral mesh to cut.
        point: A point on the cutting plane, shape (3,).
        normal: Normal vector of the cutting plane, shape (3,). Will be
            normalized internally.

    Returns:
        List of MeshSlicePolygon, one per intersected tetrahedron.
    """
    normal = normal / np.linalg.norm(normal)
    point = np.asarray(point, dtype=np.float64)

    # Build orthonormal basis for 2D projection
    u, v = _build_plane_basis(normal)

    # Signed distances of ALL nodes to plane (vectorised)
    signed_dists = (mesh.nodes - point) @ normal  # (N_nodes,)

    polygons: list[MeshSlicePolygon] = []

    for tet_idx in range(len(mesh.elements)):
        elem = mesh.elements[tet_idx]
        dists = signed_dists[elem]  # (4,)

        # Classify: all above or all below → skip
        above = dists > 0
        below = dists < 0
        if above.all() or below.all():
            continue
        # All exactly on the plane is degenerate → skip
        on_plane = np.abs(dists) < 1e-14
        if on_plane.all():
            continue

        # Find edge crossings
        # Tet has 6 edges: (0,1), (0,2), (0,3), (1,2), (1,3), (2,3)
        edges = [(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)]
        intersection_pts: list[npt.NDArray[np.float64]] = []

        for i, j in edges:
            di, dj = dists[i], dists[j]
            # Sign change → crossing (treat zero as on-plane)
            if di * dj < 0:
                # Linear interpolation: find t where d = 0
                t = di / (di - dj)
                pt = mesh.nodes[elem[i]] * (1.0 - t) + mesh.nodes[elem[j]] * t
                intersection_pts.append(pt)
            elif abs(di) < 1e-14:
                # Vertex i is on the plane
                node_pt = mesh.nodes[elem[i]]
                # Avoid duplicates
                if not any(
                    np.linalg.norm(node_pt - p) < 1e-12 for p in intersection_pts
                ):
                    intersection_pts.append(node_pt)
            elif abs(dj) < 1e-14:
                node_pt = mesh.nodes[elem[j]]
                if not any(
                    np.linalg.norm(node_pt - p) < 1e-12 for p in intersection_pts
                ):
                    intersection_pts.append(node_pt)

        if len(intersection_pts) < 3:
            continue

        pts_3d = np.array(intersection_pts, dtype=np.float64)
        pts_2d = _project_to_plane(pts_3d, point, u, v)

        # Order vertices counterclockwise
        order = _order_polygon_vertices(pts_2d)
        pts_3d = pts_3d[order]
        pts_2d = pts_2d[order]

        polygons.append(MeshSlicePolygon(
            vertices_2d=pts_2d,
            vertices_3d=pts_3d,
            tet_index=tet_idx,
            material_id=int(mesh.material_ids[tet_idx]),
        ))

    return polygons


def axial_slice(
    mesh: TetrahedralMesh,
    beam_direction: npt.NDArray[np.float64],
    beam_position: npt.NDArray[np.float64],
    depth_cm: float,
) -> tuple[npt.NDArray[np.float64], npt.NDArray[np.float64], list[MeshSlicePolygon]]:
    """Cut perpendicular to beam at given depth.

    The cutting plane is perpendicular to the beam direction, located at
    ``beam_position + depth_cm * beam_direction``.

    Args:
        mesh: The tetrahedral mesh to cut.
        beam_direction: Beam direction (unit vector), shape (3,).
        beam_position: Beam origin point, shape (3,).
        depth_cm: Distance along beam direction to place the cut [cm].

    Returns:
        Tuple of (point_on_plane, normal, list_of_polygons).
    """
    beam_direction = np.asarray(beam_direction, dtype=np.float64)
    beam_position = np.asarray(beam_position, dtype=np.float64)
    beam_direction = beam_direction / np.linalg.norm(beam_direction)

    point = beam_position + depth_cm * beam_direction
    normal = beam_direction.copy()

    polygons = cut_mesh_with_plane(mesh, point, normal)
    return point, normal, polygons


def longitudinal_slice(
    mesh: TetrahedralMesh,
    beam_direction: npt.NDArray[np.float64],
    beam_position: npt.NDArray[np.float64],
    offset_cm: float = 0.0,
) -> tuple[npt.NDArray[np.float64], npt.NDArray[np.float64], list[MeshSlicePolygon]]:
    """Cut along beam axis through center (or offset).

    The cutting plane contains the beam axis and an arbitrary perpendicular
    direction. If ``offset_cm != 0``, the plane is shifted perpendicular to
    both the beam direction and the cut plane normal.

    Args:
        mesh: The tetrahedral mesh to cut.
        beam_direction: Beam direction (unit vector), shape (3,).
        beam_position: Beam origin point, shape (3,).
        offset_cm: Lateral offset from beam center [cm]. Positive shifts the
            plane in the direction perpendicular to both beam and cut normal.

    Returns:
        Tuple of (point_on_plane, normal, list_of_polygons).
    """
    beam_direction = np.asarray(beam_direction, dtype=np.float64)
    beam_position = np.asarray(beam_position, dtype=np.float64)
    beam_direction = beam_direction / np.linalg.norm(beam_direction)

    # Build a perpendicular direction to use as the cut plane normal.
    # The cut plane contains the beam axis, so its normal is perpendicular
    # to the beam direction.
    if abs(beam_direction[0]) < 0.9:
        ref = np.array([1.0, 0.0, 0.0])
    else:
        ref = np.array([0.0, 1.0, 0.0])
    normal = np.cross(beam_direction, ref)
    normal = normal / np.linalg.norm(normal)

    # The offset direction is perpendicular to both beam and normal
    offset_dir = np.cross(beam_direction, normal)
    offset_dir = offset_dir / np.linalg.norm(offset_dir)

    point = beam_position + offset_cm * offset_dir

    polygons = cut_mesh_with_plane(mesh, point, normal)
    return point, normal, polygons

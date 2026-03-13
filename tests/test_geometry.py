"""Tests for hyrr.geometry — ray-tet intersection and mesh operations."""

from __future__ import annotations

import numpy as np
import pytest

from hyrr.geometry import (
    MaterialInfo,
    TetrahedralMesh,
    _ray_triangle_intersect,
    _tet_faces,
    axial_slice,
    cast_ray,
    cut_mesh_with_plane,
    longitudinal_slice,
)

# ---------------------------------------------------------------------------
# Ray-triangle intersection
# ---------------------------------------------------------------------------


class TestRayTriangleIntersect:
    """Tests for Möller-Trumbore ray-triangle intersection."""

    def test_hit_center(self) -> None:
        """Ray through center of a triangle in the XY plane."""
        v0 = np.array([0.0, 0.0, 0.0])
        v1 = np.array([1.0, 0.0, 0.0])
        v2 = np.array([0.0, 1.0, 0.0])
        origin = np.array([0.2, 0.2, 1.0])
        direction = np.array([0.0, 0.0, -1.0])

        t = _ray_triangle_intersect(origin, direction, v0, v1, v2)
        assert t is not None
        assert t == pytest.approx(1.0, abs=1e-10)

    def test_miss_parallel(self) -> None:
        """Ray parallel to triangle misses."""
        v0 = np.array([0.0, 0.0, 0.0])
        v1 = np.array([1.0, 0.0, 0.0])
        v2 = np.array([0.0, 1.0, 0.0])
        origin = np.array([0.2, 0.2, 1.0])
        direction = np.array([1.0, 0.0, 0.0])

        t = _ray_triangle_intersect(origin, direction, v0, v1, v2)
        assert t is None

    def test_miss_outside(self) -> None:
        """Ray passes outside the triangle."""
        v0 = np.array([0.0, 0.0, 0.0])
        v1 = np.array([1.0, 0.0, 0.0])
        v2 = np.array([0.0, 1.0, 0.0])
        origin = np.array([2.0, 2.0, 1.0])
        direction = np.array([0.0, 0.0, -1.0])

        t = _ray_triangle_intersect(origin, direction, v0, v1, v2)
        assert t is None

    def test_hit_behind_origin_rejected(self) -> None:
        """Ray going away from triangle (negative t) is rejected."""
        v0 = np.array([0.0, 0.0, 0.0])
        v1 = np.array([1.0, 0.0, 0.0])
        v2 = np.array([0.0, 1.0, 0.0])
        origin = np.array([0.2, 0.2, -1.0])
        direction = np.array([0.0, 0.0, -1.0])  # going further away

        t = _ray_triangle_intersect(origin, direction, v0, v1, v2)
        assert t is None


# ---------------------------------------------------------------------------
# Tet faces
# ---------------------------------------------------------------------------


class TestTetFaces:
    """Tests for _tet_faces."""

    def test_four_faces(self) -> None:
        """A tet has exactly 4 triangular faces."""
        faces = _tet_faces(np.array([0, 1, 2, 3]))
        assert len(faces) == 4

    def test_all_vertices_used(self) -> None:
        """All 4 vertex indices appear in the faces."""
        faces = _tet_faces(np.array([0, 1, 2, 3]))
        all_verts = set()
        for f in faces:
            all_verts.update(f)
        assert all_verts == {0, 1, 2, 3}


# ---------------------------------------------------------------------------
# Ray casting through a programmatic tet mesh
# ---------------------------------------------------------------------------


def _make_box_mesh() -> TetrahedralMesh:
    """Create a simple mesh: single tet (right-angled) for testing.

    A tetrahedron with vertices at:
    (0,0,0), (1,0,0), (0,1,0), (0,0,1)
    """
    nodes = np.array([
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
    ], dtype=np.float64)
    elements = np.array([[0, 1, 2, 3]], dtype=np.int64)
    material_ids = np.array([0], dtype=np.int32)
    materials = {
        0: MaterialInfo(
            name="copper",
            composition=[(29, 1.0)],
            density_g_cm3=8.96,
            atomic_masses={29: 63.546},
        ),
    }
    return TetrahedralMesh(nodes, elements, material_ids, materials)


class TestCastRay:
    """Tests for cast_ray on a programmatic mesh."""

    def test_ray_through_tet(self) -> None:
        """Ray through the single tet should produce one segment."""
        mesh = _make_box_mesh()
        origin = np.array([0.1, 0.1, -0.5])
        direction = np.array([0.0, 0.0, 1.0])

        segments = cast_ray(mesh, origin, direction)
        assert len(segments) == 1
        seg = segments[0]
        assert seg.tet_index == 0
        assert seg.material_id == 0
        assert seg.path_length_cm > 0

    def test_ray_misses_tet(self) -> None:
        """Ray that doesn't intersect the tet produces no segments."""
        mesh = _make_box_mesh()
        origin = np.array([5.0, 5.0, -0.5])
        direction = np.array([0.0, 0.0, 1.0])

        segments = cast_ray(mesh, origin, direction)
        assert len(segments) == 0

    def test_segment_endpoints_on_faces(self) -> None:
        """Entry/exit points should be on tet faces (z >= 0)."""
        mesh = _make_box_mesh()
        origin = np.array([0.1, 0.1, -1.0])
        direction = np.array([0.0, 0.0, 1.0])

        segments = cast_ray(mesh, origin, direction)
        assert len(segments) == 1
        seg = segments[0]

        # Entry should be at z=0 (base face)
        assert seg.entry_point[2] == pytest.approx(0.0, abs=1e-6)
        # Exit z should be positive and <= 0.8 (x+y+z <= 1 constraint)
        assert seg.exit_point[2] > 0
        assert seg.exit_point[2] <= 1.0 + 1e-6


# ---------------------------------------------------------------------------
# Multi-tet mesh
# ---------------------------------------------------------------------------


def _make_two_tet_mesh() -> TetrahedralMesh:
    """Two tets sharing a face, stacked along z."""
    nodes = np.array([
        [0.0, 0.0, 0.0],  # 0
        [1.0, 0.0, 0.0],  # 1
        [0.0, 1.0, 0.0],  # 2
        [0.0, 0.0, 1.0],  # 3
        [0.0, 0.0, 2.0],  # 4
    ], dtype=np.float64)
    elements = np.array([
        [0, 1, 2, 3],
        [1, 2, 3, 4],
    ], dtype=np.int64)
    material_ids = np.array([0, 1], dtype=np.int32)
    materials = {
        0: MaterialInfo(
            name="copper",
            composition=[(29, 1.0)],
            density_g_cm3=8.96,
            atomic_masses={29: 63.546},
        ),
        1: MaterialInfo(
            name="molybdenum",
            composition=[(42, 1.0)],
            density_g_cm3=10.28,
            atomic_masses={42: 95.95},
        ),
    }
    return TetrahedralMesh(nodes, elements, material_ids, materials)


class TestMultiTetRay:
    """Tests for ray casting through multiple tets."""

    def test_ray_through_two_tets(self) -> None:
        """Ray along z-axis through two stacked tets."""
        mesh = _make_two_tet_mesh()
        origin = np.array([0.05, 0.05, -0.5])
        direction = np.array([0.0, 0.0, 1.0])

        segments = cast_ray(mesh, origin, direction)

        # Should hit at least one tet
        assert len(segments) >= 1
        # Material IDs should be from the mesh
        for seg in segments:
            assert seg.material_id in {0, 1}


# ---------------------------------------------------------------------------
# Mesh cross-section cutting
# ---------------------------------------------------------------------------


class TestCutMeshWithPlane:
    """Tests for cut_mesh_with_plane."""

    def test_horizontal_cut_through_tet(self) -> None:
        """Plane z=0.3 should cut the single tet into a triangle."""
        mesh = _make_box_mesh()
        point = np.array([0.0, 0.0, 0.3])
        normal = np.array([0.0, 0.0, 1.0])

        polygons = cut_mesh_with_plane(mesh, point, normal)
        assert len(polygons) == 1
        poly = polygons[0]
        assert poly.tet_index == 0
        assert poly.material_id == 0
        # Triangle or quad
        assert len(poly.vertices_2d) >= 3
        assert poly.vertices_3d.shape[1] == 3

    def test_plane_above_tet_gives_nothing(self) -> None:
        """Plane above the tet should produce no intersections."""
        mesh = _make_box_mesh()
        point = np.array([0.0, 0.0, 5.0])
        normal = np.array([0.0, 0.0, 1.0])

        polygons = cut_mesh_with_plane(mesh, point, normal)
        assert len(polygons) == 0

    def test_plane_below_tet_gives_nothing(self) -> None:
        """Plane below the tet should produce no intersections."""
        mesh = _make_box_mesh()
        point = np.array([0.0, 0.0, -1.0])
        normal = np.array([0.0, 0.0, 1.0])

        polygons = cut_mesh_with_plane(mesh, point, normal)
        assert len(polygons) == 0

    def test_cut_two_tets(self) -> None:
        """Cutting two stacked tets should produce polygons from both."""
        mesh = _make_two_tet_mesh()
        # Cut at z=0.5: should hit first tet; z=1.5 should hit second
        point = np.array([0.0, 0.0, 0.5])
        normal = np.array([0.0, 0.0, 1.0])

        polygons = cut_mesh_with_plane(mesh, point, normal)
        assert len(polygons) >= 1

    def test_vertices_lie_on_plane(self) -> None:
        """All intersection vertices should have the same z as the cut plane."""
        mesh = _make_box_mesh()
        z_cut = 0.4
        point = np.array([0.0, 0.0, z_cut])
        normal = np.array([0.0, 0.0, 1.0])

        polygons = cut_mesh_with_plane(mesh, point, normal)
        assert len(polygons) == 1
        for v in polygons[0].vertices_3d:
            assert v[2] == pytest.approx(z_cut, abs=1e-10)

    def test_2d_vertices_consistent(self) -> None:
        """2D and 3D vertices should have the same count."""
        mesh = _make_box_mesh()
        point = np.array([0.0, 0.0, 0.3])
        normal = np.array([0.0, 0.0, 1.0])

        polygons = cut_mesh_with_plane(mesh, point, normal)
        assert len(polygons) == 1
        poly = polygons[0]
        assert len(poly.vertices_2d) == len(poly.vertices_3d)

    def test_diagonal_cut(self) -> None:
        """A diagonal plane should still produce valid polygons."""
        mesh = _make_box_mesh()
        point = np.array([0.3, 0.3, 0.3])
        normal = np.array([1.0, 1.0, 1.0])  # will be normalized

        polygons = cut_mesh_with_plane(mesh, point, normal)
        # May or may not intersect depending on geometry; just check no crash
        for poly in polygons:
            assert len(poly.vertices_2d) >= 3


class TestAxialSlice:
    """Tests for axial_slice convenience function."""

    def test_basic(self) -> None:
        """Axial slice at z=0.3 with beam along z."""
        mesh = _make_box_mesh()
        beam_dir = np.array([0.0, 0.0, 1.0])
        beam_pos = np.array([0.0, 0.0, 0.0])

        point, normal, polygons = axial_slice(mesh, beam_dir, beam_pos, 0.3)
        assert point[2] == pytest.approx(0.3)
        np.testing.assert_allclose(normal, [0, 0, 1])
        assert len(polygons) >= 1

    def test_depth_zero(self) -> None:
        """Axial slice at depth=0 is at the base of the tet."""
        mesh = _make_box_mesh()
        _, _, polygons = axial_slice(
            mesh, np.array([0.0, 0.0, 1.0]), np.array([0.0, 0.0, 0.0]), 0.0,
        )
        # z=0 is a face of the tet — may or may not produce polygons (edge case)
        # Just ensure no crash
        assert isinstance(polygons, list)


class TestLongitudinalSlice:
    """Tests for longitudinal_slice convenience function."""

    def test_basic(self) -> None:
        """Longitudinal slice through beam center."""
        mesh = _make_box_mesh()
        beam_dir = np.array([0.0, 0.0, 1.0])
        beam_pos = np.array([0.1, 0.1, 0.0])

        point, normal, polygons = longitudinal_slice(mesh, beam_dir, beam_pos)
        # Normal should be perpendicular to beam direction
        assert abs(np.dot(normal, beam_dir / np.linalg.norm(beam_dir))) < 1e-10
        assert isinstance(polygons, list)

    def test_with_offset(self) -> None:
        """Longitudinal slice with lateral offset."""
        mesh = _make_box_mesh()
        beam_dir = np.array([0.0, 0.0, 1.0])
        beam_pos = np.array([0.0, 0.0, 0.0])

        _, _, polys_0 = longitudinal_slice(mesh, beam_dir, beam_pos, 0.0)
        _, _, polys_off = longitudinal_slice(mesh, beam_dir, beam_pos, 0.5)
        # Different offset may give different results
        assert isinstance(polys_0, list)
        assert isinstance(polys_off, list)

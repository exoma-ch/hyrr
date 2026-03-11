"""Tests for hyrr.projectile module."""

from __future__ import annotations

import pytest

from hyrr.projectile import LIGHT_IONS, Projectile, resolve_projectile


class TestProjectile:
    """Tests for the Projectile dataclass."""

    def test_construction(self) -> None:
        p = Projectile(symbol="C-12", Z=6, A=12, charge_state=6)
        assert p.symbol == "C-12"
        assert p.Z == 6
        assert p.A == 12
        assert p.charge_state == 6

    def test_frozen(self) -> None:
        p = Projectile(symbol="p", Z=1, A=1, charge_state=1)
        with pytest.raises(AttributeError):
            p.Z = 2  # type: ignore[misc]


class TestLightIons:
    """Tests for the LIGHT_IONS lookup table."""

    def test_all_five_defined(self) -> None:
        assert set(LIGHT_IONS) == {"p", "d", "t", "h", "a"}

    @pytest.mark.parametrize(
        "name, Z, A, charge",
        [
            ("p", 1, 1, 1),
            ("d", 1, 2, 1),
            ("t", 1, 3, 1),
            ("h", 2, 3, 2),
            ("a", 2, 4, 2),
        ],
    )
    def test_light_ion_values(self, name: str, Z: int, A: int, charge: int) -> None:
        p = LIGHT_IONS[name]
        assert p.Z == Z
        assert p.A == A
        assert p.charge_state == charge


class TestResolveProjectile:
    """Tests for resolve_projectile."""

    @pytest.mark.parametrize("name", ["p", "d", "t", "h", "a"])
    def test_light_ions(self, name: str) -> None:
        p = resolve_projectile(name)
        assert p is LIGHT_IONS[name]

    def test_carbon_12(self) -> None:
        p = resolve_projectile("C-12")
        assert p == Projectile(symbol="C-12", Z=6, A=12, charge_state=6)

    def test_oxygen_16(self) -> None:
        p = resolve_projectile("O-16")
        assert p == Projectile(symbol="O-16", Z=8, A=16, charge_state=8)

    def test_neon_20(self) -> None:
        p = resolve_projectile("Ne-20")
        assert p == Projectile(symbol="Ne-20", Z=10, A=20, charge_state=10)

    def test_heavy_ion_charge_state_equals_Z(self) -> None:
        p = resolve_projectile("Si-28")
        assert p.charge_state == p.Z == 14

    def test_invalid_name(self) -> None:
        with pytest.raises(ValueError, match="Cannot parse projectile"):
            resolve_projectile("xyz")

    def test_unknown_element_symbol(self) -> None:
        with pytest.raises(ValueError, match="Unknown element symbol"):
            resolve_projectile("Qq-42")

    def test_empty_string(self) -> None:
        with pytest.raises(ValueError):
            resolve_projectile("")

    def test_numeric_only(self) -> None:
        with pytest.raises(ValueError):
            resolve_projectile("12")

    def test_missing_mass_number(self) -> None:
        with pytest.raises(ValueError):
            resolve_projectile("C-")

    def test_no_dash(self) -> None:
        with pytest.raises(ValueError):
            resolve_projectile("C12")

"""Tests for hyrr.chains — chain discovery and coupled ODE solver."""

from __future__ import annotations

import math

import numpy as np
import pytest

from hyrr.chains import ChainIsotope, discover_chains, solve_chain
from hyrr.db import DecayData, DecayMode
from hyrr.models import CurrentProfile

LN2 = math.log(2)


# ---------------------------------------------------------------------------
# ChainMockDB
# ---------------------------------------------------------------------------


class ChainMockDB:
    """Mock database with a small decay chain for testing.

    Chain: Sc-44m →(IT, BR=1.0)→ Sc-44 →(β+, BR=1.0)→ Ca-44 (stable)
    Half-lives: Sc-44m = 58.61 h = 210996 s, Sc-44 = 3.97 h = 14292 s
    """

    def __hash__(self) -> int:
        return id(self)

    def __eq__(self, other: object) -> bool:
        return self is other

    def get_cross_sections(self, projectile: str, target_Z: int, target_A: int) -> list:
        return []

    def get_stopping_power(self, source: str, target_Z: int) -> tuple:
        return np.array([]), np.array([])

    def get_natural_abundances(self, Z: int) -> dict:
        return {}

    def get_decay_data(self, Z: int, A: int, state: str = "") -> DecayData | None:
        if Z == 21 and A == 44 and state == "m":
            return DecayData(
                Z=21, A=44, state="m",
                half_life_s=210996.0,
                decay_modes=[
                    DecayMode(
                        mode="IT",
                        daughter_Z=21, daughter_A=44,
                        daughter_state="",
                        branching=1.0,
                    ),
                ],
            )
        if Z == 21 and A == 44 and state == "":
            return DecayData(
                Z=21, A=44, state="",
                half_life_s=14292.0,
                decay_modes=[
                    DecayMode(
                        mode="beta+",
                        daughter_Z=20, daughter_A=44,
                        daughter_state="",
                        branching=1.0,
                    ),
                ],
            )
        if Z == 20 and A == 44 and state == "":
            return DecayData(
                Z=20, A=44, state="",
                half_life_s=None,
                decay_modes=[
                    DecayMode(
                        mode="stable",
                        daughter_Z=None, daughter_A=None,
                        daughter_state="",
                        branching=1.0,
                    ),
                ],
            )
        return None

    def get_element_symbol(self, Z: int) -> str:
        return {20: "Ca", 21: "Sc"}.get(Z, f"Z{Z}")

    def get_element_Z(self, symbol: str) -> int:
        return {"Ca": 20, "Sc": 21}.get(symbol, 0)


# ---------------------------------------------------------------------------
# Chain discovery tests
# ---------------------------------------------------------------------------


class TestDiscoverChains:
    """Tests for discover_chains function."""

    def test_discovers_full_chain(self) -> None:
        """Starting from Sc-44m, should discover Sc-44m, Sc-44, Ca-44."""
        db = ChainMockDB()
        chain = discover_chains(db, [(21, 44, "m", 1e8)])
        keys = [iso.key for iso in chain]
        assert (21, 44, "m") in keys
        assert (21, 44, "") in keys
        assert (20, 44, "") in keys
        assert len(chain) == 3

    def test_topological_order(self) -> None:
        """Parents should come before daughters in the chain."""
        db = ChainMockDB()
        chain = discover_chains(db, [(21, 44, "m", 1e8)])
        keys = [iso.key for iso in chain]
        # Sc-44m before Sc-44
        assert keys.index((21, 44, "m")) < keys.index((21, 44, ""))
        # Sc-44 before Ca-44
        assert keys.index((21, 44, "")) < keys.index((20, 44, ""))

    def test_production_rate_assignment(self) -> None:
        """Only directly-produced isotopes should have nonzero rates."""
        db = ChainMockDB()
        chain = discover_chains(db, [(21, 44, "m", 1e8)])
        iso_map = {iso.key: iso for iso in chain}
        assert iso_map[(21, 44, "m")].production_rate == 1e8
        assert iso_map[(21, 44, "")].production_rate == 0.0
        assert iso_map[(20, 44, "")].production_rate == 0.0

    def test_merges_duplicate_production(self) -> None:
        """Same isotope from multiple sources should have summed rates."""
        db = ChainMockDB()
        chain = discover_chains(db, [
            (21, 44, "m", 1e8),
            (21, 44, "m", 2e8),
        ])
        iso_map = {iso.key: iso for iso in chain}
        assert iso_map[(21, 44, "m")].production_rate == pytest.approx(3e8)

    def test_multiple_direct_isotopes(self) -> None:
        """Two different directly-produced isotopes in the same chain."""
        db = ChainMockDB()
        chain = discover_chains(db, [
            (21, 44, "m", 1e8),
            (21, 44, "", 5e7),  # also directly produced
        ])
        iso_map = {iso.key: iso for iso in chain}
        assert iso_map[(21, 44, "m")].production_rate == pytest.approx(1e8)
        assert iso_map[(21, 44, "")].production_rate == pytest.approx(5e7)

    def test_max_depth_limits(self) -> None:
        """max_depth=0 should not discover any daughters."""
        db = ChainMockDB()
        chain = discover_chains(db, [(21, 44, "m", 1e8)], max_depth=0)
        keys = [iso.key for iso in chain]
        assert (21, 44, "m") in keys
        # Daughters should not be discovered
        assert (21, 44, "") not in keys

    def test_stable_endpoint(self) -> None:
        """Ca-44 is stable — should be in chain but have no daughters."""
        db = ChainMockDB()
        chain = discover_chains(db, [(21, 44, "m", 1e8)])
        iso_map = {iso.key: iso for iso in chain}
        ca44 = iso_map[(20, 44, "")]
        assert ca44.is_stable


# ---------------------------------------------------------------------------
# Coupled solver tests
# ---------------------------------------------------------------------------


class TestSolveChain:
    """Tests for solve_chain function."""

    def test_single_isotope_matches_bateman(self) -> None:
        """Single isotope with no daughters should match independent Bateman."""
        half_life = 14292.0  # Sc-44
        rate = 1e8
        irr = 86400.0
        cool = 86400.0

        chain = [ChainIsotope(
            Z=21, A=44, state="",
            half_life_s=half_life,
            production_rate=rate,
            decay_modes=[],
        )]

        sol = solve_chain(chain, irr, cool, 0.0)
        lam = LN2 / half_life

        # Check end of irradiation activity
        eoi_idx = len(sol.time_grid_s) // 2 - 1
        expected_eoi = rate * (1 - math.exp(-lam * irr))
        assert sol.activities[0, eoi_idx] == pytest.approx(
            expected_eoi, rel=0.02,
        )

    def test_total_equals_direct_plus_ingrowth(self) -> None:
        """Total activity should equal direct + ingrowth at every time point."""
        db = ChainMockDB()
        chain = discover_chains(db, [(21, 44, "m", 1e8), (21, 44, "", 5e7)])
        sol = solve_chain(chain, 86400.0, 86400.0, 0.0)

        for i in range(len(chain)):
            total = sol.activities[i, :]
            direct = sol.activities_direct[i, :]
            ingrowth = sol.activities_ingrowth[i, :]
            np.testing.assert_allclose(
                direct + ingrowth, total,
                rtol=1e-6, atol=1e-3,
            )

    def test_daughter_ingrowth_nonzero(self) -> None:
        """Sc-44 should have nonzero ingrowth from Sc-44m IT decay."""
        db = ChainMockDB()
        chain = discover_chains(db, [(21, 44, "m", 1e8)])
        sol = solve_chain(chain, 86400.0, 86400.0, 0.0)

        # Find Sc-44 index
        sc44_idx = next(
            i for i, iso in enumerate(sol.isotopes)
            if iso.key == (21, 44, "")
        )

        # Sc-44 has no direct production, so direct should be zero
        assert np.max(sol.activities_direct[sc44_idx, :]) == pytest.approx(0.0)
        # Ingrowth should be nonzero
        assert np.max(sol.activities_ingrowth[sc44_idx, :]) > 0

    def test_stable_isotope_zero_activity(self) -> None:
        """Stable Ca-44 should have zero activity throughout."""
        db = ChainMockDB()
        chain = discover_chains(db, [(21, 44, "m", 1e8)])
        sol = solve_chain(chain, 86400.0, 86400.0, 0.0)

        ca44_idx = next(
            i for i, iso in enumerate(sol.isotopes)
            if iso.key == (20, 44, "")
        )
        np.testing.assert_allclose(
            sol.activities[ca44_idx, :], 0.0, atol=1e-10,
        )

    def test_empty_chain(self) -> None:
        """Empty chain should return empty solution."""
        sol = solve_chain([], 86400.0, 86400.0, 0.0)
        assert len(sol.time_grid_s) == 0
        assert sol.abundances.shape == (0, 0)

    def test_time_grid_structure(self) -> None:
        """Time grid should cover irradiation + cooling."""
        db = ChainMockDB()
        chain = discover_chains(db, [(21, 44, "m", 1e8)])
        sol = solve_chain(chain, 100.0, 200.0, 0.0, n_time_points=100)

        assert sol.time_grid_s[0] == 0.0
        assert sol.time_grid_s[-1] == pytest.approx(300.0)
        assert len(sol.time_grid_s) == 100

    def test_sc44m_sc44_coupled_activity(self) -> None:
        """Verify Sc-44 activity is higher with coupling than without.

        When Sc-44m decays to Sc-44 via IT, the coupled solver should show
        more Sc-44 activity than the independent Bateman for Sc-44 alone
        (which is zero since Sc-44 has no direct production here).
        """
        db = ChainMockDB()
        chain = discover_chains(db, [(21, 44, "m", 1e8)])
        sol = solve_chain(chain, 86400.0, 86400.0, 0.0)

        sc44_idx = next(
            i for i, iso in enumerate(sol.isotopes)
            if iso.key == (21, 44, "")
        )

        # Peak Sc-44 activity should be substantial
        peak_sc44 = float(np.max(sol.activities[sc44_idx, :]))
        assert peak_sc44 > 1e6  # should be significant for R=1e8


# ---------------------------------------------------------------------------
# CurrentProfile tests
# ---------------------------------------------------------------------------


class TestCurrentProfile:
    """Tests for CurrentProfile dataclass."""

    def test_valid_construction(self) -> None:
        cp = CurrentProfile(
            times_s=np.array([0.0, 100.0, 200.0]),
            currents_mA=np.array([0.1, 0.2, 0.15]),
        )
        assert len(cp.times_s) == 3

    def test_mismatched_lengths(self) -> None:
        with pytest.raises(ValueError, match="same length"):
            CurrentProfile(
                times_s=np.array([0.0, 100.0]),
                currents_mA=np.array([0.1]),
            )

    def test_empty(self) -> None:
        with pytest.raises(ValueError, match="at least one"):
            CurrentProfile(
                times_s=np.array([]),
                currents_mA=np.array([]),
            )

    def test_non_monotonic(self) -> None:
        with pytest.raises(ValueError, match="monotonically"):
            CurrentProfile(
                times_s=np.array([0.0, 200.0, 100.0]),
                currents_mA=np.array([0.1, 0.2, 0.15]),
            )

    def test_negative_current(self) -> None:
        with pytest.raises(ValueError, match="non-negative"):
            CurrentProfile(
                times_s=np.array([0.0, 100.0]),
                currents_mA=np.array([0.1, -0.1]),
            )

    def test_current_at(self) -> None:
        cp = CurrentProfile(
            times_s=np.array([0.0, 100.0, 200.0]),
            currents_mA=np.array([0.1, 0.2, 0.15]),
        )
        assert cp.current_at(0.0) == pytest.approx(0.1)
        assert cp.current_at(50.0) == pytest.approx(0.1)
        assert cp.current_at(100.0) == pytest.approx(0.2)
        assert cp.current_at(150.0) == pytest.approx(0.2)
        assert cp.current_at(250.0) == pytest.approx(0.15)

    def test_intervals(self) -> None:
        cp = CurrentProfile(
            times_s=np.array([0.0, 100.0, 200.0]),
            currents_mA=np.array([0.1, 0.2, 0.15]),
        )
        ivs = cp.intervals(300.0)
        assert len(ivs) == 3
        assert ivs[0] == (0.0, 100.0, 0.1)
        assert ivs[1] == (100.0, 200.0, 0.2)
        assert ivs[2] == (200.0, 300.0, 0.15)


# ---------------------------------------------------------------------------
# Solve chain with current profile
# ---------------------------------------------------------------------------


class TestSolveChainWithCurrentProfile:
    """Tests for solve_chain with time-varying beam current."""

    def test_constant_profile_matches_no_profile(self) -> None:
        """A constant current profile should give same result as no profile."""
        db = ChainMockDB()
        chain = discover_chains(db, [(21, 44, "m", 1e8)])
        nominal_mA = 0.15

        # Solve without profile
        sol_const = solve_chain(
            chain, 86400.0, 86400.0, 0.0,
            nominal_current_mA=nominal_mA,
        )

        # Solve with constant profile at nominal current
        cp = CurrentProfile(
            times_s=np.array([0.0]),
            currents_mA=np.array([nominal_mA]),
        )
        sol_profile = solve_chain(
            chain, 86400.0, 86400.0, 0.0,
            current_profile=cp,
            nominal_current_mA=nominal_mA,
        )

        np.testing.assert_allclose(
            sol_profile.activities, sol_const.activities,
            rtol=0.02,
        )

    def test_zero_current_interval_no_production(self) -> None:
        """If current is zero for the full irradiation, no activity."""
        chain = [ChainIsotope(
            Z=21, A=44, state="",
            half_life_s=14292.0,
            production_rate=1e8,
            decay_modes=[],
        )]

        cp = CurrentProfile(
            times_s=np.array([0.0]),
            currents_mA=np.array([0.0]),
        )
        sol = solve_chain(
            chain, 86400.0, 86400.0, 0.0,
            current_profile=cp,
            nominal_current_mA=0.15,
        )

        np.testing.assert_allclose(sol.activities, 0.0, atol=1e-10)

    def test_half_current_half_activity(self) -> None:
        """Half current for full irradiation should give ~half EOI activity.

        For a single isotope at saturation-like conditions (long irradiation
        relative to half-life), A_eoi ≈ R, so halving R halves A_eoi.
        """
        chain = [ChainIsotope(
            Z=21, A=44, state="",
            half_life_s=14292.0,  # ~4 hours
            production_rate=1e8,
            decay_modes=[],
        )]
        nominal_mA = 0.15
        irr = 86400.0 * 3  # 3 days >> half-life, near saturation

        # Full current
        sol_full = solve_chain(
            chain, irr, 0.0, 0.0,
            nominal_current_mA=nominal_mA,
        )

        # Half current via profile
        cp = CurrentProfile(
            times_s=np.array([0.0]),
            currents_mA=np.array([nominal_mA / 2]),
        )
        sol_half = solve_chain(
            chain, irr, 0.0, 0.0,
            current_profile=cp,
            nominal_current_mA=nominal_mA,
        )

        # At near-saturation, EOI activity ≈ R, so ratio should be ~0.5
        ratio = sol_half.activities[0, -1] / sol_full.activities[0, -1]
        assert ratio == pytest.approx(0.5, rel=0.05)

    def test_stepped_current_profile(self) -> None:
        """A stepped profile should produce different activity than constant."""
        db = ChainMockDB()
        chain = discover_chains(db, [(21, 44, "m", 1e8)])
        nominal_mA = 0.15
        irr = 86400.0

        # Constant
        sol_const = solve_chain(
            chain, irr, 86400.0, 0.0,
            nominal_current_mA=nominal_mA,
        )

        # Stepped: high then low
        cp = CurrentProfile(
            times_s=np.array([0.0, irr / 2]),
            currents_mA=np.array([nominal_mA * 2, nominal_mA * 0.5]),
        )
        sol_stepped = solve_chain(
            chain, irr, 86400.0, 0.0,
            current_profile=cp,
            nominal_current_mA=nominal_mA,
        )

        # Activities should differ (not equal to constant case)
        sc44m_idx = next(
            i for i, iso in enumerate(sol_const.isotopes)
            if iso.key == (21, 44, "m")
        )
        # The stepped profile has average current = 1.25 * nominal,
        # but time-weighting matters, so just check they differ
        eoi_const = sol_const.activities[sc44m_idx, len(sol_const.time_grid_s) // 2 - 1]
        eoi_stepped = sol_stepped.activities[sc44m_idx, len(sol_stepped.time_grid_s) // 2 - 1]
        assert eoi_const != pytest.approx(eoi_stepped, rel=0.01)

    def test_total_equals_direct_plus_ingrowth_with_profile(self) -> None:
        """Total = direct + ingrowth should hold with current profile."""
        db = ChainMockDB()
        chain = discover_chains(db, [(21, 44, "m", 1e8)])

        cp = CurrentProfile(
            times_s=np.array([0.0, 43200.0]),
            currents_mA=np.array([0.2, 0.1]),
        )
        sol = solve_chain(
            chain, 86400.0, 86400.0, 0.0,
            current_profile=cp,
            nominal_current_mA=0.15,
        )

        for i in range(len(chain)):
            total = sol.activities[i, :]
            direct = sol.activities_direct[i, :]
            ingrowth = sol.activities_ingrowth[i, :]
            np.testing.assert_allclose(
                direct + ingrowth, total,
                rtol=1e-4, atol=1e-3,
            )

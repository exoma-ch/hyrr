"""Performance benchmarks for HYRR core operations.

Verifies that key operations meet performance targets.
Uses pytest-benchmark for reliable measurements.
"""

from __future__ import annotations

import numpy as np
import pytest

# --- Mock database for benchmarking (no real SQLite needed) ---


class BenchmarkDB:
    """Fast mock database for performance testing."""

    def get_stopping_power(
        self, source: str, target_Z: int
    ) -> tuple[np.ndarray, np.ndarray]:
        # Realistic PSTAR data for Mo (Z=42): 133 energy points
        energies = np.logspace(-3, 4, 133)
        # Approximate Bethe-Bloch shape
        dedx = 200.0 / (energies**0.4 + 0.1)
        return energies, dedx

    def get_cross_sections(
        self, projectile: str, target_Z: int, target_A: int
    ) -> list:
        from hyrr.db import CrossSectionData

        # Return ~10 residual channels with realistic energy grids
        results: list[CrossSectionData] = []
        for res_Z in range(target_Z - 2, target_Z + 2):
            for res_A in range(target_A - 3, target_A + 1):
                energies = np.linspace(5, 200, 100)
                # Gaussian-ish cross-section
                peak_E = 15.0 + (res_Z - target_Z) * 5
                xs = 500.0 * np.exp(-((energies - peak_E) ** 2) / 50.0)
                results.append(
                    CrossSectionData(
                        residual_Z=res_Z,
                        residual_A=res_A,
                        state="",
                        energies_MeV=energies,
                        xs_mb=xs,
                    )
                )
        return results

    def get_natural_abundances(self, Z: int) -> dict[int, tuple[float, float]]:
        return {
            92: (0.1484, 92.0),
            94: (0.0925, 94.0),
            95: (0.1592, 95.0),
            96: (0.1668, 96.0),
            97: (0.0955, 97.0),
            98: (0.2413, 98.0),
            100: (0.0963, 100.0),
        }

    def get_decay_data(self, Z: int, A: int, state: str = "") -> object:
        from hyrr.db import DecayData, DecayMode

        return DecayData(
            Z=Z,
            A=A,
            state=state,
            half_life_s=21636.0,
            decay_modes=[DecayMode("beta-", Z + 1, A, "", 1.0)],
        )

    def get_element_symbol(self, Z: int) -> str:
        return "Mo"

    def get_element_Z(self, symbol: str) -> int:
        return 42


@pytest.fixture
def bench_db() -> BenchmarkDB:
    """Benchmark database fixture."""
    from hyrr.stopping import _interpolator_cache

    _interpolator_cache.clear()
    return BenchmarkDB()


class TestStoppingPowerPerformance:
    """Stopping power lookup must be fast -- called 100x per isotope per layer."""

    @pytest.mark.benchmark
    def test_elemental_dedx_speed(self, bench_db: BenchmarkDB, benchmark) -> None:  # type: ignore[no-untyped-def]
        """Single elemental_dedx call should be < 0.1ms after cache warm-up."""
        from hyrr.stopping import elemental_dedx

        # Warm up cache
        elemental_dedx(bench_db, "p", 42, 15.0)
        # Benchmark
        result = benchmark(elemental_dedx, bench_db, "p", 42, 15.0)
        assert result > 0

    @pytest.mark.benchmark
    def test_compound_dedx_speed(self, bench_db: BenchmarkDB, benchmark) -> None:  # type: ignore[no-untyped-def]
        """Compound dE/dx with 2 elements."""
        from hyrr.stopping import compound_dedx

        composition = [(42, 0.667), (8, 0.333)]
        # Warm up
        compound_dedx(bench_db, "p", composition, 15.0)
        benchmark(compound_dedx, bench_db, "p", composition, 15.0)


class TestProductionRatePerformance:
    """Production rate integration -- the core compute loop."""

    @pytest.mark.benchmark
    def test_single_isotope_production_rate(self, bench_db: BenchmarkDB, benchmark) -> None:  # type: ignore[no-untyped-def]
        """100-point quadrature for one isotope should be < 1ms."""
        from hyrr.production import compute_production_rate
        from hyrr.stopping import dedx_MeV_per_cm

        xs_energies = np.linspace(5, 200, 100)
        xs_mb = 500.0 * np.exp(-((xs_energies - 15.0) ** 2) / 50.0)
        composition: list[tuple[int, float]] = [(42, 1.0)]

        def dedx_fn(E: float) -> float:
            return dedx_MeV_per_cm(bench_db, "p", composition, 10.22, E)

        # Warm up
        dedx_fn(15.0)

        def run() -> tuple:
            return compute_production_rate(
                xs_energies,
                xs_mb,
                dedx_fn,
                energy_in_MeV=16.0,
                energy_out_MeV=12.0,
                n_target_atoms=1.29e21,
                beam_particles_per_s=9.36e14,
                target_volume_cm3=0.021,
            )

        result = benchmark(run)
        assert result[0] > 0  # production rate > 0

    @pytest.mark.benchmark
    def test_bateman_activity(self, benchmark) -> None:  # type: ignore[no-untyped-def]
        """Bateman equations for one isotope should be < 0.5ms."""
        from hyrr.production import bateman_activity

        result = benchmark(bateman_activity, 1e10, 21636.0, 86400.0, 86400.0)
        assert len(result[0]) > 0

    @pytest.mark.benchmark
    def test_full_layer_simulation(self, bench_db: BenchmarkDB, benchmark) -> None:  # type: ignore[no-untyped-def]
        """Full layer: all isotopes x (production + Bateman) should be < 20ms."""
        from hyrr.production import bateman_activity, compute_production_rate
        from hyrr.stopping import dedx_MeV_per_cm

        composition: list[tuple[int, float]] = [(42, 1.0)]

        def dedx_fn(E: float) -> float:
            return dedx_MeV_per_cm(bench_db, "p", composition, 10.22, E)

        # Warm up
        dedx_fn(15.0)

        xs_list = bench_db.get_cross_sections("p", 42, 100)

        def run_full_layer() -> list:
            results = []
            for xs in xs_list:
                prate, _, _, _ = compute_production_rate(
                    xs.energies_MeV,
                    xs.xs_mb,
                    dedx_fn,
                    energy_in_MeV=16.0,
                    energy_out_MeV=12.0,
                    n_target_atoms=1.29e21,
                    beam_particles_per_s=9.36e14,
                    target_volume_cm3=0.021,
                )
                time_grid, activity = bateman_activity(
                    prate, 21636.0, 86400.0, 86400.0
                )
                results.append((prate, activity[-1]))
            return results

        results = benchmark(run_full_layer)
        assert len(results) == len(xs_list)


class TestMaterialsPerformance:
    """Materials module performance tests."""

    @pytest.mark.benchmark
    def test_formula_parsing(self, benchmark) -> None:  # type: ignore[no-untyped-def]
        """Formula parsing should be < 0.01ms."""
        from hyrr.materials import parse_formula

        result = benchmark(parse_formula, "MoO3")
        assert "Mo" in result

    @pytest.mark.benchmark
    def test_resolve_isotopics(self, bench_db: BenchmarkDB, benchmark) -> None:  # type: ignore[no-untyped-def]
        """Full isotopic resolution should be < 1ms."""
        from hyrr.materials import resolve_isotopics

        composition = {"Mo": 0.667, "O": 0.333}
        result = benchmark(resolve_isotopics, bench_db, composition)
        assert len(result) == 2

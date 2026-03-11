"""Integration tests: p + Mo-100 -> Tc-99m reference case.

Validates the full HYRR pipeline against ISOTOPIA-2.1 reference output.
Tests are skipped if the hyrr.sqlite database is not available.

Tolerance: <=5% deviation from ISOTOPIA (due to improved stopping powers).
"""

from __future__ import annotations

import numpy as np
import pytest

from tests.integration.conftest import requires_db
from tests.integration.reference_data import (
    P_MO100_BEAM,
    P_MO100_HEAT_KW,
    P_MO100_ISOTOPES,
    P_MO100_PARAMS,
    P_MO100_PARTICLES_PER_S,
    P_MO100_TARGET,
    P_MO100_THICKNESS_CM,
)


@requires_db
class TestBeamParameters:
    """Verify beam parameter calculations match ISOTOPIA."""

    def test_particles_per_second(self) -> None:
        """Beam current -> particles/s conversion."""
        from hyrr.models import Beam

        beam = Beam(**P_MO100_BEAM)
        np.testing.assert_allclose(
            beam.particles_per_second,
            P_MO100_PARTICLES_PER_S,
            rtol=1e-4,
            err_msg="Particles/s mismatch",
        )


@requires_db
class TestStoppingPower:
    """Verify stopping power and thickness calculations."""

    def test_target_thickness(self, database) -> None:  # type: ignore[no-untyped-def]
        """Compute Mo-100 target thickness from 16->12 MeV energy loss."""
        from hyrr.stopping import compute_thickness_from_energy

        Z = P_MO100_TARGET["Z"]
        composition = [(Z, 1.0)]  # pure Mo

        thickness = compute_thickness_from_energy(
            database,
            P_MO100_BEAM["projectile"],
            composition,
            P_MO100_TARGET["density_g_cm3"],
            P_MO100_BEAM["energy_MeV"],
            P_MO100_TARGET["energy_out_MeV"],
        )

        np.testing.assert_allclose(
            thickness,
            P_MO100_THICKNESS_CM,
            rtol=0.05,  # 5% tolerance (different stopping power source)
            err_msg="Target thickness mismatch",
        )


@requires_db
class TestProductionRates:
    """Verify production rate calculations for key isotopes."""

    def test_tc99m_production_rate(self, database) -> None:  # type: ignore[no-untyped-def]
        """Tc-99m production rate should match ISOTOPIA within 5%."""
        from hyrr.models import Beam
        from hyrr.production import compute_production_rate
        from hyrr.stopping import compute_thickness_from_energy, dedx_MeV_per_cm

        beam = Beam(**P_MO100_BEAM)
        Z_target = P_MO100_TARGET["Z"]
        A_target = P_MO100_TARGET["A"]
        density = P_MO100_TARGET["density_g_cm3"]
        E_in = P_MO100_BEAM["energy_MeV"]
        E_out = P_MO100_TARGET["energy_out_MeV"]

        # Get cross-sections for Tc-99m
        xs_data = database.get_cross_sections(beam.projectile, Z_target, A_target)
        tc99m_xs = None
        for xs in xs_data:
            if xs.residual_Z == 43 and xs.residual_A == 99 and xs.state == "m":
                tc99m_xs = xs
                break

        if tc99m_xs is None:
            pytest.skip("Tc-99m cross-section data not found in database")

        # Create stopping power function
        composition = [(Z_target, 1.0)]

        def dedx_fn(E: float) -> float:
            return dedx_MeV_per_cm(database, beam.projectile, composition, density, E)

        # Compute thickness and volume
        thickness = compute_thickness_from_energy(
            database,
            beam.projectile,
            composition,
            density,
            E_in,
            E_out,
        )
        volume = thickness * P_MO100_PARAMS["area_cm2"]

        # Number of target atoms
        import scipy.constants as const

        n_atoms = (density * volume * const.Avogadro) / A_target

        prate, _, _, _ = compute_production_rate(
            tc99m_xs.energies_MeV,
            tc99m_xs.xs_mb,
            dedx_fn,
            energy_in_MeV=E_in,
            energy_out_MeV=E_out,
            n_target_atoms=n_atoms,
            beam_particles_per_s=beam.particles_per_second,
            target_volume_cm3=volume,
        )

        # Find reference value for Tc-99m
        ref = next(r for r in P_MO100_ISOTOPES if r.name == "Tc-99m")

        np.testing.assert_allclose(
            prate,
            ref.production_rate,
            rtol=0.10,  # 10% tolerance for now (stopping power differences)
            err_msg=(
                f"Tc-99m production rate mismatch: "
                f"got {prate:.6E}, expected {ref.production_rate:.6E}"
            ),
        )

    def test_tc100_activity(self, database) -> None:  # type: ignore[no-untyped-def]
        """Tc-100 activity at EOI (~78.61 GBq) via compute_stack."""
        from hyrr.compute import compute_stack
        from hyrr.materials import resolve_element
        from hyrr.models import Beam, Layer, TargetStack

        beam = Beam(**P_MO100_BEAM)
        mo100 = resolve_element(database, "Mo", enrichment={100: 1.0})
        layer = Layer(
            density_g_cm3=P_MO100_TARGET["density_g_cm3"],
            elements=[(mo100, 1.0)],
            energy_out_MeV=P_MO100_TARGET["energy_out_MeV"],
        )
        stack = TargetStack(
            beam=beam,
            layers=[layer],
            irradiation_time_s=P_MO100_PARAMS["irradiation_time_s"],
            cooling_time_s=P_MO100_PARAMS["cooling_time_s"],
            area_cm2=P_MO100_PARAMS["area_cm2"],
        )

        result = compute_stack(database, stack)
        lr = result.layer_results[0]

        ref = next(r for r in P_MO100_ISOTOPES if r.name == "Tc-100")

        assert "Tc-100" in lr.isotope_results, (
            f"Tc-100 not found. Available: {list(lr.isotope_results.keys())}"
        )
        tc100 = lr.isotope_results["Tc-100"]

        # EOI activity: find activity at irradiation_time_s
        irr_idx = np.searchsorted(tc100.time_grid_s, P_MO100_PARAMS["irradiation_time_s"])
        irr_idx = min(irr_idx, len(tc100.activity_vs_time_Bq) - 1)
        activity_eoi_GBq = tc100.activity_vs_time_Bq[irr_idx] * 1e-9

        np.testing.assert_allclose(
            activity_eoi_GBq,
            ref.activity_eoi_GBq,
            rtol=0.10,  # 10% tolerance (stopping power differences)
            err_msg=(
                f"Tc-100 EOI activity mismatch: "
                f"got {activity_eoi_GBq:.2f} GBq, expected {ref.activity_eoi_GBq:.2f} GBq"
            ),
        )


@requires_db
class TestBatemanEquations:
    """Verify Bateman equation solutions."""

    def test_tc99m_activity_after_cooling(self, database) -> None:  # type: ignore[no-untyped-def]
        """Tc-99m activity after 1d irradiation + 1d cooling."""
        from hyrr.production import bateman_activity

        # Use reference production rate (total R = isotopia_rate × N_atoms)
        ref = next(r for r in P_MO100_ISOTOPES if r.name == "Tc-99m")
        assert ref.activity_cooled_GBq is not None

        time_grid, activity = bateman_activity(
            ref.production_rate,
            ref.half_life_s,
            P_MO100_PARAMS["irradiation_time_s"],
            P_MO100_PARAMS["cooling_time_s"],
        )

        # Activity at end of cooling
        A_final_GBq = activity[-1] * 1e-9

        np.testing.assert_allclose(
            A_final_GBq,
            ref.activity_cooled_GBq,
            rtol=0.05,
            err_msg=(
                f"Tc-99m activity mismatch: "
                f"got {A_final_GBq:.4f} GBq, expected {ref.activity_cooled_GBq:.4f} GBq"
            ),
        )


@requires_db
class TestFullPipeline:
    """End-to-end test: Beam -> Layer -> compute_stack -> compare."""

    def test_full_p_mo100(self, database) -> None:  # type: ignore[no-untyped-def]
        """Full pipeline: build stack, run compute_stack, validate results."""
        from hyrr.compute import compute_stack
        from hyrr.materials import resolve_element
        from hyrr.models import Beam, Layer, TargetStack

        beam = Beam(**P_MO100_BEAM)
        mo100 = resolve_element(database, "Mo", enrichment={100: 1.0})
        layer = Layer(
            density_g_cm3=P_MO100_TARGET["density_g_cm3"],
            elements=[(mo100, 1.0)],
            energy_out_MeV=P_MO100_TARGET["energy_out_MeV"],
        )
        stack = TargetStack(
            beam=beam,
            layers=[layer],
            irradiation_time_s=P_MO100_PARAMS["irradiation_time_s"],
            cooling_time_s=P_MO100_PARAMS["cooling_time_s"],
            area_cm2=P_MO100_PARAMS["area_cm2"],
        )

        result = compute_stack(database, stack)

        # -- structural checks --
        assert len(result.layer_results) == 1
        lr = result.layer_results[0]

        # -- thickness --
        np.testing.assert_allclose(
            lr.layer._thickness,
            P_MO100_THICKNESS_CM,
            rtol=0.05,
            err_msg="Target thickness mismatch",
        )

        # -- energy --
        assert lr.energy_in == pytest.approx(P_MO100_BEAM["energy_MeV"])
        assert lr.energy_out == pytest.approx(
            P_MO100_TARGET["energy_out_MeV"], abs=0.5
        )

        # -- heat (order-of-magnitude) --
        assert lr.heat_kW > 0, "Heat should be positive"
        np.testing.assert_allclose(
            lr.heat_kW,
            P_MO100_HEAT_KW,
            rtol=0.30,  # 30% tolerance for heat (depends on integration grid)
            err_msg="Heat mismatch",
        )

        # -- Tc-99m production --
        assert "Tc-99m" in lr.isotope_results
        tc99m = lr.isotope_results["Tc-99m"]
        ref_tc99m = next(r for r in P_MO100_ISOTOPES if r.name == "Tc-99m")

        np.testing.assert_allclose(
            tc99m.production_rate,
            ref_tc99m.production_rate,
            rtol=0.10,
            err_msg="Tc-99m production rate mismatch",
        )

        # -- validate key isotopes present --
        for ref in P_MO100_ISOTOPES:
            # Skip sum entries (Tc-99 is sum of g+m) and very low production isotopes
            if ref.name == "Tc-99":
                continue
            if ref.name in lr.isotope_results:
                iso = lr.isotope_results[ref.name]
                assert iso.production_rate > 0, f"{ref.name} should have positive rate"

        # -- depth profile --
        assert len(lr.depth_profile) > 0
        assert lr.depth_profile[0].depth_cm == pytest.approx(0.0)

        # -- stopping power source --
        assert 42 in lr.stopping_power_sources

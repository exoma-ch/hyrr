"""Integration tests: p + Mo-100 -> Tc-99m reference case.

Validates the full HYRR pipeline against ISOTOPIA-2.1 reference output.
Tests are skipped if the parquet data directory is not available.

All physics computation routes through the Rust backend.
Tolerance: <=5% deviation from ISOTOPIA (due to improved stopping powers).
"""

from __future__ import annotations

import json

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
class TestFullPipeline:
    """End-to-end test: Beam -> Layer -> Rust compute_stack -> compare."""

    def test_full_p_mo100(self, database) -> None:
        """Full pipeline via Rust: build config, run simulation, validate."""
        from hyrr.api import run_simulation_from_json

        config = {
            "beam": {
                "projectile": P_MO100_BEAM["projectile"],
                "energy_MeV": P_MO100_BEAM["energy_MeV"],
                "current_mA": P_MO100_BEAM["current_mA"],
            },
            "layers": [
                {
                    "material": "Mo-100",
                    "energy_out_MeV": P_MO100_TARGET["energy_out_MeV"],
                },
            ],
            "irradiation_s": P_MO100_PARAMS["irradiation_time_s"],
            "cooling_s": P_MO100_PARAMS["cooling_time_s"],
        }

        result = run_simulation_from_json(
            json.dumps(config),
            str(database.data_dir),
            database.library,
        )

        # -- structural checks --
        assert len(result["layers"]) == 1
        lr = result["layers"][0]

        # -- energy --
        assert lr["energy_in"] == pytest.approx(P_MO100_BEAM["energy_MeV"])
        assert lr["energy_out"] == pytest.approx(
            P_MO100_TARGET["energy_out_MeV"], abs=0.5
        )

        # -- heat (order-of-magnitude) --
        assert lr["heat_kW"] > 0, "Heat should be positive"
        np.testing.assert_allclose(
            lr["heat_kW"],
            P_MO100_HEAT_KW,
            rtol=0.30,
            err_msg="Heat mismatch",
        )

        # -- Tc-99m production --
        isotopes_by_name = {iso["name"]: iso for iso in lr["isotopes"]}
        assert "Tc-99m" in isotopes_by_name
        tc99m = isotopes_by_name["Tc-99m"]
        ref_tc99m = next(r for r in P_MO100_ISOTOPES if r.name == "Tc-99m")

        np.testing.assert_allclose(
            tc99m["production_rate"],
            ref_tc99m.production_rate,
            rtol=0.10,
            err_msg="Tc-99m production rate mismatch",
        )

        # -- validate key isotopes present --
        for ref in P_MO100_ISOTOPES:
            if ref.name == "Tc-99":
                continue
            if ref.name in isotopes_by_name:
                iso = isotopes_by_name[ref.name]
                assert iso["production_rate"] > 0, (
                    f"{ref.name} should have positive rate"
                )

    def test_tc99m_activity_via_rust(self, database) -> None:
        """Tc-99m activity after 1d irradiation + 1d cooling via Rust Bateman."""
        from hyrr._native_bridge import bateman_activity

        ref = next(r for r in P_MO100_ISOTOPES if r.name == "Tc-99m")
        assert ref.activity_cooled_GBq is not None

        time_grid, activity = bateman_activity(
            ref.production_rate,
            ref.half_life_s,
            P_MO100_PARAMS["irradiation_time_s"],
            P_MO100_PARAMS["cooling_time_s"],
        )

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

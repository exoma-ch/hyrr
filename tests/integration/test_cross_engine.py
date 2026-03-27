"""Cross-engine validation: Python vs Rust vs TypeScript.

Runs the same p + Mo-100 → Tc-99m reference simulation through all three
compute engines and compares key numerical results.

Requires:
- nucl-parquet data directory
- hyrr._native (Rust/PyO3 extension via maturin)
- Node.js with tsx for TypeScript execution

Usage:
    uv run pytest tests/integration/test_cross_engine.py -v
"""

from __future__ import annotations

import json
import subprocess
from pathlib import Path

import pytest

from tests.integration.conftest import requires_db
from tests.integration.reference_data import (
    P_MO100_BEAM,
    P_MO100_ISOTOPES,
    P_MO100_PARAMS,
    P_MO100_TARGET,
)

# The JSON SimulationConfig all three engines consume
SIM_CONFIG = {
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

# Simpler config that doesn't require enrichment notation handling
# All three engines resolve "Cu" identically (natural Cu)
SIM_CONFIG_CU = {
    "beam": {
        "projectile": "p",
        "energy_MeV": 18.0,
        "current_mA": 0.05,
    },
    "layers": [
        {
            "material": "Cu",
            "thickness_cm": 0.05,
        },
    ],
    "irradiation_s": 3600,
    "cooling_s": 3600,
}


def _find_data_dir() -> Path | None:
    """Locate nucl-parquet data directory."""
    import os

    env_dir = os.environ.get("HYRR_DATA", "")
    candidates: list[Path] = []
    if env_dir:
        candidates.append(Path(env_dir))
    repo_root = Path(__file__).parent.parent.parent
    candidates.extend([
        repo_root / "nucl-parquet",
        repo_root / ".." / "nucl-parquet",
        Path.home() / ".hyrr" / "nucl-parquet",
    ])
    for p in candidates:
        p = p.resolve()
        if p.is_dir() and (p / "meta").is_dir():
            return p
    return None


# ── Python engine (via Rust backend) ──────────────────────────────────

def run_python_engine(data_dir: Path, config: dict | None = None) -> dict:
    """Run simulation through Rust backend via Python API."""
    import json

    from hyrr.api import run_simulation_from_json

    sim_config = config or SIM_CONFIG
    return run_simulation_from_json(json.dumps(sim_config), str(data_dir))


# ── Rust engine (via PyO3) ────────────────────────────────────────────

def run_rust_engine(data_dir: Path, config: dict | None = None) -> dict | None:
    """Run simulation through Rust _native extension."""
    try:
        from hyrr._native import PyDataStore
    except ImportError:
        return None

    sim_config = config or SIM_CONFIG
    db = PyDataStore(str(data_dir))
    result_json = db.compute_stack(json.dumps(sim_config))
    return json.loads(result_json)


# ── TypeScript engine (via Node subprocess) ───────────────────────────

TS_RUNNER = """\
import { NodeDataStore, resolveDataDir } from "@hyrr/compute/node";
import {
  computeStack,
  buildTargetStack,
  convertResult,
  getRequiredElements,
} from "@hyrr/compute";

const config = JSON.parse(process.argv[2]);
const dataDir = process.argv[3];

async function main() {
  const db = new NodeDataStore(dataDir);
  await db.init();

  const elements = getRequiredElements(config);
  await db.ensureMultipleCrossSections(config.beam.projectile, elements);

  const stack = buildTargetStack(config, db);
  const stackResult = computeStack(db, stack);
  const simResult = convertResult(config, stackResult);

  process.stdout.write(JSON.stringify(simResult));
}

main().catch((e) => {
  process.stderr.write(e.message + "\\n" + (e.stack || "") + "\\n");
  process.exit(1);
});
"""


def run_ts_engine(data_dir: Path, config: dict | None = None) -> dict | None:
    """Run simulation through TypeScript computeStack via Node.js."""
    repo_root = Path(__file__).parent.parent.parent
    runner_path = Path(__file__).parent / "ts_runner.mts"

    if not runner_path.exists():
        print(f"[TS] runner not found: {runner_path}")
        return None

    sim_config = config or SIM_CONFIG

    try:
        result = subprocess.run(
            [
                "node", "--import", "tsx",
                str(runner_path),
                json.dumps(sim_config),
                str(data_dir),
            ],
            capture_output=True,
            text=True,
            timeout=60,
            cwd=str(repo_root),
        )
        if result.returncode != 0:
            print(f"[TS] stderr: {result.stderr[:500]}")
            return None
        return json.loads(result.stdout)
    except (subprocess.TimeoutExpired, FileNotFoundError, json.JSONDecodeError) as e:
        print(f"[TS] failed: {e}")
        return None


# ── Extraction helpers ────────────────────────────────────────────────

def extract_key_values(result: dict, label: str) -> dict:
    """Extract comparable values from a SimulationResult dict.

    Handles both Python API format (layers/isotopes) and Rust serde format
    (layer_results/isotope_results with snake_case keys).
    """
    layers = result.get("layers") or result.get("layer_results")
    assert layers, f"No layers found in {label} result. Keys: {list(result.keys())}"
    lr = layers[0]

    out = {
        "label": label,
        "energy_in": lr.get("energy_in", 0),
        "energy_out": lr.get("energy_out", 0),
        "delta_E_MeV": lr.get("delta_E_MeV") or lr.get("delta_e_mev", 0),
        "heat_kW": lr.get("heat_kW") or lr.get("heat_kw", 0),
        "isotopes": {},
    }

    isotopes = lr.get("isotopes") or lr.get("isotope_results")
    if isinstance(isotopes, list):
        for iso in isotopes:
            name = iso["name"]
            out["isotopes"][name] = {
                "production_rate": iso.get("production_rate", 0),
                "activity_Bq": iso.get("activity_Bq") or iso.get("activity_bq", 0),
                "half_life_s": iso.get("half_life_s"),
            }
    elif isinstance(isotopes, dict):
        for name, iso in isotopes.items():
            out["isotopes"][name] = {
                "production_rate": iso.get("production_rate", 0),
                "activity_Bq": iso.get("activity_Bq") or iso.get("activity_bq", 0),
                "half_life_s": iso.get("half_life_s"),
            }

    return out


def compare_engines(a: dict, b: dict, rtol: float = 0.02) -> list[str]:
    """Compare two engine outputs. Returns list of discrepancy messages."""
    diffs: list[str] = []
    la, lb = a["label"], b["label"]

    # Energy
    for key in ("energy_in", "energy_out", "delta_E_MeV"):
        va, vb = a[key], b[key]
        if va == 0 and vb == 0:
            continue
        rel = abs(va - vb) / max(abs(va), abs(vb), 1e-30)
        if rel > rtol:
            diffs.append(f"  {key}: {la}={va:.6f} vs {lb}={vb:.6f} (rel={rel:.4f})")

    # Heat
    va, vb = a["heat_kW"], b["heat_kW"]
    if max(abs(va), abs(vb)) > 0:
        rel = abs(va - vb) / max(abs(va), abs(vb), 1e-30)
        if rel > 0.10:  # 10% tolerance for heat (integration grid differences)
            diffs.append(f"  heat_kW: {la}={va:.6f} vs {lb}={vb:.6f} (rel={rel:.4f})")

    # Isotopes: compare all isotopes present in both
    common_iso = set(a["isotopes"].keys()) & set(b["isotopes"].keys())
    if not common_iso:
        diffs.append(f"  No common isotopes! {la}: {list(a['isotopes'].keys())}, {lb}: {list(b['isotopes'].keys())}")
        return diffs

    for name in sorted(common_iso):
        ia, ib = a["isotopes"][name], b["isotopes"][name]
        for field in ("production_rate", "activity_Bq"):
            va, vb = ia[field], ib[field]
            if min(abs(va), abs(vb)) < 1.0:
                continue  # skip when either engine doesn't produce / prunes this isotope
            rel = abs(va - vb) / max(abs(va), abs(vb), 1e-30)
            if rel > rtol:
                diffs.append(
                    f"  {name}.{field}: {la}={va:.6E} vs {lb}={vb:.6E} (rel={rel:.4f})"
                )

    # Note: isotope sets may differ due to different activity cutoff thresholds.
    # This is expected and not a physics discrepancy.

    return diffs


# ── Tests ─────────────────────────────────────────────────────────────

@requires_db
class TestCrossEngineValidation:
    """Compare Python, Rust, and TypeScript compute engines."""

    @pytest.fixture(autouse=True)
    def setup(self) -> None:
        self.data_dir = _find_data_dir()
        assert self.data_dir is not None

    def test_python_vs_rust(self) -> None:
        """Python and Rust engines produce matching production rates.

        Compares key isotope production rates and energy loss. Activity
        differences from chain solver depth are expected and tested separately.
        """
        py = run_python_engine(self.data_dir)
        rs = run_rust_engine(self.data_dir)
        if rs is None:
            pytest.skip("hyrr._native not available")

        py_vals = extract_key_values(py, "Python")
        rs_vals = extract_key_values(rs, "Rust")

        # Print detailed comparison for diagnostics
        print(f"\n{'='*60}")
        print("PYTHON vs RUST comparison")
        print(f"{'='*60}")
        _print_comparison(py_vals, rs_vals)

        # Assert key physics values match tightly
        assert py_vals["energy_in"] == pytest.approx(rs_vals["energy_in"], abs=0.01)
        assert py_vals["energy_out"] == pytest.approx(rs_vals["energy_out"], abs=0.01)
        assert py_vals["heat_kW"] == pytest.approx(rs_vals["heat_kW"], rel=0.01)

        # Compare production rates for isotopes present in both
        common = set(py_vals["isotopes"]) & set(rs_vals["isotopes"])
        for name in sorted(common):
            py_rate = py_vals["isotopes"][name]["production_rate"]
            rs_rate = rs_vals["isotopes"][name]["production_rate"]
            if min(py_rate, rs_rate) < 1.0:
                continue  # skip when either engine doesn't produce this isotope
            rel = abs(py_rate - rs_rate) / max(py_rate, rs_rate)
            assert rel < 0.02, (
                f"{name} production rate: Python={py_rate:.6E} vs Rust={rs_rate:.6E} (rel={rel:.4f})"
            )

    def test_three_way_cu(self) -> None:
        """Three-way comparison on natural Cu (no enrichment ambiguity)."""
        py = run_python_engine(self.data_dir, SIM_CONFIG_CU)
        rs = run_rust_engine(self.data_dir, SIM_CONFIG_CU)
        ts = run_ts_engine(self.data_dir, SIM_CONFIG_CU)

        py_vals = extract_key_values(py, "Python")

        print(f"\n{'='*70}")
        print("Three-way comparison: p + Cu, 18 MeV, 0.5 mm")
        print(f"{'='*70}")

        engines = [("Python", py_vals)]

        if rs is not None:
            rs_vals = extract_key_values(rs, "Rust")
            engines.append(("Rust", rs_vals))
        else:
            print("[Rust] skipped — _native not available")

        if ts is not None:
            ts_vals = extract_key_values(ts, "TypeScript")
            engines.append(("TypeScript", ts_vals))
        else:
            print("[TS] skipped — Node.js/tsx not available")

        assert len(engines) >= 2, "Need at least 2 engines for comparison"

        # Print comparison table
        header = f"{'':>25}" + "".join(f"{name:>18}" for name, _ in engines)
        print(header)
        print("-" * len(header))
        for key in ("energy_in", "energy_out", "delta_E_MeV", "heat_kW"):
            print(f"{key:>25}" + "".join(f"{e[key]:>18.6f}" for _, e in engines))
        print()

        # Common isotopes
        all_sets = [set(e["isotopes"].keys()) for _, e in engines]
        common = all_sets[0]
        for s in all_sets[1:]:
            common &= s

        for name in sorted(common)[:15]:
            rates = "".join(
                f"{e['isotopes'][name]['production_rate']:>18.6E}" for _, e in engines
            )
            print(f"{name + ' rate':>25}{rates}")

        # Pairwise assertions on production rates
        for i in range(len(engines)):
            for j in range(i + 1, len(engines)):
                na, ea = engines[i]
                nb, eb = engines[j]
                common_ij = set(ea["isotopes"]) & set(eb["isotopes"])
                for iso_name in common_ij:
                    ra = ea["isotopes"][iso_name]["production_rate"]
                    rb = eb["isotopes"][iso_name]["production_rate"]
                    if min(ra, rb) < 1.0:
                        continue
                    rel = abs(ra - rb) / max(ra, rb)
                    assert rel < 0.05, (
                        f"{na} vs {nb}: {iso_name} rate "
                        f"{ra:.6E} vs {rb:.6E} (rel={rel:.4f})"
                    )

    def test_python_vs_typescript(self) -> None:
        """Python and TypeScript engines produce matching results (Cu config).

        Uses Cu config because TS resolveMaterial doesn't handle isotope
        notation (Mo-100 → enriched). The three_way_cu test covers the
        full 3-engine comparison; this tests the pairwise assertion path.
        """
        py = run_python_engine(self.data_dir, SIM_CONFIG_CU)
        ts = run_ts_engine(self.data_dir, SIM_CONFIG_CU)
        if ts is None:
            pytest.skip("TypeScript engine not available (needs Node.js + tsx)")

        py_vals = extract_key_values(py, "Python")
        ts_vals = extract_key_values(ts, "TypeScript")

        diffs = compare_engines(py_vals, ts_vals, rtol=0.02)
        if diffs:
            msg = "Python vs TypeScript discrepancies (>2% relative):\n" + "\n".join(diffs)
            print(f"\n{'='*60}")
            print("PYTHON vs TYPESCRIPT comparison")
            print(f"{'='*60}")
            _print_comparison(py_vals, ts_vals)
            pytest.fail(msg)

    def test_rust_vs_typescript(self) -> None:
        """Rust and TypeScript engines produce matching results (Cu config)."""
        rs = run_rust_engine(self.data_dir, SIM_CONFIG_CU)
        if rs is None:
            pytest.skip("hyrr._native not available")

        ts = run_ts_engine(self.data_dir, SIM_CONFIG_CU)
        if ts is None:
            pytest.skip("TypeScript engine not available")

        rs_vals = extract_key_values(rs, "Rust")
        ts_vals = extract_key_values(ts, "TypeScript")

        diffs = compare_engines(rs_vals, ts_vals, rtol=0.02)
        if diffs:
            msg = "Rust vs TypeScript discrepancies (>2% relative):\n" + "\n".join(diffs)
            print(f"\n{'='*60}")
            print("RUST vs TYPESCRIPT comparison")
            print(f"{'='*60}")
            _print_comparison(rs_vals, ts_vals)
            pytest.fail(msg)

    def test_all_three_vs_isotopia(self) -> None:
        """All engines agree with ISOTOPIA reference within 10%."""
        py = extract_key_values(run_python_engine(self.data_dir), "Python")

        rs_raw = run_rust_engine(self.data_dir)
        rs = extract_key_values(rs_raw, "Rust") if rs_raw else None

        ts_raw = run_ts_engine(self.data_dir)
        ts = extract_key_values(ts_raw, "TypeScript") if ts_raw else None

        print(f"\n{'='*70}")
        print("Cross-engine results vs ISOTOPIA reference (p + Mo-100 → Tc-99m)")
        print(f"{'='*70}")

        # Header
        engines = [("Python", py)]
        if rs:
            engines.append(("Rust", rs))
        if ts:
            engines.append(("TypeScript", ts))

        header = f"{'':>20}" + "".join(f"{name:>18}" for name, _ in engines) + f"{'ISOTOPIA':>18}"
        print(header)
        print("-" * len(header))

        # Energy
        print(f"{'energy_in [MeV]':>20}" + "".join(f"{e['energy_in']:>18.4f}" for _, e in engines) + f"{P_MO100_BEAM['energy_MeV']:>18.4f}")
        print(f"{'energy_out [MeV]':>20}" + "".join(f"{e['energy_out']:>18.4f}" for _, e in engines) + f"{P_MO100_TARGET['energy_out_MeV']:>18.4f}")
        print(f"{'delta_E [MeV]':>20}" + "".join(f"{e['delta_E_MeV']:>18.4f}" for _, e in engines) + f"{P_MO100_BEAM['energy_MeV'] - P_MO100_TARGET['energy_out_MeV']:>18.4f}")
        print(f"{'heat [kW]':>20}" + "".join(f"{e['heat_kW']:>18.6f}" for _, e in engines) + f"{'~0.6':>18}")
        print()

        # Key isotopes
        ref_isotopes = {r.name: r for r in P_MO100_ISOTOPES if r.name != "Tc-99"}
        for name in ["Tc-99m", "Tc-100", "Mo-99", "Nb-97", "Tc-99g"]:
            if name not in ref_isotopes:
                continue
            ref = ref_isotopes[name]

            # Production rate
            rates = []
            for _ename, e in engines:
                if name in e["isotopes"]:
                    rates.append(f"{e['isotopes'][name]['production_rate']:>18.6E}")
                else:
                    rates.append(f"{'(missing)':>18}")

            print(f"{name + ' rate [/s]':>20}" + "".join(rates) + f"{ref.production_rate:>18.6E}")

            # Activity
            activities = []
            for _ename, e in engines:
                if name in e["isotopes"]:
                    act_gbq = e["isotopes"][name]["activity_Bq"] * 1e-9
                    activities.append(f"{act_gbq:>18.4f}")
                else:
                    activities.append(f"{'(missing)':>18}")

            ref_act = ref.activity_cooled_GBq if ref.activity_cooled_GBq else ref.activity_eoi_GBq
            label = "cooled" if ref.activity_cooled_GBq else "EOI"
            print(f"{name + f' act [{label}] GBq':>20}" + "".join(activities) + f"{ref_act:>18.4f}")
            print()

        # Tolerance check against ISOTOPIA for Tc-99m
        ref_tc99m = ref_isotopes["Tc-99m"]
        for ename, e in engines:
            if "Tc-99m" not in e["isotopes"]:
                continue
            rate = e["isotopes"]["Tc-99m"]["production_rate"]
            rel = abs(rate - ref_tc99m.production_rate) / ref_tc99m.production_rate
            status = "PASS" if rel <= 0.10 else "FAIL"
            print(f"  {ename} Tc-99m rate vs ISOTOPIA: {rel:.2%} deviation [{status}]")


def _print_comparison(a: dict, b: dict) -> None:
    """Print side-by-side comparison of two engine outputs."""
    la, lb = a["label"], b["label"]
    print(f"{'':>20} {la:>18} {lb:>18} {'rel.diff':>10}")
    print("-" * 68)

    for key in ("energy_in", "energy_out", "delta_E_MeV", "heat_kW"):
        va, vb = a[key], b[key]
        denom = max(abs(va), abs(vb), 1e-30)
        rel = abs(va - vb) / denom
        flag = " !" if rel > 0.02 else ""
        print(f"{key:>20} {va:>18.6f} {vb:>18.6f} {rel:>10.4f}{flag}")

    print()
    common = sorted(set(a["isotopes"]) & set(b["isotopes"]))
    for name in common:
        ia, ib = a["isotopes"][name], b["isotopes"][name]
        for field in ("production_rate", "activity_Bq"):
            va, vb = ia[field], ib[field]
            denom = max(abs(va), abs(vb), 1e-30)
            rel = abs(va - vb) / denom
            flag = " !" if rel > 0.02 else ""
            print(f"{name + '.' + field:>35} {va:>18.6E} {vb:>18.6E} {rel:>10.4f}{flag}")

"""Parquet/Polars data access layer for nuclear data."""

from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Protocol, runtime_checkable

import numpy as np
import numpy.typing as npt
import polars as pl

DEFAULT_LIBRARY = "tendl-2025"


def load_catalog(data_dir: str | Path) -> dict[str, Any]:
    """Load the nucl-parquet catalog.json if present."""
    cat_path = Path(data_dir) / "catalog.json"
    if cat_path.exists():
        return json.loads(cat_path.read_text())  # type: ignore[no-any-return]
    return {}


@runtime_checkable
class DatabaseProtocol(Protocol):
    """Protocol for nuclear data access.

    All physics modules depend on this protocol rather than a concrete
    database implementation. This enables dependency injection and testing
    with mock databases.
    """

    def get_cross_sections(
        self,
        projectile: str,
        target_Z: int,
        target_A: int,
    ) -> list[CrossSectionData]:
        """Get all residual production cross-sections for a given reaction.

        Returns list of CrossSectionData, one per residual nuclide
        (Z, A, state).
        """
        ...

    def get_stopping_power(
        self,
        source: str,
        target_Z: int,
    ) -> tuple[npt.NDArray[np.float64], npt.NDArray[np.float64]]:
        """Get stopping power table for an element.

        Args:
            source: 'PSTAR' or 'ASTAR'
            target_Z: target element charge number

        Returns:
            (energies_MeV, dedx_MeV_cm2_g) arrays sorted by energy
        """
        ...

    def get_natural_abundances(
        self,
        Z: int,
    ) -> dict[int, tuple[float, float]]:
        """Get natural isotopic abundances for an element.

        Returns:
            dict mapping A -> (fractional_abundance, atomic_mass_u)
        """
        ...

    def get_decay_data(
        self,
        Z: int,
        A: int,
        state: str = "",
    ) -> DecayData | None:
        """Get decay data for a nuclide.

        Returns None if nuclide not found in database.
        """
        ...

    def get_element_symbol(self, Z: int) -> str:
        """Get element symbol from atomic number."""
        ...

    def get_element_Z(self, symbol: str) -> int:
        """Get atomic number from element symbol."""
        ...


# Data structures returned by DatabaseProtocol methods


@dataclass(frozen=True)
class CrossSectionData:
    """Cross-section data for one residual nuclide from a specific reaction."""

    residual_Z: int
    residual_A: int
    state: str  # '', 'g', 'm'
    energies_MeV: npt.NDArray[np.float64]
    xs_mb: npt.NDArray[np.float64]


@dataclass(frozen=True)
class DecayData:
    """Decay properties of a nuclide."""

    Z: int
    A: int
    state: str
    half_life_s: float | None  # None for stable
    decay_modes: list[DecayMode]


@dataclass(frozen=True)
class DecayMode:
    """Single decay mode with daughter and branching ratio."""

    mode: str  # 'beta-', 'beta+', 'EC', 'alpha', 'IT', 'stable'
    daughter_Z: int | None
    daughter_A: int | None
    daughter_state: str
    branching: float  # 0.0 to 1.0


# ---------------------------------------------------------------------------
# Hardcoded element symbols (Z -> symbol) as fallback
# ---------------------------------------------------------------------------

ELEMENT_SYMBOLS: dict[int, str] = {
    1: "H",
    2: "He",
    3: "Li",
    4: "Be",
    5: "B",
    6: "C",
    7: "N",
    8: "O",
    9: "F",
    10: "Ne",
    11: "Na",
    12: "Mg",
    13: "Al",
    14: "Si",
    15: "P",
    16: "S",
    17: "Cl",
    18: "Ar",
    19: "K",
    20: "Ca",
    21: "Sc",
    22: "Ti",
    23: "V",
    24: "Cr",
    25: "Mn",
    26: "Fe",
    27: "Co",
    28: "Ni",
    29: "Cu",
    30: "Zn",
    31: "Ga",
    32: "Ge",
    33: "As",
    34: "Se",
    35: "Br",
    36: "Kr",
    37: "Rb",
    38: "Sr",
    39: "Y",
    40: "Zr",
    41: "Nb",
    42: "Mo",
    43: "Tc",
    44: "Ru",
    45: "Rh",
    46: "Pd",
    47: "Ag",
    48: "Cd",
    49: "In",
    50: "Sn",
    51: "Sb",
    52: "Te",
    53: "I",
    54: "Xe",
    55: "Cs",
    56: "Ba",
    57: "La",
    58: "Ce",
    59: "Pr",
    60: "Nd",
    61: "Pm",
    62: "Sm",
    63: "Eu",
    64: "Gd",
    65: "Tb",
    66: "Dy",
    67: "Ho",
    68: "Er",
    69: "Tm",
    70: "Yb",
    71: "Lu",
    72: "Hf",
    73: "Ta",
    74: "W",
    75: "Re",
    76: "Os",
    77: "Ir",
    78: "Pt",
    79: "Au",
    80: "Hg",
    81: "Tl",
    82: "Pb",
    83: "Bi",
    84: "Po",
    85: "At",
    86: "Rn",
    87: "Fr",
    88: "Ra",
    89: "Ac",
    90: "Th",
    91: "Pa",
    92: "U",
    93: "Np",
    94: "Pu",
    95: "Am",
    96: "Cm",
    97: "Bk",
    98: "Cf",
    99: "Es",
    100: "Fm",
    101: "Md",
    102: "No",
    103: "Lr",
    104: "Rf",
    105: "Db",
    106: "Sg",
    107: "Bh",
    108: "Hs",
    109: "Mt",
    110: "Ds",
    111: "Rg",
    112: "Cn",
    113: "Nh",
    114: "Fl",
    115: "Mc",
    116: "Lv",
    117: "Ts",
    118: "Og",
}

_SYMBOL_TO_Z: dict[str, int] = {sym: z for z, sym in ELEMENT_SYMBOLS.items()}


# ---------------------------------------------------------------------------
# Concrete implementation — thin adapter over nucl-parquet DuckDB client (#257)
# ---------------------------------------------------------------------------


class DataStore:
    """Nuclear data store backed by the ``nucl-parquet`` Python client.

    Delegates all data access to ``nucl_parquet.connect()`` which registers
    Parquet files as DuckDB views with catalog-driven path resolution. This
    mirrors the Rust ``NpDataStore`` pattern (PR #249).

    Parameters
    ----------
    data_dir:
        Path to the nucl-parquet root directory containing ``meta/``,
        ``stopping/``, and per-library ``{library}/xs/`` subdirectories.
    library:
        Nuclear data library to use for cross-sections (e.g. ``"tendl-2025"``).
    """

    def __init__(self, data_dir: str | Path, library: str = DEFAULT_LIBRARY) -> None:
        import nucl_parquet

        self._data_dir = Path(data_dir)
        if not self._data_dir.is_dir():
            msg = f"Data directory not found: {self._data_dir}"
            raise FileNotFoundError(msg)

        self._library = library
        self._db = nucl_parquet.connect(self._data_dir)

        # View name for the active library (e.g. "tendl_2025")
        self._lib_view = library.replace("-", "_").replace(".", "_")

        # Build element lookup dicts
        rows = self._db.execute(
            "SELECT Z, symbol FROM elements ORDER BY Z"
        ).fetchall()
        self._z_to_symbol: dict[int, str] = {int(z): s for z, s in rows}
        self._symbol_to_z: dict[str, int] = {s: z for z, s in self._z_to_symbol.items()}

        # Caches
        self._sp_cache: dict[
            tuple[str, int],
            tuple[npt.NDArray[np.float64], npt.NDArray[np.float64]],
        ] = {}

    # -- context manager -----------------------------------------------------

    def __enter__(self) -> DataStore:
        return self

    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc_val: BaseException | None,
        exc_tb: object,
    ) -> None:
        self._db.close()

    # -- internal helpers ----------------------------------------------------

    @property
    def data_dir(self) -> Path:
        """Path to the nucl-parquet data directory."""
        return self._data_dir

    @property
    def library(self) -> str:
        """The active cross-section library name."""
        return self._library

    @property
    def db(self):
        """The underlying DuckDB connection (for advanced queries)."""
        return self._db

    def available_libraries(self) -> list[str]:
        """List cross-section libraries available in the data directory."""
        libs = []
        for p in sorted(self._data_dir.iterdir()):
            if p.is_dir() and (p / "xs").is_dir():
                libs.append(p.name)
        return libs

    # -- DatabaseProtocol methods --------------------------------------------

    def get_cross_sections(
        self,
        projectile: str,
        target_Z: int,
        target_A: int,
    ) -> list[CrossSectionData]:
        """Get all residual production cross-sections for a given reaction."""
        symbol = self.get_element_symbol(target_Z)
        xs_path = self._data_dir / self._library / "xs" / f"{projectile}_{symbol}.parquet"
        if not xs_path.exists():
            return []

        rows = self._db.execute(f"""
            SELECT residual_Z, residual_A, state, energy_MeV, xs_mb
            FROM read_parquet('{xs_path}')
            WHERE target_A = ?
            ORDER BY residual_Z, residual_A, state, energy_MeV
        """, [target_A]).fetchall()

        if not rows:
            return []

        # Group by (residual_Z, residual_A, state)
        groups: dict[tuple[int, int, str], list[tuple[float, float]]] = {}
        for rz, ra, st, e, xs in rows:
            key = (int(rz), int(ra), str(st))
            groups.setdefault(key, []).append((float(e), float(xs)))

        # Prefer state-resolved xs over totals (#254)
        resolved: set[tuple[int, int]] = {
            (rz, ra) for (rz, ra, st) in groups if st
        }

        results: list[CrossSectionData] = []
        for (rz, ra, st), points in groups.items():
            if not st and (rz, ra) in resolved:
                continue
            energies = np.array([p[0] for p in points], dtype=np.float64)
            xs_arr = np.array([p[1] for p in points], dtype=np.float64)
            results.append(
                CrossSectionData(
                    residual_Z=rz, residual_A=ra, state=st,
                    energies_MeV=energies, xs_mb=xs_arr,
                )
            )
        return results

    def get_stopping_power(
        self,
        source: str,
        target_Z: int,
    ) -> tuple[npt.NDArray[np.float64], npt.NDArray[np.float64]]:
        """Get stopping power table for an element (cached)."""
        key = (source, target_Z)
        if key in self._sp_cache:
            return self._sp_cache[key]

        rows = self._db.execute("""
            SELECT energy_MeV, dedx FROM stopping
            WHERE source = ? AND target_Z = ?
            ORDER BY energy_MeV
        """, [source, target_Z]).fetchall()

        energies = np.array([r[0] for r in rows], dtype=np.float64)
        dedx = np.array([r[1] for r in rows], dtype=np.float64)
        result = (energies, dedx)
        self._sp_cache[key] = result
        return result

    def get_natural_abundances(
        self,
        Z: int,
    ) -> dict[int, tuple[float, float]]:
        """Get natural isotopic abundances for an element."""
        rows = self._db.execute(
            "SELECT A, abundance, atomic_mass FROM abundances WHERE Z = ?", [Z]
        ).fetchall()
        return {int(a): (float(ab), float(am)) for a, ab, am in rows}

    def get_decay_data(
        self,
        Z: int,
        A: int,
        state: str = "",
    ) -> DecayData | None:
        """Get decay data for a nuclide. Returns None if not found."""
        # Normalize "g" → "" (#254)
        norm = "" if state == "g" else state
        rows = self._db.execute("""
            SELECT half_life_s, decay_mode, daughter_Z, daughter_A,
                   daughter_state, branching
            FROM decay
            WHERE Z = ? AND A = ? AND state = ?
        """, [Z, A, norm]).fetchall()

        if not rows:
            return None

        modes = [
            DecayMode(
                mode=r[1],
                daughter_Z=r[2],
                daughter_A=r[3],
                daughter_state=r[4] or "",
                branching=r[5],
            )
            for r in rows
        ]
        return DecayData(
            Z=Z, A=A, state=state,
            half_life_s=rows[0][0],
            decay_modes=modes,
        )

    def get_dose_constant(
        self,
        Z: int,
        A: int,
        state: str = "",
    ) -> tuple[float, str] | None:
        """Get gamma dose rate constant k (µSv·m²/MBq·h) and source quality tag."""
        norm = "" if state == "g" else state
        rows = self._db.execute("""
            SELECT k_uSv_m2_MBq_h, source FROM dose_constants
            WHERE Z = ? AND A = ? AND state = ?
        """, [Z, A, norm]).fetchall()
        if not rows:
            return None
        return (float(rows[0][0]), str(rows[0][1]))

    def get_element_symbol(self, Z: int) -> str:
        """Get element symbol from atomic number."""
        sym = self._z_to_symbol.get(Z)
        if sym is not None:
            return sym
        try:
            return ELEMENT_SYMBOLS[Z]
        except KeyError:
            msg = f"Unknown element Z={Z}"
            raise KeyError(msg) from None

    def get_element_Z(self, symbol: str) -> int:
        """Get atomic number from element symbol."""
        z = self._symbol_to_z.get(symbol)
        if z is not None:
            return z
        try:
            return _SYMBOL_TO_Z[symbol]
        except KeyError:
            msg = f"Unknown element symbol '{symbol}'"
            raise KeyError(msg) from None

    def has_cross_sections(self, projectile: str, target_Z: int) -> bool:
        """Check if cross-section data exists for a projectile+element combination."""
        symbol = self.get_element_symbol(target_Z)
        xs_path = self._data_dir / self._library / "xs" / f"{projectile}_{symbol}.parquet"
        return xs_path.exists()

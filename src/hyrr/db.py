"""Parquet/Polars data access layer for nuclear data."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Protocol, runtime_checkable

import numpy as np
import numpy.typing as npt
import polars as pl


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
    1: "H", 2: "He", 3: "Li", 4: "Be", 5: "B", 6: "C", 7: "N", 8: "O",
    9: "F", 10: "Ne", 11: "Na", 12: "Mg", 13: "Al", 14: "Si", 15: "P",
    16: "S", 17: "Cl", 18: "Ar", 19: "K", 20: "Ca", 21: "Sc", 22: "Ti",
    23: "V", 24: "Cr", 25: "Mn", 26: "Fe", 27: "Co", 28: "Ni", 29: "Cu",
    30: "Zn", 31: "Ga", 32: "Ge", 33: "As", 34: "Se", 35: "Br", 36: "Kr",
    37: "Rb", 38: "Sr", 39: "Y", 40: "Zr", 41: "Nb", 42: "Mo", 43: "Tc",
    44: "Ru", 45: "Rh", 46: "Pd", 47: "Ag", 48: "Cd", 49: "In", 50: "Sn",
    51: "Sb", 52: "Te", 53: "I", 54: "Xe", 55: "Cs", 56: "Ba", 57: "La",
    58: "Ce", 59: "Pr", 60: "Nd", 61: "Pm", 62: "Sm", 63: "Eu", 64: "Gd",
    65: "Tb", 66: "Dy", 67: "Ho", 68: "Er", 69: "Tm", 70: "Yb", 71: "Lu",
    72: "Hf", 73: "Ta", 74: "W", 75: "Re", 76: "Os", 77: "Ir", 78: "Pt",
    79: "Au", 80: "Hg", 81: "Tl", 82: "Pb", 83: "Bi", 84: "Po", 85: "At",
    86: "Rn", 87: "Fr", 88: "Ra", 89: "Ac", 90: "Th", 91: "Pa", 92: "U",
    93: "Np", 94: "Pu", 95: "Am", 96: "Cm", 97: "Bk", 98: "Cf", 99: "Es",
    100: "Fm", 101: "Md", 102: "No", 103: "Lr", 104: "Rf", 105: "Db",
    106: "Sg", 107: "Bh", 108: "Hs", 109: "Mt", 110: "Ds", 111: "Rg",
    112: "Cn", 113: "Nh", 114: "Fl", 115: "Mc", 116: "Lv", 117: "Ts",
    118: "Og",
}

_SYMBOL_TO_Z: dict[str, int] = {sym: z for z, sym in ELEMENT_SYMBOLS.items()}


# ---------------------------------------------------------------------------
# Concrete Parquet/Polars implementation
# ---------------------------------------------------------------------------


class DataStore:
    """Parquet/Polars-backed nuclear data store implementing :class:`DatabaseProtocol`.

    Parameters
    ----------
    data_dir:
        Path to the ``data/parquet/`` directory containing ``meta/``,
        ``stopping/``, and ``xs/`` subdirectories.
    """

    def __init__(self, data_dir: str | Path) -> None:
        self._data_dir = Path(data_dir)
        if not self._data_dir.is_dir():
            msg = f"Data directory not found: {self._data_dir}"
            raise FileNotFoundError(msg)

        # Eagerly load small metadata tables
        self._elements: pl.DataFrame = pl.read_parquet(
            self._data_dir / "meta" / "elements.parquet"
        )
        self._abundances: pl.DataFrame = pl.read_parquet(
            self._data_dir / "meta" / "abundances.parquet"
        )
        self._decay: pl.DataFrame = pl.read_parquet(
            self._data_dir / "meta" / "decay.parquet"
        )
        self._stopping: pl.DataFrame = pl.read_parquet(
            self._data_dir / "stopping" / "stopping.parquet"
        )

        # Build element lookup dicts from the elements table
        self._z_to_symbol: dict[int, str] = dict(
            zip(
                self._elements["Z"].to_list(),
                self._elements["symbol"].to_list(),
                strict=True,
            )
        )
        self._symbol_to_z: dict[str, int] = {
            s: z for z, s in self._z_to_symbol.items()
        }

        # Lazy-loaded cross-section DataFrames: (projectile, symbol) -> DataFrame
        self._xs_cache: dict[str, pl.DataFrame] = {}

        # Stopping power cache: (source, target_Z) -> (energies, dedx) arrays
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
        pass  # No resources to close for Parquet/Polars

    # -- internal helpers ----------------------------------------------------

    def _get_xs_df(self, projectile: str, target_Z: int) -> pl.DataFrame | None:
        """Load cross-section DataFrame for a projectile+element, with caching."""
        symbol = self.get_element_symbol(target_Z)
        cache_key = f"{projectile}_{symbol}"

        if cache_key in self._xs_cache:
            return self._xs_cache[cache_key]

        xs_path = self._data_dir / "xs" / f"{cache_key}.parquet"
        if not xs_path.exists():
            self._xs_cache[cache_key] = pl.DataFrame()
            return None

        df = pl.read_parquet(xs_path)
        self._xs_cache[cache_key] = df
        return df

    # -- DatabaseProtocol methods --------------------------------------------

    def get_cross_sections(
        self,
        projectile: str,
        target_Z: int,
        target_A: int,
    ) -> list[CrossSectionData]:
        """Get all residual production cross-sections for a given reaction."""
        df = self._get_xs_df(projectile, target_Z)
        if df is None or df.is_empty():
            return []

        filtered = df.filter(pl.col("target_A") == target_A).sort(
            "residual_Z", "residual_A", "state", "energy_MeV"
        )
        if filtered.is_empty():
            return []

        results: list[CrossSectionData] = []
        for (rz, ra, st), group in filtered.group_by(
            ["residual_Z", "residual_A", "state"], maintain_order=True
        ):
            energies = group["energy_MeV"].to_numpy().astype(np.float64)
            xs = group["xs_mb"].to_numpy().astype(np.float64)
            results.append(
                CrossSectionData(
                    residual_Z=int(rz),
                    residual_A=int(ra),
                    state=str(st),
                    energies_MeV=energies,
                    xs_mb=xs,
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

        filtered = self._stopping.filter(
            (pl.col("source") == source) & (pl.col("target_Z") == target_Z)
        ).sort("energy_MeV")

        energies = filtered["energy_MeV"].to_numpy().astype(np.float64)
        dedx = filtered["dedx"].to_numpy().astype(np.float64)
        result = (energies, dedx)
        self._sp_cache[key] = result
        return result

    def get_natural_abundances(
        self,
        Z: int,
    ) -> dict[int, tuple[float, float]]:
        """Get natural isotopic abundances for an element."""
        filtered = self._abundances.filter(pl.col("Z") == Z)
        return {
            int(row["A"]): (float(row["abundance"]), float(row["atomic_mass"]))
            for row in filtered.iter_rows(named=True)
        }

    def get_decay_data(
        self,
        Z: int,
        A: int,
        state: str = "",
    ) -> DecayData | None:
        """Get decay data for a nuclide. Returns None if not found."""
        filtered = self._decay.filter(
            (pl.col("Z") == Z)
            & (pl.col("A") == A)
            & (pl.col("state") == state)
        )
        if filtered.is_empty():
            return None

        rows = filtered.to_dicts()
        modes = [
            DecayMode(
                mode=r["decay_mode"],
                daughter_Z=r["daughter_Z"],
                daughter_A=r["daughter_A"],
                daughter_state=r["daughter_state"] or "",
                branching=r["branching"],
            )
            for r in rows
        ]
        return DecayData(
            Z=Z,
            A=A,
            state=state,
            half_life_s=rows[0]["half_life_s"],
            decay_modes=modes,
        )

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
        xs_path = self._data_dir / "xs" / f"{projectile}_{symbol}.parquet"
        return xs_path.exists()

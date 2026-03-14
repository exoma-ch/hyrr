"""Decay chain discovery and coupled ODE solver.

Discovers daughter chains from directly-produced isotopes by walking
the decay database, then solves the coupled rate equations using
matrix exponential (scipy.linalg.expm).
"""

from __future__ import annotations

import logging
import math
from collections import deque
from dataclasses import dataclass, field
from typing import TYPE_CHECKING

import numpy as np
import numpy.typing as npt

if TYPE_CHECKING:
    from hyrr.db import DatabaseProtocol, DecayMode
    from hyrr.models import CurrentProfile

logger = logging.getLogger(__name__)

LN2 = math.log(2)


@dataclass
class ChainIsotope:
    """Single isotope in a decay chain."""

    Z: int
    A: int
    state: str
    half_life_s: float | None  # None for stable
    production_rate: float  # 0.0 for pure daughters
    decay_modes: list[DecayMode] = field(default_factory=list)

    @property
    def key(self) -> tuple[int, int, str]:
        """Unique identifier for this isotope."""
        return (self.Z, self.A, self.state)

    @property
    def is_stable(self) -> bool:
        """Whether this isotope is stable."""
        return self.half_life_s is None or self.half_life_s <= 0


@dataclass
class ChainSolution:
    """Solution of a coupled decay chain."""

    isotopes: list[ChainIsotope]
    time_grid_s: npt.NDArray[np.float64]
    abundances: npt.NDArray[np.float64]  # (n_isotopes, n_times)
    activities: npt.NDArray[np.float64]  # (n_isotopes, n_times)
    activities_direct: npt.NDArray[np.float64]  # (n_isotopes, n_times)
    activities_ingrowth: npt.NDArray[np.float64]  # (n_isotopes, n_times)


def discover_chains(
    db: DatabaseProtocol,
    direct_isotopes: list[tuple[int, int, str, float]],
    max_depth: int = 10,
) -> list[ChainIsotope]:
    """Discover full decay chains from directly-produced isotopes.

    BFS from each directly-produced isotope, following decay daughters.
    Merges production rates if the same isotope appears from multiple parents.

    Args:
        db: Nuclear data provider.
        direct_isotopes: List of (Z, A, state, production_rate) tuples.
        max_depth: Maximum chain depth to prevent infinite loops.

    Returns:
        List of ChainIsotope in topological order (parents before daughters).
    """
    # Map (Z, A, state) -> ChainIsotope
    isotope_map: dict[tuple[int, int, str], ChainIsotope] = {}

    # Track which isotopes to visit: (Z, A, state, depth)
    queue: deque[tuple[int, int, str, int]] = deque()

    # Seed with directly-produced isotopes
    for Z, A, state, rate in direct_isotopes:
        key = (Z, A, state)
        if key in isotope_map:
            isotope_map[key].production_rate += rate
        else:
            decay = db.get_decay_data(Z, A, state)
            half_life: float | None = None
            modes: list[DecayMode] = []
            if decay is not None:
                half_life = decay.half_life_s
                modes = list(decay.decay_modes)
            isotope_map[key] = ChainIsotope(
                Z=Z,
                A=A,
                state=state,
                half_life_s=half_life,
                production_rate=rate,
                decay_modes=modes,
            )
            queue.append((Z, A, state, 0))

    # BFS through daughters
    while queue:
        Z, A, state, depth = queue.popleft()
        if depth >= max_depth:
            continue

        parent = isotope_map[(Z, A, state)]
        if parent.is_stable:
            continue

        for mode in parent.decay_modes:
            if mode.daughter_Z is None or mode.daughter_A is None:
                continue
            if mode.mode == "stable":
                continue

            dkey = (mode.daughter_Z, mode.daughter_A, mode.daughter_state)
            if dkey not in isotope_map:
                decay = db.get_decay_data(
                    mode.daughter_Z,
                    mode.daughter_A,
                    mode.daughter_state,
                )
                d_half: float | None = None
                d_modes: list[DecayMode] = []
                if decay is not None:
                    d_half = decay.half_life_s
                    d_modes = list(decay.decay_modes)
                isotope_map[dkey] = ChainIsotope(
                    Z=mode.daughter_Z,
                    A=mode.daughter_A,
                    state=mode.daughter_state,
                    half_life_s=d_half,
                    production_rate=0.0,
                    decay_modes=d_modes,
                )
                queue.append(
                    (mode.daughter_Z, mode.daughter_A, mode.daughter_state, depth + 1)
                )

    # Topological sort: parents before daughters
    return _topological_sort(isotope_map)


def _topological_sort(
    isotope_map: dict[tuple[int, int, str], ChainIsotope],
) -> list[ChainIsotope]:
    """Sort isotopes so parents come before daughters."""
    # Build adjacency: parent -> set of daughter keys
    children: dict[tuple[int, int, str], set[tuple[int, int, str]]] = {
        k: set() for k in isotope_map
    }
    in_degree: dict[tuple[int, int, str], int] = dict.fromkeys(isotope_map, 0)

    for key, iso in isotope_map.items():
        for mode in iso.decay_modes:
            if mode.daughter_Z is None or mode.daughter_A is None:
                continue
            dkey = (mode.daughter_Z, mode.daughter_A, mode.daughter_state)
            if dkey in isotope_map:
                children[key].add(dkey)
                in_degree[dkey] += 1

    # Kahn's algorithm
    queue: deque[tuple[int, int, str]] = deque()
    for key, deg in in_degree.items():
        if deg == 0:
            queue.append(key)

    result: list[ChainIsotope] = []
    while queue:
        key = queue.popleft()
        result.append(isotope_map[key])
        for child in children[key]:
            in_degree[child] -= 1
            if in_degree[child] == 0:
                queue.append(child)

    # If cycle detected, append remaining (shouldn't happen in nuclear physics)
    if len(result) < len(isotope_map):
        for _key, iso in isotope_map.items():
            if iso not in result:
                result.append(iso)

    return result


def solve_chain(
    chain: list[ChainIsotope],
    irradiation_time_s: float,
    cooling_time_s: float,
    beam_particles_per_s: float,
    n_time_points: int = 200,
    current_profile: CurrentProfile | None = None,
    nominal_current_mA: float = 1.0,
) -> ChainSolution:
    """Solve coupled decay chain equations using matrix exponential.

    During irradiation: dN/dt = A*N + R(t) (augmented matrix trick)
    During cooling: dN/dt = A*N (no production)

    When ``current_profile`` is provided, production rates are scaled
    piecewise by I(t)/I_nominal, where I_nominal is the beam current
    used to compute the rates in ``ChainIsotope.production_rate``.

    Source attribution: direct component computed via independent Bateman,
    ingrowth = total - direct.

    Args:
        chain: Ordered list of ChainIsotope (from discover_chains).
        irradiation_time_s: Irradiation duration [s].
        cooling_time_s: Cooling duration [s].
        beam_particles_per_s: Beam intensity (documentary; rates already
            in ChainIsotope.production_rate).
        n_time_points: Total number of time points.
        current_profile: Optional time-varying beam current.
        nominal_current_mA: Beam current used to compute the nominal
            production rates (needed to scale when current_profile is set).

    Returns:
        ChainSolution with time grids and per-isotope activities.
    """
    from scipy.linalg import expm

    n = len(chain)
    if n == 0:
        empty = np.array([], dtype=np.float64)
        return ChainSolution(
            isotopes=chain,
            time_grid_s=empty,
            abundances=np.empty((0, 0), dtype=np.float64),
            activities=np.empty((0, 0), dtype=np.float64),
            activities_direct=np.empty((0, 0), dtype=np.float64),
            activities_ingrowth=np.empty((0, 0), dtype=np.float64),
        )

    # Build index map
    idx = {iso.key: i for i, iso in enumerate(chain)}

    # Build decay matrix A (n x n)
    A = np.zeros((n, n), dtype=np.float64)
    for i, iso in enumerate(chain):
        if iso.is_stable:
            continue
        lam = LN2 / iso.half_life_s  # type: ignore[operator]
        A[i, i] = -lam
        for mode in iso.decay_modes:
            if mode.daughter_Z is None or mode.daughter_A is None:
                continue
            dkey = (mode.daughter_Z, mode.daughter_A, mode.daughter_state)
            if dkey in idx:
                j = idx[dkey]
                A[j, i] += lam * mode.branching

    # Nominal production rate vector (at nominal beam current)
    R_nominal = np.array(
        [iso.production_rate for iso in chain],
        dtype=np.float64,
    )

    # Time grid: half irradiation, half cooling
    n_irr = n_time_points // 2
    n_cool = n_time_points - n_irr

    t_irr = np.linspace(0, irradiation_time_s, n_irr)
    t_cool = np.linspace(
        irradiation_time_s,
        irradiation_time_s + cooling_time_s,
        n_cool + 1,
    )[1:]
    time_grid = np.concatenate([t_irr, t_cool])
    n_t = len(time_grid)

    # Abundances array (number of atoms)
    abundances = np.zeros((n, n_t), dtype=np.float64)
    activities = np.zeros((n, n_t), dtype=np.float64)

    # --- Irradiation phase ---
    if current_profile is not None:
        # Piecewise-constant current: step through intervals
        abundances = _solve_irradiation_piecewise(
            A,
            R_nominal,
            t_irr,
            n,
            abundances,
            current_profile,
            nominal_current_mA,
            irradiation_time_s,
            expm,
        )
        # End-of-irradiation state
        N_eoi = abundances[:, n_irr - 1]
    else:
        # Constant current: single augmented matrix
        M_irr = np.zeros((n + 1, n + 1), dtype=np.float64)
        M_irr[:n, :n] = A
        M_irr[:n, n] = R_nominal

        N0_aug = np.zeros(n + 1, dtype=np.float64)
        N0_aug[n] = 1.0

        for ti in range(n_irr):
            t = t_irr[ti]
            if t == 0.0:
                abundances[:, ti] = 0.0
            else:
                eM = expm(M_irr * t)
                N_aug = eM @ N0_aug
                abundances[:, ti] = N_aug[:n]

        eM_eoi = expm(M_irr * irradiation_time_s)
        N_eoi = (eM_eoi @ np.append(np.zeros(n), 1.0))[:n]

    # --- Cooling phase: just decay matrix ---
    for ti in range(n_cool):
        dt = t_cool[ti] - irradiation_time_s
        if dt == 0.0:
            abundances[:, n_irr + ti] = N_eoi
        else:
            eA = expm(A * dt)
            abundances[:, n_irr + ti] = eA @ N_eoi

    # Compute activities: A_i = lambda_i * N_i
    for i, iso in enumerate(chain):
        if iso.is_stable:
            activities[i, :] = 0.0
        else:
            lam = LN2 / iso.half_life_s  # type: ignore[operator]
            activities[i, :] = lam * abundances[i, :]

    # --- Direct component (independent Bateman, no coupling) ---
    activities_direct = _compute_direct_component(
        chain,
        time_grid,
        irradiation_time_s,
        n_t,
        current_profile,
        nominal_current_mA,
    )

    # Ingrowth = total - direct (clamp to >= 0 for numerical safety)
    activities_ingrowth = np.maximum(activities - activities_direct, 0.0)

    return ChainSolution(
        isotopes=chain,
        time_grid_s=time_grid,
        abundances=abundances,
        activities=activities,
        activities_direct=activities_direct,
        activities_ingrowth=activities_ingrowth,
    )


def _solve_irradiation_piecewise(
    A: npt.NDArray[np.float64],
    R_nominal: npt.NDArray[np.float64],
    t_irr: npt.NDArray[np.float64],
    n: int,
    abundances: npt.NDArray[np.float64],
    current_profile: CurrentProfile,
    nominal_current_mA: float,
    irradiation_time_s: float,
    expm: object,
) -> npt.NDArray[np.float64]:
    """Solve irradiation phase with piecewise-constant beam current.

    Steps forward through time, merging the current-profile intervals with
    the output time grid so each sub-step has constant R.  O(n_intervals +
    n_time_points) matrix exponentials instead of O(n_intervals * n_time_points).
    """
    intervals = current_profile.intervals(irradiation_time_s)

    # Build a sorted list of all boundary times (interval edges + output times)
    # and tag each output time so we can record the state there.
    output_set = set()
    for ti in range(len(t_irr)):
        output_set.add(float(t_irr[ti]))

    # Merge interval boundaries with output times into ordered checkpoints
    boundary_times: set[float] = {0.0, irradiation_time_s}
    for i_start, i_end, _ in intervals:
        boundary_times.add(i_start)
        boundary_times.add(i_end)
    all_times = sorted(boundary_times | output_set)

    # Step forward through all_times
    N_state = np.zeros(n, dtype=np.float64)
    t_now = 0.0
    # Map output times to column indices for fast lookup
    output_idx: dict[float, list[int]] = {}
    for ti in range(len(t_irr)):
        t_val = float(t_irr[ti])
        output_idx.setdefault(t_val, []).append(ti)

    # Record t=0
    if 0.0 in output_idx:
        for ti in output_idx[0.0]:
            abundances[:, ti] = 0.0

    # Current interval tracker
    iv_idx = 0

    for t_next in all_times:
        if t_next <= t_now:
            continue
        if t_next > irradiation_time_s:
            t_next = irradiation_time_s

        dt = t_next - t_now
        if dt <= 0:
            continue

        # Find the current for this sub-step
        while iv_idx < len(intervals) - 1 and intervals[iv_idx][1] <= t_now:
            iv_idx += 1
        i_current = intervals[iv_idx][2]

        scale = i_current / nominal_current_mA if nominal_current_mA > 0 else 0.0
        R_scaled = R_nominal * scale

        M = np.zeros((n + 1, n + 1), dtype=np.float64)
        M[:n, :n] = A
        M[:n, n] = R_scaled

        N_aug = np.append(N_state, 1.0)
        eM = expm(M * dt)  # type: ignore[operator]
        N_aug = eM @ N_aug
        N_state = N_aug[:n]
        t_now = t_next

        # Record state at output times
        if t_now in output_idx:
            for ti in output_idx[t_now]:
                abundances[:, ti] = N_state

    # Ensure EOI state is stored
    abundances[:, len(t_irr) - 1] = N_state
    return abundances


def _compute_direct_component(
    chain: list[ChainIsotope],
    time_grid: npt.NDArray[np.float64],
    irradiation_time_s: float,
    n_t: int,
    current_profile: CurrentProfile | None,
    nominal_current_mA: float,
) -> npt.NDArray[np.float64]:
    """Compute direct (independent Bateman) activity component.

    For constant current, uses analytical Bateman formula.
    For time-varying current, steps forward through merged interval/output
    boundaries using the scalar recurrence:
        N(t+dt) = N(t)*exp(-lam*dt) + R/lam*(1-exp(-lam*dt))
    """
    n = len(chain)
    activities_direct = np.zeros((n, n_t), dtype=np.float64)

    for i, iso in enumerate(chain):
        if iso.production_rate <= 0 or iso.is_stable:
            continue
        lam = LN2 / iso.half_life_s  # type: ignore[operator]

        if current_profile is None:
            # Analytical Bateman (constant current)
            mask_irr = time_grid <= irradiation_time_s
            activities_direct[i, mask_irr] = iso.production_rate * (
                1.0 - np.exp(-lam * time_grid[mask_irr])
            )
            a_eoi = iso.production_rate * (1.0 - np.exp(-lam * irradiation_time_s))
            mask_cool = time_grid > irradiation_time_s
            dt_cool = time_grid[mask_cool] - irradiation_time_s
            activities_direct[i, mask_cool] = a_eoi * np.exp(-lam * dt_cool)
        else:
            # Forward-stepping through merged boundaries
            intervals = current_profile.intervals(irradiation_time_s)

            # Collect irradiation output times
            irr_outputs: list[tuple[float, int]] = []
            for ti in range(n_t):
                t = float(time_grid[ti])
                if t > irradiation_time_s:
                    break
                irr_outputs.append((t, ti))

            # Merge interval boundaries + output times
            boundary_set: set[float] = {0.0, irradiation_time_s}
            for i_s, i_e, _ in intervals:
                boundary_set.add(i_s)
                boundary_set.add(i_e)
            for t, _ in irr_outputs:
                boundary_set.add(t)
            all_times = sorted(boundary_set)

            # Output lookup
            out_map: dict[float, list[int]] = {}
            for t, ti in irr_outputs:
                out_map.setdefault(t, []).append(ti)

            N_val = 0.0
            t_now = 0.0
            iv_idx = 0

            # Record t=0
            if 0.0 in out_map:
                for ti in out_map[0.0]:
                    activities_direct[i, ti] = 0.0

            for t_next in all_times:
                if t_next <= t_now:
                    continue
                dt = t_next - t_now
                if dt <= 0:
                    continue

                while iv_idx < len(intervals) - 1 and intervals[iv_idx][1] <= t_now:
                    iv_idx += 1
                scale = (
                    intervals[iv_idx][2] / nominal_current_mA
                    if nominal_current_mA > 0
                    else 0.0
                )
                R_t = iso.production_rate * scale

                exp_ldt = math.exp(-lam * dt)
                N_val = N_val * exp_ldt + R_t / lam * (1.0 - exp_ldt)
                t_now = t_next

                if t_now in out_map:
                    for ti in out_map[t_now]:
                        activities_direct[i, ti] = lam * N_val

            # Cooling phase
            a_eoi = lam * N_val
            mask_cool = time_grid > irradiation_time_s
            dt_cool = time_grid[mask_cool] - irradiation_time_s
            activities_direct[i, mask_cool] = a_eoi * np.exp(-lam * dt_cool)

    return activities_direct

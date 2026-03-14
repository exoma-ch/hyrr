"""Projectile resolution for HYRR.

Supports light ions (p, d, t, h, a) and heavy ions (e.g., C-12, O-16, Ne-20).
"""

from __future__ import annotations

import re
from dataclasses import dataclass

from hyrr.db import _SYMBOL_TO_Z


@dataclass(frozen=True)
class Projectile:
    """Resolved projectile definition."""

    symbol: str  # "p", "d", "C-12", "O-16", etc.
    Z: int
    A: int
    charge_state: int  # for beam current -> particles/s conversion


LIGHT_IONS: dict[str, Projectile] = {
    "p": Projectile(symbol="p", Z=1, A=1, charge_state=1),
    "d": Projectile(symbol="d", Z=1, A=2, charge_state=1),
    "t": Projectile(symbol="t", Z=1, A=3, charge_state=1),
    "h": Projectile(symbol="h", Z=2, A=3, charge_state=2),
    "a": Projectile(symbol="a", Z=2, A=4, charge_state=2),
}

_HEAVY_ION_RE = re.compile(r"^([A-Z][a-z]?)-(\d+)$")


def resolve_projectile(name: str) -> Projectile:
    """Parse a projectile name into a Projectile.

    Accepts:
    - Light ions: "p", "d", "t", "h", "a"
    - Heavy ions: "C-12", "O-16", "Ne-20", etc. (Symbol-A notation)

    Heavy ions default to fully stripped (charge_state = Z).
    """
    if name in LIGHT_IONS:
        return LIGHT_IONS[name]

    m = _HEAVY_ION_RE.match(name)
    if m:
        symbol = m.group(1)
        A = int(m.group(2))
        Z = _SYMBOL_TO_Z.get(symbol)
        if Z is None:
            msg = f"Unknown element symbol in projectile: {name!r}"
            raise ValueError(msg)
        return Projectile(symbol=name, Z=Z, A=A, charge_state=Z)

    msg = (
        f"Cannot parse projectile: {name!r}. "
        f"Use light-ion codes (p, d, t, h, a) or heavy-ion notation (e.g., C-12, O-16)."
    )
    raise ValueError(msg)

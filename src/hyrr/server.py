"""FastAPI server for hyrr simulations.

Run with:
    uvicorn hyrr.server:app --reload
    # or
    python -m hyrr.server
"""

from __future__ import annotations

import os
from contextlib import asynccontextmanager
from pathlib import Path
from typing import Any

from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

from hyrr.api import run_simulation
from hyrr.db import DEFAULT_LIBRARY, DataStore

# ─── Database singleton ─────────────────────────────────────────────

_db: DataStore | None = None


def _find_data_dir() -> Path:
    """Locate the nucl-parquet data directory."""
    # 1. Environment variable
    env = os.environ.get("HYRR_DATA")
    if env:
        p = Path(env)
        if p.is_dir():
            return p

    # 2. Git submodule within hyrr repo
    repo_root = Path(__file__).parent.parent.parent
    submodule = repo_root / "nucl-parquet"
    if submodule.is_dir() and (submodule / "meta").is_dir():
        return submodule

    # 3. Sibling nucl-parquet repo
    sibling = repo_root.parent / "nucl-parquet"
    if sibling.is_dir() and (sibling / "meta").is_dir():
        return sibling

    # 4. User home
    home_dir = Path.home() / ".hyrr" / "nucl-parquet"
    if home_dir.is_dir():
        return home_dir

    msg = (
        "nucl-parquet directory not found. Set HYRR_DATA env var or clone "
        "nucl-parquet as a sibling directory."
    )
    raise FileNotFoundError(msg)


@asynccontextmanager
async def lifespan(app: FastAPI):
    global _db
    data_dir = _find_data_dir()
    library = os.environ.get("HYRR_LIBRARY", DEFAULT_LIBRARY)
    _db = DataStore(data_dir, library=library)
    yield
    _db = None


# ─── Pydantic models ────────────────────────────────────────────────


class BeamConfig(BaseModel):
    projectile: str
    energy_MeV: float = Field(gt=0, le=200)
    current_mA: float = Field(gt=0, le=100)


class LayerConfig(BaseModel):
    material: str
    enrichment: dict[str, dict[str, float]] | None = None
    thickness_cm: float | None = None
    areal_density_g_cm2: float | None = None
    energy_out_MeV: float | None = None
    is_monitor: bool = False


class SimulationRequest(BaseModel):
    beam: BeamConfig
    layers: list[LayerConfig] = Field(min_length=1, max_length=20)
    irradiation_s: float = Field(gt=0)
    cooling_s: float = Field(ge=0)


class MaterialInfo(BaseModel):
    path: str
    name: str
    category: str
    density_g_cm3: float | None = None
    formula: str | None = None


# ─── App ─────────────────────────────────────────────────────────────

app = FastAPI(
    title="HYRR API",
    description="Radionuclide production simulation API",
    version="0.1.0",
    lifespan=lifespan,
)

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_methods=["*"],
    allow_headers=["*"],
)


@app.post("/api/simulate")
async def simulate(request: SimulationRequest) -> dict[str, Any]:
    """Run a simulation and return results."""
    if _db is None:
        raise HTTPException(503, "Data store not loaded")

    config = request.model_dump()
    # Rename for internal API compatibility
    config["irradiation_s"] = config.pop("irradiation_s")
    config["cooling_s"] = config.pop("cooling_s")

    try:
        return run_simulation(_db, config)
    except Exception as e:
        raise HTTPException(422, str(e)) from e


@app.get("/api/materials")
async def list_materials() -> list[dict[str, Any]]:
    """List all available materials from py-mat."""
    try:
        from pymat import load_all

        materials: list[dict[str, Any]] = []

        def walk(mat: Any, path_parts: list[str]) -> None:
            entry: dict[str, Any] = {
                "path": ".".join(path_parts),
                "name": getattr(mat, "name", path_parts[-1]),
                "category": path_parts[0],
            }
            density = getattr(mat, "density", None)
            if density is not None:
                entry["density_g_cm3"] = float(density)
            formula = getattr(mat, "formula", None)
            if formula:
                entry["formula"] = str(formula)
            materials.append(entry)
            for key, child in getattr(mat, "_children", {}).items():
                walk(child, [*path_parts, key])

        for name, mat in sorted(load_all().items()):
            walk(mat, [name])

        return materials
    except ImportError:
        raise HTTPException(501, "py-mat not installed") from None


@app.get("/api/health")
async def health() -> dict[str, str]:
    return {"status": "ok", "db": "loaded" if _db else "not loaded"}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run("hyrr.server:app", host="0.0.0.0", port=8000, reload=True)

"""Integration test fixtures."""

from __future__ import annotations

import os
from pathlib import Path

import pytest


def _find_data_dir() -> Path | None:
    """Try to find parquet data directory."""
    env_dir = os.environ.get("HYRR_DATA", "")
    candidates: list[Path] = []
    if env_dir:
        candidates.append(Path(env_dir))
    candidates.extend([
        Path(__file__).parent.parent.parent / "data" / "parquet",
        Path.home() / ".hyrr" / "parquet",
    ])
    for p in candidates:
        if p.is_dir():
            return p
    return None


# Skip all integration tests if data not available
pytestmark = pytest.mark.integration

requires_db = pytest.mark.skipif(
    _find_data_dir() is None,
    reason="parquet data directory not found",
)


@pytest.fixture
def data_path() -> Path:
    """Path to the parquet data directory."""
    path = _find_data_dir()
    if path is None:
        pytest.skip("Data directory not available")
    return path


@pytest.fixture
def database(data_path: Path):  # type: ignore[no-untyped-def]
    """DataStore instance for integration tests."""
    from hyrr.db import DataStore

    return DataStore(data_path)

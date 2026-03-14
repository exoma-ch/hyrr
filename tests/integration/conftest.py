"""Integration test fixtures."""

from __future__ import annotations

import os
from pathlib import Path

import pytest


def _find_data_dir() -> Path | None:
    """Try to find nucl-parquet data directory."""
    env_dir = os.environ.get("HYRR_DATA", "")
    candidates: list[Path] = []
    if env_dir:
        candidates.append(Path(env_dir))
    repo_root = Path(__file__).parent.parent.parent
    candidates.extend(
        [
            repo_root / "nucl-parquet",  # git submodule
            repo_root / ".." / "nucl-parquet",  # sibling repo
            repo_root / "data" / "parquet",  # legacy
            Path.home() / ".hyrr" / "nucl-parquet",
        ]
    )
    for p in candidates:
        p = p.resolve()
        if p.is_dir() and (p / "meta").is_dir():
            return p
    return None


# Skip all integration tests if data not available
pytestmark = pytest.mark.integration

requires_db = pytest.mark.skipif(
    _find_data_dir() is None,
    reason="nucl-parquet data directory not found",
)


@pytest.fixture
def data_path() -> Path:
    """Path to the nucl-parquet data directory."""
    path = _find_data_dir()
    if path is None:
        pytest.skip("Data directory not available")
    return path


@pytest.fixture
def database(data_path: Path):  # type: ignore[no-untyped-def]
    """DataStore instance for integration tests."""
    from hyrr.db import DataStore

    return DataStore(data_path)

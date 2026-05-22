"""Root conftest — data availability check for the test suite.

Nuclear data is no longer vendored in git (#259). Tests that need it
skip automatically, but this plugin prints a one-time hint if the data
directory is missing so a new contributor knows what to do.
"""

from __future__ import annotations

import os
from pathlib import Path

_HINT_PRINTED = False


def pytest_collection_modifyitems(config, items):  # noqa: ANN001, ARG001
    """Print a one-time hint if nuclear data is not available."""
    global _HINT_PRINTED  # noqa: PLW0603
    if _HINT_PRINTED:
        return

    # Quick check: is data available anywhere?
    candidates = [
        os.environ.get("HYRR_DATA", ""),
    ]
    repo_root = Path(__file__).parent.parent
    candidates.extend([
        str(repo_root / "nucl-parquet" / "data"),
        str(repo_root / "data" / "parquet"),
        str(Path.home() / ".hyrr" / "nucl-parquet"),
    ])

    for c in candidates:
        if c and Path(c).is_dir() and (Path(c) / "meta").is_dir():
            return  # Data found, nothing to say

    _HINT_PRINTED = True
    print(
        "\n"
        "╭─────────────────────────────────────────────────────────╮\n"
        "│  Nuclear data not found — integration tests will skip. │\n"
        "│  Run: hyrr fetch-data --all                            │\n"
        "│  Or:  git submodule update --init nucl-parquet         │\n"
        "╰─────────────────────────────────────────────────────────╯\n"
    )

"""Integration tests for hyrr CLI (require nuclear data)."""

from __future__ import annotations

from pathlib import Path

import pytest

from hyrr.cli import main


class TestCliRunIntegration:
    """Integration tests requiring parquet data."""

    def test_run_sample_toml(self, data_path) -> None:
        """Run with sample TOML and real data."""
        sample = Path(__file__).parent.parent / "data" / "sample_run.toml"
        if not sample.exists():
            pytest.skip("Sample TOML not found")
        result = main(["run", str(sample), "--data-dir", str(data_path)])
        assert result == 0

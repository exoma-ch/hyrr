"""Tests for hyrr.api JSON bridge (unit tests — no nuclear data needed)."""

from hyrr.api import _safe_float


class TestSafeFloat:
    def test_normal(self):
        assert _safe_float(3.14) == 3.14

    def test_nan(self):
        assert _safe_float(float("nan")) is None

    def test_inf(self):
        assert _safe_float(float("inf")) is None

    def test_neg_inf(self):
        assert _safe_float(float("-inf")) is None

    def test_zero(self):
        assert _safe_float(0.0) == 0.0

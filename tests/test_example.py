"""Smoke test for hyrr package."""


def test_import() -> None:
    """Verify hyrr can be imported."""
    import hyrr

    assert hyrr.__version__ == "0.1.0"

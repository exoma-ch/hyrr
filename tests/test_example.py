"""Smoke test for hyrr package."""

from importlib.metadata import version


def test_import() -> None:
    """Verify hyrr can be imported and __version__ matches pyproject.toml."""
    import hyrr

    assert hyrr.__version__ == version("hyrr")

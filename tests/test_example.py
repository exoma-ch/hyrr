"""Example tests for hyrr."""


def test_example():
    """Example test that always passes."""
    assert True


def test_import():
    """Test that the package can be imported."""
    import hyrr  # noqa: F401 - renamed to project name by init-workspace.sh

    assert hyrr.__version__ == "0.1.0"

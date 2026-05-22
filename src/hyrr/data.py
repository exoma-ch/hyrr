"""Public data-management API for HYRR (#264).

Wraps the Rust data_fetch module (via the PyO3 _native extension) so
library users can programmatically fetch nuclear data without going
through the CLI.

    >>> import hyrr
    >>> hyrr.fetch_data()                         # meta + stopping (default)
    >>> hyrr.fetch_data(library="tendl-2023-iso")  # + specific library
    >>> hyrr.fetch_data(all_libs=True)             # everything (~400 MB)
"""

from __future__ import annotations

from collections.abc import Callable


def fetch_data(
    *,
    library: str | None = None,
    all_libs: bool = False,
    from_tarball: str | None = None,
    offline_bundle: str | None = None,
    progress: Callable | None = None,
) -> None:
    """Fetch nuclear data into the managed cache.

    The cache lives at ``~/.hyrr/nucl-parquet/v{V}/`` with a
    ``.complete`` sentinel. Idempotent — returns immediately on a
    warm cache.

    Parameters
    ----------
    library : str, optional
        Fetch a specific library (e.g. ``"tendl-2023-iso"``).
        Fetches meta+stopping+library.
    all_libs : bool
        Fetch every library (~400 MB).
    from_tarball : str, optional
        Install from a local ``.tar.zst`` archive (offline install).
    offline_bundle : str, optional
        Export the current cache to a portable ``.tar.zst`` tarball.
    progress : callable, optional
        ``progress(stage, bytes_done, bytes_total)`` callback.
        ``stage`` is one of ``"connecting"``, ``"downloading"``,
        ``"extracting"``, ``"verifying"``.

    Raises
    ------
    RuntimeError
        If the Rust native extension is not available.
    Exception
        On network failure, disk-full, etc.
    """
    try:
        from hyrr import _native
    except ImportError as exc:
        raise RuntimeError(
            "hyrr._native not available — fetch_data requires the compiled "
            "Rust extension. Reinstall hyrr or run `maturin develop` if "
            "working from source."
        ) from exc

    # Adapt the user-facing (stage, done, total) callback to the PyO3
    # dict-based callback shape.
    pyo3_cb = None
    if progress is not None:

        def pyo3_cb(info: dict) -> None:
            progress(
                info.get("stage", "unknown"),
                info.get("bytes_done", 0),
                info.get("bytes_total"),
            )

    _native.py_fetch_data(
        library=library,
        all_libs=all_libs,
        from_tarball=from_tarball,
        offline_bundle=offline_bundle,
        progress=pyo3_cb,
    )

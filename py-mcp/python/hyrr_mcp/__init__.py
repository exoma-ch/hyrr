"""HYRR MCP server — Python entry point for `uvx hyrr-mcp`.

The package is a thin wrapper. All MCP tool logic lives in hyrr-core
(Rust); this module just parses args, resolves the data directory,
and hands stdin/stdout to the native stdio loop.
"""

from __future__ import annotations

import os
import sys

from . import _native

__version__ = _native.__version__


def _print_help() -> None:
    default_lib = _native.default_library()
    print(
        f"hyrr-mcp {__version__}\n\n"
        "Stdio MCP server exposing HYRR radio-isotope production tools.\n\n"
        "USAGE:\n"
        "    hyrr-mcp [--data-dir PATH] [--library ID]\n\n"
        "OPTIONS:\n"
        "    --data-dir PATH    Override nucl-parquet data directory\n"
        f"    --library ID       Nuclear data library, e.g. {default_lib} (default), endfb-8.1\n"
        "    --version, -V      Print version and exit\n"
        "    --help, -h         Print this help and exit\n\n"
        "ENVIRONMENT:\n"
        "    HYRR_DATA          Nucl-parquet data directory (if --data-dir not set)\n"
        "    NUCL_PARQUET_DATA  Nucl-parquet data directory (backward-compat alternative)\n"
        "    HYRR_LIBRARY       Nuclear data library (if --library not set)\n\n"
        "On first run (no local data, no --data-dir / HYRR_DATA), fetches the\n"
        "nucl-parquet release matching the submodule-pinned data version\n"
        "(~54 MB metadata + stopping tables plus the requested library subtree).\n"
        "Cached in ~/.hyrr/nucl-parquet/v{V}/. A failed fetch is a hard error\n"
        "with a clear diagnostic — never a silent-empty data dir.\n\n"
        "Register with Claude Code:\n"
        "    claude mcp add hyrr -- uvx hyrr-mcp\n"
    )


def _arg_value(argv: list[str], flag: str) -> str | None:
    for i, arg in enumerate(argv):
        if arg == flag and i + 1 < len(argv):
            return argv[i + 1]
    return None


def main() -> int:
    argv = sys.argv[1:]
    if any(a in {"--version", "-V"} for a in argv):
        print(f"hyrr-mcp {__version__}")
        return 0
    if any(a in {"--help", "-h"} for a in argv):
        _print_help()
        return 0

    # Resolve library: --library arg → HYRR_LIBRARY env → DEFAULT_LIBRARY.
    library = _arg_value(argv, "--library") or os.environ.get("HYRR_LIBRARY") or None

    # Resolve data dir: explicit --data-dir / HYRR_DATA / NUCL_PARQUET_DATA
    # wins verbatim. Otherwise `_native.ensure_data` resolves locally
    # (managed cache / sibling clone) and falls back to fetching the
    # CalVer release matching the submodule-pinned DATA_VERSION (#529).
    # A failed fetch raises RuntimeError with the expected release tag +
    # URL + cache dir — no silent partial-cache fallthrough.
    data_dir = (
        _arg_value(argv, "--data-dir")
        or os.environ.get("HYRR_DATA")
        or os.environ.get("NUCL_PARQUET_DATA")
    )
    if data_dir is None:
        data_dir = _native.ensure_data(library)

    _native.run(data_dir, library)
    return 0


if __name__ == "__main__":  # pragma: no cover
    sys.exit(main())

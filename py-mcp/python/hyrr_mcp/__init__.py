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
    print(
        f"hyrr-mcp {__version__}\n\n"
        "Stdio MCP server exposing HYRR radio-isotope production tools.\n\n"
        "USAGE:\n"
        "    hyrr-mcp [--data-dir PATH]\n\n"
        "OPTIONS:\n"
        "    --data-dir PATH   Override nucl-parquet data directory\n"
        "    --version, -V     Print version and exit\n"
        "    --help, -h        Print this help and exit\n\n"
        "ENVIRONMENT:\n"
        "    HYRR_DATA         Nucl-parquet data directory (if --data-dir not set)\n\n"
        "Register with Claude Code:\n"
        "    claude mcp add hyrr -- uvx hyrr-mcp\n"
    )


def main() -> int:
    argv = sys.argv[1:]
    if any(a in {"--version", "-V"} for a in argv):
        print(f"hyrr-mcp {__version__}")
        return 0
    if any(a in {"--help", "-h"} for a in argv):
        _print_help()
        return 0

    # Resolve data dir: explicit --data-dir wins, otherwise fall back to
    # the shared Rust resolver (env + sibling + home).
    data_dir: str | None = None
    for i, arg in enumerate(argv):
        if arg == "--data-dir" and i + 1 < len(argv):
            data_dir = argv[i + 1]
            break
    if data_dir is None:
        data_dir = os.environ.get("HYRR_DATA") or _native.resolve_data_dir()

    _native.run(data_dir)
    return 0


if __name__ == "__main__":  # pragma: no cover
    sys.exit(main())

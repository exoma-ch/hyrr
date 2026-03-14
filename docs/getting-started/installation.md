# Installation

## Web App (no install)

The fastest way to use HYRR: **[exoma-ch.github.io/hyrr](https://exoma-ch.github.io/hyrr/)**

Everything runs in your browser — no Python, no install, no data upload.

## Python Package

### From PyPI (when released)

```bash
uv add hyrr
```

### From source

```bash
git clone --recurse-submodules https://github.com/exoma-ch/hyrr.git
cd hyrr
uv sync
```

## Nuclear Data

HYRR uses the [nucl-parquet](https://github.com/exoma-ch/nucl-parquet) data package, included as a git submodule. The data is resolved in this order:

1. `--data-dir` CLI argument
2. `HYRR_DATA` environment variable
3. `../nucl-parquet` sibling directory (submodule)
4. `~/.hyrr/nucl-parquet` fallback

To download data manually:

```bash
hyrr download-data
```

The default library is `tendl-2024`. Override with `--library` or `HYRR_LIBRARY` env var.

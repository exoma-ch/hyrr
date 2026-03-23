# Installation

## Web App (no install)

The fastest way to use HYRR: **[exoma-ch.github.io/hyrr](https://exoma-ch.github.io/hyrr/)**

Everything runs in your browser — no Python, no install, no data upload.

## Desktop App (offline)

For air-gapped machines or offline use, download the native desktop app from **[GitHub Releases](https://github.com/exoma-ch/hyrr/releases)**:

| Platform | Artifact |
|---|---|
| Windows 10+ | `.msi` installer or `.exe` (NSIS) |
| macOS 10.15+ (Apple Silicon) | `.dmg` |
| macOS 10.15+ (Intel) | `.dmg` |
| Ubuntu 22.04+ | `.deb` or `.AppImage` |

All nuclear data (~68 MB Parquet) is bundled — no internet connection required after install. Installer size ~15 MB.

## Python Package

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

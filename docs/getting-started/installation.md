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

The installer ships with the always-needed `meta/` and `stopping/` data (~54 MB) bundled. The chosen cross-section library (default `tendl-2023-iso`, ~50 MB) is downloaded on first launch into `~/.hyrr/nucl-parquet/v{version}/`. A returning user pays no network cost; switching libraries triggers another small download. **Installer size ~80 MB** (was ~15 MB before #52 — the difference is bundled meta+stopping).

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
3. Managed cache at `~/.hyrr/nucl-parquet/v{version}/` (sentinel-protected)
4. `../nucl-parquet` sibling directory (submodule, dev layout)

### Fetching data

The Python CLI populates the same managed cache the desktop app uses:

```bash
hyrr fetch-data                       # default: meta + stopping
hyrr fetch-data --library tendl-2023-iso  # specific library (~50 MB)
hyrr fetch-data --all                 # every library (~400 MB)
```

The default library is `tendl-2023-iso`. Override with `--library` or `HYRR_LIBRARY` env var. The cache is sentinel-protected: a partial download or interrupted extract leaves the dir behind for cleanup but is never picked up as a usable cache.

### Air-gapped install

For machines without internet (Faraday-cage labs, isolated networks):

1. **On a connected machine**, download the signed bundle:

   ```bash
   hyrr fetch-data --offline-bundle hyrr-data.tar.zst
   ```

   This writes **two** files: `hyrr-data.tar.zst` and `hyrr-data.tar.zst.minisig`.

2. **Copy both files** to the air-gapped machine (USB stick, internal share, …).
   The signature is small but not optional — the install refuses a bundle it
   cannot authenticate, and there is no override.

3. **On the air-gapped machine**, install the bundle:

   ```bash
   hyrr fetch-data --from hyrr-data.tar.zst
   ```

   If the signature travelled separately, point at it explicitly:

   ```bash
   hyrr fetch-data --from hyrr-data.tar.zst --signature /other/media/hyrr-data.tar.zst.minisig
   ```

Both machines must run matching `hyrr` versions for the bundle to apply cleanly — the cache is version-pinned, so a v0.9 bundle won't be picked up by a v0.10 install. Alternatively, drop the upstream GitHub Releases tarball directly. The release URL pattern — host, path layout, tarball filename — is defined by `hyrr_core::data_fetch::release_url()` / `tarball_filename()`. Read your installed version from Python with `python -c "import hyrr._native as n; print(n.py_data_version())"` and substitute it for `<V>` below:

```bash
# On a connected machine — substitute <V> with the version printed above
# (data version is CalVer YYYY.MM.MICRO, e.g. 2026.5.0; no `v` prefix).
# Fetch the SIGNATURE too — the install refuses an unsigned bundle.
curl -LO "https://github.com/exoma-ch/nucl-parquet/releases/download/data-<V>/nucl-parquet-data-<V>.tar.zst"
curl -LO "https://github.com/exoma-ch/nucl-parquet/releases/download/data-<V>/nucl-parquet-data-<V>.tar.zst.minisig"

# On the air-gapped machine
hyrr fetch-data --from "nucl-parquet-data-<V>.tar.zst"
```

The bundle's data version must match the one your `hyrr` build pins — a
validly-signed bundle for a different release is refused, because the cache is
version-keyed and because accepting an older signed release is exactly the
rollback a signature alone does not prevent.

> **Sites with content-scanning (CDR) gateways:** appliances that unpack and
> repack archives in transit will break the signature even though the data is
> intact, and the install will refuse. See
> [`docs/DATA_INTEGRITY.md`](../DATA_INTEGRITY.md) — this is a known limitation
> tracked upstream, and the workaround is a transfer route that preserves bytes.

#!/usr/bin/env bash
# Build the hyrr._native PyO3 extension (py/ crate) and install it into the
# editable source tree so `import hyrr._native` works for local development and
# the tests/integration suite.
#
# Why not `maturin develop`? The main `hyrr` package is a pure-Python hatchling
# project under src/hyrr (installed editable by `uv`). `maturin develop` with
# module-name `hyrr._native` would install a *competing* `hyrr/` package into
# site-packages and shadow the editable source. The py/ crate is also
# deliberately excluded from the cargo workspace (the pyo3 extension-module
# feature gate breaks linking under `cargo test`), so we build it standalone and
# drop the cdylib next to the Python sources. The .so is gitignored.
#
# Usage: scripts/build-native.sh [--debug]
set -euo pipefail

cd "$(dirname "$0")/.."

PROFILE="release"
PROFILE_FLAG="--release"
if [[ "${1:-}" == "--debug" ]]; then
    PROFILE="debug"
    PROFILE_FLAG=""
fi

echo "==> Building py/ (hyrr._native) in $PROFILE profile"
# shellcheck disable=SC2086
cargo build $PROFILE_FLAG --manifest-path py/Cargo.toml

SRC="py/target/$PROFILE/lib_native.so"
DST="src/hyrr/_native.so"
if [[ ! -f "$SRC" ]]; then
    echo "::error:: expected artifact not found: $SRC" >&2
    exit 1
fi

cp -f "$SRC" "$DST"
echo "==> Installed $DST"

python -c "import hyrr._native; print('==> import hyrr._native OK')" 2>/dev/null \
    || uv run python -c "import hyrr._native; print('==> import hyrr._native OK (via uv)')"

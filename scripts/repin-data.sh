#!/usr/bin/env bash
# repin-data.sh — re-pin the nuclear-data tarball integrity hash (#577).
#
# `core/build.rs` refuses to build when `hyrr.json`'s `data_tarball_version`
# disagrees with the pinned `nucl-parquet` submodule's
# `data/catalog.json::data_version`. That is deliberate: a stale pin would
# otherwise surface at runtime as a checksum mismatch, which is
# indistinguishable from a tampered download. Run this after bumping the
# submodule to clear the build error.
#
# Usage:
#   scripts/repin-data.sh                  # trust GitHub's published asset digest (fast)
#   scripts/repin-data.sh --verify-download  # stream the asset and hash it (~800 MB, authoritative)
#
# On trust: the default reads the SHA-256 GitHub records for the release
# asset. That is a *convenience*, not a security control — anyone who could
# tamper with the asset could tamper with the API response, and the release
# is mutable unless Immutable Releases is enabled on the upstream repo. What
# makes the pin meaningful is that it then lives in git and is reviewed in a
# PR like any other change. If you are pinning a release you did not cut
# yourself, or anything feels off, use --verify-download.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

VERIFY_DOWNLOAD=0
[ "${1:-}" = "--verify-download" ] && VERIFY_DOWNLOAD=1

CATALOG="nucl-parquet/data/catalog.json"
if [ ! -f "$CATALOG" ]; then
  echo "error: $CATALOG not found — the submodule isn't checked out." >&2
  echo "  run: git submodule update --init --recursive" >&2
  exit 2
fi

VERSION="$(python3 -c "import json;print(json.load(open('$CATALOG'))['data_version'])")"
ASSET="nucl-parquet-data-${VERSION}.tar.zst"
TAG="data-${VERSION}"
echo "submodule pins data_version ${VERSION} (asset ${ASSET})"

if [ "$VERIFY_DOWNLOAD" -eq 1 ]; then
  URL="https://github.com/exoma-ch/nucl-parquet/releases/download/${TAG}/${ASSET}"
  echo "streaming ${URL} to hash it (this downloads the full asset)..."
  SHA="$(python3 - "$URL" <<'PY'
import hashlib, sys, urllib.request
req = urllib.request.Request(sys.argv[1], headers={"User-Agent": "hyrr/repin"})
h = hashlib.sha256()
with urllib.request.urlopen(req) as f:
    while chunk := f.read(1 << 20):
        h.update(chunk)
print(h.hexdigest())
PY
)"
else
  echo "reading GitHub's published asset digest (use --verify-download to hash it yourself)..."
  SHA="$(python3 - "$TAG" "$ASSET" <<'PY'
import json, sys, urllib.request
tag, asset = sys.argv[1], sys.argv[2]
req = urllib.request.Request(
    f"https://api.github.com/repos/exoma-ch/nucl-parquet/releases/tags/{tag}",
    headers={"User-Agent": "hyrr/repin", "Accept": "application/vnd.github+json"},
)
rel = json.load(urllib.request.urlopen(req))
for a in rel.get("assets", []):
    if a["name"] == asset:
        digest = a.get("digest")
        if not digest:
            # Assets uploaded before GitHub started recording digests (Jun 2025)
            # have a null field. Silently pinning an empty string would disable
            # verification, so refuse and make the maintainer hash it.
            sys.exit(f"error: release asset {asset} has no digest; re-run with --verify-download")
        if not digest.startswith("sha256:"):
            sys.exit(f"error: unexpected digest algorithm: {digest}")
        print(digest.removeprefix("sha256:"))
        break
else:
    sys.exit(f"error: asset {asset} not found on release {tag}")
PY
)"
fi

case "$SHA" in
  [0-9a-f]*) [ "${#SHA}" -eq 64 ] || { echo "error: not a 64-char sha256: $SHA" >&2; exit 1; } ;;
  *) echo "error: not a lowercase hex sha256: $SHA" >&2; exit 1 ;;
esac

python3 - "$VERSION" "$SHA" <<'PY'
import json, sys

version, sha = sys.argv[1], sys.argv[2]
with open("hyrr.json") as f:
    cfg = json.load(f)

old_v = cfg.get("data_tarball_version")
old_s = cfg.get("data_tarball_sha256")
cfg["data_tarball_version"] = version
cfg["data_tarball_sha256"] = sha

with open("hyrr.json", "w") as f:
    json.dump(cfg, f, indent=2, ensure_ascii=False)
    f.write("\n")

if (old_v, old_s) == (version, sha):
    print("hyrr.json already up to date — nothing changed.")
else:
    print(f"hyrr.json: {old_v} {str(old_s)[:12]}… -> {version} {sha[:12]}…")
PY

echo
echo "Now rebuild to confirm the guard is satisfied:  cargo check --manifest-path core/Cargo.toml"
echo "Commit hyrr.json together with the submodule bump — they are one logical change."

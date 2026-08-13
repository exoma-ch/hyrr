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
#   scripts/repin-data.sh                    # read the digest from the SIGNATURE (default)
#   scripts/repin-data.sh --github-digest    # trust GitHub's published asset digest
#   scripts/repin-data.sh --verify-download  # stream the asset and hash it (~800 MB)
#
# On trust — this is the part that changed with #594.
#
# The default takes the digest from the release's detached `.minisig`, whose
# *trusted comment* carries `sha256=...` and is covered by minisign's global
# signature. That makes re-pinning stop being a trust decision: you copy what
# the holder of the offline signing key claimed, not what the release host
# currently serves. The signature itself is verified end-to-end against the
# payload by `core/src/data_fetch.rs` at fetch time; what this script checks
# is that the signature's key id matches `hyrr.json::data_signing_pubkey` and
# that its trusted comment names the expected tag.
#
# `--github-digest` is the old behaviour, kept for pinning a release cut
# before upstream #289 added signatures. It is a *convenience*, not a control:
# anyone who could tamper with the asset could tamper with the API response,
# and upstream releases are mutable. `--verify-download` hashes the bytes
# yourself — authoritative about the payload, but silent about who published
# it, which is exactly the gap the signature closes.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

MODE="signature"
case "${1:-}" in
  --verify-download) MODE="download" ;;
  --github-digest)   MODE="github" ;;
  "")                ;;
  *) echo "error: unknown option: $1" >&2; exit 1 ;;
esac
VERIFY_DOWNLOAD=0
[ "$MODE" = "download" ] && VERIFY_DOWNLOAD=1

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
elif [ "$MODE" = "signature" ]; then
  SIG_URL="https://github.com/exoma-ch/nucl-parquet/releases/download/${TAG}/${ASSET}.minisig"
  echo "reading the digest from the release signature (${SIG_URL})..."
  SHA="$(python3 - "$SIG_URL" "$TAG" <<'SIGPY'
import base64, json, sys, urllib.request

sig_url, tag = sys.argv[1], sys.argv[2]
req = urllib.request.Request(sig_url, headers={"User-Agent": "hyrr/repin"})
try:
    body = urllib.request.urlopen(req).read().decode()
except Exception as e:
    sys.exit(
        f"error: could not fetch {sig_url}: {e}\n"
        "       A release cut before nucl-parquet #289 has no signature; "
        "re-run with --github-digest or --verify-download."
    )

lines = [ln for ln in body.splitlines() if ln.strip()]
if len(lines) < 4:
    sys.exit(f"error: malformed .minisig ({len(lines)} lines, expected 4)")
sig_b64, trusted = lines[1], lines[2]
if not trusted.startswith("trusted comment:"):
    sys.exit("error: .minisig has no trusted comment")

# Key id is bytes 2..10 of both blobs. Comparing them here turns a key
# rotation into a clear message at re-pin time rather than a verification
# failure on a user's first fetch.
with open("hyrr.json") as f:
    pinned_key = json.load(f).get("data_signing_pubkey", "")
if not pinned_key:
    sys.exit("error: hyrr.json has no data_signing_pubkey to check the signature against")
sig_keyid = base64.b64decode(sig_b64)[2:10]
key_keyid = base64.b64decode(pinned_key)[2:10]
if sig_keyid != key_keyid:
    sys.exit(
        f"error: signature key id {sig_keyid.hex()} != pinned key id {key_keyid.hex()}.\n"
        "       The signing key rotated - update data_signing_pubkey deliberately."
    )

fields = trusted.removeprefix("trusted comment:").split()
digest = next((f.removeprefix("sha256=") for f in fields if f.startswith("sha256=")), None)
if digest is None:
    sys.exit("error: trusted comment carries no sha256= field; use --verify-download")
if f"tag={tag}" not in fields:
    sys.exit(f"error: the signature's trusted comment is not for {tag}: {trusted}")
print(digest.lower())
SIGPY
)"
else
  echo "reading GitHub's published asset digest (NOT a security control; see header)..."
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

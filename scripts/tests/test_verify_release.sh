#!/usr/bin/env bash
# Tests for scripts/verify-release.sh (#401), fully offline.
#
# `gh` and `curl` are stubbed onto PATH so every failure mode can be forced —
# including the ones that are, by construction, invisible in production:
# a missing latest.json (which makes every MCP server believe it is current)
# and a missing aarch64 wheel (which shipped silently three times, #461).
#
# The script is also exercised against the real world elsewhere: it passes on
# 0.19.0 and fails on 0.18.0/0.17.0/0.16.3 for exactly the missing aarch64
# wheel. These tests pin the behaviour so that stays true.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
VERIFY="$SCRIPT_DIR/verify-release.sh"

failed=0
report() {
  if [ "$2" = "pass" ]; then echo "PASS: $1"; else echo "FAIL: $1" >&2; failed=1; fi
}

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

root="$work/root"
bin="$work/bin"
mkdir -p "$root/scripts" "$bin"
cp "$VERIFY" "$root/scripts/"

# Stub `gh`: reads its answers from files the test writes per case.
cat > "$bin/gh" <<'GH'
#!/usr/bin/env bash
case "$1 $2" in
  "release view")
    case "$*" in
      *isDraft*) cat "$FIXTURES/release.json" 2>/dev/null || exit 1 ;;
      *assets*)  cat "$FIXTURES/assets.json"  2>/dev/null || exit 1 ;;
    esac
    ;;
  "release download")
    # verify-release.sh passes --output <path>
    for i in $(seq 1 $#); do
      [ "${!i}" = "--output" ] && { j=$((i + 1)); out="${!j}"; }
    done
    [ -f "$FIXTURES/latest.json" ] || exit 1
    cp "$FIXTURES/latest.json" "$out"
    ;;
  "api "*)
    # `gh api <path>` — the path is $2, and the tag is its last segment.
    tag="${2##*/}"
    grep -qxF "$tag" "$FIXTURES/tags.txt" 2>/dev/null || exit 1
    echo '{}'
    ;;
  *) exit 1 ;;
esac
GH

# Stub `curl`: serves the PyPI payload for this case.
cat > "$bin/curl" <<'CURL'
#!/usr/bin/env bash
cat "$FIXTURES/pypi.json" 2>/dev/null || true
CURL

# Stub the release-notes gate: its own behaviour is covered by
# test_check_release_notes.sh, so here it just reports what the case wants.
cat > "$root/scripts/check-release-notes.sh" <<'RN'
#!/usr/bin/env bash
[ -f "$FIXTURES/notes_ok" ] || { echo "no entry"; exit 1; }
RN
chmod +x "$bin/gh" "$bin/curl" "$root/scripts/check-release-notes.sh"

wheels_json() { # wheels_json <filename...>
  local out="" f
  for f in "$@"; do out="${out}{\"filename\":\"${f}\"},"; done
  printf '{"urls":[%s]}' "${out%,}"
}

ALL_WHEELS=(
  "hyrr_mcp-0.19.0-cp311-abi3-manylinux_2_17_x86_64.manylinux2014_x86_64.whl"
  "hyrr_mcp-0.19.0-cp311-abi3-manylinux_2_17_aarch64.manylinux2014_aarch64.whl"
  "hyrr_mcp-0.19.0-cp311-abi3-macosx_11_0_arm64.whl"
  "hyrr_mcp-0.19.0-cp311-abi3-win_amd64.whl"
  "hyrr_mcp-0.19.0.tar.gz"
)

# The desktop bundles a complete v0.19.0 release carries (#647). The first four
# are what latest.json points the updater at; the .dmg pair is direct-download
# only, so nothing in latest.json would reveal their absence.
DESKTOP_PAYLOADS=(
  "HYRR_x64.app.tar.gz"
  "HYRR_aarch64.app.tar.gz"
  "HYRR_0.19.0_x64-setup.exe"
  "HYRR_0.19.0_amd64.AppImage"
)
DESKTOP_DIRECT=("HYRR_0.19.0_x64.dmg" "HYRR_0.19.0_aarch64.dmg")

# assets.json listing latest.json + every payload (each with its .sig) + the
# direct downloads. Extra args are appended verbatim, so a case can drop one.
assets_json() { # assets_json [payload...]
  local out='{"name":"latest.json"}' f
  for f in "$@"; do out="${out},{\"name\":\"${f}\"},{\"name\":\"${f}.sig\"}"; done
  for f in "${DESKTOP_DIRECT[@]}"; do out="${out},{\"name\":\"${f}\"}"; done
  printf '{"assets":[%s]}' "$out"
}

# latest.json with one entry per "<key>=<file>" pair. A file of "" yields an
# entry with an empty signature, which is how the blank-signature case is built.
latest_json() { # latest_json <key=file>...
  local out="" kv key file sig
  for kv in "$@"; do
    key="${kv%%=*}"; file="${kv#*=}"
    sig="c2ln"
    case "$file" in
      NOSIG:*) file="${file#NOSIG:}"; sig="" ;;
    esac
    out="${out}\"${key}\":{\"signature\":\"${sig}\",\"url\":\"https://github.com/exoma-ch/hyrr/releases/download/v0.19.0/${file}\"},"
  done
  printf '{"version":"0.19.0","platforms":{%s}}' "${out%,}"
}

HEALTHY_PLATFORMS=(
  "darwin-x86_64=HYRR_x64.app.tar.gz"
  "darwin-aarch64=HYRR_aarch64.app.tar.gz"
  "windows-x86_64=HYRR_0.19.0_x64-setup.exe"
  "linux-x86_64=HYRR_0.19.0_amd64.AppImage"
)

# Reset every fixture to a fully healthy 0.19.0 release.
reset_fixtures() {
  FIXTURES="$work/fx"
  rm -rf "$FIXTURES"; mkdir -p "$FIXTURES"
  echo '{"isDraft":false,"tagName":"v0.19.0","targetCommitish":"main"}' > "$FIXTURES/release.json"
  assets_json "${DESKTOP_PAYLOADS[@]}" > "$FIXTURES/assets.json"
  latest_json "${HEALTHY_PLATFORMS[@]}" > "$FIXTURES/latest.json"
  printf 'v0.19.0\nhyrr-mcp-v0.19.0\n' > "$FIXTURES/tags.txt"
  wheels_json "${ALL_WHEELS[@]}" > "$FIXTURES/pypi.json"
  touch "$FIXTURES/notes_ok"
  export FIXTURES
}

run_verify() { # → RC / OUT
  OUT="$(PATH="$bin:$PATH" FIXTURES="$FIXTURES" "$root/scripts/verify-release.sh" "$@" 2>&1)"
  RC=$?
}

expect() { # expect <name> <want-rc> [fragment]
  local ok=1
  [ "$RC" -eq "$2" ] || ok=0
  if [ $# -ge 3 ] && [ "${OUT#*"$3"}" = "$OUT" ]; then ok=0; fi
  if [ "$ok" -eq 1 ]; then report "$1" pass; else
    report "$1 (rc=$RC want $2${3+, fragment \'$3\'}) out: $OUT" fail
  fi
}

# ── usage ──────────────────────────────────────────────────────────
reset_fixtures
run_verify
expect "no version is a usage error" 2 "usage: verify-release.sh"
run_verify 0.19.0 --wait abc
expect "a non-numeric --wait is a usage error" 2 "usage: verify-release.sh"
run_verify --wait 5
expect "a flag in place of the version is a usage error" 2 "usage: verify-release.sh"

# ── the healthy release ────────────────────────────────────────────
reset_fixtures
run_verify 0.19.0
expect "a complete release passes" 0 "verified: tags, latest.json"

# ── #461: the wheel that silently was not there ────────────────────
reset_fixtures
wheels_json "${ALL_WHEELS[0]}" "${ALL_WHEELS[2]}" "${ALL_WHEELS[3]}" "${ALL_WHEELS[4]}" \
  > "$FIXTURES/pypi.json"
run_verify 0.19.0
expect "a missing aarch64 wheel fails the release" 1 "manylinux_2_17_aarch64"

reset_fixtures
wheels_json "${ALL_WHEELS[@]}" "hyrr_mcp-0.19.0-cp311-abi3-musllinux_1_2_x86_64.whl" \
  > "$FIXTURES/pypi.json"
run_verify 0.19.0
expect "an unexpected EXTRA wheel also fails" 1 "found 5"

reset_fixtures
wheels_json "${ALL_WHEELS[0]}" "${ALL_WHEELS[1]}" "${ALL_WHEELS[2]}" "${ALL_WHEELS[3]}" \
  > "$FIXTURES/pypi.json"
run_verify 0.19.0
expect "a missing sdist fails" 1 "expected 1 sdist"

reset_fixtures
echo '{}' > "$FIXTURES/pypi.json"
run_verify 0.19.0
expect "a version absent from PyPI fails" 1 "not resolvable on PyPI"

# ── #571: the asset whose absence is silent ────────────────────────
reset_fixtures
echo '{"assets":[{"name":"hyrr.AppImage"}]}' > "$FIXTURES/assets.json"
run_verify 0.19.0
expect "a missing latest.json fails with the silent-updater warning" 1 \
  "silently believe it is up to date"

reset_fixtures
echo '{"version":"0.18.0"}' > "$FIXTURES/latest.json"
run_verify 0.19.0
expect "a latest.json naming the wrong version fails" 1 "latest.json names '0.18.0'"

# Only the version string is under test here, so keep the platforms block
# intact — otherwise this asserts "leading v is accepted" while actually
# passing/failing on the desktop check (#647).
reset_fixtures
sed 's/"version":"0.19.0"/"version":"v0.19.0"/' "$FIXTURES/latest.json" > "$FIXTURES/lj.tmp" \
  && mv "$FIXTURES/lj.tmp" "$FIXTURES/latest.json"
run_verify 0.19.0
expect "a latest.json with a leading v is accepted" 0 "latest.json names 0.19.0"

# ── tags and release state ─────────────────────────────────────────
reset_fixtures
echo '{"isDraft":true,"tagName":"v0.19.0","targetCommitish":"main"}' > "$FIXTURES/release.json"
run_verify 0.19.0
expect "a draft release fails" 1 "still a DRAFT"

reset_fixtures
printf 'v0.19.0\n' > "$FIXTURES/tags.txt"
run_verify 0.19.0
expect "a missing hyrr-mcp-v tag fails (wheels would never publish)" 1 \
  "wheels would build but never publish"

reset_fixtures
rm -f "$FIXTURES/release.json"
run_verify 0.19.0
expect "an absent release fails" 1 "no published GitHub Release"

# ── release notes ──────────────────────────────────────────────────
reset_fixtures
rm -f "$FIXTURES/notes_ok"
run_verify 0.19.0
expect "a missing release-notes entry fails" 1 "no valid entry for 0.19.0"

# ── #647: the desktop artifacts that were not there ────────────────
#
# verify-release.sh reported "verified" for 0.20.1 while ten of seventeen assets
# did not exist. These pin each way that can go wrong, because they fail
# differently and a single check would miss most of them.

# A whole OS absent from latest.json. This is the case a URL-resolvability
# check cannot catch: there is no broken link, just no entry — and every client
# on that platform silently stops seeing updates.
reset_fixtures
latest_json "${HEALTHY_PLATFORMS[0]}" "${HEALTHY_PLATFORMS[1]}" "${HEALTHY_PLATFORMS[2]}" \
  > "$FIXTURES/latest.json"
run_verify 0.19.0
expect "a platform missing from latest.json fails" 1 "NO linux-x86_64 entry"

# latest.json references a bundle that never uploaded — the updater 404s and
# tells the user nothing (#571).
reset_fixtures
assets_json "${DESKTOP_PAYLOADS[0]}" "${DESKTOP_PAYLOADS[1]}" "${DESKTOP_PAYLOADS[3]}" \
  > "$FIXTURES/assets.json"
run_verify 0.19.0
expect "a latest.json URL with no matching asset fails" 1 "HYRR_0.19.0_x64-setup.exe"

# Present but unsigned. The updater refuses unsigned payloads, so this bricks
# updates exactly as thoroughly as a missing file, while looking complete.
reset_fixtures
latest_json "${HEALTHY_PLATFORMS[0]}" "${HEALTHY_PLATFORMS[1]}" \
  "windows-x86_64=NOSIG:HYRR_0.19.0_x64-setup.exe" "${HEALTHY_PLATFORMS[3]}" \
  > "$FIXTURES/latest.json"
run_verify 0.19.0
expect "an empty signature in latest.json fails" 1 "empty signature"

# A latest.json carried over from another release: every URL resolves, but to
# the WRONG build. #516 (tauri-action v1) changes this URL form, so this is
# live surface rather than a hypothetical.
reset_fixtures
sed 's#/download/v0.19.0/#/download/v0.18.0/#g' "$FIXTURES/latest.json" \
  > "$FIXTURES/latest.json.tmp" && mv "$FIXTURES/latest.json.tmp" "$FIXTURES/latest.json"
run_verify 0.19.0
expect "latest.json pointing at another tag fails" 1 "points outside v0.19.0"

# The direct-download bundles are invisible to latest.json, so they need their
# own assertion — a macOS user landing on the release page finds nothing.
reset_fixtures
python3 - "$FIXTURES/assets.json" <<'PY'
import json, sys
p = sys.argv[1]
d = json.load(open(p))
d["assets"] = [a for a in d["assets"] if a["name"] != "HYRR_0.19.0_aarch64.dmg"]
json.dump(d, open(p, "w"))
PY
run_verify 0.19.0
expect "a missing .dmg fails even though latest.json is intact" 1 "HYRR_*_aarch64.dmg"

# ── every check runs ───────────────────────────────────────────────
# A release reported as "5 things wrong" is more useful than one that stops at
# the first, so assert the failures accumulate rather than short-circuit. The
# count is asserted literally so that adding a check without extending this
# case fails here rather than silently weakening the guarantee — which is what
# happened when the desktop check landed (#647) and this still expected 4.
reset_fixtures
rm -f "$FIXTURES/release.json" "$FIXTURES/assets.json" "$FIXTURES/notes_ok"
echo '{}' > "$FIXTURES/pypi.json"
run_verify 0.19.0
expect "all five checks run even when the first fails" 1 "FAILED verification (5)"

if [ "$failed" -eq 0 ]; then
  echo "All verify-release tests passed."
else
  echo "Some verify-release tests FAILED." >&2
fi
exit "$failed"

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

# Reset every fixture to a fully healthy 0.19.0 release.
reset_fixtures() {
  FIXTURES="$work/fx"
  rm -rf "$FIXTURES"; mkdir -p "$FIXTURES"
  echo '{"isDraft":false,"tagName":"v0.19.0","targetCommitish":"main"}' > "$FIXTURES/release.json"
  echo '{"assets":[{"name":"latest.json"},{"name":"hyrr.AppImage"}]}' > "$FIXTURES/assets.json"
  echo '{"version":"0.19.0"}' > "$FIXTURES/latest.json"
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

reset_fixtures
echo '{"version":"v0.19.0"}' > "$FIXTURES/latest.json"
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

# ── every check runs ───────────────────────────────────────────────
# A release reported as "4 things wrong" is more useful than one that stops at
# the first, so assert the failures accumulate rather than short-circuit.
reset_fixtures
rm -f "$FIXTURES/release.json" "$FIXTURES/assets.json" "$FIXTURES/notes_ok"
echo '{}' > "$FIXTURES/pypi.json"
run_verify 0.19.0
expect "all four checks run even when the first fails" 1 "FAILED verification (4)"

if [ "$failed" -eq 0 ]; then
  echo "All verify-release tests passed."
else
  echo "Some verify-release tests FAILED." >&2
fi
exit "$failed"

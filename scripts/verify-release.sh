#!/usr/bin/env bash
# Assert that a PUBLISHED release matches intent (#401).
#
#   scripts/verify-release.sh 0.19.0
#   scripts/verify-release.sh 0.19.0 --wait 2400     # poll while builds finish
#
# Everything HYRR checks today is a PRE-publish gate. #576 added good ones, but
# nothing looks at reality afterwards, and three incidents were a green pipeline
# over a wrong world:
#
#   #529        a shipped hyrr-mcp whose data auto-fetch was broken in the wild.
#   #461/#576   0.16.3, 0.17.0 and 0.18.0 each published with NO aarch64 wheel,
#               silently, because a matrix continue-on-error reports SUCCESS to
#               downstream `needs:`.
#   #565        a stale access whitelist deployed unnoticed.
#
# And #571 added a failure mode with a nasty asymmetry: MCP update-awareness
# reads latest.json from the release assets, so if that asset is missing every
# MCP server silently concludes it is up to date. A broken updater that
# reassures is worse than no updater — hence check 2.
#
# Every check runs even if an earlier one fails: a release is more useful
# reported as "3 of 4 wrong" than as "the first thing was wrong".
#
# Exit codes: 0 all checks passed · 1 at least one failed · 2 usage
#
# Requires: gh (authenticated), curl, jq.
#
# No `set -e`: the checks below deliberately keep going after a failure so the
# report is complete, which -e would defeat.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT" || exit 2

PKG="hyrr-mcp"

# One glob per wheel the release is expected to publish. MUST stay in lockstep
# with the `expected` list in release-hyrr-mcp.yml's verify-artifacts job — that
# one asserts what was BUILT, this one asserts what actually reached PyPI. The
# gap between those two is exactly where #461 lived.
EXPECTED_WHEELS=(
  '*-manylinux_2_17_x86_64*.whl'
  '*-manylinux_2_17_aarch64*.whl'
  '*-macosx_*_arm64.whl'
  '*-win_amd64.whl'
)

usage() {
  echo "usage: verify-release.sh <version> [--wait <seconds>]" >&2
  exit 2
}

VERSION="${1:-}"
[ -n "$VERSION" ] || usage
case "$VERSION" in -*) usage ;; esac
shift

WAIT=0
while [ $# -gt 0 ]; do
  case "$1" in
    --wait) WAIT="${2:-}"; [ -n "$WAIT" ] || usage; shift 2 ;;
    *) usage ;;
  esac
done
case "$WAIT" in ''|*[!0-9]*) usage ;; esac

TAG="v${VERSION}"
MCP_TAG="hyrr-mcp-v${VERSION}"

for tool in gh curl jq; do
  command -v "$tool" >/dev/null 2>&1 || {
    echo "ERROR: '$tool' is required but not installed." >&2
    exit 2
  }
done

FAILURES=()
note_pass() { echo "  ✓ $1"; }
note_fail() { echo "  ✗ $1" >&2; FAILURES+=("$1"); }

# ── 1. the tags exist and the release is really published ──────────
# A release cuts TWO tags at the same commit: v* (app + desktop installers +
# latest.json) and hyrr-mcp-v* (the wheels — release-hyrr-mcp.yml triggers only
# on that one and its publish job is gated on it). A v* tag alone would build
# the wheels and never publish them.
check_tags() {
  echo "[1/5] tags and release"
  local release
  if ! release="$(gh release view "$TAG" --json isDraft,tagName,targetCommitish 2>/dev/null)"; then
    note_fail "no published GitHub Release for ${TAG}"
    return
  fi
  if [ "$(printf '%s' "$release" | jq -r '.isDraft')" = "true" ]; then
    note_fail "release ${TAG} is still a DRAFT"
  else
    note_pass "release ${TAG} is published"
  fi

  if git rev-parse -q --verify "refs/tags/${TAG}" >/dev/null 2>&1 ||
     gh api "repos/{owner}/{repo}/git/ref/tags/${TAG}" >/dev/null 2>&1; then
    note_pass "tag ${TAG} exists"
  else
    note_fail "tag ${TAG} does not exist"
  fi

  if git rev-parse -q --verify "refs/tags/${MCP_TAG}" >/dev/null 2>&1 ||
     gh api "repos/{owner}/{repo}/git/ref/tags/${MCP_TAG}" >/dev/null 2>&1; then
    note_pass "tag ${MCP_TAG} exists (the wheels publish off this one)"
  else
    note_fail "tag ${MCP_TAG} is missing — the wheels would build but never publish"
  fi
}

# ── 2. latest.json ─────────────────────────────────────────────────
# The desktop updater manifest, uploaded to the Release by tauri-action. #571's
# MCP update-awareness reads it too. Its ABSENCE is silent by construction, so
# assert both that it exists and that it names this version.
check_latest_json() {
  echo "[2/5] latest.json"
  local assets tmp got
  if ! assets="$(gh release view "$TAG" --json assets 2>/dev/null)"; then
    note_fail "cannot read assets for ${TAG}"
    return
  fi
  if [ "$(printf '%s' "$assets" | jq -r '[.assets[] | select(.name == "latest.json")] | length')" != "1" ]; then
    note_fail "latest.json is NOT among the ${TAG} release assets — every MCP server will silently believe it is up to date"
    return
  fi
  note_pass "latest.json is present in the ${TAG} assets"

  tmp="$(mktemp)"
  if ! gh release download "$TAG" --pattern latest.json --output "$tmp" --clobber 2>/dev/null; then
    note_fail "latest.json could not be downloaded"
    rm -f "$tmp"
    return
  fi
  got="$(jq -r '.version // empty' "$tmp" 2>/dev/null)"
  rm -f "$tmp"
  # tauri writes the version bare; tolerate a leading v.
  if [ "${got#v}" = "$VERSION" ]; then
    note_pass "latest.json names ${VERSION}"
  else
    note_fail "latest.json names '${got:-<unparseable>}', expected ${VERSION}"
  fi
}

# ── 3. the desktop artifacts really reached the release ────────────
#
# #461 one surface over. That incident was three releases publishing with no
# aarch64 wheel, silently; check 4 exists because of it. The identical hole was
# open for desktop until now: this script reported "verified" for 0.20.1 while
# ten of seventeen assets — every Windows installer, every Linux package, and
# their signatures — did not yet exist (#647). It could not have said otherwise,
# and it cannot tell "still building" from "the Windows job produced nothing".
#
# The desktop bundles are built on TAG PUSH, i.e. after the tag exists, which is
# exactly the window where "don't release" has stopped being an option. That is
# what post-release verification is for.
#
# latest.json is used as the source of truth rather than a hardcoded filename
# list, because it is precisely what the updater will fetch: each platform entry
# carries the URL a client resolves and the signature it checks. A list of globs
# would drift from the build matrix; this cannot, by construction.
#
# Three things are asserted, because they fail differently:
#
#   a) the required platform keys are PRESENT. A referenced-URLs-only check
#      passes vacuously when a whole OS is missing from the file — if the Linux
#      job dies, latest.json simply has no linux entry and every Linux client
#      silently stops seeing updates. There is nothing to 404 on.
#   b) every referenced URL points at THIS tag and names an asset that exists.
#      A 404 here is #571's nastiest shape: the updater fails and the user is
#      told nothing.
#   c) every entry carries a non-empty signature. The updater refuses unsigned
#      payloads, so a blank signature bricks updates as surely as a missing file.
#
# Only the four base keys are required. Tauri also emits -app/-nsis/-msi/
# -appimage/-deb/-rpm variants; requiring those would couple this script to
# bundle-format choices, while the base keys are what guarantee every OS is
# covered.
REQUIRED_PLATFORMS=(darwin-x86_64 darwin-aarch64 windows-x86_64 linux-x86_64)

# Assets a human downloads from the release page but the updater never
# references, so (b) cannot see them. A macOS user arriving at a release with no
# .dmg has nothing to install.
EXPECTED_DIRECT=('HYRR_*_x64.dmg' 'HYRR_*_aarch64.dmg')

check_desktop_assets() {
  echo "[3/5] desktop artifacts"
  local assets names tmp glob n

  if ! assets="$(gh release view "$TAG" --json assets 2>/dev/null)"; then
    note_fail "cannot read assets for ${TAG}"
    return
  fi
  names="$(printf '%s' "$assets" | jq -r '.assets[].name')"

  tmp="$(mktemp)"
  if ! gh release download "$TAG" --pattern latest.json --output "$tmp" --clobber 2>/dev/null; then
    note_fail "latest.json could not be downloaded — cannot verify desktop artifacts"
    rm -f "$tmp"
    return
  fi

  # (a) every OS is represented.
  local plat
  for plat in "${REQUIRED_PLATFORMS[@]}"; do
    if [ "$(jq -r --arg p "$plat" '.platforms | has($p)' "$tmp" 2>/dev/null)" = "true" ]; then
      note_pass "latest.json covers ${plat}"
    else
      note_fail "latest.json has NO ${plat} entry — clients on that platform silently stop seeing updates, with nothing to 404 on"
    fi
  done

  # (b) + (c) walk every entry once.
  local entries key url sig base missing_url=0 bad_sig=0 wrong_tag=0
  entries="$(jq -r '.platforms | to_entries[] | "\(.key)\t\(.value.url // "")\t\(.value.signature // "")"' "$tmp" 2>/dev/null)"
  if [ -z "$entries" ]; then
    note_fail "latest.json has no platforms block at all"
    rm -f "$tmp"
    return
  fi

  while IFS=$'\t' read -r key url sig; do
    [ -n "$key" ] || continue
    base="${url##*/}"

    # A latest.json carried over from another release resolves fine but ships
    # the WRONG build — and #516 (tauri-action v1) changes this URL form, so
    # this is live surface, not a hypothetical.
    case "$url" in
      */releases/download/"$TAG"/*) : ;;
      *) note_fail "latest.json ${key} points outside ${TAG}: ${url}"; wrong_tag=$((wrong_tag + 1)) ;;
    esac

    if [ -z "$base" ] || ! printf '%s\n' "$names" | grep -Fxq "$base"; then
      note_fail "latest.json ${key} references '${base:-<empty>}', which is not a ${TAG} asset — the updater will 404 and tell the user nothing"
      missing_url=$((missing_url + 1))
    fi

    if [ -z "$sig" ]; then
      note_fail "latest.json ${key} has an empty signature — the updater refuses unsigned payloads"
      bad_sig=$((bad_sig + 1))
    fi
  done <<< "$entries"

  if [ "$missing_url" -eq 0 ] && [ "$bad_sig" -eq 0 ] && [ "$wrong_tag" -eq 0 ]; then
    note_pass "all $(printf '%s\n' "$entries" | grep -c .) latest.json entries resolve to signed ${TAG} assets"
  fi
  rm -f "$tmp"

  # (d) direct-download bundles.
  for glob in "${EXPECTED_DIRECT[@]}"; do
    n=0
    while IFS= read -r f; do
      # shellcheck disable=SC2254  # $glob is intentionally a pattern
      case "$f" in $glob) n=$((n + 1)) ;; esac
    done <<< "$names"
    if [ "$n" -ge 1 ]; then
      note_pass "direct download ${glob}"
    else
      note_fail "no ${TAG} asset matches '${glob}' — nothing for that platform to download from the release page"
    fi
  done
}

# ── 4. the wheels really reached PyPI ──────────────────────────────
# Post-upload, which is what verify-artifacts structurally cannot check: it
# looks at what was built, and a publish step can still upload a subset.
check_pypi() {
  echo "[4/5] PyPI artifacts for ${PKG} ${VERSION}"
  local json files glob n
  json="$(curl -sS -m 30 "https://pypi.org/pypi/${PKG}/${VERSION}/json" 2>/dev/null)"
  if [ -z "$json" ] || [ "$(printf '%s' "$json" | jq -r 'has("urls")' 2>/dev/null)" != "true" ]; then
    note_fail "${PKG} ${VERSION} is not resolvable on PyPI"
    return
  fi
  files="$(printf '%s' "$json" | jq -r '.urls[].filename')"

  for glob in "${EXPECTED_WHEELS[@]}"; do
    n=0
    while IFS= read -r f; do
      # shellcheck disable=SC2254  # $glob is intentionally a pattern
      case "$f" in $glob) n=$((n + 1)) ;; esac
    done <<< "$files"
    if [ "$n" -eq 1 ]; then
      note_pass "wheel ${glob}"
    else
      note_fail "expected exactly 1 wheel matching '${glob}' on PyPI, found ${n}"
    fi
  done

  n="$(printf '%s\n' "$files" | grep -c '\.tar\.gz$')"
  if [ "$n" -eq 1 ]; then note_pass "sdist"; else note_fail "expected 1 sdist on PyPI, found ${n}"; fi

  n="$(printf '%s\n' "$files" | grep -c '\.whl$')"
  if [ "$n" -ne "${#EXPECTED_WHEELS[@]}" ]; then
    note_fail "expected ${#EXPECTED_WHEELS[@]} wheels on PyPI, found ${n} (an unexpected extra wheel is as wrong as a missing one)"
  fi
}

# ── 4. release-notes.json ──────────────────────────────────────────
# Complements #572's release-PR gate. That one runs BEFORE the merge; this
# confirms what actually shipped carries a classified note, without which an
# agent asking "would my previous answer have been wrong?" gets silence.
check_release_notes() {
  echo "[5/5] release-notes.json"
  if [ ! -x scripts/check-release-notes.sh ]; then
    note_fail "scripts/check-release-notes.sh is missing"
    return
  fi
  if scripts/check-release-notes.sh "$VERSION" >/dev/null 2>&1; then
    note_pass "release-notes.json has a valid entry for ${VERSION}"
  else
    note_fail "release-notes.json has no valid entry for ${VERSION}"
    scripts/check-release-notes.sh "$VERSION" 2>&1 | sed 's/^/      /' >&2 || true
  fi
}

run_all() {
  FAILURES=()
  check_tags
  check_latest_json
  check_desktop_assets
  check_pypi
  check_release_notes
}

echo "=== Verifying published release ${VERSION} ==="
deadline=$((SECONDS + WAIT))
while :; do
  run_all
  [ "${#FAILURES[@]}" -eq 0 ] && break
  if [ "$SECONDS" -ge "$deadline" ]; then break; fi
  remaining=$((deadline - SECONDS))
  echo "--- ${#FAILURES[@]} check(s) failing; retrying in 60s (${remaining}s left) ---"
  sleep 60
done

echo
if [ "${#FAILURES[@]}" -eq 0 ]; then
  echo "=== Release ${VERSION} verified: tags, latest.json, desktop artifacts, PyPI artifacts, release notes ==="
  exit 0
fi

echo "=== Release ${VERSION} FAILED verification (${#FAILURES[@]}) ===" >&2
for f in "${FAILURES[@]}"; do echo "  - $f" >&2; done
exit 1

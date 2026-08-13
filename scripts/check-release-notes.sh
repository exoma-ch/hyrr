#!/usr/bin/env bash
# check-release-notes.sh — verify release-notes.json (#572) is in sync with the
# release-please version being cut.
#
# Fails if:
#   1. release-notes.json is missing or malformed JSON.
#   2. The release version recorded in .release-please-manifest.json has no
#      matching entry in release-notes.json.
#   3. Releases in the artifact are not ordered newest-first.
#   4. A release entry is missing any required field (version / date /
#      data_version / entries).
#   5. Any entry uses an unknown `impact` class.
#   6. A physics_affecting entry is missing `guidance` or `affected`, or has
#      an empty summary — those are load-bearing per #572.
#
# The point of this gate: the impact classification is a human-reviewed
# release-time artifact. If a release ships without a matching classified
# note, an agent asking "would my previous answer have been wrong?" gets
# silence — the exact failure mode the artifact exists to prevent.
#
# Usage:
#   scripts/check-release-notes.sh                 — check against manifest
#   scripts/check-release-notes.sh <version>       — check a specific version
#   scripts/check-release-notes.sh --strict-gate   — also require the artifact
#                                                    version to match manifest
#                                                    exactly (used in release
#                                                    PR CI). Default: warn if
#                                                    manifest is ahead of
#                                                    artifact (works locally
#                                                    when hacking on main).
#
# Run automatically by:
#   - prek (local git hook, non-strict)
#   - release-please.yml `sync-release-lockfiles` job (strict)

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ARTIFACT="$ROOT/release-notes.json"
MANIFEST="$ROOT/.release-please-manifest.json"
# The nuclear data version actually shipped, derived from the pinned submodule
# rather than trusted from the hand-written artifact (#606).
CATALOG="$ROOT/nucl-parquet/data/catalog.json"

strict=0
target_version=""
for arg in "$@"; do
  case "$arg" in
    --strict-gate) strict=1 ;;
    -h|--help)
      sed -n '2,30p' "$0"
      exit 0
      ;;
    *) target_version="$arg" ;;
  esac
done

if [ ! -f "$ARTIFACT" ]; then
  echo "::error::release-notes.json missing at $ARTIFACT — #572 requires this file to ship with every release." >&2
  exit 1
fi

if ! command -v python3 &>/dev/null; then
  echo "::warning::python3 not found — skipping release-notes.json validation" >&2
  exit 0
fi

# Delegate the structural checks to python3 — the schema is small enough that
# hand-coding the shape check is clearer than pulling in jsonschema.
python3 - "$ARTIFACT" "$MANIFEST" "$target_version" "$strict" "$CATALOG" <<'PY'
import json
import sys

artifact_path, manifest_path, target_version, strict_str, catalog_path = sys.argv[1:6]
strict = strict_str == "1"

VALID_IMPACTS = {
    "physics_affecting",
    "silent_failure_fixed",
    "data_update",
    "api_change",
    "ux",
    "internal",
}
REQUIRED_ENTRY_FIELDS = ("summary", "impact", "silent", "affected", "guidance", "refs")
REQUIRED_RELEASE_FIELDS = ("version", "date", "data_version", "entries")

errors = []

# --- Parse ---
try:
    with open(artifact_path) as f:
        artifact = json.load(f)
except json.JSONDecodeError as e:
    print(f"::error::release-notes.json is malformed JSON: {e}", file=sys.stderr)
    sys.exit(1)

if "releases" not in artifact or not isinstance(artifact["releases"], list):
    print("::error::release-notes.json must have a top-level `releases` array", file=sys.stderr)
    sys.exit(1)

releases = artifact["releases"]
if not releases:
    errors.append("release-notes.json.releases is empty — nothing to classify")

# --- Structural checks ---
def version_tuple(v):
    return tuple(int(x) if x.isdigit() else 0 for x in v.split("-")[0].split("."))

seen_versions = set()
for i, release in enumerate(releases):
    ctx = f"releases[{i}]"
    for field in REQUIRED_RELEASE_FIELDS:
        if field not in release:
            errors.append(f"{ctx} missing required field `{field}`")
    version = release.get("version", "")
    if version in seen_versions:
        errors.append(f"{ctx} duplicate version `{version}`")
    seen_versions.add(version)
    for j, entry in enumerate(release.get("entries", [])):
        ectx = f"{ctx}.entries[{j}]"
        for field in REQUIRED_ENTRY_FIELDS:
            if field not in entry:
                errors.append(f"{ectx} missing required field `{field}`")
        impact = entry.get("impact")
        if impact not in VALID_IMPACTS:
            errors.append(f"{ectx} unknown impact `{impact}` (must be one of {sorted(VALID_IMPACTS)})")
        summary = entry.get("summary", "").strip()
        if not summary:
            errors.append(f"{ectx} `summary` must be non-empty")
        if impact in ("physics_affecting", "silent_failure_fixed"):
            if not entry.get("guidance", "").strip():
                errors.append(
                    f"{ectx} impact=`{impact}` must carry `guidance` — a user needs to know what to re-run"
                )
            if not entry.get("affected"):
                errors.append(
                    f"{ectx} impact=`{impact}` must list `affected` tool names — [\"*\"] is legal when everything is affected"
                )
            if not entry.get("refs"):
                errors.append(
                    f"{ectx} impact=`{impact}` must cite `refs` — an agent should be able to cross-reference an issue"
                )

# --- Ordering: newest first ---
for a, b in zip(releases, releases[1:]):
    if version_tuple(a["version"]) <= version_tuple(b["version"]):
        errors.append(
            f"releases must be newest-first: {a['version']} should be > {b['version']}"
        )

# --- Manifest sync ---
manifest_version = None
try:
    with open(manifest_path) as f:
        manifest = json.load(f)
    manifest_version = manifest.get(".") or next(iter(manifest.values()), None)
except FileNotFoundError:
    print(f"::warning::{manifest_path} not found — skipping manifest sync check", file=sys.stderr)

check_version = target_version or manifest_version
if check_version:
    versions = [r.get("version") for r in releases]
    if check_version not in versions:
        msg = (
            f"release-notes.json is missing an entry for version `{check_version}` — "
            "add a classified block to release-notes.json BEFORE merging the release PR. "
            "See #572 for why this gate exists: an agent asking `would my previous answer "
            "have been wrong?` needs a human-reviewed impact class, not silence."
        )
        if strict or target_version:
            errors.append(msg)
        else:
            # Locally on main we're often between releases; a manifest ahead
            # of the artifact is normal there. Warn but don't block.
            print(f"::warning::{msg}", file=sys.stderr)

# --- data_version matches the data that actually ships (#606) ---
#
# `data_version` is a DERIVABLE fact — the pinned nucl-parquet submodule's
# catalog.json says which data tree the build stamps in (core/build.rs ->
# HYRR_DATA_VERSION). Hand-entering it and never checking is how 0.19.0 came
# to claim `2026.7.2` while actually shipping `2026.8.1`, two data releases
# newer. That field is the one place data-version history reaches an agent —
# get_changelog exposes it precisely so a caller "can tell a data-only change
# apart from a code change" (#572) — so a wrong value answers the physics
# question wrongly.
#
# Only the entry being cut is checked: historical entries would need their own
# submodule commit checked out to re-derive, and they are already published.
if check_version:
    try:
        with open(catalog_path) as f:
            shipped_data_version = json.load(f).get("data_version")
    except (FileNotFoundError, json.JSONDecodeError):
        # The submodule is not always present (some CI jobs check out without
        # it). Skip loudly rather than silently: a guard that quietly passes is
        # exactly what let this through.
        print(
            f"::warning::{catalog_path} unavailable — could not verify "
            f"`data_version` for `{check_version}` against the pinned submodule",
            file=sys.stderr,
        )
        shipped_data_version = None

    if shipped_data_version:
        entry = next((r for r in releases if r.get("version") == check_version), None)
        if entry is not None:
            claimed = entry.get("data_version")
            if claimed != shipped_data_version:
                errors.append(
                    f"releases[{check_version}].data_version is `{claimed}` but the "
                    f"pinned nucl-parquet submodule ships `{shipped_data_version}`. "
                    "This value is derivable — read it from nucl-parquet/data/catalog.json "
                    "rather than typing it. An agent uses it to tell a data change "
                    "apart from a code change (#572/#606)."
                )

if errors:
    for e in errors:
        print(f"::error::{e}", file=sys.stderr)
    sys.exit(1)

print(
    f"release-notes.json OK ({len(releases)} releases, checked against version "
    f"`{check_version or '<none>'}`)."
)
PY

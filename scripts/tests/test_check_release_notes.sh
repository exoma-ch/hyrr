#!/usr/bin/env bash
# Smoke test for scripts/check-release-notes.sh (#572 release gate).
#
# The gate is the load-bearing enforcement: without it a release could ship
# with no classified notes and an agent would have no way to answer
# "would my previous answer have been wrong?". Exercising the failure paths
# here guards against a regression that would let a bad artifact through.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
GATE="$SCRIPT_DIR/check-release-notes.sh"

failed=0
ran=0
report() {
  local name="$1" status="$2"
  ran=$((ran + 1))
  if [ "$status" = "pass" ]; then
    echo "PASS: $name"
  else
    echo "FAIL: $name" >&2
    failed=1
  fi
}

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

setup_repo() {
  # Copy just the artifact + manifest into a fake root the gate can walk.
  local dst="$work/$1"
  rm -rf "$dst"
  mkdir -p "$dst/scripts"
  cp "$GATE" "$dst/scripts/"
  cp "$ROOT/.release-please-manifest.json" "$dst/"
  cp "$ROOT/release-notes.json" "$dst/"
  # The pinned submodule's catalog.json — the SSoT the gate checks
  # data_version against (#606). Optional second arg overrides the value; pass
  # "__NONE__" to omit the file entirely and exercise the skip path.
  local data_version="${2:-}"
  if [ -z "$data_version" ]; then
    data_version="$(python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['releases'][0]['data_version'])" "$dst/release-notes.json")"
  fi
  if [ "$data_version" != "__NONE__" ]; then
    mkdir -p "$dst/nucl-parquet/data"
    printf '{"data_version": "%s"}\n' "$data_version" > "$dst/nucl-parquet/data/catalog.json"
  fi
  echo "$dst"
}

# 1. Real artifact — must pass.
d="$(setup_repo happy)"
if (cd "$d" && bash scripts/check-release-notes.sh >/dev/null 2>&1); then
  report "real artifact passes" pass
else
  report "real artifact passes" fail
fi

# 2. Missing artifact — must fail.
d="$(setup_repo missing)"
rm "$d/release-notes.json"
if ! (cd "$d" && bash scripts/check-release-notes.sh >/dev/null 2>&1); then
  report "missing artifact fails" pass
else
  report "missing artifact fails" fail
fi

# 3. Malformed JSON — must fail.
d="$(setup_repo malformed)"
echo "{not json" > "$d/release-notes.json"
if ! (cd "$d" && bash scripts/check-release-notes.sh >/dev/null 2>&1); then
  report "malformed JSON fails" pass
else
  report "malformed JSON fails" fail
fi

# 4. Missing entry for the version being cut (strict) — must fail.
d="$(setup_repo missing_entry)"
if ! (cd "$d" && bash scripts/check-release-notes.sh --strict-gate 9.9.9 >/dev/null 2>&1); then
  report "missing version fails (strict)" pass
else
  report "missing version fails (strict)" fail
fi

# 5. Missing entry for the manifest version but --strict-gate not set — must
#    warn, not fail (default local mode when hacking on main).
d="$(setup_repo missing_entry_nonstrict)"
python3 -c "import json; m=json.load(open('$d/.release-please-manifest.json')); m['.']='9.9.9'; json.dump(m, open('$d/.release-please-manifest.json','w'))"
if (cd "$d" && bash scripts/check-release-notes.sh >/dev/null 2>&1); then
  report "manifest ahead of artifact only warns" pass
else
  report "manifest ahead of artifact only warns" fail
fi

# 6. Unknown impact class — must fail.
d="$(setup_repo bad_impact)"
python3 - "$d/release-notes.json" <<'PY'
import json, sys
p = sys.argv[1]
notes = json.load(open(p))
notes["releases"][0]["entries"][0]["impact"] = "definitely-not-a-class"
json.dump(notes, open(p, "w"))
PY
if ! (cd "$d" && bash scripts/check-release-notes.sh >/dev/null 2>&1); then
  report "unknown impact class fails" pass
else
  report "unknown impact class fails" fail
fi

# 7. physics_affecting entry with empty guidance — must fail (load-bearing per #572).
d="$(setup_repo empty_guidance)"
python3 - "$d/release-notes.json" <<'PY'
import json, sys
p = sys.argv[1]
notes = json.load(open(p))
# Find a physics_affecting entry.
for r in notes["releases"]:
    for e in r["entries"]:
        if e["impact"] == "physics_affecting":
            e["guidance"] = ""
            break
    else:
        continue
    break
json.dump(notes, open(p, "w"))
PY
if ! (cd "$d" && bash scripts/check-release-notes.sh >/dev/null 2>&1); then
  report "physics_affecting w/o guidance fails" pass
else
  report "physics_affecting w/o guidance fails" fail
fi

# 8. Out-of-order releases — must fail.
d="$(setup_repo bad_order)"
python3 - "$d/release-notes.json" <<'PY'
import json, sys
p = sys.argv[1]
notes = json.load(open(p))
notes["releases"].reverse()
json.dump(notes, open(p, "w"))
PY
if ! (cd "$d" && bash scripts/check-release-notes.sh >/dev/null 2>&1); then
  report "wrong ordering fails" pass
else
  report "wrong ordering fails" fail
fi

# --- data_version vs the pinned submodule (#606) ---------------------------
# 0.19.0 shipped data 2026.8.1 while the artifact claimed 2026.7.2 — two data
# releases off, in the one field an agent uses to tell a data change apart from
# a code change. The gate validated the field's PRESENCE and nothing else.

# Matching catalog — must pass.
d="$(setup_repo dv_match)"
if (cd "$d" && bash scripts/check-release-notes.sh >/dev/null 2>&1); then
  report "data_version matching the submodule passes" pass
else
  report "data_version matching the submodule passes" fail
fi

# Catalog says something else — must fail, and name both values.
d="$(setup_repo dv_drift "1999.1.1")"
# `|| rc=$?` on the assignment: this file runs under `set -e`, so capturing the
# output of a command expected to FAIL would otherwise abort the whole suite
# before the assertion ran.
out="$(cd "$d" && bash scripts/check-release-notes.sh 2>&1)" && rc=0 || rc=$?
if [ "$rc" -ne 0 ] && [ "${out#*1999.1.1}" != "$out" ]; then
  report "data_version drifting from the submodule fails" pass
else
  report "data_version drifting from the submodule fails (rc=$rc, out: $out)" fail
fi

# No catalog (submodule not checked out) — must WARN and still pass, but the
# warning has to be visible. A guard that skips silently is what let #606
# through in the first place.
d="$(setup_repo dv_nocatalog "__NONE__")"
out="$(cd "$d" && bash scripts/check-release-notes.sh 2>&1)" && rc=0 || rc=$?
if [ "$rc" -eq 0 ] && [ "${out#*could not verify}" != "$out" ]; then
  report "absent submodule warns loudly and does not block" pass
else
  report "absent submodule warns loudly and does not block (rc=$rc, out: $out)" fail
fi

if [ "$failed" -eq 0 ]; then
  echo "All ${ran} gate-script tests passed."
else
  exit 1
fi

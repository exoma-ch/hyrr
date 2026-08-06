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
report() {
  local name="$1" status="$2"
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

if [ "$failed" -eq 0 ]; then
  echo "All 8 gate-script tests passed."
else
  exit 1
fi

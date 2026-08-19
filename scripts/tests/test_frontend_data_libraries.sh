#!/usr/bin/env bash
# Guard: the frontend data-library list is honoured everywhere.
#
# The bug this prevents (#651, epic #649): every caller of
# copy-frontend-data.sh used to spell out its own library list, and they
# disagreed. Production and CI shipped three libraries; `dev.sh` and
# `just data` shipped one. The four neutron-activation presets read from
# `neutron-xs/`, which therefore did not exist in a dev build — so they
# silently produced an empty results table, and "I'm feeling lucky" lands on
# one of them roughly 40% of the time.
#
# Two invariants:
#   1. No caller passes an explicit library list; they all read the SSoT.
#   2. Any workflow that runs the copy script sparse-checks-out every library
#      in that list — otherwise the (now hard-failing) copy step breaks the build.
#
#   scripts/tests/test_frontend_data_libraries.sh   (also: just test-scripts)

set -uo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
LIST="$ROOT/scripts/frontend-data-libraries.txt"

failed=0
ran=0
report() {
  ran=$((ran + 1))
  if [ "$2" = "pass" ]; then echo "PASS: $1"; else echo "FAIL: $1" >&2; failed=1; fi
}

[ -f "$LIST" ] || { echo "FAIL: missing $LIST" >&2; exit 1; }

# Parse the SSoT list into library names (strip the :subdir suffix).
libs=()
while IFS= read -r line; do
  line="${line%%#*}"
  line="$(echo "$line" | tr -d '[:space:]')"
  [ -n "$line" ] || continue
  libs+=("${line%%:*}")
done < "$LIST"

if [ "${#libs[@]}" -eq 0 ]; then
  report "SSoT list is non-empty" "fail"
  exit 1
fi
report "parsed ${#libs[@]} libraries from frontend-data-libraries.txt" "pass"

# ── 1. No caller passes an explicit library list ─────────────────────────────
#
# Any argument after <data-dir> <dest-dir> overrides the SSoT. That is the
# divergence this whole change removes, so it must not creep back.
callers="$(grep -rln "copy-frontend-data\.sh" \
  "$ROOT/scripts" "$ROOT/.github/workflows" "$ROOT/justfile.project" 2>/dev/null \
  | grep -v "copy-frontend-data.sh$" \
  | grep -v "frontend-data-libraries.txt$" \
  | grep -v "/tests/" || true)"

overrides=""
while IFS= read -r f; do
  [ -n "$f" ] || continue
  # An invocation with a third positional argument overrides the SSoT.
  # `grep -v '^\s*#'` so prose in a header comment doesn't count as a call.
  if grep -vE '^\s*#' "$f" \
     | grep -qE "copy-frontend-data\.sh +[^ ]+ +[^ ]+ +[^ \\\\]"; then
    overrides="$overrides ${f#"$ROOT"/}"
  fi
done <<< "$callers"

if [ -n "$overrides" ]; then
  report "no caller overrides the SSoT library list" "fail"
  echo "  these pass explicit libraries:$overrides" >&2
  echo "  drop the arguments so they read scripts/frontend-data-libraries.txt." >&2
else
  report "no caller overrides the SSoT library list" "pass"
fi

# ── 2. Workflows that copy also sparse-check-out every library ───────────────
#
# copy-frontend-data.sh now hard-fails on a library that isn't on disk (it used
# to skip silently), so a workflow that copies without checking the data out is
# a broken build rather than a quiet mis-ship.
missing_report=""
for wf in "$ROOT"/.github/workflows/*.yml; do
  grep -q "copy-frontend-data\.sh" "$wf" || continue
  grep -q "sparse-checkout" "$wf" || continue   # full checkout: nothing to assert
  for lib in "${libs[@]}"; do
    if ! grep -q "data/$lib/xs" "$wf"; then
      missing_report="$missing_report\n    $(basename "$wf"): missing data/$lib/xs"
    fi
  done
done

if [ -n "$missing_report" ]; then
  report "every copying workflow sparse-checks-out every library" "fail"
  # shellcheck disable=SC2059
  printf "$missing_report\n" >&2
  echo "  add the path to that workflow's 'git sparse-checkout set' call." >&2
else
  report "every copying workflow sparse-checks-out every library" "pass"
fi

echo
if [ "$failed" -eq 0 ]; then
  echo "All $ran check(s) passed."
else
  echo "Some checks FAILED." >&2
fi
exit "$failed"

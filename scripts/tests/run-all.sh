#!/usr/bin/env bash
# Run every shell-script test under scripts/tests/.
#
#   scripts/tests/run-all.sh        (also: just test-scripts)
#
# These tests existed before this runner did and were wired into nothing —
# neither CI nor prek ran them, so test_check_release_notes.sh had been
# guarding the release gate purely on the honour system. A test that never
# runs is documentation, not a guard (#401).
#
# Every test is run even after one fails, so a single failure does not hide
# the rest.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# GitHub Actions renders ::group:: as a collapsible section; elsewhere it is
# just a harmless line.
in_ci() { [ -n "${GITHUB_ACTIONS:-}" ]; }

rc=0
ran=0
for test in "$HERE"/test_*.sh; do
  [ -f "$test" ] || continue
  name="$(basename "$test")"
  ran=$((ran + 1))
  in_ci && echo "::group::${name}"
  echo "── ${name} ──"
  if bash "$test"; then
    status="ok"
  else
    status="FAILED"
    rc=1
  fi
  in_ci && echo "::endgroup::"
  echo "── ${name}: ${status}"
  echo
  [ "$status" = "FAILED" ] && in_ci && echo "::error::${name} failed"
done

if [ "$ran" -eq 0 ]; then
  echo "ERROR: no tests found in ${HERE}" >&2
  exit 1
fi

if [ "$rc" -eq 0 ]; then
  echo "All ${ran} script test file(s) passed."
else
  echo "Some script tests FAILED." >&2
fi
exit "$rc"

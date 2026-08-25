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
# Three invariants:
#   1. No caller passes an explicit library list; they all read the SSoT.
#   2. Any workflow that runs the copy script sparse-checks-out every library
#      in that list — otherwise the (now hard-failing) copy step breaks the build.
#   3. The bash and TypeScript readers agree on that list, byte for byte, under
#      LF *and* CRLF. Invariant 3 is #677: the two readers disagreed only on a
#      CRLF checkout, so Windows CI rejected a bundle that was complete and
#      v0.21.0 shipped with no `.exe`/`.msi` and no `windows-x86_64` entry in
#      `latest.json`. Every other platform stayed green throughout.
#
#   scripts/tests/test_frontend_data_libraries.sh   (also: just test-scripts)

set -uo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
LIST="$ROOT/scripts/frontend-data-libraries.txt"

failed=0
ran=0
skipped=0
# "skip" is distinct from "pass" on purpose. A check that quietly reports PASS
# when it did not actually run is worse than no check: it reads as coverage in
# the log while asserting nothing. Skips are counted and reprinted in the
# summary so a runner change that disables one is visible.
report() {
  ran=$((ran + 1))
  case "$2" in
    pass) echo "PASS: $1" ;;
    skip) echo "SKIP: $1"; skipped=$((skipped + 1)) ;;
    *)    echo "FAIL: $1" >&2; failed=1 ;;
  esac
}

[ -f "$LIST" ] || { echo "FAIL: missing $LIST" >&2; exit 1; }

# Parse via the shared reader rather than a fourth copy of the loop — the
# copy-pasted parsers are what drifted in the first place (#677).
# shellcheck source=scripts/frontend-data-libraries.sh
. "$ROOT/scripts/frontend-data-libraries.sh"

if ! read_frontend_data_libraries "$LIST"; then
  report "SSoT list parses" "fail"
  exit 1
fi

libs=()
for spec in "${LIBRARY_SPECS[@]}"; do
  libs+=("$(frontend_data_library "$spec")")
done
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

# ── 3. The bash and TypeScript readers agree, byte for byte ──────────────────
#
# scripts/copy-frontend-data.sh writes the bundle; frontend/vite.config.ts
# asserts the bundle matches. If they parse the SSoT differently the build
# either rejects a correct bundle or waves through an incomplete one. They did
# differ — but only on CRLF, so it surfaced solely on GitHub's Windows runners,
# as a data error naming subdirectories after this file's own comment prose
# (#677).
#
# Comparing only the real list at rest is not enough: it is LF and BOM-free, so
# it exercises exactly the one input that never broke. The divergences worth
# catching are all invisible-byte cases, so the fixtures below are written as
# explicit bytes and both readers are compared on *failure* too — "both reject
# it" is as much a parity requirement as "both accept it", and a reader that
# accepts what the other rejects is how a bad bundle gets built in the first
# place.
check_parser_parity() {
  command -v node >/dev/null 2>&1 || {
    report "bash and TS readers agree (SKIPPED: no node)" "skip"
    return
  }
  # Importing the .ts directly needs Node's type stripping: opt-in behind
  # --experimental-strip-types on 22.6–22.17, on by default from 22.18/23, and
  # the flag is gone again on some newer builds. Probe by importing the actual
  # module rather than sniffing versions or flags, and take the first
  # invocation that works. Skip only if none does — a missing runtime
  # capability is not parser drift, and a check that cries wolf gets ignored.
  local -a NODE_TS=()
  local candidate
  for candidate in "" "--experimental-strip-types"; do
    # shellcheck disable=SC2086
    if node $candidate -e 'await import(process.argv[1])' \
         "$ROOT/frontend/scripts/frontend-data-libraries.ts" >/dev/null 2>&1; then
      [ -n "$candidate" ] && NODE_TS=("$candidate")
      break
    fi
    if [ "$candidate" = "--experimental-strip-types" ]; then
      report "bash and TS readers agree (SKIPPED: node cannot import TypeScript)" "skip"
      return
    fi
  done

  local tmp mismatch=0 name file b t
  tmp="$(mktemp -d)"

  # The real list, plus the same list rewritten CRLF — the #677 input.
  cp "$LIST" "$tmp/ssot-lf.txt"
  sed 's/$/\r/' "$LIST" > "$tmp/ssot-crlf.txt"

  # Byte-exact fixtures. printf, not a heredoc, so the CR and BOM bytes are
  # unambiguous and survive any future reformatting of this file.
  printf 'tendl\nendfb-8.0:neutron-xs\n'            > "$tmp/lf.txt"
  printf 'tendl\r\nendfb-8.0:neutron-xs\r\n'        > "$tmp/crlf.txt"
  printf 'tendl\rendfb-8.0:neutron-xs\r'            > "$tmp/cr-only.txt"
  printf '\xef\xbb\xbftendl\n'                      > "$tmp/bom.txt"
  printf '\xef\xbb\xbf# hdr: prose\ntendl\n'        > "$tmp/bom-comment.txt"
  printf '\xef\xbb\xbftendl\r\n'                    > "$tmp/bom-crlf.txt"
  printf 'tendl:\n'                                 > "$tmp/trailing-colon.txt"
  printf 'tendl#c\n'                                > "$tmp/hash-no-space.txt"
  printf '\ttendl\t\n'                              > "$tmp/tabs.txt"
  printf 'tendl'                                    > "$tmp/no-final-newline.txt"
  printf 'ten\rdl\n'                                > "$tmp/interior-cr.txt"
  printf '   \n\t\n'                                > "$tmp/whitespace-only.txt"
  printf 'Format: one entry per line\n'             > "$tmp/prose-with-colon.txt"
  printf 'tendl:../../etc\n'                        > "$tmp/traversal.txt"

  for file in "$tmp"/*.txt; do
    name="$(basename "$file" .txt)"

    if read_frontend_data_libraries "$file" >/dev/null 2>&1; then
      b=""
      for spec in "${LIBRARY_SPECS[@]}"; do
        b="$b$(frontend_data_library "$spec"):$(frontend_data_subdir "$spec") "
      done
    else
      b="REJECTED"
    fi

    # SC2016: the `${s.library}` below is a JS template literal evaluated by
    # node, not a shell expansion — single quotes are exactly right here.
    # shellcheck disable=SC2016
    t="$(node ${NODE_TS+"${NODE_TS[@]}"} -e '
      const { parseLibraryList } = await import(process.argv[1]);
      const { readFileSync } = await import("node:fs");
      try {
        const out = parseLibraryList(readFileSync(process.argv[2], "utf8"))
          .map((s) => `${s.library}:${s.subdir} `).join("");
        process.stdout.write(out);
      } catch { process.stdout.write("REJECTED"); }
    ' "$ROOT/frontend/scripts/frontend-data-libraries.ts" "$file" 2>/dev/null)"

    if [ "$b" != "$t" ]; then
      report "bash and TS readers agree on '$name'" "fail"
      echo "    bash: $b" >&2
      echo "    ts:   $t" >&2
      mismatch=1
    fi
  done

  rm -rf "$tmp"

  if [ "$mismatch" -eq 0 ]; then
    report "bash and TS readers agree on all 16 parser fixtures" "pass"
  fi
}
check_parser_parity

echo
if [ "$failed" -eq 0 ]; then
  if [ "$skipped" -gt 0 ]; then
    echo "$((ran - skipped))/$ran check(s) passed, $skipped SKIPPED (see SKIP lines above)."
  else
    echo "All $ran check(s) passed."
  fi
else
  echo "Some checks FAILED." >&2
fi
exit "$failed"

#!/usr/bin/env bash
# Guard: a workflow output carrying shell metacharacters is never spliced into
# a `run:` line via `${{ }}`.
#
# The trap this exists for (#649 follow-up). e2e.yml's `config` job published
#
#     echo 'pw=--project=desktop-1280 --grep @smoke|@preset'
#
# and the e2e job ran
#
#     run: npx playwright test ${{ needs.config.outputs.pw }} --reporter=...
#
# GitHub substitutes `${{ }}` **textually, before the shell parses the line**.
# So the shell saw a pipe, forked a command named `@preset`, and the step died
# with `@preset: command not found` — exit 127, before a single test ran. The
# PR gate looked like it was enforcing 18 tests and was enforcing zero.
#
# The fix is to pass the value through `env:` instead. Parameter expansion
# field-splits and glob-expands, but its *result is not re-scanned for shell
# operators*, so a `|` inside `$PW_ARGS` stays a literal argv byte.
#
# Scope is deliberately narrow. `${{ matrix.crate }}` and friends are spliced
# into `run:` in several workflows and are fine — their values are controlled
# and metacharacter-free. This checks only what can be decided statically: an
# output whose literal `echo 'name=value'` value contains a metacharacter.
#
#   scripts/tests/test_workflow_output_splice.sh   (also: just test-scripts)

set -uo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
WF_DIR="$ROOT/.github/workflows"

# Shell operators that change parsing if they survive a textual splice.
# Not `*`/`?`: those glob but do not restructure the command, and unquoted
# globbing is a separate (and here, harmless) concern.
METACHARS='[|&;<>()`]'

failed=0
checked=0

[ -d "$WF_DIR" ] || { echo "FAIL: no workflows dir at $WF_DIR" >&2; exit 1; }

for wf in "$WF_DIR"/*.yml "$WF_DIR"/*.yaml; do
  [ -f "$wf" ] || continue
  wf_name="$(basename "$wf")"

  # ── 1. Outputs whose literal value carries a metacharacter ────────────────
  # Matches `echo 'name=value'` / `echo "name=value"` — the $GITHUB_OUTPUT
  # idiom. The value is everything after the first `=` up to the closing quote.
  while IFS= read -r line; do
    # Strip to the quoted payload.
    payload="$(printf '%s' "$line" | sed -n "s/.*echo[[:space:]]*['\"]\([^'\"]*\)['\"].*/\1/p")"
    [ -n "$payload" ] || continue
    case "$payload" in *=*) ;; *) continue ;; esac

    out_name="${payload%%=*}"
    out_value="${payload#*=}"
    # Names are simple identifiers; anything else is not an output assignment.
    printf '%s' "$out_name" | grep -qE '^[A-Za-z_][A-Za-z0-9_-]*$' || continue
    printf '%s' "$out_value" | grep -qE "$METACHARS" || continue

    checked=$((checked + 1))

    # ── 2. Is it referenced inside a `run:`? ──────────────────────────────
    # A safe reference is an `env:` binding (`NAME: ${{ ... }}`) or the job's
    # own `outputs:` declaration. Anything else that mentions it alongside a
    # command is the hazard.
    while IFS=: read -r lineno ref; do
      # `env:`-style binding or job outputs declaration → safe.
      printf '%s' "$ref" | grep -qE '^[[:space:]]*[A-Za-z_][A-Za-z0-9_-]*:[[:space:]]*\$\{\{' && continue

      echo "FAIL: $wf_name:$lineno splices metacharacter-carrying output '$out_name' into a command" >&2
      echo "  value:     $out_value" >&2
      echo "  reference: $(printf '%s' "$ref" | sed 's/^[[:space:]]*//')" >&2
      echo "  GitHub substitutes before the shell parses, so those metacharacters" >&2
      echo "  become shell operators. Pass it via 'env:' and use \$VAR instead." >&2
      failed=1
    done < <(grep -nE "outputs\.${out_name}[[:space:]]*\}\}" "$wf" || true)
  done < <(grep -nE "echo[[:space:]]*['\"][A-Za-z_][A-Za-z0-9_-]*=" "$wf" || true)
done

echo
if [ "$checked" -eq 0 ]; then
  # Not a failure: it just means no workflow currently publishes such a value.
  echo "note: no metacharacter-carrying workflow outputs found to check"
elif [ "$failed" -eq 0 ]; then
  echo "PASS: $checked metacharacter-carrying output(s) all reach commands via env:"
fi
[ "$failed" -eq 0 ] && echo "All check(s) passed." || echo "Some checks FAILED." >&2
exit "$failed"

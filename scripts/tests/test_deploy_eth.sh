#!/usr/bin/env bash
# Tests for the post-deploy verification helpers in scripts/deploy-eth.sh (#401).
#
# These guard a security-relevant assertion: verify_remote is what stops a
# deploy that silently dropped the Shibboleth gate, which would publish
# license-restricted nuclear data unauthenticated. A guard that fails open is
# worse than no guard, so every failure path is exercised here — not just the
# happy path.
#
# The script is sourced with HYRR_DEPLOY_ETH_LIB=1 (skips the dispatch) into a
# fake repo root, and `remote` is stubbed so no ETH host is needed.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
DEPLOY="$SCRIPT_DIR/deploy-eth.sh"

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

GATE_LINE='ShibRequestSetting requireSession 1'

# Build a fake repo root holding just the script and a dist/version.json, run
# verify_remote in a subshell that has sourced the helpers, and capture both the
# exit code and the combined output into CASE_RC / CASE_OUT.
#
# Args: <local-version-json> <remote-version-json> <remote-htaccess>
# A literal "__ABSENT__" means that file does not exist.
CASE_RC=0
CASE_OUT=""
run_case() {
  local local_json="$1" remote_json="$2" remote_ht="$3"
  local dst="$work/case"
  rm -rf "$dst"
  mkdir -p "$dst/scripts" "$dst/frontend/dist"
  cp "$DEPLOY" "$dst/scripts/"
  [ "$local_json" = "__ABSENT__" ] || printf '%s' "$local_json" > "$dst/frontend/dist/version.json"
  printf '%s' "$remote_json" > "$dst/remote_version.json"
  printf '%s' "$remote_ht" > "$dst/remote_htaccess"

  CASE_OUT="$(
    # shellcheck source=/dev/null  # path is built at runtime
    HYRR_DEPLOY_ETH_LIB=1 source "$dst/scripts/deploy-eth.sh"
    # Stub the SSH layer: dispatch on which file the caller asked for.
    # Overrides the definition the sourced script brought in, so verify_remote
    # needs no ETH host. shellcheck cannot see that indirect call, hence both
    # "unreachable" suppressions (which code is reported varies by version).
    # shellcheck disable=SC2317,SC2329
    remote() {
      local cmd="$2"
      case "$cmd" in
        *version.json*)
          [ "$(cat "$dst/remote_version.json")" = "__ABSENT__" ] || cat "$dst/remote_version.json" ;;
        *.htaccess*)
          [ "$(cat "$dst/remote_htaccess")" = "__ABSENT__" ] || cat "$dst/remote_htaccess" ;;
      esac
    }
    verify_remote tst /docroot 2>&1
  )"
  CASE_RC=$?
}

expect_ok() { # expect_ok <name>
  if [ "$CASE_RC" -eq 0 ]; then report "$1" pass; else
    report "$1 (rc=$CASE_RC, out: $CASE_OUT)" fail
  fi
}

# Assert BOTH the exit code and the reason. Exit code alone is not enough: a
# case that fails for an unrelated reason would otherwise pass, which is
# exactly how the first draft of this file reported five false passes.
expect_refusal() { # expect_refusal <name> <message-fragment>
  if [ "$CASE_RC" -eq 7 ] && [ "${CASE_OUT#*"$2"}" != "$CASE_OUT" ]; then
    report "$1" pass
  else
    report "$1 (rc=$CASE_RC, wanted fragment '$2', out: $CASE_OUT)" fail
  fi
}

good_json='{
  "version": "0.19.0",
  "commit": "0cb27e1e33eaf02bcded3fb1b64acf67d8078762",
  "built_at": "2026-08-06T13:55:40.294Z"
}'
good_ht="AuthType shibboleth
${GATE_LINE}
Require shib-attr principalName someone@ethz.ch"

# ── json_field ─────────────────────────────────────────────────────
expect_field() { # expect_field <name> <json> <field> <expected>
  local got
  got="$(
    printf '%s' "$2" | (
      # shellcheck source=/dev/null  # path is built at runtime
      HYRR_DEPLOY_ETH_LIB=1 source "$DEPLOY"
      json_field "$3"
    )
  )"
  if [ "$got" = "$4" ]; then report "$1" pass; else report "$1 (got '$got')" fail; fi
}

expect_field "json_field reads version from pretty-printed JSON" \
  "$good_json" version "0.19.0"
expect_field "json_field reads commit" \
  "$good_json" commit "0cb27e1e33eaf02bcded3fb1b64acf67d8078762"
expect_field "json_field reads minified JSON" \
  '{"version":"0.19.0"}' version "0.19.0"
expect_field "json_field yields empty for a missing field" \
  "$good_json" nope ""
# A version.json that is really an HTML error page (or a captive-portal
# interception) must not parse into something that accidentally matches.
expect_field "json_field yields empty for non-JSON input" \
  '<html>404</html>' version ""

# ── verify_remote: happy path ──────────────────────────────────────
run_case "$good_json" "$good_json" "$good_ht"
expect_ok "verify_remote accepts a matching version and a present gate"

# ── verify_remote: the bytes did not land ──────────────────────────
run_case "$good_json" '{"version":"0.18.0","commit":"x","built_at":"y"}' "$good_ht"
expect_refusal "verify_remote rejects a version mismatch" "deployed version does not match"

run_case "$good_json" "__ABSENT__" "$good_ht"
expect_refusal "verify_remote rejects a missing remote version.json" "no readable version.json"

# ── verify_remote: the licensing gate did not land ─────────────────
# The load-bearing case. A --delete rsync that dropped .htaccess leaves the
# instance serving restricted data to the open internet.
run_case "$good_json" "$good_json" "__ABSENT__"
expect_refusal "verify_remote rejects a missing .htaccess" "no Shibboleth gate found"

ungated="AuthType shibboleth
ShibRequestSetting requireSession 0
Require all granted"
run_case "$good_json" "$good_json" "$ungated"
expect_refusal "verify_remote rejects an .htaccess that does not require a session" \
  "no Shibboleth gate found"

# ── verify_remote: local preconditions ─────────────────────────────
run_case "__ABSENT__" "$good_json" "$good_ht"
expect_refusal "verify_remote refuses when the local dist has no version.json" \
  "frontend/dist/version.json is missing"

run_case '{"commit":"abc"}' "$good_json" "$good_ht"
expect_refusal "verify_remote refuses when the local version.json has no version" \
  "could not read 'version'"

# Empty remote output must not compare equal to an empty expected value — the
# classic fail-open bug. This must be refused at the precondition, before any
# comparison happens.
run_case '{"version":""}' "" "$good_ht"
expect_refusal "verify_remote fails closed on an empty expected version" \
  "could not read 'version'"

# ── write_gate ─────────────────────────────────────────────────────
# The generated .htaccess is the licensing gate. Assert both halves: the site
# stays closed, and version.json is the ONLY thing opened.
#
# Run from a fake repo root, so the relative SOPS_ENV_FILE does not resolve:
# on a machine that can decrypt the real SSoT, check_whitelist_fresh correctly
# refuses a synthetic whitelist (exit 6) and write_gate never runs. That guard
# working is #565 behaving; here it just has to be out of the way.
gate_root="$work/gateroot"
gate_dir="$gate_root/dist"
rm -rf "$gate_root"
mkdir -p "$gate_root/scripts" "$gate_dir"
cp "$DEPLOY" "$gate_root/scripts/"
printf '# a comment\n\nsomeone@ethz.ch\nother@ethz.ch\n' > "$work/whitelist.txt"
printf '{"version":"0.19.0"}' > "$gate_dir/version.json"
(
  # shellcheck source=/dev/null  # path is built at runtime
  HYRR_ETH_WHITELIST="$work/whitelist.txt" HYRR_DEPLOY_ETH_LIB=1 source "$gate_root/scripts/deploy-eth.sh"
  write_gate "$gate_dir"
) >/dev/null 2>&1
gate_out="$(cat "$gate_dir/.htaccess" 2>/dev/null)"

gate_has() { # gate_has <name> <fragment>
  if [ "${gate_out#*"$2"}" != "$gate_out" ]; then report "$1" pass; else
    report "$1 (missing '$2')" fail
  fi
}

gate_has "write_gate demands a session site-wide" "$GATE_LINE"
gate_has "write_gate matches the whitelist on principalName" \
  "Require shib-attr principalName someone@ethz.ch other@ethz.ch"
gate_has "write_gate matches the whitelist on mail" \
  "Require shib-attr mail someone@ethz.ch other@ethz.ch"
gate_has "write_gate opens version.json" '<Files "version.json">'
gate_has "write_gate uses a lazy session for version.json" "ShibRequestSetting requireSession 0"
gate_has "write_gate grants access to version.json" "Require all granted"

# The exception must be scoped. An unscoped "Require all granted" would open
# the entire instance — the worst possible outcome of this change.
if [ "$(printf '%s\n' "$gate_out" | grep -c 'Require all granted')" -eq 1 ] &&
   [ "$(printf '%s\n' "$gate_out" | grep -c '</Files>')" -eq 1 ]; then
  report "write_gate keeps 'Require all granted' inside exactly one <Files> block" pass
else
  report "write_gate keeps 'Require all granted' inside exactly one <Files> block" fail
fi

# A gate written by write_gate must satisfy verify_remote — otherwise the two
# halves of this feature disagree and every real deploy fails.
run_case "$good_json" "$good_json" "$gate_out"
expect_ok "verify_remote accepts the .htaccess that write_gate actually produces"

# ── assert_single_version_file ─────────────────────────────────────
# The <Files "version.json"> exception is matched by BASENAME at any depth, so
# a second version.json anywhere in the tree would also be served un-gated.
run_gate() { # run_gate  → GATE_RC / GATE_MSG, against $gate_dir as it stands
  GATE_MSG="$(
    # shellcheck source=/dev/null  # path is built at runtime
    HYRR_ETH_WHITELIST="$work/whitelist.txt" HYRR_DEPLOY_ETH_LIB=1 \
      source "$gate_root/scripts/deploy-eth.sh"
    write_gate "$gate_dir" 2>&1
  )"
  GATE_RC=$?
}

expect_gate_refusal() { # expect_gate_refusal <name> <fragment>
  if [ "$GATE_RC" -eq 5 ] && [ "${GATE_MSG#*"$2"}" != "$GATE_MSG" ]; then
    report "$1" pass
  else
    report "$1 (rc=$GATE_RC, wanted '$2', out: $GATE_MSG)" fail
  fi
}

mkdir -p "$gate_dir/data/parquet"
printf '{"version":"x"}' > "$gate_dir/data/parquet/version.json"
run_gate
expect_gate_refusal "write_gate refuses when a second version.json is nested in the tree" \
  "matches by basename at ANY depth"
rm -f "$gate_dir/data/parquet/version.json"

rm -f "$gate_dir/version.json"
run_gate
expect_gate_refusal "write_gate refuses when the root version.json is absent" \
  "expected exactly one"
printf '{"version":"0.19.0"}' > "$gate_dir/version.json"

# A differently-cased file is NOT matched by Apache's case-sensitive <Files>,
# so it must not trigger a refusal.
printf '{}' > "$gate_dir/Version.json"
run_gate
if [ "$GATE_RC" -eq 0 ]; then
  report "write_gate tolerates a differently-cased Version.json" pass
else
  report "write_gate tolerates a differently-cased Version.json (rc=$GATE_RC)" fail
fi
rm -f "$gate_dir/Version.json"

if [ "$failed" -eq 0 ]; then
  echo "All deploy-eth verification tests passed."
else
  echo "Some deploy-eth verification tests FAILED." >&2
fi
exit "$failed"

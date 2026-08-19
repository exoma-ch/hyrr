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
      local cmd="$2" body
      case "$cmd" in
        *version.json*) body="$(cat "$dst/remote_version.json")" ;;
        *.htaccess*)    body="$(cat "$dst/remote_htaccess")" ;;
        *)              return 0 ;;
      esac
      # "__ABSENT__" models a real missing file. The stub emulates the REMOTE
      # SHELL, not just ssh: `cat missing` exits 1 and ssh relays that, unless
      # the sent command ends in `|| true`, in which case the remote shell
      # swallows it and ssh exits 0 with empty output.
      #
      # That distinction is the whole point. The first version of this stub
      # always returned 0, so it could not reproduce the `set -euo pipefail`
      # abort a genuine missing file caused — verify_remote died at the
      # assignment before reaching its own diagnostic, and every absence test
      # passed for the wrong reason. Modelling it this way means deleting the
      # `|| true` from deploy-eth.sh makes these tests go red.
      if [ "$body" = "__ABSENT__" ]; then
        case "$cmd" in
          *"|| true"*) return 0 ;;
          *) return 1 ;;
        esac
      fi
      printf '%s' "$body"
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

# A remote read that fails must still produce the exit-7 diagnostic, not a
# bare abort. Guards the `|| true` on the remote side of verify_remote.
run_case "$good_json" "__ABSENT__" "$good_ht"
expect_refusal "a failing remote read still explains itself" "no readable version.json"

run_case "$good_json" "$good_json" "__ABSENT__"
expect_refusal "a failing remote .htaccess read still explains itself" "no Shibboleth gate found"

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

# ── prd preflight: only from main, clean, level with origin (#648) ───────────
#
# Every case runs against a REAL pair of git repos (a bare "origin" and a
# clone), because the assertion is entirely about git state — stubbing git
# would only test the stub. Each case asserts the refusal REASON, not just a
# non-zero exit: an assertion that rejects everything would pass a
# rc-only check while being useless.

prd_repo=""
prd_setup() { # prd_setup — fresh origin+clone, one commit on main
  local root; root="$(mktemp -d "$work/prd.XXXXXX")"
  git init --quiet --bare "$root/origin.git"
  git init --quiet -b main "$root/clone"
  (
    cd "$root/clone" || exit 1
    git config user.email t@t; git config user.name t
    git config commit.gpgsign false
    # Must live at scripts/ — deploy-eth.sh resolves REPO_ROOT as its own
    # dirname/.. and cd's there. Dropped at the top level it would cd OUT of the
    # clone, git would report branch "unknown", and every case would refuse for
    # that reason instead of the one under test. Two cases false-passed that way
    # while this was wrong, which is why each assertion checks the message.
    mkdir -p scripts
    cp "$DEPLOY" scripts/deploy-eth.sh
    echo seed > seed.txt
    git add -A && git commit --quiet -m init
    git remote add origin "$root/origin.git"
    git push --quiet -u origin main
  ) >/dev/null 2>&1
  prd_repo="$root/clone"
}

PRD_RC=0
PRD_OUT=""
run_prd() { # run_prd — assert_prd_is_released_main inside the fake clone
  PRD_OUT="$(cd "$prd_repo" && HYRR_DEPLOY_ETH_LIB=1 bash -c \
    'source ./scripts/deploy-eth.sh; assert_prd_is_released_main' 2>&1)"
  PRD_RC=$?
}

expect_prd_refusal() { # expect_prd_refusal <name> <substring>
  if [ "$PRD_RC" -ne 0 ] && printf '%s' "$PRD_OUT" | grep -qi -- "$2"; then
    report "$1" pass
  else
    report "$1 (rc=$PRD_RC, out: $(printf '%s' "$PRD_OUT" | tr '\n' ' '))" fail
  fi
}

prd_setup
run_prd
if [ "$PRD_RC" -eq 0 ]; then
  report "prd preflight passes on clean main level with origin" pass
else
  report "prd preflight passes on clean main level with origin (rc=$PRD_RC, out: $PRD_OUT)" fail
fi

# A feature branch is refused even when it is otherwise clean and pushed.
prd_setup
(cd "$prd_repo" && git checkout --quiet -b feature/x && git push --quiet -u origin feature/x) >/dev/null 2>&1
run_prd
expect_prd_refusal "prd refuses a feature branch" "only from 'main'"

# Detached HEAD is the shape a release-tag checkout takes, and it is still not
# main — production must be traceable to the branch, not to a loose commit.
prd_setup
(cd "$prd_repo" && git checkout --quiet --detach HEAD) >/dev/null 2>&1
run_prd
expect_prd_refusal "prd refuses a detached HEAD" "only from 'main'"

# Uncommitted tracked changes mean the artifact is not reproducible from GitHub.
prd_setup
(cd "$prd_repo" && echo changed > seed.txt) >/dev/null 2>&1
run_prd
expect_prd_refusal "prd refuses a dirty tree" "clean tree"

# Untracked files are NOT a reason to refuse: the gitignored whitelist is
# materialised from sops on every run, so treating them as dirty would make the
# gate fire on a correct deploy.
prd_setup
(cd "$prd_repo" && echo scratch > untracked.txt) >/dev/null 2>&1
run_prd
if [ "$PRD_RC" -eq 0 ]; then
  report "prd tolerates untracked files" pass
else
  report "prd tolerates untracked files (rc=$PRD_RC, out: $PRD_OUT)" fail
fi

# Ahead of origin: the commits have passed no gate, because CI never saw them.
prd_setup
(cd "$prd_repo" && echo more >> seed.txt && git commit --quiet -am second) >/dev/null 2>&1
run_prd
expect_prd_refusal "prd refuses when ahead of origin" "AHEAD"

# Behind origin: deploying would silently ship an older tree than main.
prd_setup
(
  cd "$prd_repo" || exit 1
  echo more >> seed.txt && git commit --quiet -am second && git push --quiet origin main
  git reset --quiet --hard HEAD~1
) >/dev/null 2>&1
run_prd
expect_prd_refusal "prd refuses when behind origin" "BEHIND"

# Diverged: neither ahead nor behind, and the message must not claim either.
prd_setup
(
  cd "$prd_repo" || exit 1
  echo theirs >> seed.txt && git commit --quiet -am theirs && git push --quiet origin main
  git reset --quiet --hard HEAD~1
  echo mine >> seed.txt && git commit --quiet -am mine
) >/dev/null 2>&1
run_prd
expect_prd_refusal "prd refuses diverged histories" "DIVERGED"

# ── confirm_prd: no TTY must REFUSE, not wave through ────────────────────────
#
# The original read `[ "$HYRR_ASSUME_YES" = 1 ] || [ ! -t 0 ] && return 0`, so
# every non-interactive caller deployed production silently. Found when the
# 0.20.1 promotion ran from an agent shell and the prompt never appeared.
# `< /dev/null` is what makes stdin a non-TTY here.

confirm_out=""
confirm_rc=0
run_confirm() { # run_confirm [env assignments...]
  confirm_out="$(env "$@" bash -c '
    HYRR_DEPLOY_ETH_LIB=1 source "'"$DEPLOY"'"
    confirm_prd
    echo CONTINUED' < /dev/null 2>&1)"
  confirm_rc=$?
}

run_confirm HYRR_ASSUME_YES=
if [ "$confirm_rc" -ne 0 ] && printf '%s' "$confirm_out" | grep -q "no TTY"; then
  report "confirm_prd refuses prd with no TTY and no explicit opt-in" pass
else
  report "confirm_prd refuses prd with no TTY (rc=$confirm_rc, out: $confirm_out)" fail
fi

# It must not merely exit non-zero — it must not have proceeded.
if printf '%s' "$confirm_out" | grep -q CONTINUED; then
  report "confirm_prd does not fall through after refusing" fail
else
  report "confirm_prd does not fall through after refusing" pass
fi

# The deliberate non-interactive path still works — CI depends on it.
run_confirm HYRR_ASSUME_YES=1
if [ "$confirm_rc" -eq 0 ] && printf '%s' "$confirm_out" | grep -q CONTINUED; then
  report "confirm_prd allows an explicit HYRR_ASSUME_YES=1" pass
else
  report "confirm_prd allows an explicit HYRR_ASSUME_YES=1 (rc=$confirm_rc, out: $confirm_out)" fail
fi

# ── whitelist is derived from the encrypted SSoT when absent ─────────────────
#
# Only the missing-file path is covered here; decrypting for real needs an age
# key, so the sops call is stubbed. What matters is that an existing file is
# never overwritten and that failure leaves nothing behind — a truncated
# whitelist deploys as a NARROWER gate, locking real users out silently.

wl_root="$work/wl"
run_materialise() { # run_materialise <sops-stdout> [existing-file-content]
  rm -rf "$wl_root"; mkdir -p "$wl_root/scripts" "$wl_root/secrets"
  cp "$DEPLOY" "$wl_root/scripts/"
  printf 'ciphertext\n' > "$wl_root/secrets/eth-deploy.sops.env"
  if [ $# -ge 2 ]; then
    mkdir -p "$wl_root/infra/eth-webhosting"
    printf '%s' "$2" > "$wl_root/infra/eth-webhosting/whitelist.txt"
  fi
  local payload="$1"
  (
    cd "$wl_root" || exit 1
    # shellcheck source=/dev/null  # path is built at runtime
    HYRR_DEPLOY_ETH_LIB=1 source "$wl_root/scripts/deploy-eth.sh"
    # Stub sops: decrypting for real needs an age key, and the point here is
    # the file handling, not the crypto. Defined AFTER sourcing so it wins.
    # shellcheck disable=SC2317,SC2329
    sops() { printf '%s\n' "$payload"; }
    materialise_whitelist_from_sops >/dev/null 2>&1
  ) 2>/dev/null
}

# Happy path: absent file, decryptable SSoT → derived, with the right contents.
b64_two="$(printf 'a@ethz.ch\nb@psi.ch\n' | base64 | tr -d '\n')"
run_materialise "WHITELIST_B64=$b64_two"
if [ -f "$wl_root/infra/eth-webhosting/whitelist.txt" ] \
   && [ "$(grep -c . "$wl_root/infra/eth-webhosting/whitelist.txt")" -eq 2 ]; then
  report "whitelist is derived from the SSoT when absent" pass
else
  report "whitelist is derived from the SSoT when absent" fail
fi

# An existing file is authoritative and must never be clobbered — an operator
# may be testing a narrower list on purpose.
run_materialise "WHITELIST_B64=$b64_two" "keepme@ethz.ch"
if [ "$(cat "$wl_root/infra/eth-webhosting/whitelist.txt")" = "keepme@ethz.ch" ]; then
  report "an existing whitelist is never overwritten" pass
else
  report "an existing whitelist is never overwritten" fail
fi

# No WHITELIST_B64 in the payload → write nothing, so write_gate still refuses
# rather than deploying an empty (ungated-by-omission) list.
run_materialise "SOMETHING_ELSE=x"
if [ ! -f "$wl_root/infra/eth-webhosting/whitelist.txt" ]; then
  report "no WHITELIST_B64 leaves no whitelist behind" pass
else
  report "no WHITELIST_B64 leaves no whitelist behind" fail
fi

if [ "$failed" -eq 0 ]; then
  echo "All deploy-eth verification tests passed."
else
  echo "Some deploy-eth verification tests FAILED." >&2
fi
exit "$failed"

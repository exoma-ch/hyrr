#!/usr/bin/env bash
# Tests for scripts/verify-deploy.sh (#401).
#
# verify-deploy.sh sources scripts/deploy-eth.sh (for env_url / validate_env /
# json_field / VERSION_FILE) relative to its OWN root, so a fake root with a
# stub deploy-eth.sh redirects it at a local HTTP server. No test-only knob in
# the production script, and the HTTP path is genuinely exercised.
#
# What cannot be tested locally is the success path of the gate assertion: it
# requires the effective URL to be on a real SWITCH AAI login host, deliberately
# matched as a host prefix so a local fake cannot satisfy it. Precision in
# production beats local testability there; the happy path was verified by
# hand against ent, tst and prd.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
VERIFY="$SCRIPT_DIR/verify-deploy.sh"
DEPLOY="$SCRIPT_DIR/deploy-eth.sh"

failed=0
report() {
  if [ "$2" = "pass" ]; then echo "PASS: $1"; else echo "FAIL: $1" >&2; failed=1; fi
}

work="$(mktemp -d)"
serve_dir="$work/www"
mkdir -p "$serve_dir"

# A plain static server. `/` returns a directory listing (HTTP 200, not a
# redirect), which is exactly the shape of an un-gated instance.
# -u: without it the "Serving HTTP on … port N" banner stays in python's stdout
# buffer when redirected, and the port can never be discovered.
python3 -u -m http.server 0 --directory "$serve_dir" --bind 127.0.0.1 >"$work/httpd.log" 2>&1 &
HTTPD_PID=$!
trap 'kill "$HTTPD_PID" 2>/dev/null; rm -rf "$work"' EXIT

PORT=""
for _ in $(seq 1 50); do
  PORT="$(sed -n 's/.*127\.0\.0\.1 port \([0-9]*\).*/\1/p' "$work/httpd.log" | head -1)"
  [ -n "$PORT" ] && break
  sleep 0.1
done
if [ -z "$PORT" ]; then
  echo "FAIL: could not start a local http server" >&2
  exit 1
fi

# Fake root: real verify-deploy.sh, stub deploy-eth.sh pointing at the server.
root="$work/root"
mkdir -p "$root/scripts" "$root/frontend/dist"
cp "$VERIFY" "$root/scripts/"
{
  echo '#!/usr/bin/env bash'
  echo 'VERSION_FILE="version.json"'
  echo "env_url() { echo \"http://127.0.0.1:${PORT}\"; }"
  # Reuse the real implementations of the pure helpers so the test does not
  # quietly diverge from what production actually runs.
  sed -n '/^validate_env()/,/^}/p' "$DEPLOY"
  sed -n '/^json_field()/,/^}/p' "$DEPLOY"
  echo 'return 0'
} > "$root/scripts/deploy-eth.sh"

run_verify() { # run_verify <args...>  → sets RC / OUT
  OUT="$(HYRR_VERIFY_TIMEOUT=0 HYRR_VERIFY_INTERVAL=1 "$root/scripts/verify-deploy.sh" "$@" 2>&1)"
  RC=$?
}

expect_rc() { # expect_rc <name> <want-rc> <message-fragment>
  if [ "$RC" -eq "$2" ] && [ "${OUT#*"$3"}" != "$OUT" ]; then
    report "$1" pass
  else
    report "$1 (rc=$RC want $2, fragment '$3', out: $OUT)" fail
  fi
}

# ── argument handling ──────────────────────────────────────────────
run_verify
expect_rc "no environment is a usage error" 2 "usage: verify-deploy.sh"

run_verify nope
expect_rc "an unknown environment is rejected" 2 "unknown environment"

run_verify tst
expect_rc "refuses when no version is given and dist has none" 2 "no expected version given"

# ── version check ──────────────────────────────────────────────────
printf '{"version":"0.19.0","commit":"abc","built_at":"2026-08-06T12:00:00.000Z"}\n' \
  > "$serve_dir/version.json"

run_verify tst 0.18.0
expect_rc "rejects a version that does not match" 8 "did not report version 0.18.0"

# Served version.json is right, so the run must get PAST the version check and
# fail on the gate — proving the version check passed against a real HTTP GET.
run_verify tst 0.19.0
expect_rc "accepts the served version, then fails on the absent gate" 9 \
  "did NOT redirect to a known SWITCH AAI login host"

if [ "${OUT#*"version.json = 0.19.0"}" != "$OUT" ]; then
  report "reports the version it actually read over HTTP" pass
else
  report "reports the version it actually read over HTTP" fail
fi

# ── the gate assertion ─────────────────────────────────────────────
# The load-bearing case: a plain 200 for an unauthenticated GET / means the
# licensing gate is off. It must exit 9 and say so in incident terms.
run_verify tst --gate-only
expect_rc "an un-gated instance is refused with its own exit code" 9 \
  "may be serving license-restricted nuclear data"

# A missing version.json must NOT be able to mask a missing gate: --gate-only
# still runs the security assertion.
rm -f "$serve_dir/version.json"
run_verify tst --gate-only
expect_rc "--gate-only still asserts the gate with no version.json present" 9 \
  "did NOT redirect to a known SWITCH AAI login host"

# ── falls back to frontend/dist/version.json ───────────────────────
printf '{"version":"9.9.9","commit":"abc","built_at":"x"}\n' > "$root/frontend/dist/version.json"
run_verify tst
expect_rc "reads the expected version from frontend/dist when not given" 8 \
  "did not report version 9.9.9"

if [ "$failed" -eq 0 ]; then
  echo "All verify-deploy tests passed."
else
  echo "Some verify-deploy tests FAILED." >&2
fi
exit "$failed"

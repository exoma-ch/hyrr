#!/usr/bin/env bash
# Verify a deployed ETH instance from the OUTSIDE, unauthenticated (#401).
#
#   scripts/verify-deploy.sh ent            # expect the version in frontend/dist
#   scripts/verify-deploy.sh prd 0.19.0     # expect an explicit version
#
# Two assertions, and the second is the one that must never be dropped:
#
#   1. GET /version.json  → 200, and the version is the one we expect.
#      Only readable because write_gate opens that single file (see
#      deploy-eth.sh). Everything else on these instances is behind SWITCH AAI.
#
#   2. GET /             → redirects to the AAI WAYF.
#      A SECURITY regression test, not a deploy check. If an unauthenticated
#      request ever gets 200 for the app itself, the licensing gate is off and
#      license-restricted nuclear data is on the open internet. That is a worse
#      failure than any stale version, so it gets its own exit code.
#
# Needs no credentials and no ETH-network access — plain HTTPS. That is the
# whole point: it runs from a GitHub-hosted runner, which cannot reach ETH SSH.
#
# Exit codes:  0 ok · 2 usage · 8 wrong/unreadable version · 9 GATE NOT ACTIVE
#
# Env knobs:
#   HYRR_VERIFY_TIMEOUT   seconds to keep retrying the version check (default 90)
#   HYRR_VERIFY_INTERVAL  seconds between attempts (default 10)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# Reuse env_url / validate_env / json_field / VERSION_FILE rather than
# restating them: the gate, the server-side check and this check must agree on
# which file is exposed, and one definition is how that stays true.
# shellcheck source=/dev/null  # resolved at runtime from $ROOT
HYRR_DEPLOY_ETH_LIB=1 source "$ROOT/scripts/deploy-eth.sh"

TIMEOUT="${HYRR_VERIFY_TIMEOUT:-90}"
INTERVAL="${HYRR_VERIFY_INTERVAL:-10}"

# -4: the instances' AAAA records are not reliably reachable from every runner,
# and the existing smoke check already pinned IPv4 for that reason.
CURL=(curl -4 -sS -m 20)

ENV_NAME="${1:-}"
[ -n "$ENV_NAME" ] || {
  echo "usage: verify-deploy.sh <ent|tst|prd> [expected-version]" >&2
  exit 2
}
validate_env "$ENV_NAME"
URL="$(env_url "$ENV_NAME")"

GATE_ONLY=0
EXPECTED="${2:-}"
if [ "$EXPECTED" = "--gate-only" ]; then
  GATE_ONLY=1
  EXPECTED=""
fi

if [ "$GATE_ONLY" = "1" ]; then
  echo "=== Verifying ${URL} (gate only) ==="
elif [ -z "$EXPECTED" ]; then
  if [ ! -f "frontend/dist/${VERSION_FILE}" ]; then
    echo "ERROR: no expected version given and frontend/dist/${VERSION_FILE} is absent." >&2
    echo "       Pass the version explicitly: verify-deploy.sh ${ENV_NAME} <version>" >&2
    exit 2
  fi
  EXPECTED="$(json_field version < "frontend/dist/${VERSION_FILE}")"
fi
if [ "$GATE_ONLY" != "1" ]; then
  [ -n "$EXPECTED" ] || { echo "ERROR: could not determine the expected version." >&2; exit 2; }
  echo "=== Verifying ${URL} (public, unauthenticated) ==="
  echo "    expecting version ${EXPECTED}"
fi

# ── 1. the deployed version ────────────────────────────────────────
# Retried: a deploy may still be settling behind a caching layer.
#
# Redirects ARE followed, because the canonical prod URL is a vanity alias:
# https://hyrr.ethz.ch/version.json 301s to https://hyrrprd.ethz.ch/version.json
# before Shibboleth is ever consulted. Refusing to follow would fail prd
# permanently.
#
# Following redirects cannot launder a failure into a pass, though: if the
# <Files> exception is missing we get bounced to the AAI WAYF, whose HTML
# login page yields no parseable version, so the comparison still fails. The
# effective URL is only used to explain WHY.
body="$(mktemp)"
trap 'rm -f "$body"' EXIT

if [ "$GATE_ONLY" != "1" ]; then
  elapsed=0
  code=""
  actual=""
  effective=""
  while :; do
    read -r code effective <<< "$(
      "${CURL[@]}" -L -o "$body" -w '%{http_code} %{url_effective}' \
        "${URL}/${VERSION_FILE}" || echo "000 -"
    )"
    if [ "$code" = "200" ]; then
      actual="$(json_field version < "$body")"
      [ "$actual" = "$EXPECTED" ] && break
    fi
    if [ "$elapsed" -ge "$TIMEOUT" ]; then
      echo "ERROR: ${URL}/${VERSION_FILE} did not report version ${EXPECTED}" >&2
      echo "       after ${TIMEOUT}s (last HTTP ${code}, version '${actual:-<unreadable>}')." >&2
      case "$effective" in
        https://wayf.switch.ch/*)
          echo "       The request was redirected to the AAI WAYF, so the" >&2
          echo "       <Files \"${VERSION_FILE}\"> exception is missing from the" >&2
          echo "       deployed .htaccess — not a version problem." >&2
          ;;
      esac
      exit 8
    fi
    echo "    waiting… (HTTP ${code}, version '${actual:-?}', ${elapsed}s/${TIMEOUT}s)"
    sleep "$INTERVAL"
    elapsed=$((elapsed + INTERVAL))
  done
  echo "    ${VERSION_FILE} = ${actual}  ✓"
fi

# ── 2. the licensing gate is still active ──────────────────────────
# Follows redirects and inspects the FINAL url: robust to however many hops
# Shibboleth takes (prod is two — the hyrr.ethz.ch vanity alias 301s to
# hyrrprd.ethz.ch first), and it fails correctly when the site is un-gated,
# because the effective URL then stays on the instance itself.
#
# Matched as a host prefix, not a substring: `*wayf.switch.ch*` would also
# accept a URL that merely contains that text in its path.
effective="$("${CURL[@]}" -L -o /dev/null -w '%{url_effective}' "${URL}/" || echo "")"
case "$effective" in
  https://wayf.switch.ch/*)
    echo "    unauthenticated / → AAI WAYF  ✓"
    ;;
  *)
    echo "ERROR: ${URL}/ did NOT redirect to the SWITCH AAI WAYF." >&2
    echo "       Ended at: ${effective:-<no response>}" >&2
    echo "" >&2
    echo "       The instance may be serving license-restricted nuclear data" >&2
    echo "       UNAUTHENTICATED. Treat this as an incident, not a flaky test:" >&2
    echo "       check the deployed .htaccess before anything else." >&2
    exit 9
    ;;
esac

if [ "$GATE_ONLY" = "1" ]; then
  echo "=== ${URL} verified: gate active ==="
else
  echo "=== ${URL} verified: version ${EXPECTED}, gate active ==="
fi

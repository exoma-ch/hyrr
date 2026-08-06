#!/usr/bin/env bash
# Deploy the HYRR frontend to an ETH Zürich web instance (ent / tst / prd).
#
#   scripts/deploy-eth.sh ent           → https://hyrrent.ethz.ch  (dev / integration)
#   scripts/deploy-eth.sh tst           → https://hyrrtst.ethz.ch  (staging)
#   scripts/deploy-eth.sh prd           → https://hyrr.ethz.ch     (PRODUCTION — confirms)
#   scripts/deploy-eth.sh ladder        # build once → ent → tst   (prd HELD)
#   scripts/deploy-eth.sh probe ent     # read-only: dump remote $HOME + conf/servers + docroot
#   scripts/deploy-eth.sh build         # just build frontend/dist (no deploy)
#
# Promotion ladder:  ent ──▶ tst ──(explicit OK)──▶ prd     (mirrors duplet-webserver)
#
# Each instance is its own Apache vhost served at "/", so the bundle is built
# with VITE_BASE_PATH=/. The remote document root is read from the instance's
# own ~/conf/servers (authoritative); override with HYRR_ETH_DOCROOT.
#
# WHERE THIS RUNS: a host with SSH (:22) reachability to *.ethz.ch and the
# w3_hyrr<env> key loaded — i.e. your Mac on the ETH network / VPN. vm-dev and
# GitHub-hosted runners CANNOT reach ETH SSH ("No route to host"); see
# .github/workflows/deploy-eth.yml.
#
# Env knobs:
#   HYRR_ETH_DOCROOT     override remote docroot (skip conf/servers detection)
#   HYRR_SKIP_BUILD=1    deploy the existing frontend/dist as-is (promote the
#                        exact artifact verified on the previous rung)
#   HYRR_ASSUME_YES=1    skip the prd confirmation prompt (for non-interactive CI)
#   HYRR_ETH_SSH_PREFIX  use ssh_config aliases "<prefix><env>" instead of
#                        connecting directly. ETH SSH (:22) is reachable only
#                        from the ETH network, so off-network hosts (vm-dev, CI)
#                        set HYRR_ETH_SSH_PREFIX=ethz-hyrr to route through the
#                        heimdall ProxyJump configured in ~/.ssh/config. A host
#                        already on the ETH VPN can leave it unset (direct).
set -euo pipefail

# BASH_SOURCE, not $0: when this file is sourced (the HYRR_DEPLOY_ETH_LIB test
# seam at the bottom) $0 is the *caller's* path, which would resolve REPO_ROOT
# to the wrong directory and make every path-dependent helper silently wrong.
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

SSH_OPTS=(-o ConnectTimeout=30 -o StrictHostKeyChecking=accept-new)

# ── env → host / user / canonical URL ──────────────────────────────
env_host() { echo "hyrr${1}.ethz.ch"; }
env_user() { echo "w3_hyrr${1}"; }
env_url()  { case "$1" in prd) echo "https://hyrr.ethz.ch" ;; *) echo "https://hyrr${1}.ethz.ch" ;; esac; }

# SSH destination for an env. With HYRR_ETH_SSH_PREFIX set (e.g. "ethz-hyrr"),
# use the matching ssh_config alias — it supplies user, hostname, key and any
# ProxyJump (the ETH heimdall jump). Otherwise connect directly, which works
# from a host already on the ETH network / VPN.
ssh_target() { # ssh_target <env>
  if [ -n "${HYRR_ETH_SSH_PREFIX:-}" ]; then
    echo "${HYRR_ETH_SSH_PREFIX}${1}"
  else
    echo "$(env_user "$1")@$(env_host "$1")"
  fi
}

validate_env() {
  case "$1" in
    ent | tst | prd) : ;;
    *) echo "ERROR: unknown environment '$1' (expected ent|tst|prd)" >&2; exit 2 ;;
  esac
}

# Extract a top-level string field from version.json, reading stdin.
#
# Hand-rolled rather than jq: this has to run on the ETH webhosting instances
# and on a maintainer's Mac, and jq is guaranteed on neither. The input is a
# file this repo generates itself (frontend/scripts/version-info.ts) with a
# fixed shape and no spaces inside any value, so stripping whitespace first
# makes the match robust against pretty-printing without risking a truncated
# value.
json_field() { # json_field <field>   (JSON on stdin)
  tr -d '\n\r\t ' | sed -n "s/.*\"$1\":\"\([^\"]*\)\".*/\1/p"
}

remote() { # remote <env> <cmd...>
  local env="$1"; shift
  # The command is intentionally sent verbatim for remote-side expansion.
  # shellcheck disable=SC2029
  ssh "${SSH_OPTS[@]}" "$(ssh_target "$env")" "$@"
}

# Resolve the document root remotely — authoritative, never guessed locally.
# 1. HYRR_ETH_DOCROOT override, else
# 2. DocumentRoot from the instance's own ~/conf/servers, else
# 3. first existing conventional docroot under $HOME.
resolve_docroot() { # resolve_docroot <env>  → prints absolute path
  local env="$1"
  if [ -n "${HYRR_ETH_DOCROOT:-}" ]; then echo "$HYRR_ETH_DOCROOT"; return; fi
  remote "$env" 'bash -s' <<'REOF'
set -e
dr=""
if [ -f "$HOME/conf/servers" ]; then
  dr=$(grep -iE '^[[:space:]]*DocumentRoot' "$HOME/conf/servers" | head -1 | awk '{print $2}' | tr -d '"')
  dr=$(eval echo "$dr" 2>/dev/null || echo "$dr")
fi
if [ -z "$dr" ] || [ ! -d "$dr" ]; then
  for d in "$HOME"/homepage "$HOME"/www "$HOME"/public_html "$HOME"/htdocs "$HOME"/html; do
    [ -d "$d" ] && { dr="$d"; break; }
  done
fi
[ -n "$dr" ] || { echo "__NO_DOCROOT__"; exit 0; }
echo "$dr"
REOF
}

# ── access gate (Shibboleth) ───────────────────────────────────────
# HYRR's nuclear data is license-restricted → the ETH instances are gated behind
# SWITCH AAI, limited to a whitelist. Delivered as a .htaccess in the deployed
# bundle (htdocs runs AllowOverride AuthConfig, which permits AuthType/Require/
# ShibRequestSetting — only FileInfo/AddType is disallowed, which is why no MIME
# is set; wasm-bindgen falls back streaming→arrayBuffer). Baked into the build so
# a redeploy can't silently un-gate the site. Whitelist is gitignored (public
# repo); each entry is matched against BOTH principalName and mail (OR'd).
#
# ⚠ CANARY ON ent FIRST when changing the generated .htaccess. AllowOverride
# here permits AuthConfig but not FileInfo, and a directive outside the allowed
# set does not degrade — it 500s the WHOLE instance. <Files> plus Require and
# ShibRequestSetting are all AuthConfig, so the version.json exception below is
# expected to be legal, but walk it up the ent → tst → prd ladder regardless.
WHITELIST_FILE="${HYRR_ETH_WHITELIST:-infra/eth-webhosting/whitelist.txt}"
SOPS_ENV_FILE="secrets/eth-deploy.sops.env"

# Build-provenance file emitted into dist/ by frontend/scripts/version-info.ts,
# and the single filename that is served without an AAI session (see write_gate
# and scripts/verify-deploy.sh). Named once so the gate, the server-side check
# and the public check cannot drift apart.
VERSION_FILE="version.json"

# The whitelist file is gitignored, so the SSoT is WHITELIST_B64 inside
# secrets/eth-deploy.sops.env. A local plaintext copy therefore goes stale the
# moment someone grants access via `sops set` — and a stale copy deploys
# SILENTLY: the run is green, the gate looks deployed, and an approved user is
# simply missing (or a revoked one still has access). Compare the two before
# writing the gate and refuse on drift.
#
# Skips cleanly when the SSoT is unreadable — CI materialises the whitelist from
# the WHITELIST_B64 Actions secret and has no age key, so there is nothing to
# compare against there and the file is authoritative by construction.
check_whitelist_fresh() {
  [ -f "$SOPS_ENV_FILE" ] || return 0
  command -v sops >/dev/null 2>&1 || return 0
  local decrypted
  decrypted=$(sops -d "$SOPS_ENV_FILE" 2>/dev/null) || return 0
  [ -n "$decrypted" ] || return 0   # no age key / cannot decrypt → nothing to check

  # Decryption worked but the key we compare against is gone. That is a config
  # error, not "no key available" — a guard whose whole purpose is to kill
  # silent failure must not skip silently itself, so say so out loud.
  local b64
  b64=$(printf '%s\n' "$decrypted" | grep '^WHITELIST_B64=' | cut -d= -f2-) || b64=""
  if [ -z "$b64" ]; then
    echo "WARNING: $SOPS_ENV_FILE decrypted but has no WHITELIST_B64 entry —" >&2
    echo "         cannot verify '$WHITELIST_FILE' is current; deploying unverified." >&2
    return 0
  fi

  local expected
  expected=$(printf '%s' "$b64" | base64 -d 2>/dev/null) || return 0
  [ -n "$expected" ] || return 0

  # Compare the entry SET (sorted, comments and blanks stripped), not raw bytes:
  # ordering and comment edits are not access changes, and a byte compare would
  # fail on a trailing newline an editor added. Case-folded because eppn/mail
  # are case-insensitive at the Shibboleth layer, so a case-only edit is not an
  # access change and must not be reported as drift.
  # `|| x=""` on both: under `set -euo pipefail` a grep that matches nothing
  # (an all-comments whitelist) would otherwise abort the script with a bare
  # exit 1, pre-empting the friendlier "whitelist is empty" error below.
  local want have
  want=$(printf '%s\n' "$expected" | grep -vE '^[[:space:]]*#|^[[:space:]]*$' \
    | tr '[:upper:]' '[:lower:]' | sort -u) || want=""
  have=$(grep -vE '^[[:space:]]*#|^[[:space:]]*$' "$WHITELIST_FILE" 2>/dev/null \
    | tr '[:upper:]' '[:lower:]' | sort -u) || have=""
  [ "$want" = "$have" ] && return 0

  # Never print the entries themselves — they are personal data (eppn/mail).
  echo "ERROR: '$WHITELIST_FILE' is out of sync with the encrypted SSoT" >&2
  echo "       ($SOPS_ENV_FILE :: WHITELIST_B64) — refusing to deploy a stale" >&2
  echo "       access list. Local entries: $(printf '%s\n' "$have" | grep -c .); SSoT entries: $(printf '%s\n' "$want" | grep -c .)." >&2
  echo "" >&2
  echo "       Regenerate the local copy from the SSoT, then re-run:" >&2
  echo "         sops -d $SOPS_ENV_FILE | grep '^WHITELIST_B64=' \\" >&2
  echo "           | cut -d= -f2- | base64 -d > $WHITELIST_FILE" >&2
  echo "" >&2
  echo "       To CHANGE access, edit the encrypted SSoT (not this file) with" >&2
  echo "       'sops $SOPS_ENV_FILE' — it decrypts WHITELIST_B64 in your editor —" >&2
  echo "       then regenerate the local copy with the command above." >&2
  exit 6
}

write_gate() { # write_gate <dir>
  if [ ! -f "$WHITELIST_FILE" ]; then
    echo "ERROR: whitelist '$WHITELIST_FILE' not found — refusing to deploy ungated" >&2
    echo "       (nuclear-data license). Create it (one eppn/mail per line) or" >&2
    echo "       set HYRR_ETH_WHITELIST." >&2
    exit 5
  fi
  check_whitelist_fresh
  local ids
  ids=$(grep -vE '^[[:space:]]*#|^[[:space:]]*$' "$WHITELIST_FILE" | tr '\n' ' ' | sed 's/[[:space:]]*$//')
  [ -n "$ids" ] || { echo "ERROR: whitelist '$WHITELIST_FILE' is empty" >&2; exit 5; }
  cat > "$1/.htaccess" <<HT
# Generated by scripts/deploy-eth.sh — DO NOT EDIT (regenerated each deploy).
# HYRR gated for nuclear-data licensing: a SWITCH AAI / Shibboleth session is
# required and access is limited to the whitelist below (principalName OR mail).
# Edit infra/eth-webhosting/whitelist.txt and redeploy to change access.
AuthType shibboleth
ShibRequestSetting requireSession 1
Require shib-attr principalName ${ids}
Require shib-attr mail ${ids}

# ${VERSION_FILE} is the ONE deliberate exception (#401). It carries version,
# commit and build timestamp — all three already public on the Releases page,
# and none of it licensed nuclear data. Exposing it is what lets a deploy be
# verified from outside the gate; without it an unauthenticated poll only ever
# reaches the AAI WAYF and "succeeds" with a 200 that means nothing.
# This is Shibboleth's documented lazy-session pattern: requireSession 0 stops
# mod_shib forcing a redirect, and the inner Require overrides the outer one
# for this file only.
<Files "${VERSION_FILE}">
  ShibRequestSetting requireSession 0
  Require all granted
</Files>
HT
}

# ── build ──────────────────────────────────────────────────────────
# No AddType in the .htaccess: ETH's Apache runs AllowOverride without FileInfo,
# so AddType returns 500. The .wasm is served without an application/wasm MIME,
# but wasm-bindgen's `--target web` glue falls back from instantiateStreaming to
# arrayBuffer on a MIME mismatch — a console warning, not a failure. HYRR uses
# hash routing (#config=…), so no SPA rewrite is needed either.
do_build() {
  if [ "${HYRR_SKIP_BUILD:-}" = "1" ]; then
    [ -d frontend/dist ] || { echo "ERROR: HYRR_SKIP_BUILD=1 but frontend/dist is missing — build first" >&2; exit 4; }
    echo "=== Reusing existing frontend/dist (HYRR_SKIP_BUILD=1) ==="
    # Still refresh the gate so a whitelist-only edit applies without a rebuild.
    write_gate frontend/dist
    return
  fi
  echo "=== 1/4 Building WASM compute engine ==="
  (cd wasm && wasm-pack build --target web --out-dir ../frontend/src/lib/compute/hyrr-wasm-pkg)

  echo "=== 2/4 Copying nuclear data into frontend/public ==="
  scripts/copy-frontend-data.sh nucl-parquet/data frontend/public/data/parquet \
    tendl-2023-iso hi-xs-prod:hi-xs-prod endfb-8.0:neutron-xs

  echo "=== 3/4 Installing frontend deps ==="
  (cd frontend && npm install)
  # npm optional-deps bug: rollup's platform binary can be skipped on a fresh
  # CI install. Only needed on Linux runners; harmless to skip elsewhere.
  if [ "$(uname -s)" = "Linux" ]; then
    (cd frontend && { npm install @rollup/rollup-linux-x64-gnu --no-save 2>/dev/null || true; })
  fi

  echo "=== 4/4 Building frontend (VITE_BASE_PATH=/) ==="
  (cd frontend && VITE_BASE_PATH=/ npm run build)
  echo "=== Writing Shibboleth access gate (.htaccess) ==="
  write_gate frontend/dist
}

# ── post-deploy verification (server-side) ─────────────────────────
# Read the deployment back over the SSH channel this script ALREADY holds.
#
# Until this existed the script rsynced and then echoed "=== Deployed … ===",
# unconditionally — a green run proved only that rsync exited 0. Three separate
# incidents were a green pipeline over a wrong reality (#529, #461, #565).
#
# Server-side is where verification starts because it needs no new credential
# and no hole in the licensing gate: the deploy is already authenticated to the
# instance. An unauthenticated HTTP check is a strictly weaker signal and lands
# separately in scripts/verify-deploy.sh.
#
# Both assertions matter, and the second one more:
#   1. version.json on disk matches the bundle just built  → the right bytes.
#   2. .htaccess is present and still demands a session    → the licensing gate
#      survived the --delete rsync. An accidentally un-gated instance publishes
#      license-restricted nuclear data to the open internet, which is a worse
#      failure than any stale version.
verify_remote() { # verify_remote <env> <docroot>
  local env="$1" docroot="$2" expected actual gate

  if [ ! -f "frontend/dist/${VERSION_FILE}" ]; then
    echo "ERROR: frontend/dist/${VERSION_FILE} is missing — cannot verify the deploy." >&2
    echo "       A dist built before #401 has no ${VERSION_FILE}; rebuild without" >&2
    echo "       HYRR_SKIP_BUILD=1 and redeploy." >&2
    exit 7
  fi
  expected="$(json_field version < "frontend/dist/${VERSION_FILE}")"
  if [ -z "$expected" ]; then
    echo "ERROR: could not read 'version' out of frontend/dist/${VERSION_FILE}." >&2
    exit 7
  fi

  echo "=== Verifying deployment on $(env_host "$env") (server-side) ==="

  # shellcheck disable=SC2029  # $docroot is expanded locally, on purpose.
  actual="$(remote "$env" "cat '${docroot}/${VERSION_FILE}' 2>/dev/null" | json_field version)"
  if [ "$actual" != "$expected" ]; then
    echo "ERROR: deployed version does not match the bundle just built." >&2
    echo "       expected: ${expected}" >&2
    echo "       on host:  ${actual:-<no readable version.json>}" >&2
    echo "       The rsync reported success, so suspect the docroot: '${docroot}'." >&2
    exit 7
  fi
  echo "    ${VERSION_FILE} = ${actual}  ✓"

  # shellcheck disable=SC2029  # $docroot is expanded locally, on purpose.
  gate="$(remote "$env" "cat '${docroot}/.htaccess' 2>/dev/null")"
  case "$gate" in
    *"ShibRequestSetting requireSession 1"*) ;;
    *)
      echo "ERROR: no Shibboleth gate found in '${docroot}/.htaccess' — the" >&2
      echo "       instance may be serving license-restricted nuclear data" >&2
      echo "       UNAUTHENTICATED. Investigate before doing anything else." >&2
      exit 7
      ;;
  esac
  echo "    .htaccess requires an AAI session  ✓"
}

# ── deploy ─────────────────────────────────────────────────────────
do_deploy() { # do_deploy <env>
  local env="$1" host url docroot
  host="$(env_host "$env")"; url="$(env_url "$env")"

  echo "=== Resolving docroot on ${host} ==="
  docroot="$(resolve_docroot "$env")"
  if [ -z "$docroot" ] || [ "$docroot" = "__NO_DOCROOT__" ]; then
    echo "ERROR: could not determine docroot on ${host}." >&2
    echo "       Run 'scripts/deploy-eth.sh probe ${env}' and set HYRR_ETH_DOCROOT." >&2
    exit 3
  fi
  echo "    docroot = ${docroot}"

  local target; target="$(ssh_target "$env")"
  echo "=== Rsync frontend/dist → ${target}:${docroot} ==="
  # --delete keeps the docroot a clean mirror of dist (like the gh-pages
  # clean:true). Preserve ACME challenges so cert renewal isn't disturbed.
  rsync -rlptzh --delete --delete-excluded --exclude '.well-known/' \
    -e "ssh ${SSH_OPTS[*]}" \
    frontend/dist/ "${target}:${docroot}/"

  verify_remote "$env" "$docroot"

  echo "=== Deployed ${env} → ${url} ==="
}

confirm_prd() {
  if [ "${HYRR_ASSUME_YES:-}" = "1" ] || [ ! -t 0 ]; then return 0; fi
  echo "⚠  PRODUCTION deploy → https://hyrr.ethz.ch (user-facing)."
  read -r -p "   Type 'hyrr-prod' to continue: " c
  [ "$c" = "hyrr-prod" ] || { echo "Aborted."; exit 1; }
}

# ── dispatch ───────────────────────────────────────────────────────
# Test seam: `HYRR_DEPLOY_ETH_LIB=1 source scripts/deploy-eth.sh` loads the
# helpers above without running a command, so scripts/tests can exercise them
# (stubbing `remote` to avoid needing an ETH host). `return` is valid here only
# because this branch is reachable only when sourced.
if [ "${HYRR_DEPLOY_ETH_LIB:-}" = "1" ]; then return 0; fi

CMD="${1:-}"
[ -n "$CMD" ] || { grep -E '^#( |$)' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 2; }
shift || true

case "$CMD" in
  build)
    do_build
    ;;
  probe)
    ENV="${1:?usage: deploy-eth.sh probe <ent|tst|prd>}"; validate_env "$ENV"
    echo "=== Probing ${ENV}: $(env_user "$ENV")@$(env_host "$ENV") ==="
    # Single quotes are intentional — these expand on the remote host.
    # shellcheck disable=SC2016
    remote "$ENV" 'echo "HOME=$HOME"; echo "--- ls -la \$HOME ---"; ls -la "$HOME"; echo "--- conf/servers ---"; cat "$HOME/conf/servers" 2>/dev/null || echo "(no conf/servers)"'
    echo "--- resolved docroot ---"
    resolve_docroot "$ENV"
    ;;
  ladder)
    # ent + tst are non-prod: mark noindex so only prod (hyrr.ethz.ch) is
    # indexed. canonical still points at prod via the default VITE_SITE_URL.
    export VITE_ROBOTS="noindex, nofollow"
    do_build
    do_deploy ent
    do_deploy tst
    echo
    echo "=== ent + tst deployed from one build (noindex). PRD HELD. ==="
    echo "=== After your OK, lift to prod (rebuilt indexable):  just elevate  ==="
    ;;
  ent | tst)
    validate_env "$CMD"
    export VITE_ROBOTS="noindex, nofollow"
    do_build
    do_deploy "$CMD"
    ;;
  prd)
    # Prod is the canonical, indexable origin — no VITE_ROBOTS override. Always
    # builds fresh so it can't inherit a non-prod build's noindex meta.
    confirm_prd
    HYRR_SKIP_BUILD="" do_build
    do_deploy prd
    ;;
  *)
    echo "ERROR: unknown command '$CMD'" >&2
    echo "usage: deploy-eth.sh <ent|tst|prd|ladder|probe <env>|build>" >&2
    exit 2
    ;;
esac

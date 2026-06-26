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

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
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

# ── build ──────────────────────────────────────────────────────────
# NB: no .htaccess is written. ETH's Apache runs AllowOverride without FileInfo,
# so an AddType in .htaccess returns 500 ("AddType not allowed here"). The .wasm
# is therefore served without an application/wasm MIME, but wasm-bindgen's
# `--target web` glue falls back from instantiateStreaming to arrayBuffer
# instantiate on a MIME mismatch — a console warning, not a failure. HYRR uses
# hash routing (#config=…), so no SPA rewrite is needed either.
do_build() {
  if [ "${HYRR_SKIP_BUILD:-}" = "1" ]; then
    [ -d frontend/dist ] || { echo "ERROR: HYRR_SKIP_BUILD=1 but frontend/dist is missing — build first" >&2; exit 4; }
    echo "=== Reusing existing frontend/dist (HYRR_SKIP_BUILD=1) ==="
    return
  fi
  echo "=== 1/4 Building WASM compute engine ==="
  (cd wasm && wasm-pack build --target web --out-dir ../frontend/src/lib/compute/hyrr-wasm-pkg)

  echo "=== 2/4 Copying nuclear data into frontend/public ==="
  scripts/copy-frontend-data.sh nucl-parquet/data frontend/public/data/parquet \
    tendl-2023-iso hi-xs-prod:hi-xs-prod endfb-8.1:neutron-xs

  echo "=== 3/4 Installing frontend deps ==="
  (cd frontend && npm install)
  # npm optional-deps bug: rollup's platform binary can be skipped on a fresh
  # CI install. Only needed on Linux runners; harmless to skip elsewhere.
  if [ "$(uname -s)" = "Linux" ]; then
    (cd frontend && { npm install @rollup/rollup-linux-x64-gnu --no-save 2>/dev/null || true; })
  fi

  echo "=== 4/4 Building frontend (VITE_BASE_PATH=/) ==="
  (cd frontend && VITE_BASE_PATH=/ npm run build)
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

  echo "=== Deployed ${env} → ${url} ==="
}

confirm_prd() {
  if [ "${HYRR_ASSUME_YES:-}" = "1" ] || [ ! -t 0 ]; then return 0; fi
  echo "⚠  PRODUCTION deploy → https://hyrr.ethz.ch (user-facing)."
  read -r -p "   Type 'hyrr-prod' to continue: " c
  [ "$c" = "hyrr-prod" ] || { echo "Aborted."; exit 1; }
}

# ── dispatch ───────────────────────────────────────────────────────
CMD="${1:-}"
[ -n "$CMD" ] || { grep -E '^#( |$)' "$0" | sed 's/^# \{0,1\}//'; exit 2; }
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

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
# WHERE THIS RUNS: any host that can reach the instances over SSH (:22), either
#   * directly — your Mac on the ETH network / VPN, or
#   * via the heimdall ProxyJump aliases (ethz-hyrr{ent,tst,prd}) in ssh_config,
#     which is how vm-dev reaches them. This file previously said vm-dev CANNOT
#     ("No route to host"); that was true before the aliases existed and is not
#     true now — a full ent deploy of 0.20.1 ran from vm-dev through heimdall
#     (#648). The prefix is auto-detected; see default_ssh_prefix below.
# Either way the w3_hyrr<env> key must be loaded:
#   ssh-add ~/.ssh/id_ed25519_ethz ~/.ssh/id_ed25519_eth-hyrr
#
# GitHub-hosted runners still cannot deploy, but for an unrelated reason: the
# bastion rejects the key inside secrets/eth-deploy.sops.env (#641). That is an
# authorisation problem, not a reachability one — see deploy-eth.yml.
#
# Env knobs:
#   HYRR_ETH_DOCROOT     override remote docroot (skip conf/servers detection)
#   HYRR_SKIP_BUILD=1    deploy the existing frontend/dist as-is (promote the
#                        exact artifact verified on the previous rung)
#   HYRR_ASSUME_YES=1    skip the prd confirmation prompt (for non-interactive CI)
#   HYRR_SKIP_PUBLIC_VERIFY=1
#                        skip the post-deploy HTTPS check (scripts/verify-deploy.sh).
#                        The server-side check is NOT skippable.
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
#
# The prefix DEFAULTS to "ethz-hyrr" when that alias is defined in ssh_config,
# because the alias is strictly more capable: it carries the heimdall ProxyJump
# that makes the instances reachable from hosts with no ETH route at all. This
# repo long documented deploys as Mac-on-VPN-only; that is not true once the
# aliases exist — a full ent deploy ran from vm-dev through heimdall (#648).
# Set HYRR_ETH_SSH_PREFIX="" explicitly to force a direct connection.
default_ssh_prefix() {
  [ -n "${HYRR_ETH_SSH_PREFIX+set}" ] && return 0   # honour an explicit value, including ""
  ssh -G ethz-hyrrent 2>/dev/null | grep -qi '^proxyjump ' && HYRR_ETH_SSH_PREFIX="ethz-hyrr"
  return 0
}
default_ssh_prefix

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
skip_whitelist_check() { # skip_whitelist_check <reason>
  echo "note: whitelist drift check skipped ($1) — '$WHITELIST_FILE' is" >&2
  echo "      authoritative for this deploy and was not verified." >&2
}

check_whitelist_fresh() {
  # Say WHY when skipping. A silent skip is indistinguishable from a passing
  # check, and this guard exists precisely because a stale whitelist deploys
  # silently (#565). A maintainer editing the list by hand on a machine without
  # sops should see that nothing verified it.
  [ -f "$SOPS_ENV_FILE" ] || { skip_whitelist_check "no $SOPS_ENV_FILE"; return 0; }
  command -v sops >/dev/null 2>&1 || { skip_whitelist_check "sops not installed"; return 0; }
  local decrypted
  decrypted=$(sops -d "$SOPS_ENV_FILE" 2>/dev/null) || { skip_whitelist_check "cannot decrypt (no age key?)"; return 0; }
  [ -n "$decrypted" ] || { skip_whitelist_check "decrypted SSoT is empty"; return 0; }

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

# Apache matches <Files> by BASENAME at any depth, so the version.json
# exception below opens EVERY file with that name in the deployed tree — not
# just the one at the root. Today there is exactly one (emitted by the vite
# build); the nuclear-data assets under data/parquet/ contain none. But a future
# data drop shipping its own version.json would be silently un-gated by a gate
# nobody re-read.
#
# So assert the invariant the exception depends on, and fail closed. This is
# the cheap half of a licensing incident.
assert_single_version_file() { # assert_single_version_file <dir>
  local dir="$1" found n
  # -name, not -iname: Apache's <Files> is case-sensitive, so a Version.json
  # would NOT be exposed and must not be reported as a problem.
  found="$(find "$dir" -type f -name "$VERSION_FILE")"
  # `|| n=0`: under `set -euo pipefail` a grep that matches nothing exits 1 and
  # would abort the whole script with a bare exit 1, pre-empting the actual
  # error message below — which is the case that matters most, since "no
  # version.json at all" is exactly when the operator needs to be told why.
  n="$(printf '%s\n' "$found" | grep -c .)" || n=0
  if [ "$n" -ne 1 ] || [ ! -f "${dir}/${VERSION_FILE}" ]; then
    echo "ERROR: expected exactly one '${VERSION_FILE}' at the root of '${dir}'," >&2
    echo "       found ${n}:" >&2
    printf '%s\n' "$found" | sed 's/^/         /' >&2
    echo "" >&2
    echo "       The gate exposes <Files \"${VERSION_FILE}\">, which Apache" >&2
    echo "       matches by basename at ANY depth — so every one of those would" >&2
    echo "       be served WITHOUT an AAI session. Refusing to deploy." >&2
    exit 5
  fi
}

# Every whitelist entry ends up verbatim on a `Require shib-attr` line in the
# generated .htaccess. An embedded CR is the interesting one: Apache's config
# reader treats it as a line boundary on some builds, so a single pasted entry
# with a stray \r could split the directive and change what the gate means.
# Quotes, spaces and NULs are equally unwelcome there.
#
# Whoever can edit the encrypted whitelist is already trusted, so this is not an
# attacker boundary — it is a typo boundary. Pasting an entry out of a mail
# client is exactly how a stray CR gets in, and the failure would be a broken
# or weakened gate rather than an obvious error.
assert_whitelist_entries_sane() {
  local bad=0 entry
  while IFS= read -r entry; do
    [ -n "$entry" ] || continue
    case "$entry" in
      *[![:alnum:]@._%+-]*)
        echo "ERROR: whitelist entry contains characters that are not valid in" >&2
        echo "       an eppn/mail and would be written verbatim into the gate." >&2
        # Never print the entry — it is personal data. Position is enough.
        echo "       Offending entry: line $((bad + 1)) of the non-comment entries." >&2
        bad=$((bad + 1))
        ;;
    esac
  done < <(grep -vE '^[[:space:]]*#|^[[:space:]]*$' "$WHITELIST_FILE" | tr -d '\r')
  if [ "$bad" -ne 0 ]; then
    echo "       Expected characters: letters, digits and @ . _ % + -" >&2
    echo "       Fix the encrypted SSoT with 'sops $SOPS_ENV_FILE'." >&2
    exit 5
  fi
}

# Materialise the whitelist from the encrypted SSoT when it is absent.
#
# The plaintext list is gitignored (the repo is PUBLIC), so a fresh checkout or
# a new worktree has no copy and the deploy refuses. The documented remedy was a
# hand-run sops | grep | cut | base64 pipeline — four places to get it subtly
# wrong, and a wrong result is a WRONG GATE rather than an error: fewer entries
# locks out real users, a mangled entry silently matches nobody.
#
# Deriving it removes that step. This cannot weaken anything: it only ever runs
# when the file is missing, it writes exactly what CI writes from the same
# ciphertext, and if sops cannot decrypt it stays silent and write_gate's
# refusal stands. Never printed — personal data.
materialise_whitelist_from_sops() {
  [ -f "$WHITELIST_FILE" ] && return 0
  [ -f "$SOPS_ENV_FILE" ] || return 0
  command -v sops >/dev/null 2>&1 || return 0
  local decrypted b64
  decrypted=$(sops -d "$SOPS_ENV_FILE" 2>/dev/null) || return 0
  b64=$(printf '%s\n' "$decrypted" | grep '^WHITELIST_B64=' | cut -d= -f2- | tr -d '"'"'"'') || b64=""
  [ -n "$b64" ] || return 0

  mkdir -p "$(dirname "$WHITELIST_FILE")"
  # Via a temp file so a decode failure cannot leave a truncated whitelist that
  # would deploy as a narrower gate than intended.
  local tmp; tmp="$(mktemp)"
  if printf '%s' "$b64" | base64 -d > "$tmp" 2>/dev/null && [ -s "$tmp" ]; then
    chmod 600 "$tmp"
    mv "$tmp" "$WHITELIST_FILE"
    echo "note: materialised '$WHITELIST_FILE' from $SOPS_ENV_FILE ($(grep -vcE '^[[:space:]]*#|^[[:space:]]*$' "$WHITELIST_FILE") entries)."
  else
    rm -f "$tmp"
  fi
}

write_gate() { # write_gate <dir>
  materialise_whitelist_from_sops
  if [ ! -f "$WHITELIST_FILE" ]; then
    echo "ERROR: whitelist '$WHITELIST_FILE' not found — refusing to deploy ungated" >&2
    echo "       (nuclear-data license). Create it (one eppn/mail per line) or" >&2
    echo "       set HYRR_ETH_WHITELIST." >&2
    echo "       It is normally derived automatically from $SOPS_ENV_FILE; that" >&2
    echo "       did not happen, so sops is missing or cannot decrypt here." >&2
    exit 5
  fi
  check_whitelist_fresh
  assert_single_version_file "$1"
  local ids
  ids=$(grep -vE '^[[:space:]]*#|^[[:space:]]*$' "$WHITELIST_FILE" | tr -d '\r' | tr '\n' ' ' | sed 's/[[:space:]]*$//')
  [ -n "$ids" ] || { echo "ERROR: whitelist '$WHITELIST_FILE' is empty" >&2; exit 5; }
  assert_whitelist_entries_sane
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

  # `|| true` runs on the REMOTE side: a missing file makes cat exit 1, and
  # under `set -euo pipefail` that would abort this script at the assignment —
  # before reaching the diagnostic below, which is exactly when the operator
  # most needs it. ssh's OWN failure (unreachable host, auth) still returns
  # non-zero and still aborts, which is correct.
  # shellcheck disable=SC2029  # $docroot is expanded locally, on purpose.
  actual="$(remote "$env" "cat '${docroot}/${VERSION_FILE}' 2>/dev/null || true" | json_field version)"
  if [ "$actual" != "$expected" ]; then
    echo "ERROR: deployed version does not match the bundle just built." >&2
    echo "       expected: ${expected}" >&2
    echo "       on host:  ${actual:-<no readable version.json>}" >&2
    echo "       The rsync reported success, so suspect the docroot: '${docroot}'." >&2
    exit 7
  fi
  echo "    ${VERSION_FILE} = ${actual}  ✓"

  # shellcheck disable=SC2029  # $docroot is expanded locally, on purpose.
  gate="$(remote "$env" "cat '${docroot}/.htaccess' 2>/dev/null || true")"

  # Line-anchored, and it counts the ungating directives rather than merely
  # looking for the gating one. A substring match would be satisfied by the
  # required text appearing in a COMMENT while a second, wider `Require all
  # granted` block sat elsewhere in the file quietly opening the site.
  #
  # The invariant: a session is demanded at top level, and there is exactly one
  # `Require all granted` — the version.json exception — inside exactly one
  # <Files> block naming version.json.
  local n_session n_granted n_files
  n_session="$(printf '%s\n' "$gate" | grep -c '^[[:space:]]*ShibRequestSetting[[:space:]]\+requireSession[[:space:]]\+1[[:space:]]*$')" || n_session=0
  n_granted="$(printf '%s\n' "$gate" | grep -c '^[[:space:]]*Require[[:space:]]\+all[[:space:]]\+granted[[:space:]]*$')" || n_granted=0
  n_files="$(printf '%s\n' "$gate" | grep -c "^[[:space:]]*<Files[[:space:]]\+\"${VERSION_FILE}\">[[:space:]]*$")" || n_files=0

  if [ "$n_session" -lt 1 ]; then
    echo "ERROR: no Shibboleth gate found in '${docroot}/.htaccess' — the" >&2
    echo "       instance may be serving license-restricted nuclear data" >&2
    echo "       UNAUTHENTICATED. Investigate before doing anything else." >&2
    exit 7
  fi
  if [ "$n_granted" -ne "$n_files" ] || [ "$n_granted" -gt 1 ]; then
    echo "ERROR: '${docroot}/.htaccess' grants unauthenticated access in a way" >&2
    echo "       this script did not write: ${n_granted} 'Require all granted'" >&2
    echo "       line(s) against ${n_files} <Files \"${VERSION_FILE}\"> block(s)." >&2
    echo "       Something may be serving restricted data UNAUTHENTICATED." >&2
    exit 7
  fi
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

  # Then from the outside, over plain HTTPS: proves Apache actually serves the
  # new bundle and that the licensing gate is still up. Skippable because it
  # depends on reaching the public internet, unlike verify_remote — which runs
  # over the connection the deploy just used and must never be skippable.
  if [ "${HYRR_SKIP_PUBLIC_VERIFY:-}" = "1" ]; then
    echo "=== Skipping public verification (HYRR_SKIP_PUBLIC_VERIFY=1) ==="
  else
    scripts/verify-deploy.sh "$env"
  fi

  echo "=== Deployed ${env} → ${url} ==="
}

confirm_prd() {
  if [ "${HYRR_ASSUME_YES:-}" = "1" ]; then return 0; fi

  # No TTY is REFUSED, not waved through. This previously read
  #   [ "${HYRR_ASSUME_YES:-}" = "1" ] || [ ! -t 0 ] && return 0
  # so any non-interactive caller — a script, a cron, an agent driving the
  # shell — deployed production with no confirmation at all. Discovered while
  # promoting 0.20.1: the prompt simply never appeared and prd shipped.
  #
  # Absence of a terminal is not consent. HYRR_ASSUME_YES already exists to say
  # "I am non-interactive and I mean it", so requiring it here costs deliberate
  # callers one variable and costs an accidental one a production deploy.
  if [ ! -t 0 ]; then
    echo "ERROR: prd deploy requires confirmation and there is no TTY to ask on." >&2
    echo "       Run it from a terminal, or set HYRR_ASSUME_YES=1 if you are" >&2
    echo "       deliberately deploying production non-interactively." >&2
    echo "       (ent and tst never prompt — only prd does.)" >&2
    exit 1
  fi

  echo "⚠  PRODUCTION deploy → https://hyrr.ethz.ch (user-facing)."
  read -r -p "   Type 'hyrr-prod' to continue: " c
  [ "$c" = "hyrr-prod" ] || { echo "Aborted."; exit 1; }
}

# Production may only be deployed from `main`, exactly as GitHub has it (#648).
#
# Deliberately BEFORE the build: the frontend build rewrites package-lock.json's
# `name` field when the checkout directory is not called "hyrr" (true in every
# git worktree), so asserting cleanliness afterwards would fail for a reason
# that has nothing to do with what is being shipped.
#
# There is NO bypass flag, matching docs/DATA_INTEGRITY.md's reasoning about the
# offline-install path: "an escape hatch is the first thing a frustrated user
# pastes from a forum thread". ent and tst stay unrestricted — they exist to
# deploy work in progress, and gating them would push people back to running
# rsync by hand, which is worse than an ungated staging rung.
assert_prd_is_released_main() {
  local branch dirty local_head remote_head

  branch="$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)"
  if [ "$branch" != "main" ]; then
    echo "ERROR: prd deploys only from 'main' — you are on '$branch'." >&2
    echo "       Production must be traceable to what GitHub calls main; a" >&2
    echo "       feature branch or detached HEAD is not reviewable after the" >&2
    echo "       fact. Deploy it to ent or tst instead." >&2
    exit 1
  fi

  # Tracked modifications only. Untracked files are not shipped by the build,
  # and the whitelist (gitignored) is materialised from sops on every run.
  dirty="$(git status --porcelain --untracked-files=no 2>/dev/null)"
  if [ -n "$dirty" ]; then
    echo "ERROR: prd deploys only from a clean tree; these are uncommitted:" >&2
    while IFS= read -r line; do echo "         $line" >&2; done <<< "$dirty"
    echo "       Whatever you would ship is not on GitHub, so nobody could" >&2
    echo "       reproduce or review the deployed artifact." >&2
    exit 1
  fi

  if ! git fetch --quiet origin main 2>/dev/null; then
    echo "ERROR: cannot reach origin to confirm main is in sync." >&2
    echo "       Refusing rather than assuming: an unverified 'in sync' is" >&2
    echo "       the claim this check exists to make." >&2
    exit 1
  fi

  local_head="$(git rev-parse HEAD)"
  remote_head="$(git rev-parse origin/main)"
  if [ "$local_head" != "$remote_head" ]; then
    echo "ERROR: local main is not in sync with origin/main." >&2
    echo "         local  : $local_head" >&2
    echo "         origin : $remote_head" >&2
    if git merge-base --is-ancestor "$local_head" "$remote_head" 2>/dev/null; then
      echo "       You are BEHIND — pull first ($(git rev-list --count "$local_head..$remote_head") commit(s))." >&2
    elif git merge-base --is-ancestor "$remote_head" "$local_head" 2>/dev/null; then
      echo "       You are AHEAD — push and let CI pass first ($(git rev-list --count "$remote_head..$local_head") commit(s))." >&2
      echo "       Deploying unpushed commits puts code in production that no" >&2
      echo "       gate has run against." >&2
    else
      echo "       The histories have DIVERGED." >&2
    fi
    exit 1
  fi

  echo "=== prd preflight: on main, clean, in sync with origin ($(git rev-parse --short HEAD)) ==="
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
    # State gate runs BEFORE the prompt: refusing an impossible deploy should
    # not cost the operator a confirmation they then watch fail (#648).
    assert_prd_is_released_main
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

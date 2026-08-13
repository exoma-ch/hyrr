#!/usr/bin/env bash
# Bootstrap (or rotate) the sops-encrypted ETH deploy secrets.
#
#   scripts/bootstrap-eth-secrets.sh [--whitelist FILE] [--set-age-secret]
#
# Collects the ETH deploy material and writes secrets/eth-deploy.sops.env,
# age-encrypted to the recipient in .sops.yaml. Each value is base64 (single
# line — private keys survive dotenv + sops round-trips cleanly); the deploy
# workflow decodes them back. Re-run any time to rotate.
#
# Inputs (override via env):
#   HYRR_HEIMDALL_KEY   heimdall bastion private key   (default ~/.ssh/id_ed25519_ethz)
#   HYRR_HYRR_KEY       ETH instance private key       (default ~/.ssh/id_ed25519_eth-hyrr)
#   --whitelist FILE    Shibboleth access list (eppn/mail, one per line). Without
#                       it a placeholder is written and the deploy will refuse to
#                       publish ungated until you fill it in.
#   --set-age-secret    also `gh secret set SOPS_AGE_KEY` from the local age key
#                       (~/.config/sops/age/keys.txt) so CI can decrypt.
#
# Needs: sops, age, ssh-keyscan, base64, (gh for --set-age-secret).
# On NixOS:  nix shell nixpkgs#sops nixpkgs#age --command scripts/bootstrap-eth-secrets.sh …
set -euo pipefail

HEIMDALL_KEY="${HYRR_HEIMDALL_KEY:-$HOME/.ssh/id_ed25519_ethz}"
HYRR_KEY="${HYRR_HYRR_KEY:-$HOME/.ssh/id_ed25519_eth-hyrr}"
AGE_KEY_FILE="${SOPS_AGE_KEY_FILE:-$HOME/.config/sops/age/keys.txt}"
OUT="secrets/eth-deploy.sops.env"
WHITELIST=""
SET_AGE_SECRET=0

while [ $# -gt 0 ]; do
  case "$1" in
    --whitelist) WHITELIST="${2:?--whitelist needs a path}"; shift 2 ;;
    --set-age-secret) SET_AGE_SECRET=1; shift ;;
    -h | --help) sed -n '2,25p' "$0"; exit 0 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

need() { command -v "$1" >/dev/null || { echo "error: '$1' not found (nix shell nixpkgs#sops nixpkgs#age)" >&2; exit 1; }; }
need sops; need ssh-keyscan; need base64
[ -f "$HEIMDALL_KEY" ] || { echo "error: heimdall key not found: $HEIMDALL_KEY" >&2; exit 1; }
[ -f "$HYRR_KEY" ]     || { echo "error: hyrr key not found: $HYRR_KEY" >&2; exit 1; }

echo ":: scanning ETH host keys…" >&2
# The bastion is directly reachable; the instances are ETH-network-only, so scan
# them from the bastion (over the ethz-heimdall alias in your ~/.ssh/config).
kh_bastion="$(ssh-keyscan -T 15 pcposeth5.ethz.ch 2>/dev/null || true)"
[ -n "$kh_bastion" ] || { echo "error: can't reach the bastion pcposeth5.ethz.ch:22" >&2; exit 1; }
kh_inst="$(ssh -o BatchMode=yes ethz-heimdall \
  'ssh-keyscan -T 15 hyrrent.ethz.ch hyrrtst.ethz.ch hyrrprd.ethz.ch' 2>/dev/null || true)"
[ -n "$kh_inst" ] || echo "warning: couldn't scan instance host keys via the bastion — pinning bastion only" >&2
known_hosts="$(printf '%s\n%s\n' "$kh_bastion" "$kh_inst" | grep -vE '^\s*$|^#')"

if [ -n "$WHITELIST" ]; then
  [ -f "$WHITELIST" ] || { echo "error: whitelist file not found: $WHITELIST" >&2; exit 1; }
  whitelist="$(cat "$WHITELIST")"
else
  whitelist="# TODO: add eppn/mail entries, one per line. Fill with:
#   sops ${OUT}   (edit WHITELIST_B64 — it decrypts to plaintext in your editor)
# The deploy REFUSES to publish until this holds at least one real entry."
  echo "warning: no --whitelist — wrote a placeholder; deploy stays gated until filled" >&2
fi

b64() { base64 -w0 2>/dev/null || base64; }  # -w0 (GNU) or plain (BSD)

mkdir -p secrets
umask 077
# Write plaintext to the final path (so sops's secrets/ creation-rule matches),
# then encrypt in place. On any failure, shred the plaintext so it can't be
# committed by accident.
{
  echo "# ETH webhosting deploy secrets — base64 values, age-encrypted via sops."
  echo "# Regenerate/rotate: scripts/bootstrap-eth-secrets.sh"
  echo "HEIMDALL_KEY_B64=$(b64 < "$HEIMDALL_KEY")"
  echo "HYRR_KEY_B64=$(b64 < "$HYRR_KEY")"
  echo "KNOWN_HOSTS_B64=$(printf '%s\n' "$known_hosts" | b64)"
  echo "WHITELIST_B64=$(printf '%s\n' "$whitelist" | b64)"
} > "$OUT"
if ! sops --encrypt --in-place "$OUT"; then
  rm -f "$OUT"
  echo "error: sops encrypt failed — plaintext removed" >&2
  exit 1
fi
echo ":: wrote $OUT (age-encrypted)" >&2

if [ "$SET_AGE_SECRET" = 1 ]; then
  need gh
  [ -f "$AGE_KEY_FILE" ] || { echo "error: age key not found: $AGE_KEY_FILE" >&2; exit 1; }
  # Derive owner/repo from origin (gh errors on multiple remotes without -R).
  repo="$(git config --get remote.origin.url | sed -E 's#.*[:/]([^/]+/[^/.]+)(\.git)?$#\1#')"
  gh secret set SOPS_AGE_KEY --repo "$repo" < "$AGE_KEY_FILE"
  echo ":: set GitHub Actions secret SOPS_AGE_KEY on $repo (from $AGE_KEY_FILE)" >&2
fi

echo ":: done. Commit $OUT + .sops.yaml, then set repo var CI_DEPLOY_ENABLED=true." >&2

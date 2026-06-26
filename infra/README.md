# Infrastructure

## ETH Zürich web hosting — production surface

HYRR is served from three ETH Zürich managed web instances, promoted in a
ladder:

```text
ent ─────deploy─────▶ tst ────(explicit OK)────▶ prd
hyrrent.ethz.ch       hyrrtst.ethz.ch            hyrr.ethz.ch
dev / integration     staging                    PRODUCTION
```

| env | URL                         | SSH user      |
|-----|-----------------------------|---------------|
| ent | <https://hyrrent.ethz.ch>   | `w3_hyrrent`  |
| tst | <https://hyrrtst.ethz.ch>   | `w3_hyrrtst`  |
| prd | **<https://hyrr.ethz.ch>**  | `w3_hyrrprd`  |

Each instance is its own Apache vhost serving at `/`, so the bundle is built
with `VITE_BASE_PATH=/`. (This is separate from the GitHub Pages deploy under
`/hyrr/tst/` + `/hyrr/`, which still runs from `deploy-frontend.yml` /
`promote-to-prod.yml`.)

### Deploying

```sh
just deploy-eth-probe ent     # read-only: discover remote docroot + conf/servers
just deploy-eth-ladder        # build once → ent → tst  (prd held)
just deploy-eth tst           # build + deploy a single instance
just elevate                  # lift the tst-verified artifact → PRODUCTION (confirms)
```

All of these build + `rsync` over SSH and **must run from a host that can reach
ETH SSH (:22)** — a Mac on the ETH VPN / eduroam, where the `w3_hyrr<env>` key
is loaded. GitHub-hosted runners and the anvil vm-dev **cannot** reach ETH SSH
(`No route to host`); see `scripts/deploy-eth.sh` and
`.github/workflows/deploy-eth.yml`.

The remote document root is read from each instance's own `~/conf/servers`
(`DocumentRoot`) — authoritative, never hardcoded. Override with
`HYRR_ETH_DOCROOT` if needed.

### Access gating (nuclear-data licensing)

The nuclear data HYRR serves is license-restricted, so the ETH instances are
**gated behind SWITCH AAI (Shibboleth)** and limited to a whitelist. The gate is
a `.htaccess` (`AuthType shibboleth` + `Require shib-attr principalName|mail …`)
that the deploy **bakes into the bundle** — htdocs runs `AllowOverride
AuthConfig`, so auth directives are allowed (only `AddType`/FileInfo isn't, hence
no wasm MIME — wasm-bindgen falls back arrayBuffer). Baking it in means a
redeploy can't silently un-gate the site.

The whitelist lives in `infra/eth-webhosting/whitelist.txt` (**gitignored** — the
repo is public; emails stay local). One identifier per line, matched against
both `principalName` (eppn) and `mail`. Edit it and redeploy to change access.
Unauthenticated requests 302 → `wayf.switch.ch` (institution picker). Certs use
DNS-01, so gating doesn't disturb renewal.

The public **GitHub Pages** site is a redirect to `hyrr.ethz.ch` (one site to
maintain; no licensed data served publicly) — see `deploy-frontend.yml`.

### CI

`.github/workflows/deploy-eth.yml` exists but is gated behind repo variable
`CI_DEPLOY_ENABLED=true`. Enable it only once CI has a jump-capable SSH path
(self-hosted runner on the ETH network, or a bastion). Required secrets/vars
when enabling: `ETH_SSH_PRIVATE_KEY`, `ETH_SSH_KNOWN_HOSTS`, and optionally
per-environment `ETH_DOCROOT`.

### Provisioning details

The verbatim instance-creation mails and per-instance notes live in
`infra/eth-webhosting/` — **gitignored** (kept local). SSH private keys never
live in the repo; they belong in `~/.ssh` / CI secrets.

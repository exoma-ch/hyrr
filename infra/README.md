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

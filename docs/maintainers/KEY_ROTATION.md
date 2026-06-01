# Updater key management

The desktop app's auto-updater (`tauri-plugin-updater`) verifies every
update bundle against an **ed25519 minisign public key** baked into the
binary at build time. The matching private key signs every release.

This document is the **operational runbook** for that keypair —
generation, storage, routine rotation, and emergency response. Picked
this single-key approach over dual-key / Sigstore-keyless after the
spike captured in [issue #116](https://github.com/exoma-ch/hyrr/issues/116).

> **Read this whole document before you ship the first signed release.**
> Lose the private key → the existing install base is unreachable for
> updates until they manually reinstall a fresh build.

---

## 1. Generating the keypair (one-time setup)

Run this **on a trusted developer machine**, never in CI, never in a
shared shell, never in this repo's clone path.

```bash
# Tauri CLI ships with the signer; install only if you don't have it.
cargo install tauri-cli --version "^2"

# Generate. Choose a strong, memorable passphrase — you'll need it
# every release until rotation.
tauri signer generate -w ~/.tauri-hyrr.key
```

This produces two files:

- `~/.tauri-hyrr.key` — **private** key, encrypted with the passphrase
- `~/.tauri-hyrr.key.pub` — public key

### Storing the private key

The private key MUST live in **at least two locations** before the first
signed release:

1. **GitHub Actions secret** `TAURI_SIGNING_PRIVATE_KEY` — paste the
   *contents* of `~/.tauri-hyrr.key` (the whole base64 blob, including
   `untrusted comment:` header).
   Plus secret `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` with the passphrase.
2. **Offline backup**, encrypted at rest. Recommended:
   - 1Password / Bitwarden vault entry, or
   - `age`-encrypted file checked into a separate private repo, or
   - a hardware security key (YubiKey OpenPGP slot)

If the GH secret is the only copy and the maintainer leaves / the
account is compromised, the entire install base is bricked from
updates. Don't be that team.

### Embedding the public key

Copy the contents of `~/.tauri-hyrr.key.pub` (one line of base64,
**including** the `untrusted comment: minisign public key: …` header
— stripping it silently fails verification, see
[tauri-action#950](https://github.com/tauri-apps/tauri-action/issues/950))
into:

```jsonc
// desktop/src-tauri/tauri.conf.json
"plugins": {
  "updater": {
    "pubkey": "<paste here>",
    ...
  }
}
```

Commit. The next release built on a tag will pick up the new pubkey.

---

## 2. Routine rotation (planned)

Tauri's plugin doesn't support multiple pubkeys natively
([tauri-apps/tauri#7585](https://github.com/tauri-apps/tauri/issues/7585)
is the open feature request). Until that lands, rotation is a
**two-version dance**:

1. Generate a new keypair (`~/.tauri-hyrr-v2.key`) via the same flow.
2. Bump the GH secret values to the new keypair.
   *Do not delete the old key from the offline backup yet.*
3. Cut a release with the old pubkey still in `tauri.conf.json` but
   signed with the new private key — **this will fail verification on
   existing installs**, so don't do it. Instead:
4. Cut a release that *only* changes `tauri.conf.json::pubkey` to the
   new pubkey, still signed with the OLD private key. Existing installs
   accept this, and the binary they then run trusts the new pubkey.
5. Once telemetry / user reports confirm uptake of the v(N+1) build
   carrying the new pubkey, retire the old key from the offline backup
   on the next release.

This dance is not pretty. It is the documented Tauri workaround. Watch
[#7585](https://github.com/tauri-apps/tauri/issues/7585) for native
multi-key support and migrate when available.

---

## 3. Emergency rotation (suspected leak)

If the private key is leaked, exposed in a log, or you have any reason
to believe an attacker has obtained it:

1. **Treat every existing install as un-recoverable through software.**
   An attacker with the key can mint valid updates — there is no
   revocation primitive in minisign / the Tauri updater.
2. Generate a new keypair (`~/.tauri-hyrr-emergency.key`).
3. Update the GH secret to the new key. Disable workflow runs that
   could re-sign with the old key (pause `tauri-build.yml`).
4. Cut a manual installer release (no auto-update path) with:
   - the new pubkey baked in
   - a clear release note: "Mandatory reinstall — security advisory"
5. Email the user list (~100 lab users) with the reinstall link.
6. Open a public security advisory on the repo
   (`Security` tab → `Report a vulnerability`).
7. After confirming all known users have reinstalled, retire the old
   keypair from offline backup and document the incident in
   `docs/maintainers/INCIDENTS.md`.

---

## 4. Maintainer-unreachable scenario

If the current maintainer is no longer available and the offline
backup is also lost, the path forward is the **emergency-rotation
flow** with one extra step at the start: a **fresh installer release
without auto-update** is the only way to bootstrap. This should be
shipped as a regular GH Release with a manual download link in the
README; users reinstall, the new build picks up a new pubkey, normal
operations resume.

The mitigation is procedural, not technical: at least one *other*
person (co-maintainer, principal investigator) should hold the offline
backup. Keep the recipient list in `docs/maintainers/KEY_HOLDERS.md`
(intentionally not in this repo's main flow).

---

## 5. Verification after release

After every release that touches the keypair or the pubkey:

```bash
# Sanity-check the published manifest signature.
curl -sL https://github.com/exoma-ch/hyrr/releases/latest/download/latest.json \
  | jq -r '.platforms["darwin-aarch64"].signature' \
  | base64 -d | xxd | head

# Optional: verify build provenance (separate Sigstore attestation).
gh attestation verify <downloaded-installer> -R exoma-ch/hyrr
```

If either check fails, do not advertise the release until resolved.

---

## 6. References

- [Tauri v2 updater plugin](https://v2.tauri.app/plugin/updater/)
- [Spike resolution on #116](https://github.com/exoma-ch/hyrr/issues/116) — why single-key over dual / Sigstore
- [Native multi-key support tracking](https://github.com/tauri-apps/tauri/issues/7585)
- [Minisign by Frank Denis](https://jedisct1.github.io/minisign/)
- [GitHub Artifact Attestations](https://docs.github.com/en/actions/security-guides/using-artifact-attestations-to-establish-provenance-for-builds) — the Sigstore-keyless layer used in parallel

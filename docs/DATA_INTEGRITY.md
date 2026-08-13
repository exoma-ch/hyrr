# Nuclear-data integrity

How HYRR decides that the nuclear data it computes from is the data it
was meant to get, and what that guarantee does *not* cover.

## What is verified

On first run, `hyrr-core` downloads a `.tar.zst` of nuclear data from a
`nucl-parquet` GitHub Release into `~/.hyrr/nucl-parquet/v{VERSION}/`.
The archive is hashed **as it streams to disk** and compared against a
SHA-256 pinned in this repo before anything is extracted. A mismatch
aborts the install; the partial download is removed and nothing is
promoted into the cache.

The archive is also checked against a **minisign signature** before it is
extracted, so the install answers two different questions:

| control | answers |
|---|---|
| SHA-256 pin | *"are these the bytes this build was tested against?"* |
| signature | *"did these bytes come from the nuclear-data team?"* |

Both must pass. Both stay — they are not redundant, and the same pairing
appears elsewhere for the same reason (Nix keeps fixed-output-derivation
hashes despite signed binary caches; Homebrew keeps bottle SHA-256 alongside
attestations).

Both pins live in `hyrr.json`:

```json
"data_tarball_version": "2026.8.2",
"data_tarball_sha256": "c19cd3fd…",
"data_signing_pubkey": "RWT9cED7yeXhJx…"
```

`core/build.rs` exports them as `DATA_TARBALL_SHA256` / `DATA_SIGNING_PUBKEY`
and **fails the build** if `data_tarball_version` disagrees with the pinned
submodule's `nucl-parquet/data/catalog.json::data_version`, or if the signing
key is missing. A build that quietly stopped checking signatures would be
indistinguishable from one that never did, so removing the check has to be a
deliberate edit rather than an oversight.

### The signature is verified without a second pass

minisign signs a BLAKE2b *prehash*, so the signature is verified
incrementally from the same 64 KiB chunks that already feed the SHA-256 — no
extra download, and ~800 MB is never held in memory. The detached `.minisig`
(~380 bytes) is fetched **first**, so a missing signature or a rotated key
fails in milliseconds instead of after a long download the user then watches
get deleted.

### Why re-pinning is no longer a trust decision

The signature's *trusted comment* carries `sha256=…` and is covered by
minisign's global signature, so that digest is a claim by the key holder.
`just repin-data` now reads the digest from there by default, rather than
from GitHub's published asset digest — which was only ever a convenience,
since upstream releases are mutable (`immutable: false`). A test
(`the_committed_pin_equals_the_publishers_signed_claim`) asserts offline that
what is committed in `hyrr.json` is what the key holder signed, so a drifted
re-pin fails CI rather than a user's first fetch.

`--github-digest` keeps the old behaviour for releases cut before signing
existed; `--verify-download` hashes the bytes yourself.

## Why the pin lives here and not next to the download

An attacker able to tamper with the download can tamper with anything
fetched over the same channel — a `.sha256` sidecar, or the GitHub API.
The pin is only meaningful because it travels **out-of-band**: in git,
reviewed in a pull request, compiled into the binary.

This matters specifically because HYRR intends to honour institutional
TLS trust stores (#578). Doing so means deliberately trusting a
TLS-inspecting middlebox to hand over ~800 MB of nuclear data. The pin is
what makes that trust decision safe: the proxy can re-sign the transport,
but it cannot rewrite a constant compiled into the binary.

## What this is *not*

**It is not a corruption check.** The archive is zstd with the content-
checksum flag set, so the decoder already rejects truncation and bit
errors on its own. The pin exists to catch *substitution*, not damage.

**The trust root is a key, not GitHub.** Verification is against an offline
minisign key held by the nuclear-data team — the same custody model as the
Tauri updater key, and deliberately a *separate* key so compromising one does
not imply the other. A GitHub account compromise does not let an attacker
produce a tarball HYRR will install. Rotation is documented in
`docs/maintainers/KEY_ROTATION.md` and upstream in
`nucl-parquet/docs/security/data-signing.md`.

**It does not cover the offline path.** `install_from_tarball()` accepts
an arbitrary user-supplied file — by design, that is how air-gapped
installs work — and no pin can apply to a bundle the user produced
themselves. This is a real gap, tracked separately, and it is called out
here rather than papered over.

## Failure modes you may see

| Error | Meaning | What to do |
|---|---|---|
| `ChecksumMismatch` | Downloaded bytes are not what this build pinned. | Do not retry blindly. Check whether you are behind a TLS-inspecting proxy, and whether your HYRR build matches the data release you expect. Report it — include both digests from the message. |
| `NoChecksumPin` | Built without the submodule, so no pin was compiled in. | Rebuild with `git submodule update --init --recursive`, or point `HYRR_DATA` at an existing data directory. |
| `SignatureInvalid` | The bytes are not what the nuclear-data team signed. | Treat as hostile until shown otherwise. Do not retry into the same network path. Report it. |
| `SignatureUnavailable` | No `.minisig` could be fetched or parsed. | Either the release predates signing (upstream #289) or the signature was stripped — those look identical from here, so it is refused rather than downgraded to hash-only. |
| `NoSigningKey` | Built without a `data_signing_pubkey`. | Same fix as `NoChecksumPin`. |
| build fails: *"data-tarball pin is stale"* | The submodule was bumped without re-pinning. | `just repin-data` |
| build fails: *"no `data_signing_pubkey`"* | The key pin was removed. | Restore it from `nucl-parquet/docs/security/data-signing-key.pub` (the key line, not the comment line). |
| `just repin-data`: *"signature key id != pinned key id"* | The signing key rotated. | Update `data_signing_pubkey` **deliberately**, after confirming the new key out of band. |

Refusing on an absent pin or key is deliberate. Treating "no pin" as "skip
verification" would silently reinstate the gap this closes, and would do
so precisely on unofficial builds. The same reasoning applies to a missing
signature: a release that lost its `.minisig` and a release that never had
one are indistinguishable from the client, so neither is trusted.

## Re-pinning after a submodule bump

```bash
git submodule update --remote nucl-parquet   # or check out the revision you want
just repin-data                              # rewrites hyrr.json
cargo check --manifest-path core/Cargo.toml  # confirms the guard is satisfied
```

Commit `hyrr.json` **in the same commit as the submodule bump** — they are
one logical change, and the build guard exists to stop them separating.

`just repin-data` reads the SHA-256 GitHub records for the release asset.
That is a convenience, not a security control: the same party who could
replace the asset could serve a matching digest, and upstream releases are
mutable unless Immutable Releases is enabled. What makes the pin
trustworthy is that it lands in git and gets reviewed. If you are pinning
a release you did not cut yourself, use:

```bash
just repin-data --verify-download   # streams the full asset and hashes it
```

## Air-gapped installs

Where the network path can't be used at all, skip it:

```bash
# on a connected machine
hyrr fetch-data --offline-bundle hyrr-data.tar.zst

# on the isolated machine
hyrr fetch-data --install-tarball hyrr-data.tar.zst
# or point at an existing tree
export HYRR_DATA=/srv/nuclear-data
```

Transfer integrity is the operator's responsibility here — verify the
bundle out-of-band (checksum it on both ends) as you would any other
media crossing an air gap.

## Follow-ups

- Signing the data tarball (publisher authenticity, and the only thing
  that can also cover the offline bundle)
- Recording the verified hash in simulation outputs, so a published
  result can be tied to the exact data that produced it
- Enabling Immutable Releases + artifact attestations upstream

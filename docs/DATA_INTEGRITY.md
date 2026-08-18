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

**It is not a guarantee that survives a repacking gateway.** See below.

## Air-gapped installs (#614)

Isolated control networks are normal at accelerator and hospital sites, so the
offline path is not an edge case — for some users it is the only path. It is
verified exactly like the network path.

```bash
# on a CONNECTED machine — downloads the signed release AND its signature
hyrr fetch-data --offline-bundle /media/usb/hyrr-data.tar.zst

# copy BOTH files across; then on the ISOLATED machine
hyrr fetch-data --from /media/usb/hyrr-data.tar.zst
#   ...or, if the signature travelled separately:
hyrr fetch-data --from ./hyrr-data.tar.zst --signature /other/media/hyrr-data.tar.zst.minisig
```

`--offline-bundle` downloads the upstream signed artefact rather than repacking
your local cache. That is not an implementation detail — it is the only way the
isolated machine can verify anything:

- a repack is byte-different from the signed archive (tar ordering, mtimes,
  compression level), so upstream's signature cannot cover it;
- you hold no signing key, so you cannot sign a replacement;
- a manifest signed by nobody, inside the bundle it describes, is rewritable by
  anyone who can rewrite the files — it would read as a control while being
  none.

**An unsigned bundle is refused, not warned about.** A stripped signature and a
release that predates signing are indistinguishable from the client, and a
warning on an isolated network becomes "press enter" within a week. There is
deliberately **no bypass flag**: an escape hatch is the first thing a frustrated
user pastes from a forum thread.

**A bundle for a different data version is refused.** The cache layout is keyed
on the version this build pins, so a foreign bundle would land in the wrong
directory — and accepting an older, still-validly-signed release is exactly the
rollback that a signature alone does not prevent.

### Known limitation: content-scanning gateways

Content Disarm & Reconstruction appliances (OPSWAT MetaDefender, Deep CDR and
similar) are standard at regulated sites. They open an archive in transit, scan
each entry and **repack** it. The nuclear data arrives intact; the signature
does not verify, because the bytes are no longer the bytes that were signed.

HYRR handles this with a **second verification route**. If a signed content
manifest is carried alongside the archive, and the byte-signature fails, the
archive is extracted to a staging directory and every file is checked against
the manifest before anything is promoted. That authenticates *contents* rather
than framing, so it survives a repack.

Copy all four release assets if your route may repack, keeping their published
names — the manifest replaces `.tar.zst` rather than being appended to it:

```text
nucl-parquet-data-2026.8.3.tar.zst                   the archive
nucl-parquet-data-2026.8.3.tar.zst.minisig           its signature
nucl-parquet-data-2026.8.3.manifest.json             the content manifest
nucl-parquet-data-2026.8.3.manifest.json.minisig     its signature
```

Put all four in one directory and point `install_from_tarball` at the archive;
the rest are found by name. Renaming the archive is fine — a manifest named
`<your-name>.manifest.json` beside it is also accepted — but renaming only
*some* of the four will leave the manifest unfound, and an unfound manifest
reads as "no second route available" rather than as an error (see below).

Three rules make this a real control rather than a comforting one:

- **The byte-signature is tried first.** It covers framing as well as contents,
  so falling straight to the manifest would be strictly weaker — a tampered
  `.tar.zst` that happens to extract to the right files would pass.
- **Verification runs in both directions.** Every listed file must be present
  and match, *and* every present file must be listed. A one-directional check
  (`sha256sum -c`) passes a tree with a planted extra file — upstream found
  exactly that inside their own implementation before release, and the threat
  model here is a gateway that can write into the tree.
- **An unsigned manifest is refused.** Whoever can rewrite the files can
  rewrite an unsigned manifest describing them.

Manifests are published from data version **2026.8.3** onward. Earlier releases
have none, and absence is treated as "no second route available", not as a
failure — so a bundle predating them still installs on the byte-signature path.

A CDR appliance that rewrites file *contents* — re-encoding a format it thinks
it understands — fails the manifest check too, correctly. This turns
"unverifiable" into "verifiably modified", which is the right outcome but not
the same as "works everywhere".

### Mirroring an existing cache

`repack_cache_unverified` still exists for copying an installed cache to a
second isolated machine where no signed upstream artefact is reachable. The
result carries **no signature and cannot be given one**, so `--from` will refuse
it; point `HYRR_DATA` at the extracted tree instead. Its integrity then rests on
the transfer medium — which is the honest description, and the reason it is a
separate function rather than a mode of the verified export.

### What an installed cache remembers

A verified install writes a `.verified` sidecar next to `.complete`, recording
the signing key and data version. A cache installed before this existed reads as
**complete but not verified**, and reads are deliberately *not* gated on it —
doing so would brick every existing offline install at upgrade time, which is
what pushes operators to `cp -r` the cache across a diode with no verification
at all.

Note this records *provenance*, not a re-check: nothing re-hashes the tree after
extraction, because upstream signs the archive rather than its contents, so
there is nothing authenticated left to compare an extracted file against. That
is the same gap #296 closes.

## What a result records about its data (#593)

The pin above protects *one download*. It says nothing to someone reading a
paper three years from now who needs to know which nuclear data produced the
numbers. So every simulation result carries a `provenance` block:

```json
"provenance": {
  "hyrr_version": "0.18.0",
  "data_version": "2026.8.1",
  "library": "tendl-2023-iso",
  "data_tarball_sha256": "a5ce01b8…",
  "data_source": "verified-tarball"
}
```

`data_source` is the field that makes the others readable. A missing
`data_tarball_sha256` is not one fact but four different ones, and they call
for opposite reactions:

| `data_source` | Means | Hash? |
|---|---|---|
| `verified-tarball` | Native, read from the managed cache — data installed from the pinned tarball after the SHA-256 check above | yes |
| `local-directory` | Native, but `--data-dir` / `HYRR_DATA` / the submodule / a sibling checkout. Real data, not data *this* process verified | no — inapplicable |
| `browser-http` | Browser (WASM): Parquet fetched per file via hyparquet. A tarball never exists on this path | no — impossible |
| `unknown` | The result predates this feature | no — unattributable |

Two deliberate choices:

- **The hash is recorded only for `verified-tarball`.** On any other origin
  the compiled-in pin describes a tarball the run did not read, so emitting
  it would assert something untrue.
- **A pre-existing result is *not* back-filled** from the running build's
  constants. Those describe the current binary, not the one that produced the
  file; stamping them on would manufacture exactly the false attribution this
  feature exists to prevent. It reads `unknown` instead.

In JSON the key is always present, with an explicit `null` next to the
`data_source` that explains it. Parquet key-value metadata is string-only, so
there the `hyrr.data_tarball_sha256` key is *omitted* rather than written
empty — but `hyrr.data_source` is always present, so the absence is never
ambiguous.

**This is provenance, not proof.** A self-reported hash inside a result is a
record, not an attestation — whoever holds the file can edit it. It exists so
an honest reconstruction is possible without re-running anything, and it
survives cache eviction, a re-cut release, and submodule drift. Tamper-evidence
would need a signature, which is the follow-up below.

The browser's inability to report a hash is a genuine limitation of that
surface, stated in the schema rather than hidden: `data_version` is the
strongest identifier a browser run can offer.

## Failure modes you may see

| Error | Meaning | What to do |
|---|---|---|
| `ChecksumMismatch` | Downloaded bytes are not what this build pinned. | Do not retry blindly. Check whether you are behind a TLS-inspecting proxy, and whether your HYRR build matches the data release you expect. Report it — include both digests from the message. |
| `NoChecksumPin` | Built without the submodule, so no pin was compiled in. | Rebuild with `git submodule update --init --recursive`, or point `HYRR_DATA` at an existing data directory. |
| `SignatureInvalid` | The bytes are not what the nuclear-data team signed. | Treat as hostile until shown otherwise. Do not retry into the same network path. Report it. |
| `SignatureUnavailable` | No `.minisig` could be fetched or parsed. | Either the release predates signing (upstream #289) or the signature was stripped — those look identical from here, so it is refused rather than downgraded to hash-only. |
| `NoSigningKey` | Built without a `data_signing_pubkey`. | Same fix as `NoChecksumPin`. |
| `VersionMismatch` | The bundle is validly signed but for a different data release than this build pins. | Use a bundle matching the version named in the message, or move HYRR to the version that ships it. Not a bypassable condition — the cache layout is keyed on that version. |
| build fails: *"the pinned data-signing key does not match upstream"* | `hyrr.json`'s key disagrees with the pinned submodule's published key. | If upstream rotated, confirm the new key **out of band** and update deliberately. Otherwise the key was edited without a matching submodule bump. Do not silence this — it is what stops the trust root being swapped in one PR. |
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

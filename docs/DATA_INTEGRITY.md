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

The pin lives in `hyrr.json`:

```json
"data_tarball_version": "2026.8.1",
"data_tarball_sha256": "a5ce01b8…"
```

`core/build.rs` exports it as `DATA_TARBALL_SHA256` and **fails the build**
if `data_tarball_version` disagrees with the pinned submodule's
`nucl-parquet/data/catalog.json::data_version`.

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

**It is not publisher authentication.** A SHA-256 says "these are the
bytes we pinned", not "these bytes came from the nuclear-data team". A
signature would say the latter. See the follow-up issues below.

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
| build fails: *"data-tarball pin is stale"* | The submodule was bumped without re-pinning. | `just repin-data` |

Refusing on an absent pin is deliberate. Treating "no pin" as "skip
verification" would silently reinstate the gap this closes, and would do
so precisely on unofficial builds.

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

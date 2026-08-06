# ADR 0005 — TLS backend stays rustls+ring; aarch64 wheels build on native ARM runners

- **Status**: accepted
- **Date**: 2026-08-06
- **Implements**: #573 (spike), closes #461
- **Scope**: `core/Cargo.toml` TLS dependency, `.github/workflows/release-hyrr-mcp.yml`

## Context

`release-hyrr-mcp.yml` builds the `hyrr-mcp` PyPI wheel on a four-target
matrix. The `linux-aarch64` leg had been failing since at least
`hyrr-mcp-v0.12.0`, always the same way:

```text
Using ghcr.io/rust-cross/manylinux2014-cross:aarch64 Docker image
warning: ring@0.17.14: include/ring-core/asm_base.h:73:2:
         error: #error "ARM assembler must define __ARM_ARCH"
warning: ring@0.17.14: ToolExecError: ... "aarch64-unknown-linux-gnu-gcc" ...
         "-c" ".../pregenerated/chacha-armv8-linux64.S"
```

`ring` arrives through the only network call in the product: `hyrr-core`'s
lazy nuclear-data download (`core/src/data_fetch.rs`), which uses
`reqwest` → `rustls` → `ring` to fetch one tarball from GitHub Releases.

Two things made this worse than a normal red build.

First, the leg carried `continue-on-error: ${{ matrix.target.name ==
'linux-aarch64' }}`. A `continue-on-error` matrix leg is reported to
downstream `needs:` as a **success**, so `parity-test` and `publish` ran
anyway. Releases 0.16.3, 0.17.0 and 0.18.0 all published to PyPI with a
green checkmark and no aarch64 Linux wheel. Nobody noticed for four
releases.

Second, the failure looked like an architecture problem and was not one.
`aarch64-apple-darwin` builds `ring` on every release without incident in
both this workflow (`macos-14`) and `tauri-build.yml` (`macos-latest`).
The single leg that failed was the single leg that **cross-compiled**.

The obvious reading — "`ring`'s hand-written ARM assembly is the problem,
swap the TLS backend" — was the proposal this ADR was opened to test.

## Decision

### 1. Keep `rustls` + `ring`. Do not switch the TLS backend

The failure is a **cross-toolchain defect**, not a `ring` defect. `ring`
guards its pregenerated ARMv8 assembly on `__ARM_ARCH` (`asm_base.h`). A
properly configured aarch64 GCC predefines that macro whether or not it is
a cross-compiler; the crosstool-ng build inside
`ghcr.io/rust-cross/manylinux2014-cross:aarch64` does not, because it was
configured without a reliable default `--with-arch`. Filed upstream twice
(ring#1728, ring#1789, maturin-action#245). The intermittency across
releases is explained by `:aarch64` being a **floating tag** that
maturin-action re-pulls on every run — each release got whatever image
snapshot existed that day.

Remove the cross-compiler and the failure class is gone. Nothing about the
dependency needs to change.

### 2. Build the aarch64 wheel on a native ARM runner

`PyO3/maturin-action` selects its build container from the **host**
architecture. From the exact commit `@v1` resolves to (`e83996d`,
`src/index.ts`):

| host | target | `manylinux: auto/2014` resolves to |
|---|---|---|
| `x64` | `aarch64-unknown-linux-gnu` | `ghcr.io/rust-cross/manylinux2014-cross:aarch64` |
| `arm64` | `aarch64-unknown-linux-gnu` | `quay.io/pypa/manylinux2014_aarch64` |

So moving that one leg to `ubuntu-24.04-arm` swaps the unmaintained cross
image for the official PyPA native one. No cross-GCC, no QEMU, no
`__ARM_ARCH`. `ubuntu-24.04-arm` is GA and free/unlimited on public repos,
and `exoma-ch/hyrr` is public — no cost, no self-hosted infrastructure.

### 3. A leg that does not ship must go red

`continue-on-error` is deleted, and two independent guards are added
because the original hole was not one bug but three interacting ones
(`continue-on-error` laundering failure into success, `download-artifact`
with no `name:` succeeding on a partial set, and `parity-test` only ever
exercising the x86_64 wheel):

- a per-leg **wheel-tag assertion** — each matrix leg declares the platform
  tag it must produce and fails if it produced anything else, so a leg that
  silently emits a wrong-tagged wheel cannot pass;
- a **`verify-artifacts`** job that `publish` depends on, asserting the
  exact expected artifact set (four wheels with named platform tags, plus
  one sdist) before anything reaches PyPI;
- a **`smoke-test-aarch64`** job that installs the real aarch64 wheel on a
  real arm64 runner and replays the first line of the canonical parity
  fixture, so the artifact is proven to *run*, not merely to exist.

### 4. Commit `py-mcp/Cargo.lock`

`py-mcp/` is the crate published to PyPI, and it had `Cargo.lock` in its
`.gitignore` — so every release re-resolved its entire dependency graph
against crates.io. The lock was **seeded from `core/Cargo.lock`** rather
than freshly generated: a fresh resolve drifted 88 packages from what the
rest of the tree pins, including major bumps (`syn` 2→3, `shlex` 1→2) that
would have landed in a published wheel with no review. Seeded, the shared
subgraph matches `core` exactly and only pyo3 and its build deps are added.

## Alternatives considered

| Option | Why it lost |
|---|---|
| **`rustls` + `aws-lc-rs`** | The reflexive answer, and wrong. It trades `ring`'s assembly for aws-lc's perlasm-generated assembly plus a cmake/bindgen build requirement on non-prebuilt targets — *more* cross-compile surface, not less. It also took four RUSTSEC advisories in 2026 (AES-CCM side channel; PKCS7 chain and signature bypass; CRL DP scope) against `ring` 0.17.x's clean record. Being the newer rustls default is not by itself a reason. |
| **`native-tls` / OpenSSL** | Best OS-trust-store story, worst everything else: kills the single-static-wheel model, needs OpenSSL headers per target, and manylinux2014's ancient OpenSSL is a compatibility trap. |
| **`CFLAGS_aarch64_unknown_linux_gnu=-D__ARM_ARCH=8`** | Actually works, and is not really a hack — it supplies the value the compiler should have predefined, and `8` is correct for the armv8-a baseline (`ring` gates anything above it on `__ARM_FEATURE_*`). Rejected because it *keeps* us on the unmaintained cross image, whose next rebuild can break differently. It fixes this symptom and preserves the failure class. |
| **Drop HTTPS from core / shell out to `curl`** | Makes the download unauditable and non-portable (Windows, Tauri sandbox), and loses the streaming progress callback that drives the desktop splash bar. Trading a working in-process download for a subprocess is a downgrade. |
| **Pin the cross image to a known-good digest** | Freezes today's luck. Still a cross-compile, still the same toolchain, and now also frozen against upstream fixes. |
| **Drop the aarch64 leg entirely** | This is the platform of every `linux/arm64` container on an Apple-Silicon Mac — Docker Desktop's default — plus Graviton/Ampere cloud. Dropping it makes `uvx hyrr-mcp` fall back to an sdist build needing a Rust toolchain. |
| **`reqwest` → `ureq`** | Genuinely attractive: sheds ~50 crates and the async runtime for what is one blocking GET. Deferred, not rejected — it is unrelated to this failure and carries real behavioural risk (GitHub Releases 302-redirects to a signed S3 URL on another host; the progress callback feeds the desktop splash). Tracked separately. |
| **FIPS-validated crypto (`aws-lc-fips-sys`)** | Declined. One HTTPS GET of a *public* asset touches no FIPS boundary — no key material, no protected data. Compliance theatre with real friction and no assurance gain in this threat model. |

## Consequences

**Good.** The aarch64 wheel ships again, and the whole cross-compilation
toolchain leaves the Linux side of the pipeline. A partial release now
fails loudly at three independent points. The published wheel's dependency
graph is reproducible and matches what CI tests. No dependency churn, so
the release that proves the fix is not also carrying a TLS change.

**Neutral.** The aarch64 leg's build time is now native rather than
cross — expected to be comparable or faster, but on a different runner
pool.

**Deliberately left open.** Two real defects surfaced during this spike
that are *not* fixed here, because they are separate failure classes with
their own blast radius:

1. **The download is not integrity-verified.** There is no checksum or
   signature on the ~400 MB data tarball. `FetchStage::Verifying` is a
   structural guard (`entries_seen > 0 && files_written == 0`), not a
   cryptographic one. The integrity story today is "trust TLS and trust
   GitHub Releases". For data feeding radioisotope-production
   calculations this is the highest-value remaining fix.
2. **The OS trust store is ignored.** `rustls-tls` means
   `rustls-tls-webpki-roots` — a Mozilla root store compiled into the
   binary (`webpki-roots` present in every lockfile, `rustls-native-certs`
   absent). Behind a TLS-inspecting institutional proxy the download fails
   and the user has no remedy: installing the corporate CA into the OS
   store has no effect, and unlike `pip`/`uv` we honour no
   `SSL_CERT_FILE`-style override. The fix is *not* bare
   `rustls-tls-native-roots` — that is worse on macOS/Windows (skips the
   platform verifier IT actually pushes CAs into) and yields a silently
   empty root store on minimal containers with no `ca-certificates`. The
   right shape is `rustls-platform-verifier` with a `webpki-roots`
   fallback, plus an explicit PEM override.

Both are filed as follow-ups against this ADR.

## References

- Issue #461 — originating failure report
- Issue #573 — the spike issue, with verification receipts
- ring#1728, ring#1789, PyO3/maturin-action#245 — upstream reports
- `PyO3/maturin-action@v1` → `e83996d`, `src/index.ts` `DEFAULT_CONTAINERS`
- PEP 599 — manylinux2014 (glibc 2.17), the lowest floor supporting aarch64
- GitHub Actions changelog — arm64 hosted runners GA for public repos
- `core/src/data_fetch.rs` — the sole TLS consumer

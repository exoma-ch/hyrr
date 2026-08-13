# ADR 0006 — HTTP client becomes `ureq`; TLS roots come from the OS trust store

- **Status**: accepted
- **Date**: 2026-08-06
- **Implements**: closes #578, closes #579
- **Scope**: `core/Cargo.toml`, `core/src/data_fetch.rs`, `core/src/update_check.rs`
- **Relationship to ADR 0005**: **extends, does not reverse.** ADR 0005 decided
  the *TLS backend* is `rustls` + `ring`. That is unchanged — `ring` is still
  the crypto provider and `aws-lc-rs` is still absent. This ADR changes the
  *HTTP client layered above it*, and where the trust roots come from.

## Context

Two issues turned out to be one decision.

**#578 — the OS trust store is ignored.** `core/Cargo.toml` requested
`reqwest = { features = ["rustls-tls"] }`, which in reqwest 0.12 resolves to
`rustls-tls-webpki-roots`: Mozilla's root list **compiled into the binary**.
Confirmed across every committed lockfile — `webpki-roots` present,
`rustls-native-certs` absent.

The consequence is a support dead end at exactly the institutions HYRR targets.
Accelerator and hospital networks routinely run TLS-inspecting proxies. When IT
says *"install our CA into the system trust store"* — the standard remedy — it
does nothing, because HYRR never reads that store. The user has no way to fix
their own install.

**#579 — 70 crates for one blocking GET.** The product's entire network surface
is a single synchronous streamed download of a release tarball. No auth, no
POST, no client certs, no connection reuse, no HTTP/2, no concurrency.
`reqwest::blocking` spins up a Tokio runtime internally to service a call that
is synchronous by construction.

These are coupled: both rewrite `build_http_client`, and the trust-root choice
is a feature flag on whichever client wins. Landing #578 on reqwest and then
issue #579 on ureq would mean doing the trust-store work — and re-validating it
across six independent lockfiles — twice.

## Decision

### 1. Replace `reqwest` with `ureq` 3.3

```toml
ureq = { version = "3", default-features = false,
         features = ["rustls", "platform-verifier", "gzip"] }
```

The gating unknown was redirect behaviour: GitHub Releases answers with a 302
to a signed S3 URL **on a different host**, and cross-host signed-URL handling
is what HTTP clients get subtly wrong. Verified against the real
`data-2026.8.1` asset before any production code was written:

```text
status            : 200 OK
content-length hdr: Some("785024730")
streamed          : 3159861 bytes in 226 chunks
zstd magic        : [28, B5, 2F, FD]
```

The body is a plain `Read`, so the existing 64 KiB chunk loop, progress
callback and streaming SHA-256 are driven by us and carry over unchanged.

**ADR 0005 is preserved, not reversed.** ureq's `rustls` feature expands to
`rustls-no-provider` + `_ring` + `rustls-webpki-roots`, so `rustls::crypto::ring`
is installed when no process-wide provider is set. `aws-lc-rs` — and with it
`cmake`, and with that the aarch64 wheel cross-build failure ADR 0005 exists to
prevent — never enters the graph.

Measured across all six lockfiles:

| lockfile | before | after | Δ |
|---|---|---|---|
| `core` | 262 | 214 | **−48** |
| `py` | 264 | 217 | −47 |
| `py-mcp` | 244 | 196 | −48 |
| `hyrr-mcp` | 252 | 206 | −46 |
| `wasm` | 192 | 146 | −46 |
| `desktop/src-tauri` | 655 | 645 | −10 |

`tokio`, `hyper`, `hyper-rustls`, `mio`, `tower` and `h2` leave every graph.
They remain in `desktop/src-tauri` only because Tauri's own updater plugin
depends on them — that is Tauri's tree, not ours.

### 2. Trust roots come from the OS store by default

`RootCerts::PlatformVerifier` → `rustls-platform-verifier`. This is the
substance of #578: "install our CA into the system trust store" becomes an
effective remedy.

### 3. An explicit PEM override, checked in order

`HYRR_CACERT`, then `SSL_CERT_FILE`. Ours wins so a user can override a
site-wide setting without unsetting it; `SSL_CERT_FILE` is honoured because
curl, Python and Go already do, so an institution that has exported it once
gets HYRR working for free.

The override is needed *in addition to* the platform verifier because the
verifier only consults `SSL_CERT_FILE` on Linux/BSD — on macOS and Windows it
goes to the native APIs. The explicit variable is what makes the escape hatch
behave identically on all three.

An override **replaces** the platform roots rather than merging with them. An
operator pointing HYRR at a specific bundle is stating what they trust;
silently unioning the system store back in would defeat that.

### 4. Fail closed

A bundle that exists but yields zero usable certificates is an error, never a
silent fall-back to the platform or bundled roots. A typo'd `SSL_CERT_FILE`
must not quietly downgrade a deliberate trust decision — that is precisely the
failure a trust-store feature exists to prevent. `FetchError::TlsTrustStore` is
a distinct variant from `Network` because the remedy differs: this is local
configuration the user can fix, not a transport failure to retry. Its wire
payload carries the env-var names so a Tauri recovery card can name the escape
hatch without hard-coding it.

## Pitfalls recorded

- **`RootCerts::PlatformVerifier` without the `platform-verifier` feature is a
  runtime `panic!`, not a compile error** (ureq `tls/rustls.rs:183`).
  `RootCerts::WebPki` panics the same way when webpki is disabled — which is
  why `rustls-webpki-roots` is deliberately left enabled. There is no clean
  compile-time guard available from `core`: `rustls-platform-verifier` is
  ureq's dependency, not ours, and the panic arm is only reached on a real
  connection. The protection is the pinned feature list in `core/Cargo.toml`
  plus its comment, backed by a unit test that pins the selected arm and an
  opt-in live test that performs a real handshake.
- **An empty env var must read as unset.** `SSL_CERT_FILE=` in a shell profile
  would otherwise hard-fail every fetch.

## Alternatives considered

**reqwest 0.13 + `rustls-no-provider` + a guarded `ring` install + merged
roots.** This was the design originally recorded on #578, and it works. It
loses on dependency surface (~70 crates vs ~36), keeps an async runtime for one
blocking call, and leaves #579 open. Rejected once ureq was shown to express
the same trust-root policy natively.

**reqwest 0.12 with `rustls-tls-native-roots`.** Smallest possible diff, but it
switches to `rustls-native-certs`, which reads a *snapshot* of the system store
rather than delegating to the platform verifier — it misses Windows/macOS
policy semantics, and still leaves #579 open.

**Merging the override into the platform roots instead of replacing them.**
Friendlier on a misconfiguration, but it makes "what does this binary trust?"
unanswerable from the configuration alone. Rejected in favour of fail-closed.

**Dropping HTTPS from core and making the fetch the caller's problem.** Would
have removed the dependency entirely, but every surface (CLI, MCP, desktop)
needs the same fetch, so it relocates the problem and multiplies it.

## Consequences

- Institutional users behind a TLS-inspecting proxy can install HYRR. This was
  previously impossible without patching the binary.
- ~48 fewer crates to audit in the crates that ship to users; the advisory and
  licence surface shrinks accordingly (`cargo-deny`, #580).
- `update_check` shares the trust policy, so a CA configured once works for
  both. It stays fail-silent on a trust-store misconfiguration — an update
  check must never surface an error, and the data fetch is where the user gets
  a real diagnostic.
- One new public constant, `data_fetch::CACERT_ENV_VARS`, so UIs can name the
  escape hatch without duplicating strings.

## References

- Issue #578 — honour the OS trust store
- Issue #579 — ureq instead of reqwest
- ADR 0005 — rustls + ring (preserved by this ADR)
- Issue #573 — the spike that raised both
- `docs/DATA_INTEGRITY.md` — TLS is no longer the only integrity control (#577)

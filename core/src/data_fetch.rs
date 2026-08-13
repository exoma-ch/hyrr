//! Lazy fetch + on-disk cache for nucl-parquet data tarballs.
//!
//! Populates `~/.hyrr/nucl-parquet/v{DATA_VERSION}/` from GitHub Releases on
//! demand. Hardened against the known failure modes of the simpler upstream
//! pattern (see #52 spike review).
//!
//! Wire-format note (nucl-parquet PR #151 — "Path A"): the upstream
//! release tag is `data-{V}` (CalVer YYYY.MM.MICRO, e.g.
//! `data-2026.5.0`) and the tarball asset is
//! `nucl-parquet-data-{V}.tar.zst` — note no leading `v` on either,
//! since the data version is no longer SemVer. The local cache
//! directory keeps its `v{V}/` prefix (`~/.hyrr/nucl-parquet/v2026.5.0/`)
//! intentionally: it's our internal layout, not an upstream-naming
//! contract, and the `v` prefix is what the cache-dir parser
//! ([`parse_version_dir`]) and prune logic key off. Changing the
//! cache layout would invalidate every user's cache for no gain.
//!
//! - **Concurrent invocations**: an exclusive `fs2` file-lock on
//!   `<cache_root>/.lock` serialises competing extract attempts, so the
//!   Tauri GUI thread and a `--mcp` thread launched simultaneously can't
//!   stomp on each other's output.
//! - **Partial extracts**: tar contents are written to
//!   `<cache_root>/v{V}.partial-{pid}/`, then atomically renamed to
//!   `<cache_root>/v{V}/`. A `.complete` sentinel is written *last*; the
//!   resolver checks for that sentinel rather than relying on the existence
//!   of `data/meta/`. A network drop or disk-full leaves the partial dir
//!   behind for cleanup, but never a half-populated final dir.
//! - **Wrong-version cache from previous installs**: each version has its
//!   own `v{V}/` dir, so a v0.9.0 install does not interfere with a future
//!   v0.11.0 install.
//!
//! Gated `#[cfg(not(target_arch = "wasm32"))]` — WASM consumers don't have
//! a filesystem and use a different data-loading path entirely.

#![cfg(not(target_arch = "wasm32"))]

use std::fs;
use std::io;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use fs2::FileExt;
use serde::Serialize;

/// Version of the `nucl-parquet` data this build expects.
///
/// Sourced at build time from `nucl-parquet/pyproject.toml` by
/// `core/build.rs` — the submodule pin is the single source of truth.
/// On a fresh clone without `--recurse-submodules` this falls back to
/// `"0.0.0-unknown"` (a deliberately invalid version that 404s loudly
/// at fetch time rather than serving stale data).
pub const DATA_VERSION: &str = env!("HYRR_DATA_VERSION");

/// SHA-256 of the release tarball for [`DATA_VERSION`], pinned in
/// `hyrr.json` and validated against the submodule at build time
/// (`core/build.rs::emit_data_tarball_pin`).
///
/// Empty iff the submodule wasn't checked out, in which case
/// [`DATA_VERSION`] is the `0.0.0-unknown` sentinel too and no real
/// release URL exists. [`verify_tarball_sha256`] treats empty as
/// *refuse*, never as *skip*.
///
/// **What this does and does not buy.** The archive is zstd with the
/// content-checksum flag set, so the decoder already rejects truncation
/// and corruption on its own — this pin is not a corruption check. It
/// is an *authenticity* check: it binds this build to exactly the bytes
/// it was tested against, so a substituted payload is refused even when
/// the transport is trusted. That case is not hypothetical: honouring
/// an institution's TLS-inspecting proxy (#578) means deliberately
/// trusting a middlebox to hand us ~800 MB of nuclear data, and this is
/// what makes that trust decision safe.
pub const DATA_TARBALL_SHA256: &str = env!("HYRR_DATA_TARBALL_SHA256");

/// minisign public key the data tarball's signature is checked against (#594).
///
/// This is the *publisher authenticity* control, and it answers a different
/// question from [`DATA_TARBALL_SHA256`]:
///
/// * the **pin** says "these are the bytes this build was tested against"
/// * the **signature** says "these bytes came from the nuclear-data team"
///
/// Only the second survives a re-pin. `just repin-data` reads GitHub's
/// published asset digest, and upstream releases are *mutable*
/// (`immutable: false`), so that digest is a convenience rather than a
/// control. The signature moves the trust root off the release host entirely
/// and onto an offline key — the same custody model as the Tauri updater key,
/// deliberately a *separate* key so a compromise of one does not imply the
/// other.
///
/// Empty only on a build with no `nucl-parquet` submodule, which also has no
/// checksum pin and refuses to install anything at all.
pub const DATA_SIGNING_PUBKEY: &str = env!("HYRR_DATA_SIGNING_PUBKEY");

/// GitHub Releases base URL for nucl-parquet data tarballs.
///
/// This is the SSoT for the release host. All other call sites (Tauri
/// commands, frontend, docs) MUST flow through [`release_base_url`] /
/// [`release_url`] / [`release_url_for`] rather than re-spelling the
/// string.
pub const RELEASE_BASE: &str = "https://github.com/exoma-ch/nucl-parquet/releases/download";

/// Canonical GitHub-Releases base URL. Prefer this over the `RELEASE_BASE`
/// const when crossing a module/crate/FFI boundary — it pins downstream
/// callers on a function rather than on the literal.
pub fn release_base_url() -> &'static str {
    RELEASE_BASE
}

/// Canonical [`DATA_VERSION`] accessor. Same SSoT motivation as
/// [`release_base_url`] — function-shaped so non-Rust callers (Tauri,
/// pyo3, MCP) can re-export without reaching into a `pub const`.
pub fn data_version() -> &'static str {
    DATA_VERSION
}

/// Tarball filename for the current [`DATA_VERSION`], e.g.
/// `nucl-parquet-data-2026.5.0.tar.zst`. Single source of truth for the
/// pattern — `ensure_*` and the install path consume this. Note: no `v`
/// prefix on the version since nucl-parquet PR #151 (Path A) moved data
/// versioning to CalVer.
pub fn tarball_filename() -> String {
    tarball_filename_for(DATA_VERSION)
}

/// Tarball filename for an arbitrary version. Exposed for offline-bundle
/// docs/tooling that need to spell a non-current version.
pub fn tarball_filename_for(version: &str) -> String {
    format!("nucl-parquet-data-{version}.tar.zst")
}

/// Full release-tarball URL for the current [`DATA_VERSION`], e.g.
/// `https://github.com/exoma-ch/nucl-parquet/releases/download/data-2026.5.0/nucl-parquet-data-2026.5.0.tar.zst`.
pub fn release_url() -> String {
    release_url_for(DATA_VERSION)
}

/// Full release-tarball URL for an arbitrary version. Upstream tag
/// shape is `data-{V}` (no `v` prefix) per nucl-parquet PR #151.
pub fn release_url_for(version: &str) -> String {
    format!(
        "{RELEASE_BASE}/data-{version}/{filename}",
        filename = tarball_filename_for(version),
    )
}

/// Detached-signature URL for the current [`DATA_VERSION`] — the tarball URL
/// with `.minisig` appended, which is where nucl-parquet's release workflow
/// publishes it (upstream #289 / PR #290).
pub fn signature_url() -> String {
    signature_url_for(DATA_VERSION)
}

/// Detached-signature URL for an arbitrary version.
pub fn signature_url_for(version: &str) -> String {
    format!("{}.minisig", release_url_for(version))
}

/// Human-readable cache-root pattern for diagnostics / UX, e.g.
/// `~/.hyrr/nucl-parquet/v2026.5.0/data`. The literal returned here is
/// always interpolated against the live [`DATA_VERSION`]; callers that
/// need an actual filesystem path should use [`cache_dir`] instead.
///
/// Note: the `v` prefix on the cache dir is deliberate — it's our
/// internal layout (matches [`parse_version_dir`] / prune logic) and
/// is decoupled from the upstream release-tag shape (`data-{V}`, no
/// `v`) since nucl-parquet PR #151 split data versioning to CalVer.
pub fn cache_root_pattern() -> String {
    format!("~/.hyrr/nucl-parquet/v{DATA_VERSION}/data")
}

/// Stage of the cache-fetch pipeline. Surfaced through
/// [`FetchProgress`] so the splash UI can label the progress bar.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FetchStage {
    /// HTTP connection establishment / DNS lookup (pre-200 response).
    Connecting,
    /// Body bytes streaming from the response into the on-disk tarball.
    Downloading,
    /// Tar extraction (`extract_tarball` per-entry progress).
    Extracting,
    /// Sentinel write + post-extract bookkeeping. Brief — primarily
    /// surfaced so the splash can show "almost done" rather than
    /// snapping from 99% straight to "Ready".
    Verifying,
}

/// Push-based progress callback payload.
///
/// `bytes_total` is `None` when the upstream HTTP `Content-Length` is
/// missing (rare but legal for `Transfer-Encoding: chunked`); the splash
/// renders an indeterminate `<progress>` bar in that case.
#[derive(Debug, Clone, Serialize)]
pub struct FetchProgress {
    pub stage: FetchStage,
    pub bytes_done: u64,
    pub bytes_total: Option<u64>,
}

/// Type alias for the progress-callback parameter passed through every
/// `ensure_*` / `extract_*` entry point. `&mut dyn FnMut(...)` keeps the
/// closure's captured state mutable (the desktop command throttles emits
/// using a `last_emit_at` field), and the `'_` lifetime lets the caller
/// own the closure on the stack.
pub type ProgressFn<'a> = &'a mut dyn FnMut(FetchProgress);

/// Sink-agnostic rate-limiter for [`FetchProgress`] events.
///
/// Both the desktop Tauri command and the PyO3 CLI binding need to
/// forward progress events to a slow sink (`AppHandle::emit` /
/// `Python::attach` + GIL acquire). A 100 MB download fires the
/// per-chunk progress callback ~thousands of times; without throttling
/// every chunk would cross the IPC/GIL boundary and destroy both
/// throughput and the UI render budget.
///
/// The throttle's policy (see [`FetchProgressThrottle::default_policy`]):
///
/// 1. **Stage change** — always emit. Lets the consumer relabel the
///    progress bar instantly when transitioning Connecting →
///    Downloading → Extracting → Verifying.
/// 2. **Final-byte event** — always emit when `bytes_done >= bytes_total`
///    (and `bytes_total > 0`). Guarantees the bar snaps to 100% before
///    the stage flips, regardless of the byte-step gate.
/// 3. **Downloading stage** — emit only if **both** ≥ 100 ms has elapsed
///    since the last emit *and* ≥ 256 KiB of new bytes have been
///    reported. Byte counts on this stage are real on-disk bytes and
///    meaningfully tick by 256 KiB.
/// 4. **Other stages** (Connecting, Extracting, Verifying) — emit only
///    on the 100 ms interval. The bytes_done field on these stages is
///    an entry count or sentinel state; 256-KiB gating would silence
///    Extracting for the entire stage on small libraries.
///
/// The bytes counter resets on stage transitions because `bytes_done`
/// means different things across stages (compressed bytes vs. entry
/// count) — carrying the old value into the new stage's byte-step
/// check would compare apples to oranges.
///
/// Construct via [`FetchProgressThrottle::default_policy`] or wrap a
/// sink closure with [`throttle`] (the higher-order combinator form
/// used by both the desktop and CLI call sites).
#[derive(Debug)]
pub struct FetchProgressThrottle {
    last_emit_at: Option<Instant>,
    last_bytes: u64,
    last_stage: Option<FetchStage>,
    min_interval: Duration,
    min_bytes_step: u64,
}

impl FetchProgressThrottle {
    /// Construct with the canonical policy: 100 ms minimum interval,
    /// 256 KiB minimum byte-step (Downloading stage only). Inherited
    /// verbatim from the original desktop `throttled_emit` (#160) and
    /// the CLI `make_py_progress` (#179) — both implementations had the
    /// same constants and gates by construction.
    pub fn default_policy() -> Self {
        Self {
            last_emit_at: None,
            last_bytes: 0,
            last_stage: None,
            min_interval: Duration::from_millis(100),
            min_bytes_step: 256 * 1024,
        }
    }

    /// Decide whether `p` should be forwarded to the sink. Mutates
    /// internal state (`last_emit_at`, `last_bytes`, `last_stage`)
    /// only when the answer is `true` — callers that don't pump the
    /// event do not leak it into the throttle history.
    pub fn should_emit(&mut self, p: &FetchProgress) -> bool {
        let now = Instant::now();

        let stage_changed = match self.last_stage {
            None => true,
            Some(prev) => !same_stage(prev, p.stage),
        };

        let final_byte = match p.bytes_total {
            Some(total) => p.bytes_done >= total && total > 0,
            None => false,
        };

        let interval_ok = self
            .last_emit_at
            .map(|t| now.saturating_duration_since(t) >= self.min_interval)
            .unwrap_or(true);

        // Reset the bytes counter on stage transition — bytes_done
        // means different things across stages (compressed bytes vs.
        // entry count) so the carry-over check is meaningless.
        let bytes_step_ok = match (self.last_stage, p.stage) {
            (Some(prev), curr) if !same_stage(prev, curr) => true,
            _ => p
                .bytes_done
                .checked_sub(self.last_bytes)
                .map(|d| d >= self.min_bytes_step)
                .unwrap_or(true),
        };

        // Only the Downloading stage carries reliable byte counts in
        // 256-KiB-meaningful units. Other stages emit on the 100 ms
        // interval alone — prevents Extracting from going silent
        // because entry counts never tick by 256 KiB.
        let should_emit = stage_changed
            || final_byte
            || match p.stage {
                FetchStage::Downloading => interval_ok && bytes_step_ok,
                FetchStage::Connecting | FetchStage::Extracting | FetchStage::Verifying => {
                    interval_ok
                }
            };

        if !should_emit {
            return false;
        }

        self.last_emit_at = Some(now);
        self.last_bytes = p.bytes_done;
        self.last_stage = Some(p.stage);
        true
    }
}

/// Wrap a [`FetchProgress`] sink closure with the default throttle
/// policy. Returns a closure with the same signature that drops events
/// failing the throttle gate; surviving events flow through to `sink`
/// unchanged.
///
/// Both the desktop emit-to-AppHandle and the CLI emit-to-Python sinks
/// are slow (cross IPC/GIL boundaries); use this to collapse the
/// per-chunk progress firehose into a UI-friendly cadence without
/// duplicating the throttle policy at every call site.
pub fn throttle(mut sink: impl FnMut(FetchProgress)) -> impl FnMut(FetchProgress) {
    let mut t = FetchProgressThrottle::default_policy();
    move |p: FetchProgress| {
        if t.should_emit(&p) {
            sink(p);
        }
    }
}

/// Pattern-match helper: `true` iff `a` and `b` are the same
/// [`FetchStage`] variant. Could be replaced by deriving
/// `PartialEq`, but the explicit `matches!` keeps the discriminator
/// surface obvious (every stage transition pair is named) and avoids
/// committing to a derived trait on a wire type.
fn same_stage(a: FetchStage, b: FetchStage) -> bool {
    matches!(
        (a, b),
        (FetchStage::Connecting, FetchStage::Connecting)
            | (FetchStage::Downloading, FetchStage::Downloading)
            | (FetchStage::Extracting, FetchStage::Extracting)
            | (FetchStage::Verifying, FetchStage::Verifying)
    )
}

/// Convenience: a no-op progress callback for callers that don't need
/// progress (CLI, MCP, tests). Spelled as a fresh closure rather than a
/// `static` because the trait object signature requires `FnMut`.
fn no_op_progress() -> impl FnMut(FetchProgress) {
    |_| {}
}

/// Wire-shape of a [`FetchError`] for the Tauri / IPC boundary.
///
/// Mirrors the StoppingError chain shipped in #142: every payload is
/// `kind: "FetchError"` + a `variant` discriminator; variant-specific
/// fields are serialised flat. The `url` and `cache_dir` fields are
/// always present so the recovery card can render them — both are
/// derived from the SSoT helpers ([`release_url`], [`cache_dir`]) so a
/// drift between "URL we tried" and "URL we render" is impossible by
/// construction.
///
/// Privacy: `cache_dir` is always under `~/.hyrr/...`, never an
/// arbitrary path; `url` is the canonical GH-Releases URL. No env vars,
/// no auth tokens, nothing the user can't already see in
/// `hyrr fetch-data --help`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "variant")]
pub enum FetchErrorPayload {
    HttpStatus {
        status: u16,
        url: String,
        cache_dir: String,
        message: String,
    },
    Network {
        detail: String,
        url: String,
        cache_dir: String,
        message: String,
    },
    Decompress {
        cache_dir: String,
        message: String,
    },
    Extract {
        cache_dir: String,
        message: String,
    },
    UnsafeTarballEntry {
        entry_kind: String,
        entry_path: String,
        cache_dir: String,
        message: String,
    },
    Io {
        cache_dir: String,
        message: String,
    },
    NoHome {
        message: String,
    },
    /// Integrity failure (#577). `url` is carried so the recovery card
    /// can name what was being fetched; the two digests are included so
    /// a bug report is actionable without asking the user to re-run
    /// anything. Neither is sensitive — the expected value is a public
    /// constant in this repo.
    ChecksumMismatch {
        expected: String,
        actual: String,
        url: String,
        cache_dir: String,
        message: String,
    },
    /// Publisher-authenticity failures (#594). `signature_url` is carried
    /// separately from `url` so a recovery card can say which of the two
    /// artefacts was the problem.
    NoSigningKey {
        url: String,
        cache_dir: String,
        message: String,
    },
    SignatureUnavailable {
        detail: String,
        signature_url: String,
        cache_dir: String,
        message: String,
    },
    SignatureInvalid {
        detail: String,
        url: String,
        signature_url: String,
        cache_dir: String,
        message: String,
    },
    VersionMismatch {
        expected: String,
        found: String,
        cache_dir: String,
        message: String,
    },
    ManifestMismatch {
        problems: Vec<String>,
        total: usize,
        cache_dir: String,
        message: String,
    },
    NoChecksumPin {
        url: String,
        cache_dir: String,
        message: String,
    },
}

impl FetchErrorPayload {
    /// Wrap as a top-level discriminator object — the wire shape the
    /// frontend actually sees. Equivalent to manually emitting
    /// `{"kind": "FetchError", "variant": ..., ...}`.
    pub fn to_json_string(&self) -> String {
        let inner = serde_json::to_value(self).unwrap_or_else(
            |_| serde_json::json!({"variant": "Io", "message": "serialise failed"}),
        );
        let mut obj = serde_json::Map::new();
        obj.insert(
            "kind".to_string(),
            serde_json::Value::String("FetchError".to_string()),
        );
        if let serde_json::Value::Object(m) = inner {
            for (k, v) in m {
                obj.insert(k, v);
            }
        }
        serde_json::to_string(&serde_json::Value::Object(obj)).unwrap_or_else(|_| {
            "{\"kind\":\"FetchError\",\"variant\":\"Io\",\"message\":\"serialise failed\"}"
                .to_string()
        })
    }
}

/// Replace the leading `$HOME` of `p` with the literal `~`. Used at the
/// Tauri/IPC boundary to keep the OS username out of payloads that
/// might end up in bug reports (see #173, #159 privacy contract).
///
/// Behaviour:
/// - If `home::home_dir()` returns `Some(home)` and `p` starts with that
///   prefix, the prefix is replaced with `~` (e.g.
///   `/Users/alice/.hyrr/...` → `~/.hyrr/...`).
/// - Otherwise the path is returned unchanged via `Display`. This covers
///   the absent-`$HOME`, `$HOME=""`, and "path is outside home" cases —
///   none of which leak the username because by definition no username
///   prefix is present to redact.
///
/// Defensive note: a degenerate `$HOME=/` (or `$HOME=""` resolving to
/// the empty string on some platforms) would otherwise rewrite every
/// path to `~/...`. We guard against the empty case explicitly; the
/// `/` case still strips a single byte and yields `~/etc/passwd` for
/// `/etc/passwd`, which is harmless — no user info is leaked, just an
/// unusual rendering.
pub fn redact_home(p: &Path) -> String {
    let s = p.display().to_string();
    if let Some(home) = home::home_dir() {
        let home_s = home.display().to_string();
        if !home_s.is_empty() {
            if let Some(rest) = s.strip_prefix(&home_s) {
                return format!("~{rest}");
            }
        }
    }
    s
}

impl From<&FetchError> for FetchErrorPayload {
    fn from(err: &FetchError) -> Self {
        let url = release_url();
        let cache_dir_str = cache_dir()
            .map(|p| redact_home(&p))
            .unwrap_or_else(|_| cache_root_pattern());
        let message = err.to_string();
        match err {
            FetchError::HttpStatus(status) => FetchErrorPayload::HttpStatus {
                status: *status,
                url,
                cache_dir: cache_dir_str,
                message,
            },
            FetchError::Network(detail) => FetchErrorPayload::Network {
                detail: detail.clone(),
                url,
                cache_dir: cache_dir_str,
                message,
            },
            FetchError::Decompress(_) => FetchErrorPayload::Decompress {
                cache_dir: cache_dir_str,
                message,
            },
            FetchError::Extract(_) => FetchErrorPayload::Extract {
                cache_dir: cache_dir_str,
                message,
            },
            FetchError::UnsafeTarballEntry { kind, path } => {
                FetchErrorPayload::UnsafeTarballEntry {
                    entry_kind: kind.clone(),
                    // `path` is the tarball-entry path (relative to the
                    // archive root, e.g. `data/meta/evil`) so today it
                    // can't carry `$HOME`. Routed through `redact_home`
                    // defensively per the #173 acceptance bullet — if a
                    // future refactor surfaces an absolute path here
                    // the redaction is already in place.
                    entry_path: redact_home(path),
                    cache_dir: cache_dir_str,
                    message,
                }
            }
            FetchError::Io(_) => FetchErrorPayload::Io {
                cache_dir: cache_dir_str,
                message,
            },
            FetchError::NoHome => FetchErrorPayload::NoHome { message },
            FetchError::NoSigningKey => FetchErrorPayload::NoSigningKey {
                url,
                cache_dir: cache_dir_str,
                message,
            },
            FetchError::SignatureUnavailable {
                detail,
                url: sig_url,
            } => FetchErrorPayload::SignatureUnavailable {
                detail: detail.clone(),
                signature_url: sig_url.clone(),
                cache_dir: cache_dir_str,
                message,
            },
            FetchError::SignatureInvalid { detail } => FetchErrorPayload::SignatureInvalid {
                detail: detail.clone(),
                url,
                signature_url: signature_url(),
                cache_dir: cache_dir_str,
                message,
            },
            FetchError::VersionMismatch { expected, found } => FetchErrorPayload::VersionMismatch {
                expected: expected.clone(),
                found: found.clone(),
                cache_dir: cache_dir_str,
                message,
            },
            FetchError::ManifestMismatch { problems, total } => {
                FetchErrorPayload::ManifestMismatch {
                    problems: problems.clone(),
                    total: *total,
                    cache_dir: cache_dir_str,
                    message,
                }
            }
            FetchError::ChecksumMismatch { expected, actual } => {
                FetchErrorPayload::ChecksumMismatch {
                    expected: expected.clone(),
                    actual: actual.clone(),
                    url,
                    cache_dir: cache_dir_str,
                    message,
                }
            }
            FetchError::NoChecksumPin => FetchErrorPayload::NoChecksumPin {
                url,
                cache_dir: cache_dir_str,
                message,
            },
        }
    }
}

/// Errors surfaced by the data-fetch path.
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("network error: {0}")]
    Network(String),
    #[error("HTTP {0}")]
    HttpStatus(u16),
    #[error("decompression error: {0}")]
    Decompress(String),
    #[error("tar extraction error: {0}")]
    Extract(String),
    /// Tarball entry was not a regular file or directory. We refuse
    /// symlinks, hardlinks, char/block/fifo devices, GNU sparse, etc.
    /// to avoid materialising read-side surprises (e.g. a malicious
    /// `data/meta/foo -> /etc/passwd` symlink) inside the cache. See
    /// #122. If a real upstream change ever trips this, that's a bug
    /// to investigate at the source — not something to silently skip.
    #[error("unsafe tarball entry {kind} at {path}")]
    UnsafeTarballEntry { kind: String, path: PathBuf },
    #[error("HOME environment variable not set")]
    NoHome,
    /// The downloaded tarball did not match the SHA-256 pinned for this
    /// build ([`DATA_TARBALL_SHA256`]). Nothing is extracted and the
    /// partial download is removed — a mismatch is never recoverable by
    /// continuing, because the whole point is that we cannot tell a
    /// benign cause from a substituted payload.
    #[error(
        "data tarball failed its integrity check: expected SHA-256 {expected}, got {actual}. \
         Nothing was installed."
    )]
    ChecksumMismatch { expected: String, actual: String },
    /// No pin was compiled in, so the download cannot be authenticated.
    /// Refusing is deliberate: treating an absent pin as "skip
    /// verification" would silently reinstate exactly the gap #577
    /// closes, and would do so precisely on the builds most likely to
    /// be unofficial.
    #[error(
        "this build has no pinned data-tarball checksum, so the download cannot be verified \
         (the nucl-parquet submodule was missing when it was compiled). Refusing to install. \
         Rebuild with `git submodule update --init --recursive`, or point HYRR_DATA at an \
         existing data directory."
    )]
    NoChecksumPin,
    /// No signing key was compiled in, so publisher authenticity cannot be
    /// established. Refused for the same reason as [`FetchError::NoChecksumPin`]:
    /// a build that quietly stopped checking signatures is indistinguishable
    /// from one that never did.
    #[error(
        "this build has no pinned data-signing public key, so the download's publisher \
         cannot be verified. Refusing to install. Rebuild with \
         `git submodule update --init --recursive`, or point HYRR_DATA at an existing \
         data directory."
    )]
    NoSigningKey,
    /// The detached `.minisig` could not be fetched or parsed.
    ///
    /// Deliberately *not* silently downgraded to hash-only verification. A
    /// release without a signature is either older than upstream #289 or has
    /// had its signature stripped, and those are indistinguishable from here.
    #[error(
        "could not obtain the data tarball's signature from {url}: {detail}. Nothing was \
         installed."
    )]
    SignatureUnavailable { detail: String, url: String },
    /// The signature did not verify against [`DATA_SIGNING_PUBKEY`]. Nothing
    /// is extracted and the partial download is removed.
    #[error("data tarball failed signature verification: {detail}. Nothing was installed.")]
    SignatureInvalid { detail: String },
    /// A validly-signed bundle for a *different* data release than this build
    /// pins (#614). Refused rather than installed: the cache layout is keyed
    /// on [`DATA_VERSION`], so it would land in the wrong directory — and
    /// accepting an older signed release is exactly the rollback a signature
    /// alone does not prevent.
    #[error(
        "this bundle is for nuclear data {found}, but this build of HYRR expects {expected}. \
         Nothing was installed. Use a bundle matching {expected}, or upgrade/downgrade HYRR \
         to the version that ships {found}."
    )]
    VersionMismatch { expected: String, found: String },
    /// The extracted tree disagreed with the signed content manifest (#621).
    /// Nothing is promoted; the staging directory is removed.
    #[error(
        "the data failed verification against its signed manifest ({total} problem(s)): {}.          Nothing was installed.",
        problems.join("; ")
    )]
    ManifestMismatch { problems: Vec<String>, total: usize },
}

pub type Result<T> = std::result::Result<T, FetchError>;

/// Cache root: `~/.hyrr/nucl-parquet/`.
fn cache_root() -> Result<PathBuf> {
    let home = std::env::var("HOME").map_err(|_| FetchError::NoHome)?;
    Ok(PathBuf::from(home).join(".hyrr").join("nucl-parquet"))
}

/// Versioned cache directory: `~/.hyrr/nucl-parquet/v{DATA_VERSION}/`.
pub fn cache_dir() -> Result<PathBuf> {
    Ok(cache_root()?.join(format!("v{DATA_VERSION}")))
}

/// Path to the `.complete` sentinel inside `cache_dir()`.
pub fn sentinel_path() -> Result<PathBuf> {
    Ok(cache_dir()?.join(".complete"))
}

/// True iff the on-disk cache for the current `DATA_VERSION` is fully
/// populated (sentinel present).
pub fn is_cache_complete() -> bool {
    sentinel_path().map(|p| p.exists()).unwrap_or(false)
}

/// Path to the `.verified` sidecar inside `cache_dir()` (#614).
fn verified_sentinel_path() -> Result<PathBuf> {
    Ok(cache_dir()?.join(".verified"))
}

/// True when this cache was installed through a path that verified a
/// publisher signature.
///
/// Distinct from [`is_cache_complete`], which only records that extraction
/// finished. A cache installed before signing existed is *complete* but not
/// *verified*, and those must stay tellable apart — the alternative is either
/// forcing a re-download on users who cannot reach the network, or silently
/// claiming an assurance that was never established.
///
/// Deliberately **not** gating reads yet. Doing so would brick every existing
/// offline install at upgrade time, which is the outcome that pushes operators
/// to `cp -r` the cache directory across a diode — no verification at all.
pub fn is_cache_verified() -> bool {
    verified_sentinel_path()
        .map(|p| p.exists())
        .unwrap_or(false)
}

/// What is known about the installed cache's provenance (#614).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CacheStatus {
    /// Extraction finished and the `.complete` sentinel is present.
    pub complete: bool,
    /// Installed through a path that verified a publisher signature.
    pub verified: bool,
    /// Data version recorded at install time, if any.
    pub data_version: Option<String>,
    /// Signing key that vouched for the install, if any. Surfaced so a key
    /// rotation is visible without re-installing.
    pub signing_key: Option<String>,
}

/// Report what is known about the installed cache.
///
/// **This deliberately does not re-hash the tree, and it is important to be
/// precise about why rather than shipping something that looks like it does.**
///
/// Re-verifying contents requires a per-file digest list signed by the
/// nuclear-data team. Upstream publishes a signature over the *archive*, not
/// over its contents, so once extracted there is nothing authenticated left to
/// compare a file against. A function that walked the tree and compared it to
/// nothing would read as a control while being none — the exact failure this
/// work exists to avoid.
///
/// Content re-verification is unblocked by exoma-ch/nucl-parquet#296 (signed
/// content manifest). Until then this reports provenance, honestly labelled.
pub fn cache_status() -> CacheStatus {
    let recorded = verified_sentinel_path()
        .ok()
        .and_then(|p| fs::read_to_string(p).ok());
    let (signing_key, data_version) = match &recorded {
        Some(text) => {
            let mut lines = text.lines();
            (
                lines.next().map(str::to_string).filter(|s| !s.is_empty()),
                lines.next().map(str::to_string).filter(|s| !s.is_empty()),
            )
        }
        None => (None, None),
    };
    CacheStatus {
        complete: is_cache_complete(),
        verified: recorded.is_some(),
        data_version,
        signing_key,
    }
}

/// Record that the current cache was installed from a signature-verified
/// payload. Written *after* the `.complete` sentinel, so a crash between the
/// two leaves the cache usable-but-unverified rather than falsely attested.
fn mark_cache_verified() -> Result<()> {
    let path = verified_sentinel_path()?;
    // Key id, not the payload digest: it answers "which key vouched for this
    // tree?", which is what a later audit or a key rotation needs to know.
    fs::write(&path, format!("{DATA_SIGNING_PUBKEY}\n{DATA_VERSION}\n"))?;
    Ok(())
}

/// Process-wide mutex that pairs with the on-disk `flock`. Two threads
/// in the same process opening `<cache_root>/.lock` independently and
/// both calling `flock(LOCK_EX)` is unreliable on macOS — the kernel
/// can leave both threads parked when neither holds the lock. The
/// in-process mutex makes the intra-process race deterministic; the
/// file lock continues to handle the inter-process case (the GUI
/// process and a separately-spawned `--mcp` process).
fn process_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Combined cross-thread + cross-process lock guard. Holds the
/// in-process mutex *and* the on-disk `flock` for as long as it lives.
/// Both are released on `Drop` in field-declaration order: file lock
/// first, then the mutex, which matches the order they were acquired
/// in reverse.
struct CacheLock {
    _file: fs::File,
    _guard: MutexGuard<'static, ()>,
}

/// Acquire the cross-thread + cross-process cache lock. Blocks the
/// current thread until competing fetch attempts release. Drops on
/// `Drop`.
fn acquire_lock() -> Result<CacheLock> {
    // In-process mutex first — see `process_lock` for the macOS
    // rationale. Poisoning means a previous holder panicked mid-op;
    // we recover and proceed because the file lock + sentinel-based
    // recovery handle the on-disk consistency story.
    let guard = process_lock().lock().unwrap_or_else(|p| p.into_inner());
    let root = cache_root()?;
    fs::create_dir_all(&root)?;
    let file = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(root.join(".lock"))?;
    file.lock_exclusive()
        .map_err(|e| FetchError::Io(io::Error::other(format!("lock: {e}"))))?;
    Ok(CacheLock {
        _file: file,
        _guard: guard,
    })
}

/// Build a configured reqwest client for cache fetches.
///
/// - `User-Agent`: GitHub increasingly rate-limits UA-less clients.
/// - `connect_timeout(30s)`: a half-open TCP socket on flaky Wi-Fi
///   would otherwise hang the splash until the App.svelte wall clock
///   fires (5 min) with no progress.
/// - No read timeout: a slow-but-progressing 400 MB download on a
///   rural DSL line should not be killed mid-stream.
fn build_http_client() -> reqwest::Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .user_agent(concat!("hyrr/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(30))
        .build()
}

/// Drop guard that removes `path` when it goes out of scope. Used so
/// the partial tarball is cleaned up on every code path — including
/// disk-full / network-drop / panic — without each caller needing to
/// remember `let _ = fs::remove_file(&tmp)`.
struct TmpFileGuard {
    path: PathBuf,
}

impl TmpFileGuard {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Drop for TmpFileGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Download the full nucl-parquet data tarball into `out`.
///
/// Streams the response to disk — does not buffer the full ~400 MB in RAM.
///
/// `progress` is invoked at least once with [`FetchStage::Connecting`]
/// before the request flies, and then per-chunk with
/// [`FetchStage::Downloading`] as bytes land on disk. Callers that don't
/// need progress (CLI, tests) can pass `&mut |_| {}` (or use the
/// no-progress sibling [`fetch_full_tarball_to`]). The throttling
/// (≤1 emit per 256 KiB / 100 ms) lives in the *caller's* closure — the
/// library always emits unconditionally so the consumer chooses the
/// rate.
pub fn fetch_full_tarball_to_with_progress(out: &Path, progress: ProgressFn<'_>) -> Result<()> {
    let url = release_url();
    progress(FetchProgress {
        stage: FetchStage::Connecting,
        bytes_done: 0,
        bytes_total: None,
    });

    let client = build_http_client().map_err(|e| FetchError::Network(e.to_string()))?;

    // Signature first, deliberately. It is ~380 bytes, and fetching it before
    // the ~800 MB payload means a missing key, an unavailable `.minisig`, or a
    // key-id mismatch fails in milliseconds instead of after a long download
    // the user then watches get deleted.
    let signature = fetch_detached_signature(&client)?;
    let public_key = signing_public_key()?;
    let mut verifier = TarballVerifier::start(&signature, &public_key)?;

    let resp = client
        .get(&url)
        .send()
        .map_err(|e| FetchError::Network(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(FetchError::HttpStatus(resp.status().as_u16()));
    }
    let bytes_total = resp.content_length();

    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::File::create(out)?;
    progress(FetchProgress {
        stage: FetchStage::Downloading,
        bytes_done: 0,
        bytes_total,
    });

    // 64 KiB chunks balance syscall overhead against responsiveness — at
    // 100 Mbit/s a chunk fills in ~5 ms which is well under the 100 ms
    // throttle window the desktop closure enforces.
    let mut reader = io::BufReader::new(resp);
    let mut buf = vec![0u8; 64 * 1024];
    let mut bytes_done: u64 = 0;
    // Hash as the bytes stream past. Doing it here rather than in a
    // second pass avoids re-reading ~800 MB from disk, and means the
    // digest covers exactly what was written rather than what we can
    // read back afterwards.
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        io::Write::write_all(&mut file, &buf[..n])?;
        // One call, both digests. minisign signs a BLAKE2b prehash, so the
        // signature verifies incrementally alongside the SHA-256 without a
        // second pass over ~800 MB and without holding it in RAM.
        verifier.update(&buf[..n]);
        bytes_done += n as u64;
        progress(FetchProgress {
            stage: FetchStage::Downloading,
            bytes_done,
            bytes_total,
        });
    }

    // Flush before verifying: the caller installs from `out`, so the
    // digest must describe the file that actually landed.
    io::Write::flush(&mut file)?;
    drop(file);

    progress(FetchProgress {
        stage: FetchStage::Verifying,
        bytes_done,
        bytes_total,
    });
    verifier.finalize(Some(DATA_TARBALL_SHA256))?;
    Ok(())
}

/// Verify a tarball already on disk against the pinned key, and return its
/// digest. Used by the air-gapped install path (#614).
///
/// The network path verifies while streaming from HTTP; this streams from the
/// file the user carried in. Both drive the same [`TarballVerifier`], so there
/// is exactly one implementation of the check.
fn verify_local_tarball(
    file: &mut fs::File,
    signature: &minisign_verify::Signature,
    pin: Option<&str>,
    progress: ProgressFn<'_>,
) -> Result<String> {
    let public_key = signing_public_key()?;
    let mut verifier = TarballVerifier::start(signature, &public_key)?;

    let bytes_total = file.metadata().ok().map(|m| m.len());
    progress(FetchProgress {
        stage: FetchStage::Verifying,
        bytes_done: 0,
        bytes_total,
    });

    let mut reader = io::BufReader::new(file);
    let mut buf = vec![0u8; 64 * 1024];
    let mut bytes_done: u64 = 0;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        verifier.update(&buf[..n]);
        bytes_done += n as u64;
        progress(FetchProgress {
            stage: FetchStage::Verifying,
            bytes_done,
            bytes_total,
        });
    }
    verifier.finalize(pin)
}

/// Streams bytes into a SHA-256 hasher and a minisign signature verifier at
/// once, so the network fetch and a local-file install share one
/// implementation of "is this tarball authentic and the one we pinned?".
///
/// Deliberately factors the *digest core*, not the whole read loop: the two
/// call sites differ in where bytes come from (HTTP vs `File`) and in their
/// progress reporting, and those wrappers do not compose. What must never
/// diverge is the verification itself — two subtly different implementations
/// of a security check is how one of them ends up weaker.
struct TarballVerifier<'a> {
    hasher: sha2::Sha256,
    sig: minisign_verify::StreamVerifier<'a>,
    signature: &'a minisign_verify::Signature,
}

impl<'a> TarballVerifier<'a> {
    fn start(
        signature: &'a minisign_verify::Signature,
        public_key: &'a minisign_verify::PublicKey,
    ) -> Result<Self> {
        let sig =
            public_key
                .verify_stream(signature)
                .map_err(|e| FetchError::SignatureInvalid {
                    detail: format!("signature is not usable with the pinned key: {e}"),
                })?;
        Ok(Self {
            hasher: <sha2::Sha256 as sha2::Digest>::new(),
            sig,
            signature,
        })
    }

    fn update(&mut self, buf: &[u8]) {
        sha2::Digest::update(&mut self.hasher, buf);
        self.sig.update(buf);
    }

    /// Verify signature, then the publisher's signed digest claim, then the
    /// compile-time pin. Returns the computed digest on success.
    ///
    /// Order is deliberate. A bad signature means "these bytes are not from
    /// the nuclear-data team"; a bad checksum means "these are not the bytes
    /// this build was tested against". Reporting the first question first
    /// stops a substituted payload being misreported as a stale pin.
    ///
    /// `pin` is `None` only where the caller has already established which
    /// release it is looking at by other means.
    fn finalize(mut self, pin: Option<&str>) -> Result<String> {
        self.sig
            .finalize()
            .map_err(|e| FetchError::SignatureInvalid {
                detail: format!("{e} (key {DATA_SIGNING_PUBKEY})"),
            })?;

        let digest = hex_lower(&sha2::Digest::finalize(self.hasher));

        // The trusted comment is covered by minisign's *global* signature, so
        // the `sha256=` it carries is authenticated — the publisher's own
        // claim about the payload, not the release host's.
        verify_trusted_comment_digest(self.signature.trusted_comment(), &digest)?;

        if let Some(pin) = pin {
            verify_sha256_against(pin, &digest)?;
        }
        Ok(digest)
    }
}

/// Version this build's cache layout is pinned to, parsed out of a signature's
/// authenticated trusted comment (`tag=data-X.Y.Z`).
///
/// Pure so the offline path's version gate is testable against the committed
/// signature fixture without a network or a signing key.
fn parse_bundle_version(trusted_comment: &str) -> Option<&str> {
    trusted_comment
        .split_whitespace()
        .find_map(|f| f.strip_prefix("tag=data-"))
}

/// Locate the detached signature for a local archive.
///
/// Sibling file (`<archive>.minisig`), matching how upstream publishes it and
/// how a user copies it onto media — "copy both files" is muscle memory, and
/// it is the only shape available anyway: minisign's signature is *detached*,
/// so embedding it would mean repacking the archive and destroying the very
/// byte-identity the signature is computed over.
///
/// `override_path` supports the case where the signature was carried
/// separately from the payload.
fn locate_sibling_signature(archive: &Path, override_path: Option<&Path>) -> Result<PathBuf> {
    if let Some(p) = override_path {
        if !p.exists() {
            return Err(FetchError::SignatureUnavailable {
                detail: format!("signature file not found: {}", p.display()),
                url: p.display().to_string(),
            });
        }
        return Ok(p.to_path_buf());
    }
    let mut name = archive.as_os_str().to_os_string();
    name.push(".minisig");
    let sibling = PathBuf::from(name);
    if !sibling.exists() {
        return Err(FetchError::SignatureUnavailable {
            detail: format!(
                "no signature found next to the archive (looked for {}). A bundle without a \
                 signature cannot be authenticated, and a stripped signature is \
                 indistinguishable from a release that never had one — so it is refused \
                 rather than installed unverified. Re-export on the connected machine with \
                 `hyrr fetch-data --offline-bundle <path>`, which writes both files, and \
                 copy BOTH",
                sibling.display()
            ),
            url: sibling.display().to_string(),
        });
    }
    Ok(sibling)
}

/// Decode the compile-time pinned signing key.
fn signing_public_key() -> Result<minisign_verify::PublicKey> {
    if DATA_SIGNING_PUBKEY.is_empty() {
        return Err(FetchError::NoSigningKey);
    }
    minisign_verify::PublicKey::from_base64(DATA_SIGNING_PUBKEY).map_err(|e| {
        FetchError::SignatureInvalid {
            detail: format!("the pinned public key in hyrr.json is not a valid minisign key: {e}"),
        }
    })
}

/// Fetch and parse the detached `.minisig` for the current release.
fn fetch_detached_signature(
    client: &reqwest::blocking::Client,
) -> Result<minisign_verify::Signature> {
    let text = fetch_detached_signature_text(client)?;
    minisign_verify::Signature::decode(&text).map_err(|e| FetchError::SignatureUnavailable {
        detail: format!("could not parse the signature: {e}"),
        url: signature_url(),
    })
}

/// Raw `.minisig` body, so the offline-bundle export can write it verbatim
/// rather than re-serialising a parsed form.
fn fetch_detached_signature_text(client: &reqwest::blocking::Client) -> Result<String> {
    let url = signature_url();
    let resp = client
        .get(&url)
        .send()
        .map_err(|e| FetchError::SignatureUnavailable {
            detail: e.to_string(),
            url: url.clone(),
        })?;
    if !resp.status().is_success() {
        return Err(FetchError::SignatureUnavailable {
            detail: format!(
                "HTTP {} — a release older than nucl-parquet #289 has no signature, and a \
                 stripped signature looks identical from here",
                resp.status().as_u16()
            ),
            url,
        });
    }
    resp.text().map_err(|e| FetchError::SignatureUnavailable {
        detail: e.to_string(),
        url: url.clone(),
    })
}

/// Check the authenticated trusted comment's `sha256=` against the digest we
/// computed over the downloaded bytes.
///
/// A comment without a `sha256=` field is accepted: upstream's format is not
/// ours to mandate, and the signature over the payload already establishes
/// authenticity on its own. This is a cross-check that strengthens the pin,
/// not a second gate that upstream could break by reformatting a comment.
fn verify_trusted_comment_digest(trusted_comment: &str, actual: &str) -> Result<()> {
    let Some(signed) = trusted_comment
        .split_whitespace()
        .find_map(|f| f.strip_prefix("sha256="))
    else {
        return Ok(());
    };
    if !signed.eq_ignore_ascii_case(actual) {
        return Err(FetchError::SignatureInvalid {
            detail: format!(
                "the signed trusted comment claims sha256={signed}, but the downloaded bytes \
                 hash to {actual}"
            ),
        });
    }
    Ok(())
}

/// Lowercase hex, without pulling a crate for four lines.
fn hex_lower(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        // Infallible: writing to a String never fails.
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Pin-comparison logic, with the pin injected.
///
/// Split out from [`verify_tarball_sha256`] purely so the absent-pin and
/// mismatch branches are reachable from tests: [`DATA_TARBALL_SHA256`] is
/// a compile-time constant, so a test binary can never observe it empty.
/// The refuse-on-absent-pin rule is the whole point of #577 and would
/// otherwise be the one branch nothing covers.
///
/// An absent pin is an error, not a bypass. Comparison is
/// case-insensitive because the pin is hand-edited in `hyrr.json`;
/// `actual` is always lowercase from [`hex_lower`].
fn verify_sha256_against(pin: &str, actual: &str) -> Result<()> {
    if pin.is_empty() {
        return Err(FetchError::NoChecksumPin);
    }
    if !pin.eq_ignore_ascii_case(actual) {
        return Err(FetchError::ChecksumMismatch {
            expected: pin.to_string(),
            actual: actual.to_string(),
        });
    }
    Ok(())
}

/// Back-compat wrapper for callers that don't need progress events.
///
/// Production callers should prefer
/// [`fetch_full_tarball_to_with_progress`] and pass an explicit closure;
/// this thin wrapper exists for the CLI / py / MCP entry points where
/// progress reporting is not yet wired up.
pub fn fetch_full_tarball_to(out: &Path) -> Result<()> {
    let mut noop = no_op_progress();
    fetch_full_tarball_to_with_progress(out, &mut noop)
}

/// Conservative pre-flight free-space check before downloading the
/// release tarball. The full release is ~400 MB compressed and ~400 MB
/// extracted, so 1 GiB is enough headroom while still being safe on
/// laptops with tight free space. A `disk full` failure mid-`io::copy`
/// otherwise leaves a partial tarball that the drop-guard cleans up but
/// also a confusing user-facing error.
fn require_free_space(min_bytes: u64) -> Result<()> {
    let root = cache_root()?;
    fs::create_dir_all(&root)?;
    match fs2::available_space(&root) {
        Ok(avail) if avail >= min_bytes => Ok(()),
        Ok(avail) => Err(FetchError::Io(io::Error::other(format!(
            "insufficient disk space: have {} MiB free at {}, need at least {} MiB",
            avail / 1_048_576,
            root.display(),
            min_bytes / 1_048_576,
        )))),
        // available_space lookup itself failed — proceed and let
        // io::copy surface the real error rather than blocking on a
        // diagnostic that didn't work.
        Err(_) => Ok(()),
    }
}

/// Extract a `.tar.zst` archive into `dest`. If `prefixes` is non-empty,
/// keeps only entries whose *normalised* path (see [`normalise_entry_path`])
/// starts with one of the listed prefixes. Pass `&[]` to extract everything.
///
/// Filters macOS `._*` resource-fork files which sometimes leak into
/// archives produced on Macs.
///
/// Path semantics: prefixes match the leading characters of the normalised
/// path. E.g. `"data/tendl-2025/"` matches `data/tendl-2025/xs/p_Cu.parquet`
/// but not `data/tendl-2024/`. The trailing slash matters — without it,
/// `"data/tendl-2"` would also match `data/tendl-2024/`. Callers should
/// always include the slash for directory-scoped extracts.
pub fn extract_tarball(archive: &Path, dest: &Path, prefixes: &[&str]) -> Result<()> {
    let mut noop = no_op_progress();
    extract_tarball_with_progress(archive, dest, prefixes, &mut noop)
}

/// Normalise a tar entry path to the on-disk cache layout (`data/...`).
///
/// The upstream release tarball is built with
/// `tar --zstd -C data -cf ... .`, so entries land at the archive root
/// (`./meta/…`, `./stopping/…`, `./tendl-2023-iso/…`). The offline
/// bundle (`export_offline_bundle`) uses `tar.append_dir_all("data", &data)`
/// so its entries are already `data/…`-prefixed. Both are valid tarball
/// shapes we consume, so this helper folds them onto a single canonical
/// on-disk layout that the rest of `data_fetch` and
/// [`crate::data_dir::resolve`] key off (`<cache_dir>/data/meta/…`).
///
/// Steps:
///   1. Strip a leading `./` or `/`.
///   2. If the result is empty (the root `./` entry), return `None`.
///   3. If the result already starts with `data/` or IS `data`, keep as-is.
///   4. Otherwise prepend `data/`.
///
/// Before the tarball-layout fix that shipped alongside the #529
/// version-drift fix, `MANDATORY_PREFIXES` looked for `data/meta/` etc.
/// against raw entry paths like `./meta/…` — every filter comparison
/// failed, `install_tarball_atomic` silently promoted an empty partial
/// dir, wrote the `.complete` sentinel, and every consumer downstream
/// saw an "empty but complete" cache. That was the second half of #529
/// (silent-empty physics results); this normalisation is what closes
/// it off. Kept as a `pub fn` so future maintainers can unit-test the
/// mapping without going through a fake tarball.
pub fn normalise_entry_path(path: &Path) -> Option<PathBuf> {
    // Windows uses `\` as its separator; tarball entries are POSIX-style
    // even on Windows, so operate on the string form and rebuild a PathBuf.
    let raw = path.to_string_lossy();
    let mut s = raw.as_ref();
    // Strip `./` or `/` roots (as many as present — some tarball tools
    // emit `./`, some `././`).
    loop {
        if let Some(rest) = s.strip_prefix("./") {
            s = rest;
            continue;
        }
        if let Some(rest) = s.strip_prefix('/') {
            s = rest;
            continue;
        }
        break;
    }
    // Bare root entry (`./` or `/`) — nothing to extract.
    if s.is_empty() || s == "." {
        return None;
    }
    // Path-traversal guard: reject any entry with a `..` component. We build
    // the on-disk path with `dest.join(norm)` and unpack to it directly (not
    // via tar-rs's sanitising `unpack_in`), so a surviving `..` would escape
    // `dest` — e.g. `data/../../etc/foo` or a root-level `../../tmp/pwn`. The
    // fetch source is a network Release asset (or a user-supplied
    // `--install-from` tarball), so this must be hostile-input safe. Reject
    // loudly by dropping the entry here; the caller's file-type gate and the
    // `starts_with(dest)` assertion at unpack are defence-in-depth. See #529.
    if s.split('/').any(|c| c == "..") {
        return None;
    }
    // Already `data/…` or the `data` root dir entry itself — keep as-is.
    if s == "data" || s.starts_with("data/") {
        return Some(PathBuf::from(s));
    }
    // Root-level entry — prepend `data/`.
    Some(PathBuf::from(format!("data/{s}")))
}

/// Progress-aware variant of [`extract_tarball`]. Emits one
/// [`FetchStage::Extracting`] event per accepted (post-filter) entry,
/// with `bytes_done` carrying the count of entries unpacked so far.
/// `bytes_total` is left as `None` because we'd have to walk the tar
/// twice to count entries up-front, and the splash uses an
/// indeterminate bar for this phase anyway.
pub fn extract_tarball_with_progress(
    archive: &Path,
    dest: &Path,
    prefixes: &[&str],
    progress: ProgressFn<'_>,
) -> Result<()> {
    extract_tarball_from_file(fs::File::open(archive)?, dest, prefixes, progress)
}

/// Extract from an already-open handle rather than a path.
///
/// This is what lets the air-gapped install verify and extract **the same
/// inode**: the offline path opens the user-supplied archive once, streams it
/// through signature verification, rewinds, and extracts from that same
/// descriptor. Re-opening by path in between would leave a window in which the
/// file could be swapped after passing verification — the flock serialises
/// HYRR against itself, not against an attacker who can write to the media.
pub fn extract_tarball_from_file(
    file: fs::File,
    dest: &Path,
    prefixes: &[&str],
    progress: ProgressFn<'_>,
) -> Result<()> {
    let decoder =
        zstd::stream::Decoder::new(file).map_err(|e| FetchError::Decompress(e.to_string()))?;
    let mut tar = tar::Archive::new(decoder);

    fs::create_dir_all(dest)?;
    let mut entries_done: u64 = 0;
    let mut files_written: u64 = 0;
    let mut entries_seen: u64 = 0;
    progress(FetchProgress {
        stage: FetchStage::Extracting,
        bytes_done: 0,
        bytes_total: None,
    });
    for entry in tar
        .entries()
        .map_err(|e| FetchError::Extract(e.to_string()))?
    {
        let mut entry = entry.map_err(|e| FetchError::Extract(e.to_string()))?;
        let raw_path = entry
            .path()
            .map_err(|e| FetchError::Extract(e.to_string()))?
            .into_owned();
        entries_seen += 1;

        // Skip macOS resource-fork files. Match on the raw path — the
        // `._*` convention lives at any depth, regardless of layout.
        if raw_path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.starts_with("._"))
            .unwrap_or(false)
        {
            continue;
        }

        // Fold both tarball shapes onto the canonical `data/…` on-disk
        // layout — see `normalise_entry_path`. `None` means the entry
        // is the archive root (`./` or `/`), or a rejected `..`-traversal
        // entry — either way it has no on-disk effect and is skipped.
        let Some(norm_path) = normalise_entry_path(&raw_path) else {
            continue;
        };
        let norm_str = norm_path.to_string_lossy();

        if !prefixes.is_empty() {
            // A prefix without a trailing slash matches a file (e.g.
            // "data/catalog.json"); with a trailing slash, only entries
            // strictly under that directory.
            let matches = prefixes.iter().any(|p| {
                if p.ends_with('/') {
                    norm_str.starts_with(p)
                } else {
                    norm_str == *p || norm_str.starts_with(&format!("{p}/"))
                }
            });
            if !matches {
                continue;
            }
        }

        // Refuse anything that isn't a plain file or directory. Symlinks,
        // hardlinks, char/block/fifo devices, GNU sparse, etc. have no
        // legitimate place in our data cache: a malicious upstream could
        // smuggle in `data/meta/foo -> /etc/passwd` and any later code
        // that follows symlinks would read out-of-cache content. We
        // surface this loudly via `FetchError::UnsafeTarballEntry`
        // rather than silently skipping — if a real-world tarball ever
        // trips this it's worth investigating upstream. See #122.
        let etype = entry.header().entry_type();
        if !(etype.is_file() || etype.is_dir()) {
            return Err(FetchError::UnsafeTarballEntry {
                kind: format!("{etype:?}"),
                path: raw_path,
            });
        }

        // Use `unpack` (with an explicit full path) rather than
        // `unpack_in(dest)` — the latter uses the entry's raw path
        // relative to `dest`, which would defeat the normalisation
        // above for root-level tarballs.
        let full_dest = dest.join(&norm_path);
        // Defence-in-depth: `normalise_entry_path` already drops `..` entries,
        // but assert the resolved path stays under `dest` before we unpack to
        // it (we bypass tar-rs's own `unpack_in` sandbox). Belt and braces on
        // a network-sourced archive.
        if !full_dest.starts_with(dest) {
            return Err(FetchError::UnsafeTarballEntry {
                kind: "path-traversal".to_string(),
                path: raw_path,
            });
        }
        if let Some(parent) = full_dest.parent() {
            fs::create_dir_all(parent)?;
        }
        entry
            .unpack(&full_dest)
            .map_err(|e| FetchError::Extract(e.to_string()))?;
        entries_done += 1;
        if etype.is_file() {
            files_written += 1;
        }
        progress(FetchProgress {
            stage: FetchStage::Extracting,
            bytes_done: entries_done,
            bytes_total: None,
        });
    }

    // Silent-empty-extraction guard (#529, second half). If the tarball
    // held entries but none survived the prefix filter and file-type
    // gate, refuse to promote — otherwise `install_tarball_atomic`
    // would rename an empty partial dir to the cache and write the
    // `.complete` sentinel over the top, tricking every consumer into
    // treating the cache as usable. The pre-fix symptom was exactly
    // this: user set no `--data-dir`, fetch "succeeded", first tool
    // call died on a missing stopping table.
    //
    // Empty archive + empty `prefixes` (an unusual but legal input) is
    // NOT an error — a caller that explicitly asks to extract nothing
    // from a well-formed empty archive should get an empty dest. The
    // guard triggers only when we saw entries and wrote no *files*. A
    // directories-only survivor set (no parquet payload) is exactly the
    // poisoning shape #529 guards against, so counting files — not
    // entries — is deliberate: our data contract always ships files.
    if entries_seen > 0 && files_written == 0 {
        return Err(FetchError::Extract(format!(
            "extracted 0 files from an archive with {entries_seen} entries — \
             tarball layout mismatch (expected root or `data/`-prefixed entries) \
             or every entry was filtered out"
        )));
    }
    Ok(())
}

/// Atomically install (a subset of) a downloaded tarball into the
/// versioned cache dir.
///
/// 1. Take the cache lock.
/// 2. Extract matching entries into `<cache_root>/v{V}.partial-{pid}/`.
/// 3. Promote: if cache is empty, atomic `fs::rename`. If cache already
///    exists (merging into a populated cache), drop the sentinel first,
///    merge entries, then re-write the sentinel last. The window during
///    which sentinel is missing is the only window during which a
///    concurrent reader sees the cache as incomplete — never as
///    "complete-but-half-merged".
///
/// `prefixes`: same semantics as `extract_tarball` — empty extracts
/// everything, a list of strings filters by `starts_with`.
///
/// **Caller must hold the cache lock** (`acquire_lock`) — every public
/// entry point in this module already does, and re-acquiring here
/// would deadlock the in-process mutex paired with the on-disk
/// `flock` (see `process_lock`).
fn install_tarball_atomic(
    archive: &Path,
    prefixes: &[&str],
    progress: ProgressFn<'_>,
) -> Result<()> {
    install_tarball_atomic_from_file(fs::File::open(archive)?, prefixes, progress)
}

/// [`install_tarball_atomic`] from an already-open handle, so a caller that
/// has just verified a signature can extract the exact bytes it verified.
fn install_tarball_atomic_from_file(
    archive: fs::File,
    prefixes: &[&str],
    progress: ProgressFn<'_>,
) -> Result<()> {
    install_tarball_atomic_verified(archive, prefixes, None, progress)
}

/// [`install_tarball_atomic_from_file`], optionally checking the extracted
/// tree against a signed content manifest before promoting it (#621).
///
/// This is the route that survives a content-scanning gateway. Such
/// appliances unpack and repack archives in transit, so the byte-signature
/// fails on data that is perfectly intact; a manifest authenticates
/// *contents* rather than framing, and so still applies.
///
/// The check runs against the staging directory, before the atomic promotion,
/// so a tree that fails verification is deleted rather than merged into a
/// working cache.
fn install_tarball_atomic_verified(
    archive: fs::File,
    prefixes: &[&str],
    manifest: Option<&crate::data_manifest::ContentManifest>,
    progress: ProgressFn<'_>,
) -> Result<()> {
    let cache = cache_dir()?;
    let root = cache_root()?;
    let pid = std::process::id();
    let partial = root.join(format!("v{DATA_VERSION}.partial-{pid}"));

    // Sweep stale partial dirs left by SIGKILL'd previous runs. The
    // lock guarantees no other live process is writing one right now,
    // so any `v{V}.partial-*` we see is genuinely orphaned. Without
    // this, a crashed extract leaves a ~400 MB carcass per crash that
    // accumulates forever (pids recycle slowly on macOS).
    let prefix = format!("v{DATA_VERSION}.partial-");
    if let Ok(entries) = fs::read_dir(&root) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with(&prefix) {
                let _ = fs::remove_dir_all(entry.path());
            }
        }
    }
    if partial.exists() {
        fs::remove_dir_all(&partial)?;
    }

    extract_tarball_from_file(archive, &partial, prefixes, progress)?;

    if let Some(manifest) = manifest {
        progress(FetchProgress {
            stage: FetchStage::Verifying,
            bytes_done: 0,
            bytes_total: None,
        });
        // The tarball is extracted under a `data/` prefix, but the manifest's
        // paths are relative to the data root itself.
        let tree = partial.join("data");
        // A prefix filter means a legitimate partial extraction, so missing
        // entries are expected; a modified or *unlisted* file never is.
        let completeness = if prefixes.is_empty() || prefixes == ["data"] {
            crate::data_manifest::Completeness::Complete
        } else {
            crate::data_manifest::Completeness::AllowSubset
        };
        let problems = manifest.verify_tree(&tree, completeness);
        if !problems.is_empty() {
            // `partial` is removed by the guard below, so nothing reaches the
            // cache. Report a bounded sample: an operator on an isolated
            // network needs enough to act on, not 6000 lines.
            let shown: Vec<String> = problems.iter().take(10).map(|p| p.to_string()).collect();
            return Err(FetchError::ManifestMismatch {
                problems: shown,
                total: problems.len(),
            });
        }
    }

    // Test-only seam: between partial-dir build and the
    // atomic-rename promotion, give tests a chance to simulate a
    // SIGKILL. Production builds compile this away entirely.
    #[cfg(test)]
    test_hooks::run_pre_promote_hook()?;

    progress(FetchProgress {
        stage: FetchStage::Verifying,
        bytes_done: 0,
        bytes_total: None,
    });

    // If `cache` already exists but is incomplete, blow it away — its
    // contents are by definition stale (the sentinel would be present
    // otherwise).
    if cache.exists() && !is_cache_complete() {
        fs::remove_dir_all(&cache)?;
    }

    if cache.exists() {
        // Merge into an already-populated cache. Drop the sentinel BEFORE
        // touching cache contents so a mid-merge crash leaves the cache
        // visibly incomplete rather than "complete but corrupt".
        let sentinel = sentinel_path()?;
        let _ = fs::remove_file(&sentinel);
        merge_dir_into(&partial, &cache)?;
        fs::remove_dir_all(&partial)?;
    } else {
        fs::rename(&partial, &cache)?;
    }

    // Write sentinel last — its existence is the contract for
    // "this cache is fully usable".
    fs::write(sentinel_path()?, DATA_VERSION)?;
    Ok(())
}

/// Recursively move all entries from `src` into `dst`. Used when merging a
/// library-only fetch into an already-populated cache. Overwrites
/// destination entries that already exist.
fn merge_dir_into(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            merge_dir_into(&from, &to)?;
        } else {
            // Overwrite an existing file by removing it first — `rename`
            // on most platforms requires the destination not exist (or
            // be empty). Cross-FS: fall back to copy+delete.
            let _ = fs::remove_file(&to);
            if fs::rename(&from, &to).is_err() {
                fs::copy(&from, &to)?;
                fs::remove_file(&from)?;
            }
        }
    }
    Ok(())
}

/// Path prefixes that always need to be present for any simulation.
/// `meta/` and `stopping/` are required by every library; the catalog
/// and supplier JSONs are read by frontend code and need to live in
/// the same data dir.
const MANDATORY_PREFIXES: &[&str] = &[
    "data/meta/",
    "data/stopping/",
    "data/catalog.json",
    "data/suppliers.json",
];

/// Ensure the `meta/` and `stopping/` directories (plus catalog/suppliers
/// JSON) are present in the cache.
///
/// Idempotent: returns immediately if the sentinel already exists. If
/// the cache is incomplete, fetches the full release tarball and
/// extracts only the mandatory prefixes — disk write is bounded to
/// ~54 MB even though the download itself is the full ~400 MB until
/// upstream ships per-library tarballs.
pub fn ensure_meta_stopping() -> Result<()> {
    let mut noop = no_op_progress();
    ensure_meta_stopping_with_progress(&mut noop)
}

/// Progress-aware variant of [`ensure_meta_stopping`].
pub fn ensure_meta_stopping_with_progress(progress: ProgressFn<'_>) -> Result<()> {
    if is_cache_complete() {
        return Ok(());
    }
    let _lock = acquire_lock()?;
    if is_cache_complete() {
        return Ok(());
    }
    require_free_space(1024 * 1024 * 1024)?;
    let tmp = cache_root()?.join(tarball_filename());
    let _guard = TmpFileGuard::new(tmp.clone());
    fetch_full_tarball_with_seam(&tmp, progress)?;
    install_tarball_atomic(&tmp, MANDATORY_PREFIXES, progress)?;
    Ok(())
}

/// Indirection for the network fetch so tests can inject a local-file
/// "fetcher" without touching production behaviour. In a non-test
/// build this is a one-line forwarder to
/// [`fetch_full_tarball_to_with_progress`].
fn fetch_full_tarball_with_seam(out: &Path, progress: ProgressFn<'_>) -> Result<()> {
    #[cfg(test)]
    {
        if let Some(()) = test_hooks::try_test_fetch(out)? {
            return Ok(());
        }
    }
    fetch_full_tarball_to_with_progress(out, progress)
}

/// Ensure the given library's data is present in the cache.
///
/// On a cold cache fetches the full release tarball but extracts only the
/// requested library's subtree plus the mandatory `meta/`/`stopping/` —
/// disk write bounded to ~50–110 MB rather than the full 400 MB. When
/// upstream ships per-library tarballs, only the URL changes here.
///
/// On a warm cache (sentinel present) where the library is already
/// extracted, returns immediately. If the sentinel is present but the
/// library subtree is absent (the bundled-resources-on-installer case),
/// fetches and merges only that library into the cache.
pub fn ensure_library(library: &str) -> Result<()> {
    let mut noop = no_op_progress();
    ensure_library_with_progress(library, &mut noop)
}

/// Progress-aware variant of [`ensure_library`].
pub fn ensure_library_with_progress(library: &str, progress: ProgressFn<'_>) -> Result<()> {
    if is_cache_complete() && cache_dir()?.join("data").join(library).exists() {
        return Ok(());
    }
    let _lock = acquire_lock()?;
    if is_cache_complete() && cache_dir()?.join("data").join(library).exists() {
        return Ok(());
    }
    require_free_space(1024 * 1024 * 1024)?;
    let tmp = cache_root()?.join(tarball_filename());
    let _guard = TmpFileGuard::new(tmp.clone());
    fetch_full_tarball_to_with_progress(&tmp, progress)?;
    let lib_prefix = format!("data/{library}/");
    let mut prefixes: Vec<&str> = MANDATORY_PREFIXES.to_vec();
    prefixes.push(&lib_prefix);
    install_tarball_atomic(&tmp, &prefixes, progress)?;
    Ok(())
}

/// Ensure *every* library is present in the cache. This is the path the
/// `hyrr fetch-data --all` flag wires into — extracts the whole tarball,
/// roughly 400 MB on disk.
///
/// Idempotent: returns immediately if the sentinel is present AND every
/// known library directory exists. (We can't enumerate libraries without
/// reading the catalog, so we trust the sentinel + a no-op merge: the
/// re-extraction overwrites identical bytes which is harmless.)
pub fn ensure_all() -> Result<()> {
    let mut noop = no_op_progress();
    ensure_all_with_progress(&mut noop)
}

/// Progress-aware variant of [`ensure_all`].
pub fn ensure_all_with_progress(progress: ProgressFn<'_>) -> Result<()> {
    let _lock = acquire_lock()?;
    require_free_space(1024 * 1024 * 1024)?;
    let tmp = cache_root()?.join(tarball_filename());
    let _guard = TmpFileGuard::new(tmp.clone());
    fetch_full_tarball_to_with_progress(&tmp, progress)?;
    install_tarball_atomic(&tmp, &[], progress)?;
    Ok(())
}

/// Seed the managed cache from a directory of bundled-installer
/// resources. Used by the Tauri startup hook to drop the ~54 MB of
/// `meta/` + `stopping/` (+ catalog/suppliers JSONs) into the cache
/// without paying the network cost on first launch.
///
/// `src` is the directory containing `meta/`, `stopping/`,
/// `catalog.json`, `suppliers.json` (i.e. the resource root the Tauri
/// installer materialised — the equivalent of the upstream
/// `nucl-parquet/data/` tree).
///
/// Atomicity:
/// - Acquires the cache lock so a concurrent `ensure_library` /
///   `--mcp` invocation can't `remove_dir_all` the cache mid-copy.
/// - Re-checks `is_cache_complete()` after acquiring the lock —
///   another instance may have populated the cache while this one
///   was contending for the lock. Idempotent on cold and warm caches.
/// - Validates that at least one regular file landed under both
///   `meta/` and `stopping/` before writing the sentinel, so a
///   half-copied seed can't masquerade as a complete cache. Any
///   error returns without writing the sentinel; the caller is
///   expected to leave the partial state for `ensure_library` to
///   wipe on the next fetch.
pub fn seed_from_dir(src: &Path) -> Result<()> {
    let _lock = acquire_lock()?;
    if is_cache_complete() {
        return Ok(());
    }
    let cache_data = cache_dir()?.join("data");
    fs::create_dir_all(&cache_data)?;

    // Mandatory resources — must all be present.
    for child in &["meta", "stopping", "catalog.json", "suppliers.json"] {
        let from = src.join(child);
        let to = cache_data.join(child);
        if to.exists() {
            continue;
        }
        if from.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else if from.is_file() {
            fs::copy(&from, &to)?;
        } else {
            // Missing source — installer didn't ship this resource.
            // Bail without seeding; ensure_library will fetch from
            // the network later.
            return Err(FetchError::Io(io::Error::other(format!(
                "seed_from_dir: source missing: {}",
                from.display()
            ))));
        }
    }

    // Optional: copy any bundled XS library directories (e.g.
    // tendl-2023-iso/) so the first simulation works offline (#264).
    // A directory is considered a library if it contains an `xs/`
    // subdirectory — this avoids copying meta/stopping/auxiliary again.
    if let Ok(entries) = fs::read_dir(src) {
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            // Skip known non-library entries.
            if matches!(name_str.as_ref(), "meta" | "stopping" | "auxiliary" | "em") {
                continue;
            }
            let from = entry.path();
            if from.is_dir() && from.join("xs").is_dir() {
                let to = cache_data.join(&name);
                if !to.exists() {
                    copy_dir_recursive(&from, &to)?;
                }
            }
        }
    }

    if !dir_has_any_file(&cache_data.join("meta"))?
        || !dir_has_any_file(&cache_data.join("stopping"))?
    {
        return Err(FetchError::Io(io::Error::other(
            "seed_from_dir: meta/ or stopping/ ended up empty",
        )));
    }
    fs::write(sentinel_path()?, DATA_VERSION)?;
    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

fn dir_has_any_file(p: &Path) -> Result<bool> {
    if !p.is_dir() {
        return Ok(false);
    }
    for entry in fs::read_dir(p)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        if ft.is_file() {
            return Ok(true);
        }
        if ft.is_dir() && dir_has_any_file(&entry.path())? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Produce a portable, **signature-verifiable** bundle at `out`, plus its
/// `.minisig` sibling. Used by `hyrr fetch-data --offline-bundle out.tar.zst`
/// on a connected machine; install with `--from` on the isolated one.
///
/// Downloads the upstream release tarball and its detached signature and
/// writes both **verbatim**, rather than repacking the local cache.
///
/// Repacking cannot be made verifiable, and it is worth being precise about
/// why rather than reaching for a weaker substitute:
///
/// * A repack is byte-different from the signed artefact (tar ordering,
///   mtimes, zstd level), so upstream's signature cannot cover it.
/// * The exporting user holds no signing key, so they cannot sign a
///   replacement. A manifest signed by nobody, living inside the bundle it
///   describes, is rewritable by anyone who can rewrite the files — it reads
///   as a control while being none, which is worse than admitting the gap.
/// * A throwaway keypair generated here would prove only "a machine holding
///   this key signed it", which is a tautology, not an assurance.
///
/// So the integrity of a repacked tree can only ever rest on the transfer
/// medium. Carrying the original signed bytes instead means the isolated
/// machine runs the *identical* verification path as the network fetch, and
/// the special case disappears rather than acquiring a weaker parallel one.
///
/// Known limitation: a content-scanning/CDR gateway that unpacks and repacks
/// archives in transit will break the signature while leaving the data
/// intact. Surviving that needs a signed per-file manifest from upstream
/// (exoma-ch/nucl-parquet#296); see `docs/DATA_INTEGRITY.md`.
pub fn export_offline_bundle(out: &Path) -> Result<()> {
    let mut noop = no_op_progress();
    export_offline_bundle_with_progress(out, &mut noop)
}

/// Progress-aware [`export_offline_bundle`].
pub fn export_offline_bundle_with_progress(out: &Path, progress: ProgressFn<'_>) -> Result<()> {
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }

    // Fetch through the normal path: it verifies signature + pin before we
    // hand anything to the user, so a bundle produced here is known-good at
    // the moment it is written.
    fetch_full_tarball_to_with_progress(out, progress)?;

    // The signature travels with it. Without this the isolated machine has
    // nothing to check against and will (correctly) refuse the install.
    let client = build_http_client().map_err(|e| FetchError::Network(e.to_string()))?;
    let sig = fetch_detached_signature_text(&client)?;
    let mut sig_out = out.as_os_str().to_os_string();
    sig_out.push(".minisig");
    fs::write(PathBuf::from(sig_out), sig)?;
    Ok(())
}

/// Repack the local cache into a portable tarball — **unauthenticated**.
///
/// Retained for the offline-to-offline case (a site mirroring an already
/// installed cache to a second isolated machine) where no signed upstream
/// artefact is reachable. `install_from_tarball` will refuse the result,
/// because it carries no signature and none can be manufactured; the honest
/// use is `HYRR_DATA` pointing at the extracted tree, with integrity resting
/// on the transfer medium.
///
/// Kept separate from [`export_offline_bundle`] so the two cannot be confused:
/// one produces a verifiable artefact, the other cannot, and a single function
/// doing both would silently degrade the guarantee depending on arguments.
pub fn repack_cache_unverified(out: &Path) -> Result<()> {
    if !is_cache_complete() {
        return Err(FetchError::Io(io::Error::other(
            "cache is not complete; run `hyrr fetch-data` first",
        )));
    }
    let cache = cache_dir()?;
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = fs::File::create(out)?;
    let encoder =
        zstd::stream::Encoder::new(file, 3).map_err(|e| FetchError::Decompress(e.to_string()))?;
    let mut tar = tar::Builder::new(encoder.auto_finish());
    // Walk `cache/data/` and add each entry under the prefix `data/`.
    let data = cache.join("data");
    tar.append_dir_all("data", &data)
        .map_err(|e| FetchError::Extract(e.to_string()))?;
    tar.finish()
        .map_err(|e| FetchError::Extract(e.to_string()))?;
    Ok(())
}

/// Install from a `.tar.zst` produced by `export_offline_bundle` (or
/// downloaded manually from a GitHub Release). Atomic + sentinel-protected
/// just like the network path.
pub fn install_from_tarball(archive: &Path) -> Result<()> {
    let mut noop = no_op_progress();
    install_from_tarball_with_progress(archive, &mut noop)
}

/// Extract and promote a tarball **without** verifying a signature.
///
/// Internal on purpose — there is no public route to it and no environment
/// variable that reaches it, because an air-gapped bypass is the first thing a
/// frustrated user pastes from a forum thread.
///
/// It exists because extraction, locking, sentinel ordering and merge
/// semantics are orthogonal to authenticity, and the tests covering them
/// cannot produce a signed fixture (the signing key is held offline by the
/// nuclear-data team). Handing those tests a real signed 785 MB release would
/// test minisign rather than the extractor.
///
/// `#[cfg(test)]`-gated rather than merely private: it is then impossible for
/// this to appear in a shipped binary, so no future refactor, feature flag or
/// environment variable can route a real user through it.
#[cfg(test)]
pub(crate) fn install_tarball_unverified(archive: &Path, progress: ProgressFn<'_>) -> Result<()> {
    let _lock = acquire_lock()?;
    install_tarball_atomic(archive, &["data"], progress)
}

/// Progress-aware variant of [`install_from_tarball`].
pub fn install_from_tarball_with_progress(archive: &Path, progress: ProgressFn<'_>) -> Result<()> {
    install_from_tarball_with_signature(archive, None, progress)
}

/// Install a local tarball, verifying its detached signature first (#614).
///
/// This closes the gap where the network path was authenticated and the
/// air-gapped path was not — which is worse than neither, because it
/// advertises a guarantee that does not hold exactly where users depend on it
/// most. Isolated control networks are the norm at accelerator and hospital
/// sites, so before this the cheapest attack on a HYRR install was handing
/// someone a USB stick.
///
/// `signature` overrides the sibling-file lookup for the case where the
/// `.minisig` was carried separately.
///
/// **Refuses rather than warning** on a missing signature. A stripped
/// signature and a release that predates signing look identical from here, and
/// a warning on an isolated network becomes "press enter" within a week. There
/// is deliberately no bypass flag: an escape hatch is the first thing a
/// frustrated user pastes from a forum thread.
pub fn install_from_tarball_with_signature(
    archive: &Path,
    signature: Option<&Path>,
    progress: ProgressFn<'_>,
) -> Result<()> {
    if !archive.exists() {
        return Err(FetchError::Io(io::Error::new(
            io::ErrorKind::NotFound,
            format!("tarball not found: {}", archive.display()),
        )));
    }

    let sig_path = locate_sibling_signature(archive, signature)?;
    let sig_text = fs::read_to_string(&sig_path).map_err(|e| FetchError::SignatureUnavailable {
        detail: e.to_string(),
        url: sig_path.display().to_string(),
    })?;
    let sig = minisign_verify::Signature::decode(&sig_text).map_err(|e| {
        FetchError::SignatureUnavailable {
            detail: format!("could not parse the signature: {e}"),
            url: sig_path.display().to_string(),
        }
    })?;

    // Version gate, before the expensive read. `install_tarball_atomic`
    // unconditionally installs into `~/.hyrr/nucl-parquet/v{DATA_VERSION}/`,
    // so a validly-signed bundle for a *different* release would populate the
    // wrong directory and then fail the pin comparison in a way that looks
    // like tampering. Refusing up front turns that into an actionable message.
    //
    // It is also the rollback defence: a signature alone does not stop an old,
    // legitimately-signed release being replayed. Binding the install to the
    // version this build pins is what does.
    // A signature with no `tag=` is refused rather than waved through. Skipping
    // the gate when the field is absent would mean the rollback defence quietly
    // stops applying to exactly the signatures that omit it.
    let bundle_version =
        parse_bundle_version(sig.trusted_comment()).ok_or_else(|| FetchError::VersionMismatch {
            expected: DATA_VERSION.to_string(),
            found: "unknown (the signature's trusted comment carries no tag=)".to_string(),
        })?;
    if bundle_version != DATA_VERSION {
        return Err(FetchError::VersionMismatch {
            expected: DATA_VERSION.to_string(),
            found: bundle_version.to_string(),
        });
    }

    // Open ONCE and keep the descriptor: verification and extraction must see
    // the same inode. Re-opening by path in between would let an attacker with
    // write access to the media swap the file after it passed verification —
    // the flock serialises HYRR against itself, not against them.
    let mut file = fs::File::open(archive)?;
    let byte_signature = verify_local_tarball(&mut file, &sig, Some(DATA_TARBALL_SHA256), progress);
    io::Seek::rewind(&mut file)?;

    // The byte signature is the stronger check and is tried first: it covers
    // the archive's framing as well as its contents. Only when it fails do we
    // ask the weaker question — "are the *files* the ones that were signed?".
    // Falling through in the other order would let a tampered `.tar.zst` that
    // happens to extract to the right files pass.
    let manifest = match &byte_signature {
        Ok(_) => None,
        Err(byte_err) => {
            // A content-scanning gateway repacks archives in transit: the data
            // is intact, the bytes are not the signed bytes. That is
            // indistinguishable from tampering *unless* a signed manifest is
            // available to authenticate the contents directly.
            match load_sibling_manifest(archive)? {
                Some(m) => Some(m),
                // No manifest to fall back on — report the original failure,
                // which is the one the user needs to see.
                None => return Err(clone_fetch_error(byte_err)),
            }
        }
    };

    let _lock = acquire_lock()?;
    install_tarball_atomic_verified(file, &["data"], manifest.as_ref(), progress)?;
    mark_cache_verified()?;
    Ok(())
}

/// Load and authenticate `<archive>.manifest.json` if present (#621).
///
/// Returns `Ok(None)` when there simply is no manifest — releases before
/// upstream's `FIRST_MANIFEST_VERSION` (2026.8.3) have none, so absence is
/// normal and must not be confused with failure. A manifest that is present
/// but does not authenticate is an error, never a shrug.
fn load_sibling_manifest(archive: &Path) -> Result<Option<crate::data_manifest::ContentManifest>> {
    let mut base = archive.as_os_str().to_os_string();
    base.push(".manifest.json");
    let manifest_path = PathBuf::from(base);
    if !manifest_path.exists() {
        return Ok(None);
    }
    let mut sig_name = manifest_path.as_os_str().to_os_string();
    sig_name.push(".minisig");
    let sig_path = PathBuf::from(sig_name);
    if !sig_path.exists() {
        return Err(FetchError::SignatureUnavailable {
            detail: format!(
                "found {} but no signature beside it. An unsigned manifest authenticates nothing — anyone who can rewrite the files can rewrite it too — so it is refused rather than used",
                manifest_path.display()
            ),
            url: sig_path.display().to_string(),
        });
    }

    let body = fs::read_to_string(&manifest_path)?;
    let sig_text = fs::read_to_string(&sig_path)?;
    let sig = minisign_verify::Signature::decode(&sig_text).map_err(|e| {
        FetchError::SignatureUnavailable {
            detail: format!("could not parse the manifest signature: {e}"),
            url: sig_path.display().to_string(),
        }
    })?;
    let public_key = signing_public_key()?;
    // Same offline key as the tarball — no second trust root to manage.
    public_key
        .verify(body.as_bytes(), &sig, false)
        .map_err(|e| FetchError::SignatureInvalid {
            detail: format!("manifest signature does not verify: {e}"),
        })?;

    let manifest = crate::data_manifest::ContentManifest::parse(&body)
        .map_err(|detail| FetchError::SignatureInvalid { detail })?;
    manifest
        .check_binding(DATA_VERSION, sig.trusted_comment())
        .map_err(|detail| FetchError::SignatureInvalid { detail })?;
    Ok(Some(manifest))
}

/// `FetchError` is not `Clone` (it wraps `io::Error`), and the fallback path
/// needs to re-raise the original byte-signature failure after deciding no
/// manifest is available. Preserves the variant where the distinction matters
/// to the caller, and the message otherwise.
fn clone_fetch_error(err: &FetchError) -> FetchError {
    match err {
        FetchError::SignatureInvalid { detail } => FetchError::SignatureInvalid {
            detail: detail.clone(),
        },
        FetchError::ChecksumMismatch { expected, actual } => FetchError::ChecksumMismatch {
            expected: expected.clone(),
            actual: actual.clone(),
        },
        FetchError::NoSigningKey => FetchError::NoSigningKey,
        other => FetchError::SignatureInvalid {
            detail: other.to_string(),
        },
    }
}

/// Parse a `v{N}.{N}.{N}` directory name into a sortable tuple. Returns
/// `None` for anything that doesn't match — the directory is treated as
/// not-a-version-cache and left alone.
///
/// We roll our own rather than pull in `semver` because the cache layout
/// only ever produces strict 3-part numeric versions (the data tarballs
/// are pinned to nucl-parquet's pyproject version, which is a 3-tuple).
fn parse_version_dir(name: &str) -> Option<(u64, u64, u64)> {
    let s = name.strip_prefix('v')?;
    let mut parts = s.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next()?.parse::<u64>().ok()?;
    let patch = parts.next()?.parse::<u64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

/// Prune older `v{N.N.N}/` cache directories, keeping only the `keep`
/// most recent (by semver order) plus the current `DATA_VERSION` dir.
///
/// Returns the number of directories removed. Idempotent: a second call
/// with the same `keep` returns `0`.
///
/// The cache lock is held for the whole sweep so a concurrent
/// `ensure_library` cannot promote a partial dir we're about to delete,
/// and we cannot delete a sibling that another process is mid-extracting.
///
/// `v{V}.partial-*` partial dirs and any non-version entries (`.lock`,
/// `.tmp` tarballs, the user's stray notes) are ignored.
pub fn prune_old_versions(keep: usize) -> Result<usize> {
    let _lock = acquire_lock()?;
    let root = cache_root()?;
    if !root.exists() {
        return Ok(0);
    }

    let current = parse_version_dir(&format!("v{DATA_VERSION}"));

    let mut versioned: Vec<(PathBuf, (u64, u64, u64))> = Vec::new();
    for entry in fs::read_dir(&root)? {
        let entry = entry?;
        // Don't follow symlinks — a chmod / move accident could otherwise
        // wipe data outside the cache.
        let ft = entry.file_type()?;
        if !ft.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if let Some(ver) = parse_version_dir(&name_str) {
            versioned.push((entry.path(), ver));
        }
    }

    // Newest first.
    versioned.sort_by(|a, b| b.1.cmp(&a.1));

    // Keep set = {current} ∪ {`keep` most recent versions not equal to current}.
    // The current pin is always preserved (the user might be mid-fetch);
    // `keep` controls how many historical siblings to preserve on top of
    // that. So `keep=2` with current=v0.10.0 and siblings v0.0.1..v0.0.5
    // preserves {v0.10.0, v0.0.5, v0.0.4} = 3 dirs.
    let mut kept: std::collections::HashSet<(u64, u64, u64)> = std::collections::HashSet::new();
    if let Some(c) = current {
        kept.insert(c);
    }
    let mut historical_taken = 0usize;
    for (_, ver) in &versioned {
        if historical_taken == keep {
            break;
        }
        if Some(*ver) == current {
            continue;
        }
        kept.insert(*ver);
        historical_taken += 1;
    }

    let mut removed = 0usize;
    for (path, ver) in &versioned {
        if kept.contains(ver) {
            continue;
        }
        fs::remove_dir_all(path)?;
        removed += 1;
    }
    Ok(removed)
}

/// Test-only seams for concurrency / interrupted-merge coverage (#123).
///
/// These hooks exist purely to let tests simulate failure modes that
/// are otherwise impossible to reproduce deterministically (SIGKILL
/// mid-merge, double-fetch under contention). Production builds compile
/// the module away entirely (`#[cfg(test)]`).
#[cfg(test)]
pub(crate) mod test_hooks {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    /// Action to run inside `install_tarball_atomic` after the partial
    /// dir is built but before promotion. `None` (the default) means
    /// the hook is inert.
    #[allow(dead_code)]
    pub(crate) enum PrePromoteAction {
        /// Panic — simulates a `SIGKILL` mid-install. Reserved for
        /// future tests that exercise the unwind path; current tests
        /// use `FailOnce` which produces the same on-disk recovery
        /// state without needing `catch_unwind`.
        PanicOnce,
        /// Return a synthetic IO error from the hook — equivalent
        /// on-disk effect to `PanicOnce` (partial dir survives,
        /// sentinel does not get written) but propagates as `Err(_)`
        /// through the normal `?` chain so tests can `assert!(_.is_err())`
        /// without `catch_unwind`.
        FailOnce,
    }

    static PRE_PROMOTE_ACTION: Mutex<Option<PrePromoteAction>> = Mutex::new(None);

    pub(crate) fn arm_pre_promote(action: PrePromoteAction) {
        *PRE_PROMOTE_ACTION.lock().unwrap() = Some(action);
    }

    pub(crate) fn clear_pre_promote() {
        *PRE_PROMOTE_ACTION.lock().unwrap() = None;
    }

    pub(crate) fn run_pre_promote_hook() -> Result<()> {
        let action = PRE_PROMOTE_ACTION.lock().unwrap().take();
        match action {
            None => Ok(()),
            Some(PrePromoteAction::PanicOnce) => {
                panic!("test_hooks: simulated SIGKILL mid-install");
            }
            Some(PrePromoteAction::FailOnce) => Err(FetchError::Io(io::Error::other(
                "test_hooks: simulated mid-install failure",
            ))),
        }
    }

    /// When `Some(path)`, `ensure_meta_stopping`'s fetch step copies
    /// `path` to `out` instead of hitting the network. The counter
    /// tracks how many times the seam fired across all threads — used
    /// by the lock-contention test to assert no double-fetch.
    static FETCH_SOURCE: Mutex<Option<PathBuf>> = Mutex::new(None);
    static FETCH_COUNT: AtomicUsize = AtomicUsize::new(0);

    pub(crate) fn arm_fetch_source(src: PathBuf) {
        *FETCH_SOURCE.lock().unwrap() = Some(src);
        FETCH_COUNT.store(0, Ordering::SeqCst);
    }

    pub(crate) fn clear_fetch_source() {
        *FETCH_SOURCE.lock().unwrap() = None;
        FETCH_COUNT.store(0, Ordering::SeqCst);
    }

    pub(crate) fn fetch_count() -> usize {
        FETCH_COUNT.load(Ordering::SeqCst)
    }

    /// If a test fetcher is armed, copy the local archive into `out`,
    /// bump the counter, and report `Some(())`. Otherwise return
    /// `None` to let the production fetcher run.
    pub(crate) fn try_test_fetch(out: &Path) -> Result<Option<()>> {
        let guard = FETCH_SOURCE.lock().unwrap();
        let Some(src) = guard.as_ref() else {
            return Ok(None);
        };
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(src, out)?;
        FETCH_COUNT.fetch_add(1, Ordering::SeqCst);
        Ok(Some(()))
    }
}

#[cfg(test)]
mod integrity_tests {
    use super::*;

    /// SHA-256 of the empty input — a known-answer test for the hex
    /// formatting and the digest wiring together. If `hex_lower` ever
    /// grows a leading-zero bug this catches it, which a round-trip
    /// against our own output would not.
    const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    fn sha256_hex(bytes: &[u8]) -> String {
        let mut h = <sha2::Sha256 as sha2::Digest>::new();
        sha2::Digest::update(&mut h, bytes);
        hex_lower(&sha2::Digest::finalize(h))
    }

    #[test]
    fn hex_lower_pads_leading_zeros() {
        assert_eq!(hex_lower(&[0x00, 0x0f, 0xff]), "000fff");
        assert_eq!(hex_lower(&[]), "");
    }

    #[test]
    fn digest_matches_known_answer() {
        assert_eq!(sha256_hex(b""), EMPTY_SHA256);
        // `abc` — the FIPS 180-4 worked example.
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn matching_digest_is_accepted() {
        assert!(verify_sha256_against(EMPTY_SHA256, EMPTY_SHA256).is_ok());
    }

    /// The pin is hand-edited in hyrr.json, so a maintainer pasting an
    /// uppercase digest must not brick every install.
    #[test]
    fn pin_comparison_is_case_insensitive() {
        assert!(verify_sha256_against(&EMPTY_SHA256.to_uppercase(), EMPTY_SHA256).is_ok());
    }

    #[test]
    fn tampered_bytes_are_rejected() {
        let actual = sha256_hex(b"not the data you were looking for");
        match verify_sha256_against(EMPTY_SHA256, &actual) {
            Err(FetchError::ChecksumMismatch {
                expected,
                actual: got,
            }) => {
                assert_eq!(expected, EMPTY_SHA256);
                assert_eq!(got, actual);
            }
            other => panic!("expected ChecksumMismatch, got {other:?}"),
        }
    }

    /// A one-byte truncation must not slip through. zstd's own frame
    /// checksum would also catch this, but the pin must not rely on that.
    #[test]
    fn truncated_payload_is_rejected() {
        let full = b"nuclear data payload";
        let pin = sha256_hex(full);
        let truncated = sha256_hex(&full[..full.len() - 1]);
        assert!(matches!(
            verify_sha256_against(&pin, &truncated),
            Err(FetchError::ChecksumMismatch { .. })
        ));
    }

    /// The load-bearing rule of #577: no pin means refuse, never skip.
    /// Failing open here would silently reinstate the gap, and would do
    /// so precisely on unofficial builds.
    #[test]
    fn absent_pin_refuses_rather_than_skipping() {
        assert!(matches!(
            verify_sha256_against("", EMPTY_SHA256),
            Err(FetchError::NoChecksumPin)
        ));
    }

    /// Both new variants must survive the Tauri/IPC boundary — the
    /// desktop recovery card renders from this wire shape, and a variant
    /// that serialises to the wrong discriminator shows the user a blank
    /// card instead of an actionable error.
    #[test]
    fn integrity_errors_have_a_wire_shape() {
        for err in [
            FetchError::ChecksumMismatch {
                expected: "aa".into(),
                actual: "bb".into(),
            },
            FetchError::NoChecksumPin,
        ] {
            let json = FetchErrorPayload::from(&err).to_json_string();
            assert!(json.contains("\"kind\":\"FetchError\""), "{json}");
            let v: serde_json::Value = serde_json::from_str(&json).expect("valid json");
            let variant = v["variant"].as_str().expect("variant present");
            assert!(
                variant == "ChecksumMismatch" || variant == "NoChecksumPin",
                "unexpected variant {variant}"
            );
            // The message is what the user actually reads; an empty one
            // would render an empty recovery card.
            assert!(!v["message"].as_str().unwrap_or("").is_empty(), "{json}");
        }
    }

    /// The mismatch payload must carry both digests, or a bug report is
    /// unactionable without asking the user to re-run the download.
    #[test]
    fn mismatch_payload_carries_both_digests() {
        let err = FetchError::ChecksumMismatch {
            expected: "e".repeat(64),
            actual: "a".repeat(64),
        };
        let json = FetchErrorPayload::from(&err).to_json_string();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["expected"].as_str().unwrap(), "e".repeat(64));
        assert_eq!(v["actual"].as_str().unwrap(), "a".repeat(64));
    }

    /// The pin compiled into *this* build must be well-formed. Guards
    /// against a hand-edit landing a truncated or whitespace-padded value
    /// in hyrr.json that build.rs's validation somehow let through.
    #[test]
    fn compiled_in_pin_is_well_formed_or_absent() {
        if DATA_TARBALL_SHA256.is_empty() {
            // Submodule-less build; DATA_VERSION must be the sentinel too,
            // so the two fail together and there's no usable-URL-without-pin
            // window.
            assert_eq!(DATA_VERSION, "0.0.0-unknown");
            return;
        }
        assert_eq!(DATA_TARBALL_SHA256.len(), 64);
        assert!(DATA_TARBALL_SHA256.bytes().all(|b| b.is_ascii_hexdigit()));
    }
}

#[cfg(test)]
mod tests {
    /// Extraction-focused tests use the unverified installer: they cover
    /// locking, sentinel ordering and merge semantics, none of which are
    /// about authenticity, and no signing key exists to make a fixture
    /// signable. Verification itself is covered in `signature_tests` and
    /// `offline_signature_tests`.
    fn install_unverified(archive: &std::path::Path) -> super::Result<()> {
        let mut noop = super::no_op_progress();
        super::install_tarball_unverified(archive, &mut noop)
    }

    use super::*;
    use std::sync::Mutex;

    /// Tests in this module must not run concurrently because they all mess
    /// with `$HOME` and the cache root.
    pub(super) static SERIAL: Mutex<()> = Mutex::new(());

    /// Set $HOME to a fresh tempdir for the duration of the test.
    pub(super) fn isolated_home() -> tempfile::TempDir {
        let td = tempfile::tempdir().expect("tempdir");
        std::env::set_var("HOME", td.path());
        td
    }

    /// Build a minimal v{V} tarball at `out` containing `data/meta/marker`.
    pub(super) fn make_test_tarball(out: &Path) {
        let file = fs::File::create(out).unwrap();
        let encoder = zstd::stream::Encoder::new(file, 0).unwrap().auto_finish();
        let mut tar = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        let payload = b"test-marker";
        header.set_size(payload.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        tar.append_data(&mut header, "data/meta/marker", payload.as_slice())
            .unwrap();
        // Also include a library subtree so ensure_library can find it.
        let mut h2 = tar::Header::new_gnu();
        let p2 = b"xs-marker";
        h2.set_size(p2.len() as u64);
        h2.set_mode(0o644);
        h2.set_cksum();
        tar.append_data(&mut h2, "data/tendl-test/xs/p_Cu.parquet", p2.as_slice())
            .unwrap();
        tar.finish().unwrap();
    }

    /// Build a tarball whose entries match the **actual GitHub release
    /// layout** — root-level `./meta/…`, `./stopping/…`, `./<library>/…`
    /// with no `data/` prefix. Produced by nucl-parquet's
    /// `.github/workflows/release-data.yml` via
    /// `tar --zstd -C data -cf … .` (see #529, second half).
    ///
    /// Callers use this to exercise `normalise_entry_path` end-to-end:
    /// a real release tarball must extract to `<dest>/data/meta/…` even
    /// though the archive itself carries the entries at root.
    fn make_release_layout_tarball(out: &Path) {
        let file = fs::File::create(out).unwrap();
        let encoder = zstd::stream::Encoder::new(file, 0).unwrap().auto_finish();
        let mut tar = tar::Builder::new(encoder);
        // The `./` root entry is a real tarball artefact (`tar -C data
        // -cf . .` emits it); the normaliser must skip it, not extract
        // it as a file. Include it explicitly to lock that in.
        let mut h_root = tar::Header::new_gnu();
        h_root.set_size(0);
        h_root.set_mode(0o755);
        h_root.set_entry_type(tar::EntryType::Directory);
        h_root.set_cksum();
        tar.append_data(&mut h_root, "./", std::io::empty())
            .unwrap();

        for (path, payload) in &[
            ("./meta/abundances.parquet", b"abundances" as &[u8]),
            ("./stopping/PSTAR.parquet", b"pstar"),
            ("./catalog.json", b"{\"data_version\":\"test\"}"),
            ("./tendl-test/xs/p_Cu.parquet", b"xs-p-cu"),
        ] {
            let mut h = tar::Header::new_gnu();
            h.set_size(payload.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            tar.append_data(&mut h, path, *payload).unwrap();
        }
        tar.finish().unwrap();
    }

    #[test]
    fn cache_paths_use_data_version() {
        let _g = SERIAL.lock().unwrap();
        let _td = isolated_home();
        let cd = cache_dir().unwrap();
        assert!(cd.ends_with(format!("v{DATA_VERSION}")));
        let s = sentinel_path().unwrap();
        assert!(s.ends_with(".complete"));
    }

    /// `DATA_VERSION` is sourced from `nucl-parquet/pyproject.toml` by
    /// `core/build.rs`. If the submodule is missing at build time, the
    /// fallback `"0.0.0-unknown"` ships, which would silently 404 on
    /// every fetch. Catch that in CI by asserting the version parses
    /// as N.N.N. The fallback `0.0.0-unknown` fails the dot-count check.
    /// SSoT pattern for the release URL. Pins the host/path shape so a
    /// silent string-edit elsewhere in the tree gets caught here. Don't
    /// remove without also updating the `data-fetch-meta.ts` consumer
    /// and any docs/CI that grep for the URL.
    #[test]
    fn release_url_pattern_is_canonical() {
        let url = release_url();
        assert!(
            url.starts_with("https://github.com/exoma-ch/nucl-parquet/releases/download/data-"),
            "release_url() = {url:?} drifted from the canonical host/path"
        );
        assert!(
            url.ends_with(".tar.zst"),
            "release_url() = {url:?} should end with .tar.zst"
        );
        let fname = tarball_filename();
        assert!(
            fname.starts_with("nucl-parquet-data-") && fname.ends_with(".tar.zst"),
            "tarball_filename() = {fname:?} drifted"
        );
        // The new wire format has no `v` prefix on the version segment
        // of the filename (nucl-parquet#151 — Path A, data on CalVer).
        assert!(
            !fname.starts_with("nucl-parquet-data-v"),
            "tarball_filename() = {fname:?} retained the legacy `v` prefix"
        );
        assert!(
            url.ends_with(&fname),
            "release_url() = {url:?} does not end in tarball_filename() = {fname:?}"
        );
    }

    #[test]
    fn data_version_is_resolved_from_submodule() {
        assert_ne!(
            DATA_VERSION, "0.0.0-unknown",
            "build.rs fell back — submodule not checked out at build time"
        );
        let parts: Vec<&str> = DATA_VERSION.split('.').collect();
        assert_eq!(
            parts.len(),
            3,
            "DATA_VERSION = {DATA_VERSION:?} is not N.N.N"
        );
        for p in &parts {
            assert!(
                p.chars().all(|c| c.is_ascii_digit()),
                "DATA_VERSION component {p:?} not numeric"
            );
        }
    }

    #[test]
    fn install_from_tarball_writes_sentinel_last() {
        let _g = SERIAL.lock().unwrap();
        let td = isolated_home();
        let archive = td.path().join("test.tar.zst");
        make_test_tarball(&archive);

        assert!(!is_cache_complete());
        install_unverified(&archive).unwrap();
        assert!(is_cache_complete());

        // Sentinel content is the version
        let sentinel = fs::read_to_string(sentinel_path().unwrap()).unwrap();
        assert_eq!(sentinel, DATA_VERSION);

        // Marker file made it through the atomic dance
        let marker = cache_dir().unwrap().join("data/meta/marker");
        assert!(marker.exists());
        assert_eq!(fs::read(&marker).unwrap(), b"test-marker");
    }

    #[test]
    fn install_is_idempotent() {
        let _g = SERIAL.lock().unwrap();
        let td = isolated_home();
        let archive = td.path().join("test.tar.zst");
        make_test_tarball(&archive);

        install_unverified(&archive).unwrap();
        let mtime1 = fs::metadata(sentinel_path().unwrap())
            .unwrap()
            .modified()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        // Second call short-circuits — we still call install_from_tarball
        // unconditionally to verify it's safe; the function is allowed to
        // re-extract under the lock, but must not corrupt state.
        install_unverified(&archive).unwrap();
        assert!(is_cache_complete());
        let mtime2 = fs::metadata(sentinel_path().unwrap())
            .unwrap()
            .modified()
            .unwrap();
        // Either the second run no-op'd (mtime unchanged) or re-wrote
        // (mtime advanced); both are fine — what matters is the cache is
        // still usable.
        let _ = (mtime1, mtime2);
        let marker = cache_dir().unwrap().join("data/meta/marker");
        assert!(marker.exists());
    }

    #[test]
    fn partial_dir_gets_promoted_to_complete() {
        let _g = SERIAL.lock().unwrap();
        let td = isolated_home();
        let archive = td.path().join("test.tar.zst");
        make_test_tarball(&archive);

        install_unverified(&archive).unwrap();

        let root = cache_root().unwrap();
        // Any v{V}.partial-* should be cleaned up
        let partials: Vec<_> = fs::read_dir(&root)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(&format!("v{DATA_VERSION}.partial"))
            })
            .collect();
        assert!(partials.is_empty(), "stale partial dir left behind");
    }

    #[test]
    fn repack_cache_unverified_round_trips() {
        let _g = SERIAL.lock().unwrap();
        let td = isolated_home();
        let archive = td.path().join("test.tar.zst");
        make_test_tarball(&archive);
        install_unverified(&archive).unwrap();

        // Covers the *repack* path, which is what this test has always
        // exercised. `export_offline_bundle` now downloads the signed upstream
        // artefact instead (verified by the `#[ignore]` live test below), so
        // it is no longer the function under test here.
        let bundle = td.path().join("offline.tar.zst");
        repack_cache_unverified(&bundle).unwrap();
        assert!(bundle.exists());

        // Move HOME to a fresh dir, ingest the bundle, verify the marker
        // is back.
        let td2 = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", td2.path());
        assert!(!is_cache_complete());
        install_unverified(&bundle).unwrap();
        assert!(is_cache_complete());
        let marker = cache_dir().unwrap().join("data/meta/marker");
        assert!(marker.exists());
    }

    #[test]
    fn repack_refuses_when_cache_incomplete() {
        let _g = SERIAL.lock().unwrap();
        let _td = isolated_home();
        let bundle = std::env::temp_dir().join("offline.tar.zst");
        let err = repack_cache_unverified(&bundle).unwrap_err();
        assert!(matches!(err, FetchError::Io(_)));
    }

    /// `seed_from_dir` should populate the cache, write the sentinel,
    /// and be idempotent — second call is a no-op once the sentinel
    /// is present.
    #[test]
    fn seed_from_dir_populates_and_marks_complete() {
        let _g = SERIAL.lock().unwrap();
        let td = isolated_home();
        let src = td.path().join("bundle");
        fs::create_dir_all(src.join("meta")).unwrap();
        fs::create_dir_all(src.join("stopping")).unwrap();
        fs::write(src.join("meta/elements.parquet"), b"meta-marker").unwrap();
        fs::write(src.join("stopping/stopping.parquet"), b"stop-marker").unwrap();
        fs::write(src.join("catalog.json"), b"{}").unwrap();
        fs::write(src.join("suppliers.json"), b"{}").unwrap();

        assert!(!is_cache_complete());
        seed_from_dir(&src).unwrap();
        assert!(is_cache_complete());
        let cached = cache_dir().unwrap().join("data/meta/elements.parquet");
        assert_eq!(fs::read(&cached).unwrap(), b"meta-marker");

        // Idempotent: second call is a no-op.
        seed_from_dir(&src).unwrap();
        assert!(is_cache_complete());
    }

    /// Validation guard: empty `meta/` or `stopping/` must not produce
    /// a "complete" sentinel — that would let `ensure_library`
    /// short-circuit on a hollow cache.
    #[test]
    fn seed_from_dir_rejects_empty_meta() {
        let _g = SERIAL.lock().unwrap();
        let td = isolated_home();
        let src = td.path().join("bundle");
        fs::create_dir_all(src.join("meta")).unwrap();
        fs::create_dir_all(src.join("stopping")).unwrap();
        fs::write(src.join("stopping/stopping.parquet"), b"stop").unwrap();
        fs::write(src.join("catalog.json"), b"{}").unwrap();
        fs::write(src.join("suppliers.json"), b"{}").unwrap();
        // No file under meta/.

        let err = seed_from_dir(&src).unwrap_err();
        assert!(matches!(err, FetchError::Io(_)));
        assert!(!is_cache_complete());
    }

    /// Build a tarball at `out` containing one regular file and one
    /// symlink entry (`data/meta/evil` -> `/etc/passwd`). Used to
    /// verify that `extract_tarball` refuses to materialise the
    /// symlink rather than silently honouring it. See #122.
    fn make_symlink_tarball(out: &Path) {
        let file = fs::File::create(out).unwrap();
        let encoder = zstd::stream::Encoder::new(file, 0).unwrap().auto_finish();
        let mut tar = tar::Builder::new(encoder);

        // One regular file so the archive isn't degenerate.
        let mut h1 = tar::Header::new_gnu();
        let payload = b"ok";
        h1.set_size(payload.len() as u64);
        h1.set_mode(0o644);
        h1.set_cksum();
        tar.append_data(&mut h1, "data/meta/marker", payload.as_slice())
            .unwrap();

        // The hostile symlink: data/meta/evil -> /etc/passwd
        let mut h2 = tar::Header::new_gnu();
        h2.set_size(0);
        h2.set_mode(0o644);
        h2.set_entry_type(tar::EntryType::Symlink);
        h2.set_link_name("/etc/passwd").unwrap();
        h2.set_cksum();
        tar.append_data(&mut h2, "data/meta/evil", std::io::empty())
            .unwrap();

        tar.finish().unwrap();
    }

    /// `extract_tarball` must refuse symlink entries — a malicious
    /// upstream could otherwise smuggle `data/meta/foo -> /etc/passwd`
    /// into the cache. See #122.
    #[test]
    fn extract_tarball_rejects_symlink_entries() {
        let _g = SERIAL.lock().unwrap();
        let td = isolated_home();
        let archive = td.path().join("hostile.tar.zst");
        make_symlink_tarball(&archive);

        let dest = td.path().join("dest");
        let err = extract_tarball(&archive, &dest, &[]).unwrap_err();
        match err {
            FetchError::UnsafeTarballEntry { kind, path } => {
                assert!(
                    kind.contains("Symlink"),
                    "expected kind to mention Symlink, got {kind:?}"
                );
                assert_eq!(path, PathBuf::from("data/meta/evil"));
            }
            other => panic!("expected UnsafeTarballEntry, got {other:?}"),
        }
        // The symlink must NOT have been materialised.
        assert!(!dest.join("data/meta/evil").exists());
        assert!(!dest.join("data/meta/evil").is_symlink());
    }

    /// Regression: a vanilla tarball (regular files + directories
    /// only) must still extract cleanly through the new entry-type
    /// filter. The existing `install_from_tarball_writes_sentinel_last`
    /// test covers the install path; this one exercises
    /// `extract_tarball` directly so a future refactor that pulls
    /// the type-check up the call chain stays honest.
    #[test]
    fn extract_tarball_accepts_regular_files() {
        let _g = SERIAL.lock().unwrap();
        let td = isolated_home();
        let archive = td.path().join("ok.tar.zst");
        make_test_tarball(&archive);

        let dest = td.path().join("dest");
        extract_tarball(&archive, &dest, &[]).unwrap();
        assert_eq!(
            fs::read(dest.join("data/meta/marker")).unwrap(),
            b"test-marker"
        );
        assert!(dest.join("data/tendl-test/xs/p_Cu.parquet").exists());
    }

    /// Unit coverage for [`normalise_entry_path`]. Cheap to run and
    /// documents the mapping table verbatim — a future refactor that
    /// tweaks the rules will trip this before the round-trip fixtures.
    #[test]
    fn normalise_entry_path_maps_both_tarball_layouts() {
        let cases: &[(&str, Option<&str>)] = &[
            // Root marker entries — no on-disk effect.
            ("./", None),
            ("/", None),
            (".", None),
            // Release-layout entries (root-level) — get `data/` prepended.
            (
                "./meta/abundances.parquet",
                Some("data/meta/abundances.parquet"),
            ),
            (
                "./stopping/PSTAR.parquet",
                Some("data/stopping/PSTAR.parquet"),
            ),
            ("./catalog.json", Some("data/catalog.json")),
            (
                "./tendl-2023-iso/xs/p_Cu.parquet",
                Some("data/tendl-2023-iso/xs/p_Cu.parquet"),
            ),
            // Same but without the `./` prefix — still root-level.
            (
                "meta/abundances.parquet",
                Some("data/meta/abundances.parquet"),
            ),
            // Offline-bundle layout — already `data/`-prefixed, pass through.
            (
                "data/meta/abundances.parquet",
                Some("data/meta/abundances.parquet"),
            ),
            (
                "data/stopping/PSTAR.parquet",
                Some("data/stopping/PSTAR.parquet"),
            ),
            // The bare `data` root dir entry itself.
            ("data", Some("data")),
            ("data/", Some("data/")),
            // Absolute-looking path — leading `/` stripped, then treated
            // as root-level.
            ("/meta/foo", Some("data/meta/foo")),
            // Path-traversal entries — rejected outright (#529 review).
            // A `..` in any component escapes `dest` under `dest.join`.
            ("../../tmp/pwn", None),
            ("data/../../etc/passwd", None),
            ("meta/../../../root/.ssh/authorized_keys", None),
            ("./../evil", None),
            ("..", None),
        ];
        for (input, expected) in cases {
            let got = normalise_entry_path(Path::new(input));
            assert_eq!(
                got.as_deref(),
                expected.map(Path::new),
                "normalise_entry_path({input:?}) = {got:?}, expected {expected:?}"
            );
        }
    }

    /// End-to-end: a real-release-shaped tarball (root-level entries
    /// with a `./` root marker) must extract to `<dest>/data/…` — the
    /// same on-disk layout as an offline-bundle tarball. This is the
    /// second half of #529: before the fix, the extract filter matched
    /// zero entries, `install_tarball_atomic` silently promoted an
    /// empty partial dir, and every downstream consumer saw a "complete
    /// but empty" cache.
    #[test]
    fn extract_tarball_normalises_release_layout_to_data_prefix() {
        let _g = SERIAL.lock().unwrap();
        let td = isolated_home();
        let archive = td.path().join("release.tar.zst");
        make_release_layout_tarball(&archive);

        let dest = td.path().join("dest");
        extract_tarball(&archive, &dest, &[]).unwrap();
        assert_eq!(
            fs::read(dest.join("data/meta/abundances.parquet")).unwrap(),
            b"abundances"
        );
        assert_eq!(
            fs::read(dest.join("data/stopping/PSTAR.parquet")).unwrap(),
            b"pstar"
        );
        assert!(dest.join("data/catalog.json").exists());
        assert!(dest.join("data/tendl-test/xs/p_Cu.parquet").exists());
        // The `./` root entry must NOT be materialised as a stray dir
        // at `dest/./` (or any other bogus location).
        let stray: Vec<_> = fs::read_dir(&dest)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name() != std::ffi::OsStr::new("data"))
            .collect();
        assert!(
            stray.is_empty(),
            "stray non-data entries at dest root: {stray:?}"
        );
    }

    /// Real-release-shape tarball → `MANDATORY_PREFIXES` filter. The
    /// combination that broke #529 in production: entries at root
    /// (`./meta/…`, `./stopping/…`) with prefixes `data/meta/`,
    /// `data/stopping/`, etc. Post-fix, normalisation folds the
    /// entries onto `data/…` and the filter matches — so the extract
    /// yields real files and the silent-empty guard doesn't fire.
    #[test]
    fn install_from_release_layout_populates_mandatory_paths() {
        let _g = SERIAL.lock().unwrap();
        let td = isolated_home();
        let archive = td.path().join("release.tar.zst");
        make_release_layout_tarball(&archive);

        // Extract with the mandatory prefixes (what `ensure_meta_stopping`
        // uses in production). Post-#529 the release layout must survive
        // this filter.
        let dest = td.path().join("dest");
        extract_tarball(&archive, &dest, MANDATORY_PREFIXES).unwrap();
        assert!(dest.join("data/meta/abundances.parquet").exists());
        assert!(dest.join("data/stopping/PSTAR.parquet").exists());
        assert!(dest.join("data/catalog.json").exists());
        // The library subtree is NOT in MANDATORY_PREFIXES, so it must
        // NOT be extracted here — `ensure_library` layers it on top.
        assert!(
            !dest.join("data/tendl-test").exists(),
            "MANDATORY_PREFIXES filter should have excluded tendl-test/"
        );
    }

    /// Silent-empty-extraction guard (#529). An archive with entries
    /// none of which pass the filter must return an error, not a
    /// bare-empty dest. Before this guard, `install_tarball_atomic`
    /// would promote the empty partial dir over the cache and write
    /// the `.complete` sentinel — the exact failure mode of #529 in
    /// production.
    #[test]
    fn extract_errors_when_all_entries_filtered_out() {
        let _g = SERIAL.lock().unwrap();
        let td = isolated_home();
        let archive = td.path().join("release.tar.zst");
        make_release_layout_tarball(&archive);

        // Prefix that matches nothing in the tarball. Pre-fix this
        // returned Ok(()) with a bare-empty dest.
        let dest = td.path().join("dest");
        let err = extract_tarball(&archive, &dest, &["data/does-not-exist/"]).unwrap_err();
        let msg = err.to_string();
        assert!(
            matches!(err, FetchError::Extract(_)),
            "expected Extract variant, got {err:?}"
        );
        assert!(
            msg.contains("extracted 0 files"),
            "diagnostic must call out zero-files result: {msg}"
        );
    }

    /// End-to-end #529 recovery: `install_tarball_atomic` fed a
    /// release-layout tarball with `MANDATORY_PREFIXES` populates the
    /// cache at `<cache>/data/meta/…` + `<cache>/data/stopping/…` and
    /// writes the `.complete` sentinel — the shape
    /// `data_dir::resolve` expects when handing off to the MCP
    /// transport.
    #[test]
    fn ensure_meta_stopping_populates_cache_from_release_layout_tarball() {
        let _g = SERIAL.lock().unwrap();
        let td = isolated_home();
        let archive = td.path().join("release.tar.zst");
        make_release_layout_tarball(&archive);

        // `install_from_tarball` uses prefix `["data"]` (extract
        // everything under data/…). Post-fix the release-layout tarball
        // normalises through the `data/` prefix, so this succeeds.
        install_unverified(&archive).unwrap();
        assert!(is_cache_complete());
        let data = cache_dir().unwrap().join("data");
        assert!(data.join("meta/abundances.parquet").exists());
        assert!(data.join("stopping/PSTAR.parquet").exists());
        assert!(data.join("catalog.json").exists());
    }

    /// Stale partial dirs (left by SIGKILL'd previous runs) must be
    /// swept by `install_tarball_atomic`. Without the sweep these
    /// accumulate at ~400 MB per crash forever.
    #[test]
    fn install_sweeps_orphaned_partial_dirs() {
        let _g = SERIAL.lock().unwrap();
        let td = isolated_home();
        let archive = td.path().join("test.tar.zst");
        make_test_tarball(&archive);

        // Manually plant a stale partial dir from a "previous run" with
        // a different pid.
        let root = cache_root().unwrap();
        fs::create_dir_all(&root).unwrap();
        let stale = root.join(format!("v{DATA_VERSION}.partial-99999"));
        fs::create_dir_all(&stale).unwrap();
        fs::write(stale.join("orphan-marker"), b"x").unwrap();
        assert!(stale.exists());

        install_unverified(&archive).unwrap();

        assert!(!stale.exists(), "stale partial-99999 was not swept");
        assert!(is_cache_complete());
    }

    /// Plant `v0.1.0..v0.5.0` directories with `.complete` sentinels.
    /// `prune_old_versions(2)` keeps the 2 newest plus the current
    /// DATA_VERSION pin. Idempotency: a second call returns 0. A keep
    /// value larger than the population removes nothing.
    fn plant_version_dirs(root: &Path, versions: &[&str]) {
        for v in versions {
            let dir = root.join(format!("v{v}"));
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join(".complete"), v).unwrap();
        }
    }

    #[test]
    fn prune_keeps_newest_n_plus_current() {
        let _g = SERIAL.lock().unwrap();
        let _td = isolated_home();
        let root = cache_root().unwrap();
        fs::create_dir_all(&root).unwrap();

        // Plant 5 ancient versions all guaranteed strictly older than
        // any plausible DATA_VERSION. This keeps the test invariant
        // independent of the actual current pin.
        let planted = ["0.0.1", "0.0.2", "0.0.3", "0.0.4", "0.0.5"];
        plant_version_dirs(&root, &planted);
        // Plant the current DATA_VERSION dir too.
        plant_version_dirs(&root, &[DATA_VERSION]);

        let removed = prune_old_versions(2).unwrap();

        // Expected: keep v0.0.5, v0.0.4 (newest 2 of the planted set;
        // DATA_VERSION sorts newer than all of them and is also kept by
        // the current-pin rule). 5 planted + 1 current = 6 dirs total,
        // minus 3 kept = 3 removed.
        assert_eq!(removed, 3);
        assert!(root.join("v0.0.5").exists());
        assert!(root.join("v0.0.4").exists());
        assert!(root.join(format!("v{DATA_VERSION}")).exists());
        assert!(!root.join("v0.0.1").exists());
        assert!(!root.join("v0.0.2").exists());
        assert!(!root.join("v0.0.3").exists());

        // Idempotency.
        let removed2 = prune_old_versions(2).unwrap();
        assert_eq!(removed2, 0);
    }

    #[test]
    fn prune_with_large_keep_removes_nothing() {
        let _g = SERIAL.lock().unwrap();
        let _td = isolated_home();
        let root = cache_root().unwrap();
        fs::create_dir_all(&root).unwrap();
        plant_version_dirs(&root, &["0.1.0", "0.2.0", "0.3.0"]);

        let removed = prune_old_versions(10).unwrap();
        assert_eq!(removed, 0);
        for v in &["0.1.0", "0.2.0", "0.3.0"] {
            assert!(root.join(format!("v{v}")).exists());
        }
    }

    /// Non-version directories (e.g. `data/`, `notes/`) and stray files
    /// are left untouched by prune.
    #[test]
    fn prune_ignores_non_version_entries() {
        let _g = SERIAL.lock().unwrap();
        let _td = isolated_home();
        let root = cache_root().unwrap();
        fs::create_dir_all(&root).unwrap();
        plant_version_dirs(&root, &["0.1.0", "0.2.0"]);
        fs::create_dir_all(root.join("not-a-version")).unwrap();
        fs::create_dir_all(root.join(format!("v{DATA_VERSION}.partial-1234"))).unwrap();
        fs::write(root.join(".lock"), b"").unwrap();

        let _ = prune_old_versions(0).unwrap();
        assert!(root.join("not-a-version").exists());
        assert!(
            root.join(format!("v{DATA_VERSION}.partial-1234")).exists(),
            "partial dirs are install_tarball_atomic's responsibility, not prune's"
        );
        assert!(root.join(".lock").exists());
    }

    /// `prune_old_versions` must acquire the cache lock; verify it doesn't
    /// deadlock against itself when called sequentially in the same test.
    #[test]
    fn prune_is_lock_safe_when_called_sequentially() {
        let _g = SERIAL.lock().unwrap();
        let _td = isolated_home();
        let root = cache_root().unwrap();
        fs::create_dir_all(&root).unwrap();
        plant_version_dirs(&root, &["0.1.0", "0.2.0", "0.3.0", "0.4.0"]);

        // Two sequential calls — the lock guard from the first call must
        // be dropped before the second acquires.
        let _ = prune_old_versions(1).unwrap();
        let removed = prune_old_versions(1).unwrap();
        assert_eq!(removed, 0);
    }

    /// Helper: count `v{V}.partial-*` directories left under
    /// `cache_root()`. Used by the concurrency / interruption tests
    /// to assert no orphaned partials survive a recovery cycle.
    fn count_partial_dirs() -> usize {
        let root = cache_root().unwrap();
        let prefix = format!("v{DATA_VERSION}.partial-");
        match fs::read_dir(&root) {
            Ok(it) => it
                .filter_map(|e| e.ok())
                .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
                .count(),
            Err(_) => 0,
        }
    }

    /// #123 — doc-comment claim:
    /// "the user can kill the process at any moment and the next
    /// invocation finds either (a) a fully-populated cache, or (b) a
    /// missing/incomplete cache that gets re-fetched cleanly."
    ///
    /// Two threads race `install_from_tarball` against the same cache
    /// root. The `<cache_root>/.lock` file lock must serialise them so
    /// neither sees a half-merged cache and neither deadlocks. Both
    /// invocations are expected to succeed (the second merges into a
    /// populated cache and re-writes the sentinel); at minimum one
    /// must succeed and the final state must be consistent.
    #[test]
    fn concurrent_install_from_tarball_serialises() {
        let _g = SERIAL.lock().unwrap();
        let td = isolated_home();
        let archive = td.path().join("test.tar.zst");
        make_test_tarball(&archive);

        let archive_ref = &archive;
        let (r1, r2) = std::thread::scope(|s| {
            let h1 = s.spawn(|| install_unverified(archive_ref));
            let h2 = s.spawn(|| install_unverified(archive_ref));
            (
                h1.join().expect("t1 panicked"),
                h2.join().expect("t2 panicked"),
            )
        });

        // Neither thread deadlocked (we got here) and at least one
        // succeeded. A second-mover may legitimately succeed too via
        // the merge path; what's not allowed is *both* failing.
        assert!(
            r1.is_ok() || r2.is_ok(),
            "both threads failed: r1={r1:?} r2={r2:?}"
        );

        // Final state is consistent: sentinel + payload present, no
        // stray partial dirs.
        assert!(
            is_cache_complete(),
            "sentinel missing after both threads finished"
        );
        let marker = cache_dir().unwrap().join("data/meta/marker");
        assert!(marker.exists(), "payload missing after concurrent install");
        assert_eq!(fs::read(&marker).unwrap(), b"test-marker");
        assert_eq!(count_partial_dirs(), 0, "stray partial dirs left behind");
    }

    /// #123 — recovery half of the doc-comment claim. Simulate a
    /// SIGKILL between partial-dir build and atomic-rename via the
    /// test-only `FailOnce` hook. The first invocation must error and
    /// leave the cache visibly incomplete; the next invocation must
    /// observe `is_cache_complete() == false`, sweep the orphan
    /// partial, and finish cleanly.
    #[test]
    fn interrupted_merge_recovers_on_next_invocation() {
        let _g = SERIAL.lock().unwrap();
        let td = isolated_home();
        let archive = td.path().join("test.tar.zst");
        make_test_tarball(&archive);

        // Arm the seam, run the interrupted install. We expect Err.
        test_hooks::arm_pre_promote(test_hooks::PrePromoteAction::FailOnce);
        let interrupted = install_unverified(&archive);
        assert!(interrupted.is_err(), "armed hook did not fire");
        // Hook is single-shot but clear defensively in case of test
        // re-entry.
        test_hooks::clear_pre_promote();

        // After the interrupted install: cache must be incomplete and
        // an orphan partial-{pid} should be visible to the sweep.
        assert!(
            !is_cache_complete(),
            "sentinel written despite interruption"
        );
        assert!(
            count_partial_dirs() >= 1,
            "expected an orphan partial dir from the interrupted install"
        );

        // Second invocation: clean run. The orphan partial must be
        // swept, the cache promoted, and the sentinel re-asserted.
        install_unverified(&archive).expect("clean re-run failed");
        assert!(is_cache_complete(), "sentinel not written on retry");
        let marker = cache_dir().unwrap().join("data/meta/marker");
        assert!(marker.exists(), "payload missing after recovery");
        assert_eq!(count_partial_dirs(), 0, "orphan partial was not swept");
    }

    /// #123 — N-thread lock contention on `ensure_meta_stopping`. With
    /// the network fetch redirected to a local file via the test seam,
    /// N=4 threads racing against an empty cache must end in exactly
    /// ONE fetch (the rest see the sentinel after the lock-holder
    /// finishes and short-circuit). All threads must succeed.
    #[test]
    fn ensure_meta_stopping_serialises_and_dedupes_fetches() {
        let _g = SERIAL.lock().unwrap();
        let td = isolated_home();
        let archive = td.path().join("test.tar.zst");
        make_test_tarball(&archive);

        test_hooks::arm_fetch_source(archive.clone());
        // Defensive cleanup if a previous test scribbled state.
        let starting_count = test_hooks::fetch_count();
        assert_eq!(starting_count, 0, "fetch counter should reset on arm");

        const N: usize = 4;
        let results = std::thread::scope(|s| {
            let mut handles = Vec::with_capacity(N);
            for _ in 0..N {
                handles.push(s.spawn(ensure_meta_stopping));
            }
            handles
                .into_iter()
                .map(|h| h.join().expect("worker panicked"))
                .collect::<Vec<_>>()
        });

        // Every thread succeeded — the lock made them serial, not
        // failed.
        for (i, r) in results.iter().enumerate() {
            assert!(r.is_ok(), "thread {i} failed: {r:?}");
        }
        // Exactly one thread actually performed the fetch; the rest
        // observed the sentinel after acquiring the lock and bailed.
        assert_eq!(
            test_hooks::fetch_count(),
            1,
            "expected exactly one fetch under contention"
        );
        assert!(is_cache_complete());
        let marker = cache_dir().unwrap().join("data/meta/marker");
        assert!(
            marker.exists(),
            "meta/marker missing after ensure_meta_stopping race"
        );
        assert_eq!(count_partial_dirs(), 0);

        test_hooks::clear_fetch_source();
    }

    // ---------------------------------------------------------------------
    // #118 — progress callback + FetchErrorPayload tests
    // ---------------------------------------------------------------------

    /// `extract_tarball_with_progress` must invoke the callback at least
    /// once with `Extracting` per accepted entry, and the entry counter
    /// (`bytes_done`) must monotonically advance. Reaches the same
    /// progress code path that `install_from_tarball_with_progress`
    /// uses on the cache fill.
    #[test]
    fn progress_callback_fires_on_extract() {
        let _g = SERIAL.lock().unwrap();
        let td = isolated_home();
        let archive = td.path().join("test.tar.zst");
        make_test_tarball(&archive);
        let dest = td.path().join("dest");

        let mut events: Vec<FetchProgress> = Vec::new();
        extract_tarball_with_progress(&archive, &dest, &[], &mut |p| events.push(p)).unwrap();

        assert!(
            !events.is_empty(),
            "extract_tarball_with_progress did not emit any progress events"
        );
        // At least one Extracting event with bytes_done > 0.
        let max_done = events
            .iter()
            .filter(|e| matches!(e.stage, FetchStage::Extracting))
            .map(|e| e.bytes_done)
            .max()
            .unwrap_or(0);
        assert!(
            max_done >= 1,
            "expected ≥1 Extracting event with bytes_done ≥ 1, got events: {events:?}"
        );

        // Monotonic non-decreasing entry count.
        let extracting: Vec<u64> = events
            .iter()
            .filter(|e| matches!(e.stage, FetchStage::Extracting))
            .map(|e| e.bytes_done)
            .collect();
        for w in extracting.windows(2) {
            assert!(w[1] >= w[0], "Extracting progress went backwards: {w:?}");
        }
    }

    /// `install_from_tarball_with_progress` must emit at least one
    /// `Extracting` and one `Verifying` event. Production-shape: this
    /// is what the desktop `ensure_data` command calls on a warm cache
    /// without a network round trip.
    #[test]
    fn progress_callback_emits_extracting_and_verifying() {
        let _g = SERIAL.lock().unwrap();
        let td = isolated_home();
        let archive = td.path().join("test.tar.zst");
        make_test_tarball(&archive);

        let mut stages: Vec<FetchStage> = Vec::new();
        install_tarball_unverified(&archive, &mut |p| stages.push(p.stage)).unwrap();

        assert!(
            stages.iter().any(|s| matches!(s, FetchStage::Extracting)),
            "no Extracting stage observed: {stages:?}"
        );
        assert!(
            stages.iter().any(|s| matches!(s, FetchStage::Verifying)),
            "no Verifying stage observed: {stages:?}"
        );
        assert!(is_cache_complete());
    }

    /// `FetchErrorPayload::from(&FetchError)` must always populate
    /// `url` and `cache_dir` for the variants that carry them, and
    /// the JSON wire-form must include the `kind: "FetchError"` tag.
    #[test]
    fn fetch_error_payload_serializes_with_url_and_cache_dir() {
        let _g = SERIAL.lock().unwrap();
        let _td = isolated_home();

        let cases: Vec<FetchError> = vec![
            FetchError::HttpStatus(404),
            FetchError::Network("dns lookup failed".to_string()),
            FetchError::Decompress("zstd: bad magic".to_string()),
            FetchError::Extract("unexpected eof".to_string()),
            FetchError::Io(io::Error::other("disk full")),
            FetchError::UnsafeTarballEntry {
                kind: "Symlink".to_string(),
                path: PathBuf::from("data/meta/evil"),
            },
            FetchError::NoHome,
        ];

        for err in &cases {
            let payload = FetchErrorPayload::from(err);
            let s = payload.to_json_string();
            // Every variant carries the top-level kind.
            assert!(
                s.contains("\"kind\":\"FetchError\""),
                "missing kind discriminator for {err:?}: {s}"
            );

            // Variants that should expose the canonical URL.
            let needs_url = matches!(err, FetchError::HttpStatus(_) | FetchError::Network(_));
            if needs_url {
                let url = release_url();
                assert!(
                    s.contains(&url),
                    "variant {err:?} should embed release URL {url:?}, got {s}"
                );
            }

            // Variants that should expose the cache_dir (everything except NoHome).
            let needs_cache_dir = !matches!(err, FetchError::NoHome);
            if needs_cache_dir {
                assert!(
                    s.contains("\"cache_dir\":"),
                    "variant {err:?} should embed cache_dir, got {s}"
                );
            }
        }
    }

    /// JSON wire-form for an HttpStatus error must round-trip into a
    /// shape the frontend `parseFetchError` parser expects: top-level
    /// kind/variant + flat fields.
    #[test]
    fn fetch_error_payload_http_status_wire_shape() {
        let _g = SERIAL.lock().unwrap();
        let _td = isolated_home();

        let payload = FetchErrorPayload::from(&FetchError::HttpStatus(404));
        let s = payload.to_json_string();
        let v: serde_json::Value = serde_json::from_str(&s).expect("payload is valid JSON");
        assert_eq!(v["kind"], "FetchError");
        assert_eq!(v["variant"], "HttpStatus");
        assert_eq!(v["status"], 404);
        assert!(v["url"].is_string());
        assert!(v["cache_dir"].is_string());
        assert!(v["message"].is_string());
    }

    // ----- #173 redact_home unit tests ---------------------------------
    //
    // These cover the boundary helper directly so a future refactor that
    // moves the call site can't silently drop redaction. The
    // `FetchErrorPayload` regression test in the next test asserts the
    // end-to-end JSON has no $HOME literal.

    #[test]
    fn redact_home_strips_home_prefix() {
        let _g = SERIAL.lock().unwrap();
        // We can't rely on `isolated_home()` here because `home_dir()`
        // on macOS/Linux reads `$HOME` directly, which is what we want
        // for this test — but we need a stable, known prefix.
        std::env::set_var("HOME", "/tmp/fakehome");
        let p = PathBuf::from("/tmp/fakehome/.hyrr/nucl-parquet/v0.10.0");
        let got = redact_home(&p);
        assert_eq!(got, "~/.hyrr/nucl-parquet/v0.10.0");
    }

    #[test]
    fn redact_home_passthrough_for_non_home_paths() {
        let _g = SERIAL.lock().unwrap();
        std::env::set_var("HOME", "/tmp/fakehome");
        let p = PathBuf::from("/etc/passwd");
        let got = redact_home(&p);
        assert_eq!(got, "/etc/passwd");
    }

    #[test]
    fn redact_home_handles_empty_home_env() {
        let _g = SERIAL.lock().unwrap();
        // Empty $HOME would otherwise rewrite every path to `~/...` via
        // a zero-length strip_prefix match. We guard against that.
        std::env::set_var("HOME", "");
        let p = PathBuf::from("/etc/passwd");
        let got = redact_home(&p);
        assert_eq!(got, "/etc/passwd");
    }

    #[test]
    fn redact_home_handles_missing_home_env() {
        let _g = SERIAL.lock().unwrap();
        std::env::remove_var("HOME");
        let p = PathBuf::from("/etc/passwd");
        let got = redact_home(&p);
        // With $HOME unset `home::home_dir()` may consult passwd / SHGetFolderPathW.
        // The contract is "no panic, sensible string out" — exact equality
        // depends on the platform's fallback, so just assert the path
        // doesn't gain a spurious `~` prefix when it didn't match home.
        assert!(
            got == "/etc/passwd" || !got.starts_with("~/etc"),
            "unexpected rewrite of non-home path: {got:?}"
        );
    }

    /// #173 regression guard: the structured `cache_dir` / `entry_path`
    /// fields of the JSON wire-form MUST NOT contain the user's `$HOME`
    /// literal. This pins the redaction at the IPC boundary so a future
    /// refactor that re-routes a path through `.display()` without
    /// `redact_home` gets caught at CI time rather than in a bug-report
    /// comment thread.
    ///
    /// Scope note (#173 issue body, "Out of scope" bullet): the
    /// per-variant `message` string is built from
    /// `FetchError::to_string()`, which today re-emits absolute paths
    /// via the thiserror `Display` impl (e.g.
    /// `unsafe tarball entry Symlink at /Users/alice/...`). Redacting
    /// those error strings is tracked under the broader #159 privacy
    /// contract, so this regression test deliberately asserts only on
    /// the structured fields — not on the full payload body.
    #[test]
    fn fetch_error_payload_json_redacts_home() {
        let _g = SERIAL.lock().unwrap();
        // Use a marker `$HOME` that's trivially identifiable in the
        // output so the assert is unambiguous.
        let td = tempfile::tempdir().expect("tempdir");
        std::env::set_var("HOME", td.path());
        let home_str = td.path().display().to_string();

        // Variant 1: Io carries cache_dir, which is built from
        // cache_dir() → $HOME/.hyrr/nucl-parquet/v{V}.
        let err = FetchError::Io(io::Error::other("disk full"));
        let payload = FetchErrorPayload::from(&err);
        let s = payload.to_json_string();
        let v: serde_json::Value = serde_json::from_str(&s).expect("valid JSON");
        let cache_dir = v["cache_dir"].as_str().expect("cache_dir is a string");
        assert!(
            !cache_dir.contains(&home_str),
            "cache_dir leaked $HOME ({home_str:?}): {cache_dir}"
        );
        assert!(
            cache_dir.starts_with("~/.hyrr/nucl-parquet"),
            "cache_dir should be redacted to ~/.hyrr/..., got {cache_dir}"
        );
        // Also assert no platform-shaped username prefix snuck in via
        // some other transform.
        for needle in &["/Users/", "/home/", "C:\\Users\\"] {
            assert!(
                !cache_dir.contains(needle),
                "cache_dir contained suspicious path prefix {needle:?}: {cache_dir}"
            );
        }

        // Variant 2: UnsafeTarballEntry with an absolute path that
        // happens to live under $HOME. Today this can't arise through
        // production code (tarball paths are relative), but we exercise
        // the defensive routing the issue body calls out.
        let evil = td.path().join("evil");
        let err2 = FetchError::UnsafeTarballEntry {
            kind: "Symlink".to_string(),
            path: evil,
        };
        let payload2 = FetchErrorPayload::from(&err2);
        let s2 = payload2.to_json_string();
        let v2: serde_json::Value = serde_json::from_str(&s2).expect("valid JSON");
        let entry_path = v2["entry_path"].as_str().expect("entry_path is a string");
        let cache_dir2 = v2["cache_dir"].as_str().expect("cache_dir is a string");
        assert!(
            !entry_path.contains(&home_str),
            "entry_path leaked $HOME ({home_str:?}): {entry_path}"
        );
        assert_eq!(entry_path, "~/evil", "entry_path should redact to ~/evil");
        assert!(
            !cache_dir2.contains(&home_str),
            "cache_dir leaked $HOME in UnsafeTarballEntry variant ({home_str:?}): {cache_dir2}"
        );
    }

    /// #176 regression guard: the `init_data_store` Tauri command in
    /// `desktop/src-tauri/src/commands.rs` formats its `resolved` data
    /// directory into the error string it returns over IPC. When
    /// `resolved` is produced by `data_dir::resolve()` (the empty-
    /// `data_dir` branch) the path lives under `~/.hyrr/...` — i.e.
    /// `/Users/<name>/.hyrr/...` on macOS, leaking the OS username.
    ///
    /// The desktop crate has no `[dev-dependencies]` test harness and
    /// `init_data_store` requires a Tauri runtime to drive end-to-end,
    /// so we mirror the formatter inline and exercise the same boundary
    /// the production code uses (`redact_home(Path::new(&resolved))`).
    /// If a future refactor of `commands.rs` drops the `redact_home`
    /// call, this test still passes — but the partner assertion that
    /// `redact_home` _itself_ keeps producing `~/...` for the home-
    /// prefixed input is what catches the regression class.
    #[test]
    fn init_data_store_error_string_redacts_home() {
        let _g = SERIAL.lock().unwrap();
        let td = tempfile::tempdir().expect("tempdir");
        std::env::set_var("HOME", td.path());
        let home_str = td.path().display().to_string();

        // Shape that `data_dir::resolve()` returns when the managed
        // cache is the resolution winner — i.e. the path the issue
        // body singles out as the leak source. We don't call
        // `resolve()` directly because it consults the live cache;
        // the literal shape is what we're pinning.
        let resolved = format!("{home_str}/.hyrr/nucl-parquet/v0.10.0/data");

        // Mirror the formatter in `init_data_store` exactly. The fake
        // inner error stands in for `ParquetDataStore::new`'s Err —
        // we only care that the path interpolation is redacted, not
        // the wrapped error's content.
        let resolved_display = redact_home(Path::new(&resolved));
        let inner_err = "missing file: meta/abundances.parquet";
        let err_string = format!("Failed to init DB at {resolved_display}: {inner_err}");

        // Primary assertion: no literal `$HOME` in the error string.
        assert!(
            !err_string.contains(&home_str),
            "init_data_store error leaked $HOME ({home_str:?}): {err_string}"
        );
        // The redaction should also avoid platform-shaped username
        // prefixes (defensive — `home_dir()` could return one of these
        // under exotic environments).
        for needle in &["/Users/", "/home/", "C:\\Users\\"] {
            assert!(
                !err_string.contains(needle),
                "init_data_store error contained suspicious path prefix \
                 {needle:?}: {err_string}"
            );
        }
        // And the positive shape — the redacted form should appear so
        // we know the helper actually fired (not "passed because the
        // path was empty").
        assert!(
            err_string.contains("~/.hyrr/nucl-parquet"),
            "expected ~/.hyrr/... shape in error, got: {err_string}"
        );
    }

    // -----------------------------------------------------------------
    // FetchProgressThrottle — sink-agnostic rate-limiter (issue #180)
    // -----------------------------------------------------------------

    fn dl(bytes_done: u64, bytes_total: Option<u64>) -> FetchProgress {
        FetchProgress {
            stage: FetchStage::Downloading,
            bytes_done,
            bytes_total,
        }
    }

    fn ev(stage: FetchStage, bytes_done: u64, bytes_total: Option<u64>) -> FetchProgress {
        FetchProgress {
            stage,
            bytes_done,
            bytes_total,
        }
    }

    /// First-ever event always emits (stage-changed bypass: last_stage
    /// is `None`). Then a Downloading→Extracting transition immediately
    /// emits, even though only nanoseconds have passed — the stage
    /// change bypasses both the 100 ms interval and the 256 KiB
    /// byte-step gate.
    #[test]
    fn throttle_emits_on_stage_change() {
        let mut t = FetchProgressThrottle::default_policy();

        // First event ever — stage_changed=true (None → Some).
        assert!(t.should_emit(&dl(0, Some(1_000_000))));

        // Second Downloading event arrives immediately and < 256 KiB
        // later — should be dropped on the byte gate.
        assert!(!t.should_emit(&dl(1_024, Some(1_000_000))));

        // Stage flips to Extracting — should emit immediately,
        // bypassing the 100 ms interval that hasn't elapsed.
        assert!(t.should_emit(&ev(FetchStage::Extracting, 0, Some(10))));

        // And Extracting → Verifying flips immediately too.
        assert!(t.should_emit(&ev(FetchStage::Verifying, 0, None)));
    }

    /// On non-Downloading stages (where the byte-step gate is
    /// dropped), events arriving inside the 100 ms window are
    /// suppressed; events arriving after ≥100 ms pass through.
    #[test]
    fn throttle_respects_100ms_interval() {
        let mut t = FetchProgressThrottle::default_policy();

        // Seed: first Extracting event always emits.
        assert!(t.should_emit(&ev(FetchStage::Extracting, 1, Some(100))));

        // Immediate follow-up — within 100 ms, no stage change, so
        // dropped on the interval gate (byte-step doesn't apply).
        assert!(!t.should_emit(&ev(FetchStage::Extracting, 2, Some(100))));

        // After ≥100 ms, the next event passes.
        std::thread::sleep(Duration::from_millis(120));
        assert!(t.should_emit(&ev(FetchStage::Extracting, 3, Some(100))));
    }

    /// On the Downloading stage, the 256 KiB byte-step gate ANDs with
    /// the 100 ms interval gate — a small byte delta is dropped even
    /// after the interval elapses.
    #[test]
    fn throttle_respects_256kib_byte_step_on_downloading() {
        let mut t = FetchProgressThrottle::default_policy();

        // Seed at 0 bytes.
        assert!(t.should_emit(&dl(0, Some(10_000_000))));

        // Wait past the interval gate so only the byte gate can block.
        std::thread::sleep(Duration::from_millis(120));

        // Δ = 64 KiB < 256 KiB — dropped on the byte gate even
        // though the 100 ms window has elapsed.
        assert!(!t.should_emit(&dl(64 * 1024, Some(10_000_000))));

        // Δ = 256 KiB exactly — meets the threshold, emits.
        assert!(t.should_emit(&dl(256 * 1024, Some(10_000_000))));
    }

    /// The final-byte event (`bytes_done >= bytes_total`, with
    /// `bytes_total > 0`) bypasses both gates so the progress bar
    /// snaps cleanly to 100% before the stage flips.
    #[test]
    fn throttle_emits_final_byte_event_regardless_of_throttle() {
        let mut t = FetchProgressThrottle::default_policy();
        let total: u64 = 1_000_000;

        // Seed with the first chunk; stage-changed bypass fires.
        assert!(t.should_emit(&dl(0, Some(total))));

        // Immediate follow-up at the final byte — would normally be
        // suppressed (interval < 100 ms, byte gate could fail too),
        // but the final_byte bypass forces an emit.
        assert!(t.should_emit(&dl(total, Some(total))));

        // bytes_total = None means we can't compute "final" — those
        // events fall through the normal gates. With no time elapsed
        // and a small byte delta, the second event drops.
        let mut t2 = FetchProgressThrottle::default_policy();
        assert!(t2.should_emit(&dl(0, None)));
        assert!(!t2.should_emit(&dl(1, None)));
    }

    /// `bytes_done` carries different semantics across stages
    /// (compressed download bytes vs. entry counts), so on a stage
    /// transition the bytes counter resets — the next event in the
    /// new stage must not be measured against the old stage's
    /// `last_bytes`.
    #[test]
    fn throttle_resets_bytes_on_stage_transition() {
        let mut t = FetchProgressThrottle::default_policy();

        // Walk through a download up to 10 MiB.
        assert!(t.should_emit(&dl(0, Some(10 * 1024 * 1024))));
        std::thread::sleep(Duration::from_millis(120));
        assert!(t.should_emit(&dl(10 * 1024 * 1024, Some(10 * 1024 * 1024))));

        // Now the stage flips to Extracting with bytes_done=1
        // (entry index, not a 10 MiB regression). Stage-change
        // bypass fires.
        assert!(t.should_emit(&ev(FetchStage::Extracting, 1, Some(100))));

        // Wait past the interval. The next Extracting event at
        // bytes_done=2 must emit — last_bytes was reset to 1 on the
        // transition, so the byte gate (which Extracting doesn't even
        // apply, but the internal reset matters) doesn't compare 2
        // against the old 10 MiB value.
        std::thread::sleep(Duration::from_millis(120));
        assert!(t.should_emit(&ev(FetchStage::Extracting, 2, Some(100))));
    }

    // ---------------------------------------------------------------------
    // #263 — data-fetch path coverage
    // ---------------------------------------------------------------------

    /// `seed_from_dir` copies bundled XS library directories (those
    /// containing an `xs/` subdirectory) alongside the mandatory
    /// meta/stopping seed. Verifies the #264 US1 installer path.
    #[test]
    fn seed_from_dir_copies_bundled_xs_library() {
        let _g = SERIAL.lock().unwrap();
        let td = isolated_home();
        let src = td.path().join("bundle");
        // Mandatory resources.
        fs::create_dir_all(src.join("meta")).unwrap();
        fs::create_dir_all(src.join("stopping")).unwrap();
        fs::write(src.join("meta/elements.parquet"), b"meta").unwrap();
        fs::write(src.join("stopping/stopping.parquet"), b"stop").unwrap();
        fs::write(src.join("catalog.json"), b"{}").unwrap();
        fs::write(src.join("suppliers.json"), b"{}").unwrap();
        // Bundled library.
        fs::create_dir_all(src.join("tendl-2023-iso/xs")).unwrap();
        fs::write(src.join("tendl-2023-iso/xs/p_Cu.parquet"), b"xs-data").unwrap();
        fs::write(src.join("tendl-2023-iso/manifest.json"), b"{}").unwrap();

        seed_from_dir(&src).unwrap();
        assert!(is_cache_complete());

        let cached_xs = cache_dir()
            .unwrap()
            .join("data/tendl-2023-iso/xs/p_Cu.parquet");
        assert!(cached_xs.exists(), "bundled XS library was not seeded");
        assert_eq!(fs::read(&cached_xs).unwrap(), b"xs-data");
    }

    /// Non-library directories (no `xs/` subdir) in the source MUST NOT
    /// be copied by the library-scan step. Only `xs/`-bearing dirs pass.
    #[test]
    fn seed_from_dir_skips_non_library_dirs() {
        let _g = SERIAL.lock().unwrap();
        let td = isolated_home();
        let src = td.path().join("bundle");
        fs::create_dir_all(src.join("meta")).unwrap();
        fs::create_dir_all(src.join("stopping")).unwrap();
        fs::write(src.join("meta/elements.parquet"), b"meta").unwrap();
        fs::write(src.join("stopping/stopping.parquet"), b"stop").unwrap();
        fs::write(src.join("catalog.json"), b"{}").unwrap();
        fs::write(src.join("suppliers.json"), b"{}").unwrap();
        // A directory without xs/ — should NOT be copied.
        fs::create_dir_all(src.join("random-dir")).unwrap();
        fs::write(src.join("random-dir/junk"), b"nope").unwrap();

        seed_from_dir(&src).unwrap();
        assert!(!cache_dir().unwrap().join("data/random-dir").exists());
    }

    /// `ensure_library` short-circuits when the sentinel is present
    /// AND the library directory already exists (the warm-cache path).
    /// Install the test tarball first to populate the cache, then
    /// verify ensure_library returns immediately without fetching.
    #[test]
    fn ensure_library_short_circuits_on_warm_cache() {
        let _g = SERIAL.lock().unwrap();
        let td = isolated_home();
        let archive = td.path().join("test.tar.zst");
        make_test_tarball(&archive); // contains data/tendl-test/xs/p_Cu.parquet

        // Populate the cache via install_from_tarball (which doesn't
        // need network).
        install_unverified(&archive).unwrap();
        assert!(is_cache_complete());

        let lib_dir = cache_dir().unwrap().join("data/tendl-test");
        assert!(lib_dir.exists(), "library subtree was not extracted");

        // ensure_library with the installed library is a no-op.
        // It should return Ok immediately without hitting the network.
        ensure_library("tendl-test").unwrap();
    }

    /// `ensure_library` returns an error (not a panic) when the cache
    /// is complete but the requested library doesn't exist and
    /// network fetch fails (simulated by the test seam returning a
    /// tarball that doesn't contain the requested library).
    #[test]
    fn ensure_library_on_missing_library_tries_fetch() {
        let _g = SERIAL.lock().unwrap();
        let td = isolated_home();
        let archive = td.path().join("test.tar.zst");
        make_test_tarball(&archive);

        // Install to get a complete cache with tendl-test only.
        install_unverified(&archive).unwrap();
        assert!(is_cache_complete());

        // tendl-test exists, but tendl-nonexistent does not.
        // On a real system this would try to fetch. With the test
        // seam we can verify it doesn't short-circuit.
        let cd = cache_dir().unwrap();
        assert!(!cd.join("data/tendl-nonexistent").exists());
        // Don't actually call ensure_library("tendl-nonexistent") here
        // because it would try to fetch from GitHub. The point is
        // established: the sentinel check alone isn't sufficient,
        // the library directory must also exist.
    }
}

/// Publisher-authenticity tests (#594).
///
/// The `.minisig` below is the **real** signature nucl-parquet published for
/// `data-2026.8.2` (upstream #289 / PR #290), committed as a fixture. That is
/// what lets these run offline while still exercising the actual key.
#[cfg(test)]
mod signature_tests {
    use super::*;

    /// Verbatim `nucl-parquet-data-2026.8.2.tar.zst.minisig`.
    pub(super) const REAL_SIGNATURE: &str = "untrusted comment: signature from minisign secret key\n\
RUT9cED7yeXhJxFaigiCR27bM7KoB6YOIbeHCe3QjgsRO06wewCbcKiz5sV/s9rEZMSFckcIw0lFe03g2qB+itZhC1eve+nUlwg=\n\
trusted comment: nucl-parquet data 2026.8.2 tag=data-2026.8.2 sha256=c19cd3fd650ee3747f9eb5c3b2c171a199a86bfbb2426c1afa1f589b12be4166\n\
3UWhi07Z3NGnqV06uJa1I1AnTOqFUVQiUKWZ2adA7vb29UrN/dTO91f+/ESC59VBN0F6P6nEYI1IEhNt+jhmBg==\n";

    #[test]
    fn the_pinned_public_key_decodes() {
        signing_public_key().expect("hyrr.json's data_signing_pubkey must be a valid minisign key");
    }

    /// The published signature must actually be usable with the key we pinned.
    /// A key-id mismatch — the shape a key rotation takes — fails here rather
    /// than after an ~800 MB download on a user's machine.
    fn real_signature() -> minisign_verify::Signature {
        minisign_verify::Signature::decode(REAL_SIGNATURE).expect("fixture must parse")
    }

    #[test]
    fn the_published_signature_matches_the_pinned_key() {
        let pk = signing_public_key().unwrap();
        pk.verify_stream(&real_signature())
            .expect("pinned key must be able to verify the published signature");
    }

    /// **The pin is anchored to the signing key, not to GitHub.**
    ///
    /// `just repin-data` reads GitHub's asset digest, and upstream releases
    /// are mutable — so that digest is a convenience, not a control. This
    /// asserts that what we pinned in `hyrr.json` is what the *key holder*
    /// signed. It runs offline, so a re-pin that drifted from the signature
    /// is caught in CI rather than at a user's first fetch.
    #[test]
    fn the_committed_pin_equals_the_publishers_signed_claim() {
        let sig = real_signature();
        let signed = sig
            .trusted_comment()
            .split_whitespace()
            .find_map(|f| f.strip_prefix("sha256="))
            .expect("upstream's trusted comment carries the digest");
        assert!(
            signed.eq_ignore_ascii_case(DATA_TARBALL_SHA256),
            "hyrr.json pins {DATA_TARBALL_SHA256} but the publisher signed {signed}"
        );
    }

    /// Tampering must be caught. Feeds the real signature bytes that are not
    /// the payload it covers.
    #[test]
    fn a_tampered_payload_fails_verification() {
        let pk = signing_public_key().unwrap();
        let sig = real_signature();
        let mut v = pk.verify_stream(&sig).unwrap();
        v.update(b"this is emphatically not 785 MB of nuclear data");
        assert!(
            v.finalize().is_err(),
            "a payload the key did not sign must be refused"
        );
    }

    /// An empty payload is still a payload — the streaming verifier must not
    /// treat "no bytes fed" as success.
    #[test]
    fn an_empty_payload_fails_verification() {
        let pk = signing_public_key().unwrap();
        let sig = real_signature();
        assert!(pk.verify_stream(&sig).unwrap().finalize().is_err());
    }

    #[test]
    fn trusted_comment_digest_agrees() {
        assert!(verify_trusted_comment_digest("tag=x sha256=ABCD", "abcd").is_ok());
    }

    #[test]
    fn trusted_comment_digest_disagreeing_is_refused() {
        let err = verify_trusted_comment_digest("tag=x sha256=aaaa", "bbbb")
            .expect_err("a comment that contradicts the bytes must be refused");
        assert!(matches!(err, FetchError::SignatureInvalid { .. }));
        assert!(err.to_string().contains("aaaa"));
    }

    /// Upstream's comment format is not ours to mandate: the signature over
    /// the payload already establishes authenticity, so a comment without a
    /// digest field is a cross-check we simply skip — not a second gate that
    /// upstream could break by reformatting.
    #[test]
    fn trusted_comment_without_a_digest_is_not_an_error() {
        assert!(verify_trusted_comment_digest("timestamp:1234 file:x.tar.zst", "abcd").is_ok());
    }

    /// Live end-to-end: fetch the real `.minisig` and confirm it is present,
    /// parses, and matches the pinned key. Opt-in — CI must not depend on
    /// GitHub being reachable.
    ///
    /// ```text
    /// cargo test --manifest-path core/Cargo.toml -- --ignored live_signature
    /// ```
    #[test]
    #[ignore = "requires network"]
    fn live_signature_is_published_and_matches_the_pin() {
        let client = build_http_client().unwrap();
        let sig = fetch_detached_signature(&client).expect("release must publish a .minisig");
        let pk = signing_public_key().unwrap();
        pk.verify_stream(&sig).expect("key id must match");
        let signed = sig
            .trusted_comment()
            .split_whitespace()
            .find_map(|f| f.strip_prefix("sha256="))
            .expect("digest in trusted comment");
        assert!(signed.eq_ignore_ascii_case(DATA_TARBALL_SHA256));
    }
}

/// Air-gapped install-path tests (#614).
///
/// The offline path is the one users at isolated sites depend on most, and
/// until this work it was the *only* unverified way into the cache. These
/// cover the refusal rules, not the happy path — a bundle that installs is
/// already covered by the extraction tests.
#[cfg(test)]
mod offline_signature_tests {
    use super::*;

    fn fixture_signature() -> minisign_verify::Signature {
        minisign_verify::Signature::decode(super::signature_tests::REAL_SIGNATURE).unwrap()
    }

    /// The core refusal. A bundle with no signature beside it must not
    /// install — a stripped signature and a pre-signing release are
    /// indistinguishable from here.
    #[test]
    fn a_bundle_without_a_signature_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("bundle.tar.zst");
        fs::write(&archive, b"not really a tarball").unwrap();

        let err = locate_sibling_signature(&archive, None)
            .expect_err("an unsigned bundle must be refused, never installed unverified");
        assert!(matches!(err, FetchError::SignatureUnavailable { .. }));
        let msg = err.to_string();
        assert!(
            msg.contains("minisig"),
            "must name what it looked for: {msg}"
        );
        assert!(
            msg.contains("copy BOTH"),
            "must be actionable with no network to consult: {msg}"
        );
    }

    #[test]
    fn a_sibling_signature_is_found() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("bundle.tar.zst");
        fs::write(&archive, b"payload").unwrap();
        let sig = dir.path().join("bundle.tar.zst.minisig");
        fs::write(&sig, "x").unwrap();
        assert_eq!(locate_sibling_signature(&archive, None).unwrap(), sig);
    }

    /// The signature may have been carried separately from the payload.
    #[test]
    fn an_explicit_signature_path_overrides_the_sibling() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("bundle.tar.zst");
        fs::write(&archive, b"payload").unwrap();
        let elsewhere = dir.path().join("carried-separately.minisig");
        fs::write(&elsewhere, "x").unwrap();
        assert_eq!(
            locate_sibling_signature(&archive, Some(&elsewhere)).unwrap(),
            elsewhere
        );
    }

    #[test]
    fn an_override_pointing_at_nothing_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("bundle.tar.zst");
        fs::write(&archive, b"payload").unwrap();
        let missing = dir.path().join("nope.minisig");
        assert!(matches!(
            locate_sibling_signature(&archive, Some(&missing)),
            Err(FetchError::SignatureUnavailable { .. })
        ));
    }

    /// The version gate, read off the real published signature.
    #[test]
    fn bundle_version_is_parsed_from_the_signed_trusted_comment() {
        let sig = fixture_signature();
        assert_eq!(
            parse_bundle_version(sig.trusted_comment()),
            Some("2026.8.2")
        );
    }

    #[test]
    fn a_comment_without_a_tag_yields_no_version() {
        assert_eq!(parse_bundle_version("sha256=abc timestamp=1"), None);
    }

    /// A signature that omits `tag=` must be REFUSED, not waved through.
    /// Skipping the gate when the field is absent would mean the rollback
    /// defence quietly stops applying to exactly the signatures that omit it —
    /// which is the shape an attacker would choose.
    #[test]
    fn a_signature_without_a_tag_is_refused_not_skipped() {
        // Mirrors the production branch: `None` maps to VersionMismatch.
        let err = parse_bundle_version("sha256=abc")
            .ok_or_else(|| FetchError::VersionMismatch {
                expected: DATA_VERSION.to_string(),
                found: "unknown (the signature's trusted comment carries no tag=)".to_string(),
            })
            .unwrap_err();
        assert!(matches!(err, FetchError::VersionMismatch { .. }));
        assert!(err.to_string().contains("unknown"));
    }

    /// **Rollback defence.** A signature alone does not stop an old but
    /// validly-signed release being replayed onto a user; binding the install
    /// to the version this build pins is what does. The cache layout is keyed
    /// on `DATA_VERSION`, so a foreign-version bundle would also land in the
    /// wrong directory.
    #[test]
    fn a_foreign_version_bundle_is_refused() {
        let older = "nucl-parquet data 2026.7.2 tag=data-2026.7.2 sha256=deadbeef";
        let found = parse_bundle_version(older).unwrap();
        assert_ne!(
            found, DATA_VERSION,
            "fixture must differ from this build's pin for the test to mean anything"
        );
        let err = FetchError::VersionMismatch {
            expected: DATA_VERSION.to_string(),
            found: found.to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("2026.7.2") && msg.contains(DATA_VERSION));
        assert!(
            msg.contains("Nothing was installed"),
            "must state that the cache is untouched: {msg}"
        );
    }

    #[test]
    fn version_mismatch_has_a_wire_payload() {
        let err = FetchError::VersionMismatch {
            expected: "2026.8.2".into(),
            found: "2026.7.2".into(),
        };
        match FetchErrorPayload::from(&err) {
            FetchErrorPayload::VersionMismatch {
                expected, found, ..
            } => {
                assert_eq!(expected, "2026.8.2");
                assert_eq!(found, "2026.7.2");
            }
            other => panic!("wrong payload variant: {other:?}"),
        }
    }

    /// A cache installed before verification existed must read as
    /// complete-but-unverified, never as verified. Silently claiming an
    /// assurance that was never established is worse than reporting none.
    #[test]
    fn a_legacy_cache_reports_complete_but_not_verified() {
        let _g = tests::SERIAL.lock().unwrap();
        let td = tests::isolated_home();
        let archive = td.path().join("legacy.tar.zst");
        tests::make_test_tarball(&archive);
        install_tarball_unverified(&archive, &mut no_op_progress()).unwrap();

        let status = cache_status();
        assert!(status.complete, "extraction finished");
        assert!(
            !status.verified,
            "nothing verified this tree, so it must not claim otherwise"
        );
        assert_eq!(status.signing_key, None);
        assert!(!is_cache_verified());
    }

    /// And once a verified install records it, the key is recoverable — so a
    /// later key rotation is visible without re-installing.
    #[test]
    fn a_verified_install_records_the_key_and_version() {
        let _g = tests::SERIAL.lock().unwrap();
        let td = tests::isolated_home();
        let archive = td.path().join("ok.tar.zst");
        tests::make_test_tarball(&archive);
        install_tarball_unverified(&archive, &mut no_op_progress()).unwrap();
        mark_cache_verified().unwrap();

        let status = cache_status();
        assert!(status.complete && status.verified);
        assert_eq!(status.signing_key.as_deref(), Some(DATA_SIGNING_PUBKEY));
        assert_eq!(status.data_version.as_deref(), Some(DATA_VERSION));
        assert!(is_cache_verified());
    }
}

/// End-to-end coverage of the manifest route (#621) — the path that survives a
/// content-scanning gateway repacking the archive in transit.
#[cfg(test)]
mod manifest_install_tests {
    use super::*;
    use crate::data_manifest::{ContentManifest, ManifestEntry};
    use std::collections::BTreeMap;

    /// `make_test_tarball` writes exactly these two members under `data/`.
    fn manifest_matching_the_fixture() -> ContentManifest {
        let mut files = BTreeMap::new();
        files.insert(
            "meta/marker".to_string(),
            ManifestEntry {
                sha256: hex_lower(&<sha2::Sha256 as sha2::Digest>::digest(b"test-marker")),
                size: 11,
            },
        );
        files.insert(
            "tendl-test/xs/p_Cu.parquet".to_string(),
            ManifestEntry {
                sha256: hex_lower(&<sha2::Sha256 as sha2::Digest>::digest(b"xs-marker")),
                size: 9,
            },
        );
        ContentManifest {
            manifest_version: 1,
            data_version: DATA_VERSION.to_string(),
            tag: format!("data-{DATA_VERSION}"),
            data_sha256: String::new(),
            file_count: files.len(),
            tarball_sha256: None,
            files,
        }
    }

    /// A repacked-but-intact archive installs when its contents match the
    /// signed manifest. This is the whole point: the framing changed, the data
    /// did not.
    #[test]
    fn a_tree_matching_the_manifest_is_promoted() {
        let _g = tests::SERIAL.lock().unwrap();
        let td = tests::isolated_home();
        let archive = td.path().join("repacked.tar.zst");
        tests::make_test_tarball(&archive);

        install_tarball_atomic_verified(
            fs::File::open(&archive).unwrap(),
            &["data"],
            Some(&manifest_matching_the_fixture()),
            &mut no_op_progress(),
        )
        .expect("contents match the manifest, so the repack is still trustworthy");

        assert!(is_cache_complete());
        assert!(cache_dir().unwrap().join("data/meta/marker").exists());
    }

    /// **Nothing is promoted when verification fails.** The check runs against
    /// the staging directory, so a tree that fails is deleted rather than
    /// merged into a working cache — a half-verified cache would be worse than
    /// a refused install.
    #[test]
    fn a_tree_disagreeing_with_the_manifest_is_not_promoted() {
        let _g = tests::SERIAL.lock().unwrap();
        let td = tests::isolated_home();
        let archive = td.path().join("tampered.tar.zst");
        tests::make_test_tarball(&archive);

        // Manifest omits `tendl-test/xs/p_Cu.parquet`, so the extracted tree
        // carries a file nobody signed for — the planted-file shape.
        let mut manifest = manifest_matching_the_fixture();
        manifest.files.remove("tendl-test/xs/p_Cu.parquet");
        manifest.file_count = manifest.files.len();

        let err = install_tarball_atomic_verified(
            fs::File::open(&archive).unwrap(),
            &["data"],
            Some(&manifest),
            &mut no_op_progress(),
        )
        .expect_err("an unlisted file must block the install");

        match &err {
            FetchError::ManifestMismatch { problems, total } => {
                assert_eq!(*total, 1);
                assert!(problems[0].contains("unexpected file"), "{problems:?}");
                assert!(problems[0].contains("p_Cu.parquet"), "{problems:?}");
            }
            other => panic!("wrong error: {other:?}"),
        }

        assert!(
            !is_cache_complete(),
            "a failed manifest check must leave the cache untouched"
        );
        assert!(!cache_dir().unwrap().join("data/meta/marker").exists());
    }

    /// A modified file is caught too, not just an added one.
    #[test]
    fn a_modified_file_blocks_the_install() {
        let _g = tests::SERIAL.lock().unwrap();
        let td = tests::isolated_home();
        let archive = td.path().join("modified.tar.zst");
        tests::make_test_tarball(&archive);

        let mut manifest = manifest_matching_the_fixture();
        manifest.files.get_mut("meta/marker").unwrap().sha256 = "0".repeat(64);

        let err = install_tarball_atomic_verified(
            fs::File::open(&archive).unwrap(),
            &["data"],
            Some(&manifest),
            &mut no_op_progress(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("modified"), "{err}");
        assert!(!is_cache_complete());
    }

    /// No manifest → unchanged behaviour, so releases predating
    /// `FIRST_MANIFEST_VERSION` (2026.8.3 upstream) still install.
    #[test]
    fn without_a_manifest_the_install_is_unchanged() {
        let _g = tests::SERIAL.lock().unwrap();
        let td = tests::isolated_home();
        let archive = td.path().join("plain.tar.zst");
        tests::make_test_tarball(&archive);

        install_tarball_atomic_verified(
            fs::File::open(&archive).unwrap(),
            &["data"],
            None,
            &mut no_op_progress(),
        )
        .unwrap();
        assert!(is_cache_complete());
    }

    /// Absence is normal — releases before upstream's FIRST_MANIFEST_VERSION
    /// (2026.8.3) have none — and must not read as failure.
    #[test]
    fn no_sibling_manifest_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("bundle.tar.zst");
        fs::write(&archive, b"x").unwrap();
        assert!(load_sibling_manifest(&archive).unwrap().is_none());
    }

    /// But a manifest with no signature beside it is refused. An unsigned
    /// manifest authenticates nothing: whoever can rewrite the files can
    /// rewrite it too.
    #[test]
    fn an_unsigned_sibling_manifest_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("bundle.tar.zst");
        fs::write(&archive, b"x").unwrap();
        fs::write(dir.path().join("bundle.tar.zst.manifest.json"), b"{}").unwrap();

        let err =
            load_sibling_manifest(&archive).expect_err("an unsigned manifest must not be used");
        assert!(matches!(err, FetchError::SignatureUnavailable { .. }));
        assert!(err.to_string().contains("authenticates nothing"));
    }

    /// A manifest signed by anything other than the pinned key is refused —
    /// this exercises the real key, using a signature that is valid but is for
    /// a tarball rather than this JSON.
    #[test]
    fn a_manifest_whose_signature_does_not_cover_it_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("bundle.tar.zst");
        fs::write(&archive, b"x").unwrap();
        fs::write(dir.path().join("bundle.tar.zst.manifest.json"), b"{}").unwrap();
        fs::write(
            dir.path().join("bundle.tar.zst.manifest.json.minisig"),
            super::signature_tests::REAL_SIGNATURE,
        )
        .unwrap();

        let err = load_sibling_manifest(&archive).unwrap_err();
        assert!(matches!(err, FetchError::SignatureInvalid { .. }), "{err}");
    }

    #[test]
    fn manifest_mismatch_has_a_wire_payload() {
        let err = FetchError::ManifestMismatch {
            problems: vec!["unexpected file: rogue.parquet".into()],
            total: 3,
        };
        match FetchErrorPayload::from(&err) {
            FetchErrorPayload::ManifestMismatch {
                problems, total, ..
            } => {
                assert_eq!(total, 3);
                assert_eq!(problems.len(), 1, "payload carries a bounded sample");
            }
            other => panic!("wrong payload: {other:?}"),
        }
    }
}

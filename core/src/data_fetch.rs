//! Lazy fetch + on-disk cache for nucl-parquet data tarballs.
//!
//! Populates `~/.hyrr/nucl-parquet/v{DATA_VERSION}/` from GitHub Releases on
//! demand. Hardened against the known failure modes of the simpler upstream
//! pattern (see #52 spike review):
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
/// `nucl-parquet-data-v0.10.0.tar.zst`. Single source of truth for the
/// pattern — `ensure_*` and the install path consume this.
pub fn tarball_filename() -> String {
    tarball_filename_for(DATA_VERSION)
}

/// Tarball filename for an arbitrary version. Exposed for offline-bundle
/// docs/tooling that need to spell a non-current version.
pub fn tarball_filename_for(version: &str) -> String {
    format!("nucl-parquet-data-v{version}.tar.zst")
}

/// Full release-tarball URL for the current [`DATA_VERSION`].
pub fn release_url() -> String {
    release_url_for(DATA_VERSION)
}

/// Full release-tarball URL for an arbitrary version.
pub fn release_url_for(version: &str) -> String {
    format!(
        "{RELEASE_BASE}/v{version}/{filename}",
        filename = tarball_filename_for(version),
    )
}

/// Human-readable cache-root pattern for diagnostics / UX, e.g.
/// `~/.hyrr/nucl-parquet/v0.10.0/data`. The literal returned here is
/// always interpolated against the live [`DATA_VERSION`]; callers that
/// need an actual filesystem path should use [`cache_dir`] instead.
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
}

impl FetchErrorPayload {
    /// Wrap as a top-level discriminator object — the wire shape the
    /// frontend actually sees. Equivalent to manually emitting
    /// `{"kind": "FetchError", "variant": ..., ...}`.
    pub fn to_json_string(&self) -> String {
        let inner = serde_json::to_value(self).unwrap_or_else(|_| {
            serde_json::json!({"variant": "Io", "message": "serialise failed"})
        });
        let mut obj = serde_json::Map::new();
        obj.insert("kind".to_string(), serde_json::Value::String("FetchError".to_string()));
        if let serde_json::Value::Object(m) = inner {
            for (k, v) in m {
                obj.insert(k, v);
            }
        }
        serde_json::to_string(&serde_json::Value::Object(obj))
            .unwrap_or_else(|_| "{\"kind\":\"FetchError\",\"variant\":\"Io\",\"message\":\"serialise failed\"}".to_string())
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
fn redact_home(p: &Path) -> String {
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
    let guard = process_lock()
        .lock()
        .unwrap_or_else(|p| p.into_inner());
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
    Ok(CacheLock { _file: file, _guard: guard })
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
    progress(FetchProgress { stage: FetchStage::Connecting, bytes_done: 0, bytes_total: None });

    let client = build_http_client().map_err(|e| FetchError::Network(e.to_string()))?;
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
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        io::Write::write_all(&mut file, &buf[..n])?;
        bytes_done += n as u64;
        progress(FetchProgress {
            stage: FetchStage::Downloading,
            bytes_done,
            bytes_total,
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
/// keeps only entries whose path *starts with* one of the listed prefixes
/// (the prefix is matched as a literal string against the entry path).
/// Pass `&[]` to extract everything.
///
/// Filters macOS `._*` resource-fork files which sometimes leak into
/// archives produced on Macs.
///
/// Path semantics: prefixes match the leading characters of `entry.path()`.
/// E.g. `"data/tendl-2025/"` matches `data/tendl-2025/xs/p_Cu.parquet` but
/// not `data/tendl-2024/`. The trailing slash matters — without it,
/// `"data/tendl-2"` would also match `data/tendl-2024/`. Callers should
/// always include the slash for directory-scoped extracts.
pub fn extract_tarball(
    archive: &Path,
    dest: &Path,
    prefixes: &[&str],
) -> Result<()> {
    let mut noop = no_op_progress();
    extract_tarball_with_progress(archive, dest, prefixes, &mut noop)
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
    let file = fs::File::open(archive)?;
    let decoder = zstd::stream::Decoder::new(file)
        .map_err(|e| FetchError::Decompress(e.to_string()))?;
    let mut tar = tar::Archive::new(decoder);

    fs::create_dir_all(dest)?;
    let mut entries_done: u64 = 0;
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
        let path = entry
            .path()
            .map_err(|e| FetchError::Extract(e.to_string()))?
            .into_owned();

        // Skip macOS resource-fork files
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.starts_with("._"))
            .unwrap_or(false)
        {
            continue;
        }

        if !prefixes.is_empty() {
            let path_str = path.to_string_lossy();
            // A prefix without a trailing slash matches a file (e.g.
            // "data/catalog.json"); with a trailing slash, only entries
            // strictly under that directory.
            let matches = prefixes.iter().any(|p| {
                if p.ends_with('/') {
                    path_str.starts_with(p)
                } else {
                    path_str == *p || path_str.starts_with(&format!("{p}/"))
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
                path,
            });
        }

        entry
            .unpack_in(dest)
            .map_err(|e| FetchError::Extract(e.to_string()))?;
        entries_done += 1;
        progress(FetchProgress {
            stage: FetchStage::Extracting,
            bytes_done: entries_done,
            bytes_total: None,
        });
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

    extract_tarball_with_progress(archive, &partial, prefixes, progress)?;

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

/// Produce a portable tarball of the current cache to `out`. Used by
/// `hyrr fetch-data --offline-bundle out.tar.zst`. The resulting tarball is
/// drop-in compatible with `install_from_tarball`.
pub fn export_offline_bundle(out: &Path) -> Result<()> {
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
    let encoder = zstd::stream::Encoder::new(file, 3)
        .map_err(|e| FetchError::Decompress(e.to_string()))?;
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

/// Progress-aware variant of [`install_from_tarball`].
pub fn install_from_tarball_with_progress(
    archive: &Path,
    progress: ProgressFn<'_>,
) -> Result<()> {
    if !archive.exists() {
        return Err(FetchError::Io(io::Error::new(
            io::ErrorKind::NotFound,
            format!("tarball not found: {}", archive.display()),
        )));
    }
    let _lock = acquire_lock()?;
    install_tarball_atomic(archive, &["data"], progress)
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
    let mut kept: std::collections::HashSet<(u64, u64, u64)> =
        std::collections::HashSet::new();
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
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Tests in this module must not run concurrently because they all mess
    /// with `$HOME` and the cache root.
    static SERIAL: Mutex<()> = Mutex::new(());

    /// Set $HOME to a fresh tempdir for the duration of the test.
    fn isolated_home() -> tempfile::TempDir {
        let td = tempfile::tempdir().expect("tempdir");
        std::env::set_var("HOME", td.path());
        td
    }

    /// Build a minimal v{V} tarball at `out` containing `data/meta/marker`.
    fn make_test_tarball(out: &Path) {
        let file = fs::File::create(out).unwrap();
        let encoder = zstd::stream::Encoder::new(file, 0).unwrap().auto_finish();
        let mut tar = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        let payload = b"test-marker";
        header.set_size(payload.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        tar.append_data(&mut header, "data/meta/marker", payload.as_slice()).unwrap();
        // Also include a library subtree so ensure_library can find it.
        let mut h2 = tar::Header::new_gnu();
        let p2 = b"xs-marker";
        h2.set_size(p2.len() as u64);
        h2.set_mode(0o644);
        h2.set_cksum();
        tar.append_data(&mut h2, "data/tendl-test/xs/p_Cu.parquet", p2.as_slice()).unwrap();
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
            url.starts_with("https://github.com/exoma-ch/nucl-parquet/releases/download/v"),
            "release_url() = {url:?} drifted from the canonical host/path"
        );
        assert!(
            url.ends_with(".tar.zst"),
            "release_url() = {url:?} should end with .tar.zst"
        );
        let fname = tarball_filename();
        assert!(
            fname.starts_with("nucl-parquet-data-v") && fname.ends_with(".tar.zst"),
            "tarball_filename() = {fname:?} drifted"
        );
        assert!(
            url.ends_with(&fname),
            "release_url() = {url:?} does not end in tarball_filename() = {fname:?}"
        );
    }

    #[test]
    fn data_version_is_resolved_from_submodule() {
        assert_ne!(DATA_VERSION, "0.0.0-unknown",
            "build.rs fell back — submodule not checked out at build time");
        let parts: Vec<&str> = DATA_VERSION.split('.').collect();
        assert_eq!(parts.len(), 3, "DATA_VERSION = {DATA_VERSION:?} is not N.N.N");
        for p in &parts {
            assert!(p.chars().all(|c| c.is_ascii_digit()),
                "DATA_VERSION component {p:?} not numeric");
        }
    }

    #[test]
    fn install_from_tarball_writes_sentinel_last() {
        let _g = SERIAL.lock().unwrap();
        let td = isolated_home();
        let archive = td.path().join("test.tar.zst");
        make_test_tarball(&archive);

        assert!(!is_cache_complete());
        install_from_tarball(&archive).unwrap();
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

        install_from_tarball(&archive).unwrap();
        let mtime1 = fs::metadata(sentinel_path().unwrap()).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        // Second call short-circuits — we still call install_from_tarball
        // unconditionally to verify it's safe; the function is allowed to
        // re-extract under the lock, but must not corrupt state.
        install_from_tarball(&archive).unwrap();
        assert!(is_cache_complete());
        let mtime2 = fs::metadata(sentinel_path().unwrap()).unwrap().modified().unwrap();
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

        install_from_tarball(&archive).unwrap();

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
    fn export_offline_bundle_round_trips() {
        let _g = SERIAL.lock().unwrap();
        let td = isolated_home();
        let archive = td.path().join("test.tar.zst");
        make_test_tarball(&archive);
        install_from_tarball(&archive).unwrap();

        let bundle = td.path().join("offline.tar.zst");
        export_offline_bundle(&bundle).unwrap();
        assert!(bundle.exists());

        // Move HOME to a fresh dir, ingest the bundle, verify the marker
        // is back.
        let td2 = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", td2.path());
        assert!(!is_cache_complete());
        install_from_tarball(&bundle).unwrap();
        assert!(is_cache_complete());
        let marker = cache_dir().unwrap().join("data/meta/marker");
        assert!(marker.exists());
    }

    #[test]
    fn export_refuses_when_cache_incomplete() {
        let _g = SERIAL.lock().unwrap();
        let _td = isolated_home();
        let bundle = std::env::temp_dir().join("offline.tar.zst");
        let err = export_offline_bundle(&bundle).unwrap_err();
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

        install_from_tarball(&archive).unwrap();

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
            let h1 = s.spawn(|| install_from_tarball(archive_ref));
            let h2 = s.spawn(|| install_from_tarball(archive_ref));
            (h1.join().expect("t1 panicked"), h2.join().expect("t2 panicked"))
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
        assert!(is_cache_complete(), "sentinel missing after both threads finished");
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
        let interrupted = install_from_tarball(&archive);
        assert!(interrupted.is_err(), "armed hook did not fire");
        // Hook is single-shot but clear defensively in case of test
        // re-entry.
        test_hooks::clear_pre_promote();

        // After the interrupted install: cache must be incomplete and
        // an orphan partial-{pid} should be visible to the sweep.
        assert!(!is_cache_complete(), "sentinel written despite interruption");
        assert!(
            count_partial_dirs() >= 1,
            "expected an orphan partial dir from the interrupted install"
        );

        // Second invocation: clean run. The orphan partial must be
        // swept, the cache promoted, and the sentinel re-asserted.
        install_from_tarball(&archive).expect("clean re-run failed");
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
                handles.push(s.spawn(|| ensure_meta_stopping()));
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
        assert!(marker.exists(), "meta/marker missing after ensure_meta_stopping race");
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
            assert!(
                w[1] >= w[0],
                "Extracting progress went backwards: {w:?}"
            );
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
        install_from_tarball_with_progress(&archive, &mut |p| stages.push(p.stage)).unwrap();

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
            let needs_url = matches!(
                err,
                FetchError::HttpStatus(_) | FetchError::Network(_)
            );
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
}

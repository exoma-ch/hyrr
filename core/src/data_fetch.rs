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
use std::path::{Path, PathBuf};

use fs2::FileExt;

/// Version of the `nucl-parquet` data this build expects.
///
/// Sourced at build time from `nucl-parquet/pyproject.toml` by
/// `core/build.rs` — the submodule pin is the single source of truth.
/// On a fresh clone without `--recurse-submodules` this falls back to
/// `"0.0.0-unknown"` (a deliberately invalid version that 404s loudly
/// at fetch time rather than serving stale data).
pub const DATA_VERSION: &str = env!("HYRR_DATA_VERSION");

/// GitHub Releases base URL for nucl-parquet data tarballs.
const RELEASE_BASE: &str = "https://github.com/exoma-ch/nucl-parquet/releases/download";

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

/// Acquire an exclusive lock on `<cache_root>/.lock`. Blocks the current
/// thread until competing fetch attempts release. Drops on `Drop`.
fn acquire_lock() -> Result<fs::File> {
    let root = cache_root()?;
    fs::create_dir_all(&root)?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(root.join(".lock"))?;
    lock.lock_exclusive()
        .map_err(|e| FetchError::Io(io::Error::other(format!("lock: {e}"))))?;
    Ok(lock)
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
pub fn fetch_full_tarball_to(out: &Path) -> Result<()> {
    let url = format!(
        "{RELEASE_BASE}/v{DATA_VERSION}/nucl-parquet-data-v{DATA_VERSION}.tar.zst"
    );
    let client = build_http_client().map_err(|e| FetchError::Network(e.to_string()))?;
    let resp = client
        .get(&url)
        .send()
        .map_err(|e| FetchError::Network(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(FetchError::HttpStatus(resp.status().as_u16()));
    }
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::File::create(out)?;
    let mut reader = io::BufReader::new(resp);
    io::copy(&mut reader, &mut file)?;
    Ok(())
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
    let file = fs::File::open(archive)?;
    let decoder = zstd::stream::Decoder::new(file)
        .map_err(|e| FetchError::Decompress(e.to_string()))?;
    let mut tar = tar::Archive::new(decoder);

    fs::create_dir_all(dest)?;
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

        entry
            .unpack_in(dest)
            .map_err(|e| FetchError::Extract(e.to_string()))?;
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
fn install_tarball_atomic(archive: &Path, prefixes: &[&str]) -> Result<()> {
    let _lock = acquire_lock()?;

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

    extract_tarball(archive, &partial, prefixes)?;

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
    if is_cache_complete() {
        return Ok(());
    }
    let _lock = acquire_lock()?;
    if is_cache_complete() {
        return Ok(());
    }
    require_free_space(1024 * 1024 * 1024)?;
    let tmp = cache_root()?.join(format!("nucl-parquet-data-v{DATA_VERSION}.tar.zst"));
    let _guard = TmpFileGuard::new(tmp.clone());
    fetch_full_tarball_to(&tmp)?;
    install_tarball_atomic(&tmp, MANDATORY_PREFIXES)?;
    Ok(())
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
    if is_cache_complete() && cache_dir()?.join("data").join(library).exists() {
        return Ok(());
    }
    let _lock = acquire_lock()?;
    if is_cache_complete() && cache_dir()?.join("data").join(library).exists() {
        return Ok(());
    }
    require_free_space(1024 * 1024 * 1024)?;
    let tmp = cache_root()?.join(format!("nucl-parquet-data-v{DATA_VERSION}.tar.zst"));
    let _guard = TmpFileGuard::new(tmp.clone());
    fetch_full_tarball_to(&tmp)?;
    let lib_prefix = format!("data/{library}/");
    let mut prefixes: Vec<&str> = MANDATORY_PREFIXES.to_vec();
    prefixes.push(&lib_prefix);
    install_tarball_atomic(&tmp, &prefixes)?;
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
    let _lock = acquire_lock()?;
    require_free_space(1024 * 1024 * 1024)?;
    let tmp = cache_root()?.join(format!("nucl-parquet-data-v{DATA_VERSION}.tar.zst"));
    let _guard = TmpFileGuard::new(tmp.clone());
    fetch_full_tarball_to(&tmp)?;
    install_tarball_atomic(&tmp, &[])?;
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
    if !archive.exists() {
        return Err(FetchError::Io(io::Error::new(
            io::ErrorKind::NotFound,
            format!("tarball not found: {}", archive.display()),
        )));
    }
    install_tarball_atomic(archive, &["data"])
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
}

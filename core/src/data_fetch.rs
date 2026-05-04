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
/// MUST match the version of the pinned `nucl-parquet/` submodule. When
/// bumping the submodule, also bump this constant. Mismatch will produce a
/// 404 from GitHub Releases at fetch time, which is detectable but ugly.
pub const DATA_VERSION: &str = "0.9.0";

/// GitHub Releases base URL for nucl-parquet data tarballs.
const RELEASE_BASE: &str = "https://github.com/exoma-ch/nucl-parquet/releases/download";

/// Subdirectories that are required by *every* simulation regardless of the
/// chosen library. Bundled in the Tauri installer; `ensure_meta_stopping`
/// fetches them only when not already present (e.g. the Python CLI path).
pub const ALWAYS_NEEDED: &[&str] = &["meta", "stopping"];

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

/// Download the full nucl-parquet data tarball into `out`.
///
/// Streams the response to disk — does not buffer the full ~400 MB in RAM.
pub fn fetch_full_tarball_to(out: &Path) -> Result<()> {
    let url = format!(
        "{RELEASE_BASE}/v{DATA_VERSION}/nucl-parquet-data-v{DATA_VERSION}.tar.zst"
    );
    let resp = reqwest::blocking::get(&url)
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

/// Extract a `.tar.zst` archive into `dest`, keeping only entries whose
/// top-level path component is in `keep` (or all entries if `keep` is empty).
///
/// Filters macOS `._*` resource-fork files which sometimes leak into archives
/// produced on Macs.
pub fn extract_tarball(
    archive: &Path,
    dest: &Path,
    keep: &[&str],
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

        if !keep.is_empty() {
            let top = path.components().next().and_then(|c| c.as_os_str().to_str());
            if !top.map(|t| keep.contains(&t)).unwrap_or(false) {
                continue;
            }
        }

        entry
            .unpack_in(dest)
            .map_err(|e| FetchError::Extract(e.to_string()))?;
    }
    Ok(())
}

/// Atomically install a downloaded tarball into the versioned cache dir.
///
/// 1. Take the cache lock.
/// 2. Extract into `<cache_root>/v{V}.partial-{pid}/`.
/// 3. `fs::rename` to `<cache_root>/v{V}/`.
/// 4. Write the `.complete` sentinel.
///
/// `keep` filters which top-level directories from the archive are extracted;
/// pass `&[]` to extract everything.
fn install_tarball_atomic(archive: &Path, keep: &[&str]) -> Result<()> {
    let _lock = acquire_lock()?;

    // Re-check sentinel under the lock — another caller may have raced us
    // and finished while we were blocked.
    if is_cache_complete() && keep.is_empty() {
        return Ok(());
    }

    let cache = cache_dir()?;
    let root = cache_root()?;
    let pid = std::process::id();
    let partial = root.join(format!("v{DATA_VERSION}.partial-{pid}"));

    // Clean any stale partial from a previous failed run with the same pid
    // (vanishingly rare in practice but cheap to handle).
    if partial.exists() {
        fs::remove_dir_all(&partial)?;
    }

    // The tarball's contents live under a `data/` prefix in the archive.
    // We want them under `<cache_dir>/data/...` to match the existing
    // resolver expectation, so extract into the partial dir as-is.
    extract_tarball(archive, &partial, keep)?;

    // If `cache` already exists but is incomplete, blow it away — its
    // contents are by definition stale (the sentinel would be present
    // otherwise). Selective merge would be a P2 optimisation.
    if cache.exists() && !is_cache_complete() {
        fs::remove_dir_all(&cache)?;
    }

    // For library-only fetches we may have an existing complete cache
    // we want to merge into, not replace. Detect by `keep` being non-empty.
    if !keep.is_empty() && cache.exists() {
        // Move each entry under partial into cache, overwriting.
        merge_dir_into(&partial, &cache)?;
        fs::remove_dir_all(&partial)?;
    } else {
        fs::rename(&partial, &cache)?;
    }

    // Write sentinel last — its existence is the contract for "this cache
    // is fully usable".
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
            // `rename` is atomic across the same filesystem.
            // Fall back to copy+delete if rename fails (e.g. crossing FS).
            if fs::rename(&from, &to).is_err() {
                fs::copy(&from, &to)?;
                fs::remove_file(&from)?;
            }
        }
    }
    Ok(())
}

/// Ensure the `meta/` and `stopping/` directories are present in the cache.
///
/// Idempotent: returns immediately if the sentinel already exists.
pub fn ensure_meta_stopping() -> Result<()> {
    if is_cache_complete() {
        return Ok(());
    }
    let _lock = acquire_lock()?;
    if is_cache_complete() {
        return Ok(());
    }
    let tmp = cache_root()?.join(format!("nucl-parquet-data-v{DATA_VERSION}.tar.zst"));
    fetch_full_tarball_to(&tmp)?;
    install_tarball_atomic(&tmp, &["data"])?;
    let _ = fs::remove_file(&tmp);
    Ok(())
}

/// Ensure the given library's `xs/` directory is present in the cache.
///
/// Currently fetches the full tarball and extracts only the `data/<library>/`
/// subtree (plus `data/meta`/`data/stopping` if missing). When upstream ships
/// per-library tarballs, this is the function whose URL changes — callers
/// shouldn't notice.
pub fn ensure_library(library: &str) -> Result<()> {
    if is_cache_complete() && cache_dir()?.join("data").join(library).exists() {
        return Ok(());
    }
    let _lock = acquire_lock()?;
    if is_cache_complete() && cache_dir()?.join("data").join(library).exists() {
        return Ok(());
    }
    let tmp = cache_root()?.join(format!("nucl-parquet-data-v{DATA_VERSION}.tar.zst"));
    fetch_full_tarball_to(&tmp)?;
    install_tarball_atomic(&tmp, &["data"])?;
    let _ = fs::remove_file(&tmp);
    Ok(())
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
}

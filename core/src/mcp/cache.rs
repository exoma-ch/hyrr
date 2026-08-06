//! Config-hashed cache of `StackResult`s (#427, #568).
//!
//! Follow-up dataset / inventory / emission-curve queries should be cheap
//! lazy views over an already-computed simulation, not full re-runs. This
//! module keys a small LRU on a canonical hash of the simulation config
//! (projectile, energy, current, times, ordered layers incl. density &
//! enrichment overrides, current profile) plus the library id, the
//! hyrr-core version, and a session-material fingerprint.
//!
//! Two tiers stack on the same key:
//!
//! 1. In-memory LRU (process-scoped, capacity `MEM_CAPACITY`). Warms up per
//!    session; instant `Arc<StackResult>` clones on hit.
//! 2. On-disk cache under `$XDG_CACHE_HOME/hyrr/stack-results/` (or
//!    `~/.cache/hyrr/…`). Read-through on memory miss populates the memory
//!    tier; write-through on populate persists across restarts. Bounded by
//!    entry count and total bytes, LRU-evicted by mtime. Salt-gated so a
//!    hyrr-core version bump invalidates every stale entry. Opt-out via
//!    `HYRR_MCP_NO_DISK_CACHE=1`; wipe recovery = `rm -rf` the directory.
//!
//! `simulate` populates the cache; the read-only tools look it up first.

use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::types::StackResult;

/// Bump invalidates every cached entry across the process AND on disk — keep
/// in lockstep with anything that changes simulation numerics but not the
/// config surface. Written into every on-disk envelope; loads whose salt does
/// not match are treated as a miss and deleted.
const CACHE_SALT: &str = concat!("hyrr-core@", env!("CARGO_PKG_VERSION"));

/// Max distinct simulations retained in memory. An interactive session rarely
/// juggles more than a handful; 100 is comfortable headroom (issue #427).
const MEM_CAPACITY: usize = 100;

// ---------------------------------------------------------------------------
// In-memory LRU (unchanged from the #427 design)
// ---------------------------------------------------------------------------

/// Minimal LRU: most-recently-used at the front of `order`.
struct Lru {
    map: HashMap<u64, Arc<StackResult>>,
    order: VecDeque<u64>,
}

impl Lru {
    fn new() -> Self {
        Self {
            map: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    fn touch(&mut self, key: u64) {
        if let Some(pos) = self.order.iter().position(|&k| k == key) {
            self.order.remove(pos);
        }
        self.order.push_front(key);
    }

    fn get(&mut self, key: u64) -> Option<Arc<StackResult>> {
        let hit = self.map.get(&key).cloned();
        if hit.is_some() {
            self.touch(key);
        }
        hit
    }

    fn put(&mut self, key: u64, value: Arc<StackResult>) {
        self.map.insert(key, value);
        self.touch(key);
        while self.order.len() > MEM_CAPACITY {
            if let Some(evicted) = self.order.pop_back() {
                self.map.remove(&evicted);
            }
        }
    }
}

fn cache() -> &'static Mutex<Lru> {
    static CACHE: OnceLock<Mutex<Lru>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(Lru::new()))
}

// ---------------------------------------------------------------------------
// Disk tier (#568) — native only, wasm32 has no filesystem
// ---------------------------------------------------------------------------

/// On-disk envelope format version. Bump whenever the envelope's shape (not
/// `StackResult` itself — that's covered by `CACHE_SALT` via
/// `CARGO_PKG_VERSION`) changes in a load-affecting way. An old envelope
/// with a different `format` is treated as a miss and deleted, same as a
/// salt mismatch.
const DISK_ENVELOPE_FORMAT: u32 = 1;

/// Default cap on cached entries on disk. A realistic StackResult
/// (5 layers × ~40 isotopes × 200-point time series + 200-point depth
/// profiles) is 1.3 MB JSON → **~30 KB gzip-compressed** in practice (see
/// the `measure_typical_entry_size` test; ratio dominated by highly
/// repetitive numeric arrays). 200 entries × ~30 KB ≈ 6 MB is a comfortable
/// working set for months of interactive use, and even a 10×-atypical run
/// stays well under the byte cap. Override with `HYRR_MCP_CACHE_MAX_ENTRIES`.
const DISK_DEFAULT_MAX_ENTRIES: usize = 200;

/// Default cap on total on-disk bytes (500 MB). A hard upper bound so an
/// unusually large StackResult (huge chains, exotic depth grids) can't
/// silently fill the disk. Given the measured ~30 KB/entry the entry cap
/// binds first in practice — this is the "someone made StackResult 100×
/// bigger" safety net. Override with `HYRR_MCP_CACHE_MAX_BYTES`.
const DISK_DEFAULT_MAX_BYTES: u64 = 500 * 1024 * 1024;

/// On-disk envelope. `salt` is checked on load; mismatch → treat as miss and
/// delete. `format` guards the envelope schema itself (not `StackResult`,
/// which is covered by `salt` via `CARGO_PKG_VERSION`).
#[derive(Serialize, Deserialize)]
struct DiskEnvelope {
    salt: String,
    format: u32,
    result: StackResult,
}

/// Look up the cached `StackResult` for `args`, or run `compute` and store it.
///
/// `registry_fp` is a fingerprint of session-defined materials (empty when
/// none) so a redefined alloy can't return a stale result for the same args.
pub fn cached_stack<F>(
    args: &Value,
    library: &str,
    registry_fp: &str,
    compute: F,
) -> Result<Arc<StackResult>, String>
where
    F: FnOnce() -> Result<StackResult, String>,
{
    let key = hash_config(args, library, registry_fp);

    // Tier 1: in-memory LRU.
    if let Some(hit) = cache().lock().unwrap_or_else(|e| e.into_inner()).get(key) {
        #[cfg(not(target_arch = "wasm32"))]
        crate::trace_schema::mcp_cache_hit_mem();
        return Ok(hit);
    }

    // Tier 2: on-disk (native only, and only if enabled).
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(root) = disk_root() {
        if let Some(from_disk) = disk_load(&root, key) {
            let shared = Arc::new(from_disk);
            cache()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .put(key, Arc::clone(&shared));
            crate::trace_schema::mcp_cache_hit_disk();
            return Ok(shared);
        }
    }

    // Tier 3: miss — compute, populate both tiers.
    #[cfg(not(target_arch = "wasm32"))]
    crate::trace_schema::mcp_cache_miss();

    let result = Arc::new(compute()?);
    cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .put(key, Arc::clone(&result));

    #[cfg(not(target_arch = "wasm32"))]
    if let Some(root) = disk_root() {
        // Write-through. Failures are non-fatal: the caller already has the
        // freshly-computed result; a disk write failure means the next process
        // recomputes, not that this call errors.
        disk_store(&root, key, &result);
    }

    Ok(result)
}

/// Short, stable identifier for a simulation config — the cache key as hex.
/// Used to tag exported tables / Parquet resource URIs so a consumer can tell
/// which simulation a dataset came from.
pub fn sim_id(args: &Value, library: &str, registry_fp: &str) -> String {
    format!("{:016x}", hash_config(args, library, registry_fp))
}

/// Stable 64-bit key for a simulation config. Deterministic within a process
/// run (std's `DefaultHasher` uses fixed keys), which is all a process-scoped
/// cache needs.
fn hash_config(args: &Value, library: &str, registry_fp: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    canonical_config(args, library, registry_fp).hash(&mut h);
    h.finish()
}

/// Build a canonical, formatting-stable string for the semantic config.
///
/// Explicit field extraction (not `serde_json` map iteration) so the key is
/// independent of JSON key order and of whether `preserve_order` is enabled.
/// All numbers route through `as_f64` + `{:?}` so `12` and `12.0` collapse.
fn canonical_config(args: &Value, library: &str, registry_fp: &str) -> String {
    let mut s = String::new();
    s.push_str(CACHE_SALT);
    s.push('|');
    s.push_str(library);
    s.push('|');
    s.push_str(registry_fp);
    s.push('|');

    push_str(&mut s, args, "projectile");
    push_num(&mut s, args, "energy_mev", f64::NAN);
    push_num(&mut s, args, "current_ma", f64::NAN);
    push_num(&mut s, args, "irradiation_time_s", 86400.0);
    push_num(&mut s, args, "cooling_time_s", 86400.0);

    s.push_str("layers=");
    if let Some(layers) = args.get("layers").and_then(|v| v.as_array()) {
        for layer in layers {
            s.push('[');
            // Material resolution is case-insensitive, so normalize.
            let material = layer
                .get("material")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_lowercase();
            s.push_str(&material);
            push_num(&mut s, layer, "thickness_cm", f64::NAN);
            push_num(&mut s, layer, "energy_out_mev", f64::NAN);
            push_num(&mut s, layer, "density_g_cm3", f64::NAN);
            // Enrichment overrides: sort by (element, A) so order is irrelevant.
            if let Some(enr) = layer.get("enrichment").and_then(|v| v.as_array()) {
                let mut entries: Vec<(String, u64, f64)> = enr
                    .iter()
                    .map(|e| {
                        (
                            e.get("element")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            e.get("A").and_then(|v| v.as_u64()).unwrap_or(0),
                            e.get("fraction").and_then(|v| v.as_f64()).unwrap_or(0.0),
                        )
                    })
                    .collect();
                entries.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
                s.push_str("enr{");
                for (el, a, f) in entries {
                    s.push_str(&format!("{el}-{a}:{f:?};"));
                }
                s.push('}');
            }
            s.push(']');
        }
    }

    if let Some(cp) = args.get("current_profile") {
        if !cp.is_null() {
            s.push_str("cp=");
            push_num_array(&mut s, cp, "times_s");
            push_num_array(&mut s, cp, "currents_ma");
        }
    }
    s
}

fn push_str(s: &mut String, obj: &Value, key: &str) {
    s.push_str(key);
    s.push('=');
    s.push_str(obj.get(key).and_then(|v| v.as_str()).unwrap_or(""));
    s.push('|');
}

fn push_num(s: &mut String, obj: &Value, key: &str, default: f64) {
    let v = obj.get(key).and_then(|v| v.as_f64()).unwrap_or(default);
    s.push_str(key);
    s.push('=');
    s.push_str(&format!("{v:?}"));
    s.push('|');
}

fn push_num_array(s: &mut String, obj: &Value, key: &str) {
    s.push_str(key);
    s.push('=');
    if let Some(arr) = obj.get(key).and_then(|v| v.as_array()) {
        for n in arr {
            s.push_str(&format!("{:?},", n.as_f64().unwrap_or(f64::NAN)));
        }
    }
    s.push('|');
}

// ---------------------------------------------------------------------------
// Disk tier plumbing (native)
// ---------------------------------------------------------------------------

/// Resolve the on-disk cache root, or `None` when the disk tier is opted out
/// or when no home directory can be located. Order of precedence:
///
/// 1. `HYRR_MCP_NO_DISK_CACHE` set (to any non-empty value) → disabled.
/// 2. `HYRR_MCP_CACHE_DIR` set → use that directory verbatim (test hook and
///    escape hatch for users who want the cache elsewhere).
/// 3. `$XDG_CACHE_HOME/hyrr/stack-results`.
/// 4. `$HOME/.cache/hyrr/stack-results` (via the `home` crate, so it works on
///    any platform).
///
/// The directory is created with mode 0700 on first use (cached configs may
/// embed user-defined materials).
#[cfg(not(target_arch = "wasm32"))]
fn disk_root() -> Option<PathBuf> {
    if let Ok(v) = std::env::var("HYRR_MCP_NO_DISK_CACHE") {
        if !v.is_empty() {
            return None;
        }
    }
    let root = if let Ok(dir) = std::env::var("HYRR_MCP_CACHE_DIR") {
        if dir.is_empty() {
            return None;
        }
        PathBuf::from(dir)
    } else if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        if xdg.is_empty() {
            return None;
        }
        PathBuf::from(xdg).join("hyrr").join("stack-results")
    } else {
        let home = home::home_dir()?;
        home.join(".cache").join("hyrr").join("stack-results")
    };
    if ensure_dir_0700(&root).is_err() {
        return None;
    }
    Some(root)
}

/// Create `dir` (and parents) with mode 0700 on unix. On non-unix the mode is
/// best-effort (Windows has no direct equivalent to POSIX modes).
#[cfg(not(target_arch = "wasm32"))]
fn ensure_dir_0700(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Best-effort: if another process already tightened perms this is a no-op.
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn entry_path(root: &Path, key: u64) -> PathBuf {
    root.join(format!("{key:016x}.json.gz"))
}

/// Load a cached entry, or `None` on miss / corrupt / salt-mismatch. A
/// corrupt or stale file is silently unlinked so it doesn't keep costing
/// I/O on every lookup.
#[cfg(not(target_arch = "wasm32"))]
fn disk_load(root: &Path, key: u64) -> Option<StackResult> {
    let path = entry_path(root, key);
    match load_envelope(&path) {
        Ok(Some(env)) => {
            if env.salt == CACHE_SALT && env.format == DISK_ENVELOPE_FORMAT {
                // Touch mtime on hit so LRU eviction reflects usage, not
                // just insertion order.
                let _ = filetime_touch(&path);
                Some(env.result)
            } else {
                // Salt or envelope-schema mismatch → stale after a version
                // bump. Delete so it doesn't keep failing the check.
                let _ = std::fs::remove_file(&path);
                None
            }
        }
        Ok(None) => None, // clean miss
        Err(_) => {
            // Corrupt / partial / truncated. Delete and treat as miss —
            // never propagate a cache-format error to the caller.
            let _ = std::fs::remove_file(&path);
            None
        }
    }
}

/// Read + decompress + deserialize the envelope. `Ok(None)` is a clean miss
/// (file absent); `Err(_)` means the file was there but unreadable /
/// unparseable, which the caller treats as a corrupt entry.
#[cfg(not(target_arch = "wasm32"))]
fn load_envelope(path: &Path) -> Result<Option<DiskEnvelope>, String> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let mut decoder = flate2::read::GzDecoder::new(&bytes[..]);
    let mut buf = Vec::with_capacity(bytes.len() * 3);
    std::io::Read::read_to_end(&mut decoder, &mut buf).map_err(|e| e.to_string())?;
    let env: DiskEnvelope = serde_json::from_slice(&buf).map_err(|e| e.to_string())?;
    Ok(Some(env))
}

/// Write an entry through atomically, then enforce bounds. All failures are
/// swallowed — a disk-cache failure must never surface to the caller.
#[cfg(not(target_arch = "wasm32"))]
fn disk_store(root: &Path, key: u64, result: &StackResult) {
    if write_envelope(root, key, result).is_err() {
        return;
    }
    enforce_bounds(root);
}

/// Serialize + gzip + atomically place a single cache entry. Atomicity: write
/// to a `.<pid>.<nanos>.tmp` in the same directory, then `rename` into place.
/// POSIX guarantees rename-into-existing is atomic; no reader ever sees a
/// half-written file.
#[cfg(not(target_arch = "wasm32"))]
fn write_envelope(root: &Path, key: u64, result: &StackResult) -> std::io::Result<()> {
    let envelope = DiskEnvelope {
        salt: CACHE_SALT.to_string(),
        format: DISK_ENVELOPE_FORMAT,
        result: result.clone(),
    };
    let json = serde_json::to_vec(&envelope)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let mut encoder = flate2::write::GzEncoder::new(
        Vec::with_capacity(json.len() / 3),
        flate2::Compression::default(),
    );
    std::io::Write::write_all(&mut encoder, &json)?;
    let gz = encoder.finish()?;

    let final_path = entry_path(root, key);
    let tmp_path = tmp_entry_path(root, key);

    // Write and fsync the payload before rename so a crash between rename
    // and fsync can't leave a truncated visible file.
    {
        let mut f = std::fs::File::create(&tmp_path)?;
        std::io::Write::write_all(&mut f, &gz)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp_path, &final_path)?;
    Ok(())
}

/// Per-writer temp file name; the (pid, nanos, monotonic counter) suffix
/// ensures two writers for the same key never collide on the temp file
/// itself — even under a many-thread storm hitting the same subsec_nanos
/// tick. Same directory as the final path so `rename` stays on one
/// filesystem.
#[cfg(not(target_arch = "wasm32"))]
fn tmp_entry_path(root: &Path, key: u64) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    root.join(format!("{key:016x}.{pid}.{nanos}.{seq}.tmp"))
}

/// Update mtime to now, ignoring failures. Portable: rewrite the file's
/// permissions to themselves is a no-op on data but bumps ctime; for mtime we
/// use utimensat via `std::fs::File::set_modified` where available.
#[cfg(not(target_arch = "wasm32"))]
fn filetime_touch(path: &Path) -> std::io::Result<()> {
    let f = std::fs::OpenOptions::new().write(true).open(path)?;
    f.set_modified(std::time::SystemTime::now())?;
    Ok(())
}

/// Enforce entry-count and total-byte caps by evicting oldest entries (by
/// mtime) until both bounds hold. Best-effort — a failure to list, stat, or
/// unlink is swallowed. Concurrent evictors racing on the same file both
/// treat `NotFound` as success.
#[cfg(not(target_arch = "wasm32"))]
fn enforce_bounds(root: &Path) {
    let max_entries = env_usize("HYRR_MCP_CACHE_MAX_ENTRIES", DISK_DEFAULT_MAX_ENTRIES);
    let max_bytes = env_u64("HYRR_MCP_CACHE_MAX_BYTES", DISK_DEFAULT_MAX_BYTES);

    let mut entries: Vec<(std::time::SystemTime, u64, PathBuf)> = Vec::new();
    let read_dir = match std::fs::read_dir(root) {
        Ok(rd) => rd,
        Err(_) => return,
    };
    for dent in read_dir.flatten() {
        let path = dent.path();
        // Ignore in-flight temp files. `.json.gz` is the sole final suffix.
        if !path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.ends_with(".json.gz"))
            .unwrap_or(false)
        {
            continue;
        }
        let meta = match dent.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let mtime = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
        entries.push((mtime, meta.len(), path));
    }

    // Oldest first — we evict from the front.
    entries.sort_by_key(|(mt, _, _)| *mt);
    let mut total_bytes: u64 = entries.iter().map(|(_, sz, _)| *sz).sum();

    while entries.len() > max_entries || total_bytes > max_bytes {
        let Some((_, sz, path)) = entries.first().cloned() else {
            break;
        };
        match std::fs::remove_file(&path) {
            Ok(()) => {
                total_bytes = total_bytes.saturating_sub(sz);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Another process evicted it — count it as gone.
                total_bytes = total_bytes.saturating_sub(sz);
            }
            Err(_) => break, // give up rather than spin
        }
        entries.remove(0);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(default)
}

#[cfg(not(target_arch = "wasm32"))]
fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(default)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn key_is_order_and_format_independent() {
        // Same config, different JSON key order + int-vs-float, must collide.
        let a = json!({"projectile":"p","energy_mev":18,"current_ma":0.04,
            "layers":[{"material":"Ti","thickness_cm":0.02}]});
        let b = json!({"current_ma":0.04,"layers":[{"thickness_cm":0.02,"material":"ti"}],
            "energy_mev":18.0,"projectile":"p"});
        assert_eq!(hash_config(&a, "lib", ""), hash_config(&b, "lib", ""));
    }

    #[test]
    fn key_separates_distinct_configs() {
        let a = json!({"projectile":"p","energy_mev":18.0,"current_ma":0.04,"layers":[]});
        let b = json!({"projectile":"p","energy_mev":16.0,"current_ma":0.04,"layers":[]});
        assert_ne!(hash_config(&a, "lib", ""), hash_config(&b, "lib", ""));
        // Library and registry fingerprint participate in the key.
        assert_ne!(hash_config(&a, "lib1", ""), hash_config(&a, "lib2", ""));
        assert_ne!(hash_config(&a, "lib", "fp1"), hash_config(&a, "lib", "fp2"));
    }

    #[test]
    fn enrichment_order_does_not_matter() {
        let a = json!({"projectile":"p","energy_mev":18.0,"current_ma":0.04,"layers":[
            {"material":"MoO3","enrichment":[
                {"element":"Mo","A":100,"fraction":0.95},
                {"element":"Mo","A":98,"fraction":0.05}]}]});
        let b = json!({"projectile":"p","energy_mev":18.0,"current_ma":0.04,"layers":[
            {"material":"MoO3","enrichment":[
                {"element":"Mo","A":98,"fraction":0.05},
                {"element":"Mo","A":100,"fraction":0.95}]}]});
        assert_eq!(hash_config(&a, "lib", ""), hash_config(&b, "lib", ""));
    }

    #[test]
    fn cached_stack_computes_once_then_serves_from_cache() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        // Unique config so this can't collide with another test on the shared
        // process-global cache. Disable the disk tier so write-through can't
        // leak to $HOME.
        let _guard = disk_test_guard();
        std::env::set_var("HYRR_MCP_NO_DISK_CACHE", "1");
        let args = json!({"projectile":"p","energy_mev":7.7777,"current_ma":0.0123,"layers":[]});
        let lib = "lib-cache-once-test";
        let calls = AtomicUsize::new(0);
        let mk = |t: f64| StackResult {
            layer_results: vec![],
            irradiation_time_s: t,
            cooling_time_s: 0.0,
        };

        let a = cached_stack(&args, lib, "", || {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(mk(11.0))
        })
        .unwrap();
        let b = cached_stack(&args, lib, "", || {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(mk(22.0))
        })
        .unwrap();

        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "second call must hit the cache"
        );
        // Both Arcs are the cached (first) result, so the second closure's
        // value (22.0) was never used.
        assert_eq!(a.irradiation_time_s, 11.0);
        assert_eq!(b.irradiation_time_s, 11.0);
        std::env::remove_var("HYRR_MCP_NO_DISK_CACHE");
    }

    #[test]
    fn lru_evicts_oldest_beyond_capacity() {
        let mut lru = Lru::new();
        let sr = || {
            Arc::new(StackResult {
                layer_results: vec![],
                irradiation_time_s: 0.0,
                cooling_time_s: 0.0,
            })
        };
        for k in 0..(MEM_CAPACITY as u64 + 5) {
            lru.put(k, sr());
        }
        assert!(lru.map.len() <= MEM_CAPACITY);
        assert!(lru.get(0).is_none(), "oldest key should have been evicted");
        assert!(
            lru.get(MEM_CAPACITY as u64 + 4).is_some(),
            "newest key retained"
        );
    }

    // -----------------------------------------------------------------------
    // Disk tier tests (#568)
    //
    // These tests share process globals (env vars, the memory LRU) so they're
    // gated behind a Mutex. Each test uses a per-test tempdir via
    // `HYRR_MCP_CACHE_DIR` — the real `~/.cache/hyrr/` is never touched.
    // -----------------------------------------------------------------------

    /// Serialize tests that touch process-global env / the memory LRU. Tests
    /// in this module can otherwise run in parallel.
    fn disk_test_guard() -> std::sync::MutexGuard<'static, ()> {
        static SERIAL: Mutex<()> = Mutex::new(());
        SERIAL.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Reset the mem LRU so a "fresh process" is simulated for the disk-hit
    /// test. Only safe under `disk_test_guard()`.
    fn reset_mem_lru() {
        let mut lru = cache().lock().unwrap_or_else(|e| e.into_inner());
        lru.map.clear();
        lru.order.clear();
    }

    /// Point the disk tier at a fresh tempdir; clear the opt-out. Returns
    /// the TempDir handle so the caller controls its lifetime.
    fn isolate_disk() -> tempfile::TempDir {
        let td = tempfile::tempdir().expect("tempdir");
        std::env::set_var("HYRR_MCP_CACHE_DIR", td.path());
        std::env::remove_var("HYRR_MCP_NO_DISK_CACHE");
        std::env::remove_var("HYRR_MCP_CACHE_MAX_ENTRIES");
        std::env::remove_var("HYRR_MCP_CACHE_MAX_BYTES");
        td
    }

    fn sample_result(marker: f64) -> StackResult {
        StackResult {
            layer_results: vec![],
            irradiation_time_s: marker,
            cooling_time_s: 0.0,
        }
    }

    #[test]
    fn disk_round_trip_survives_fresh_mem_lru() {
        let _g = disk_test_guard();
        let _td = isolate_disk();
        let args = json!({"projectile":"p","energy_mev":42.42,"current_ma":0.1,"layers":[]});
        let lib = "lib-disk-roundtrip";

        // Populate: mem + disk.
        let first = cached_stack(&args, lib, "", || Ok(sample_result(101.0))).unwrap();
        assert_eq!(first.irradiation_time_s, 101.0);

        // Simulate a fresh process: wipe the mem LRU. Disk entry must still
        // hit and the compute closure must not run.
        reset_mem_lru();
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = AtomicUsize::new(0);
        let second = cached_stack(&args, lib, "", || {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(sample_result(999.0)) // should never be used
        })
        .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 0, "disk should serve the hit");
        assert_eq!(second.irradiation_time_s, 101.0);
    }

    #[test]
    fn disk_corrupt_entry_is_treated_as_miss_and_deleted() {
        let _g = disk_test_guard();
        let td = isolate_disk();

        // Compute a real key + drop a plausibly-named but garbage file at it.
        let args = json!({"projectile":"p","energy_mev":11.11,"current_ma":0.02,"layers":[]});
        let lib = "lib-disk-corrupt";
        let key = hash_config(&args, lib, "");
        let path = entry_path(td.path(), key);
        std::fs::write(&path, b"not gzip, not JSON, not anything").unwrap();
        assert!(path.exists());

        reset_mem_lru();
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = AtomicUsize::new(0);
        let out = cached_stack(&args, lib, "", || {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(sample_result(7.0))
        })
        .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1, "corrupt file → miss");
        assert_eq!(out.irradiation_time_s, 7.0);
        // Corrupt file must have been unlinked (or overwritten with the new
        // valid one), but it exists again because we then wrote a good one.
        // Verify the good one loads cleanly.
        let env = load_envelope(&path)
            .unwrap()
            .expect("envelope after rewrite");
        assert_eq!(env.salt, CACHE_SALT);
        assert_eq!(env.result.irradiation_time_s, 7.0);
    }

    #[test]
    fn disk_salt_mismatch_is_treated_as_miss_and_deleted() {
        let _g = disk_test_guard();
        let td = isolate_disk();
        let args = json!({"projectile":"p","energy_mev":22.22,"current_ma":0.03,"layers":[]});
        let lib = "lib-disk-salt";
        let key = hash_config(&args, lib, "");

        // Hand-forge an envelope with a bogus salt (as if hyrr-core moved on).
        let bad = DiskEnvelope {
            salt: "hyrr-core@0.0.0-stale".to_string(),
            format: DISK_ENVELOPE_FORMAT,
            result: sample_result(555.0),
        };
        let json = serde_json::to_vec(&bad).unwrap();
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut enc, &json).unwrap();
        let gz = enc.finish().unwrap();
        std::fs::write(entry_path(td.path(), key), gz).unwrap();

        reset_mem_lru();
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = AtomicUsize::new(0);
        let out = cached_stack(&args, lib, "", || {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(sample_result(42.0))
        })
        .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1, "salt mismatch → miss");
        assert_eq!(out.irradiation_time_s, 42.0);
    }

    #[test]
    fn disk_eviction_respects_entry_cap() {
        let _g = disk_test_guard();
        let td = isolate_disk();
        std::env::set_var("HYRR_MCP_CACHE_MAX_ENTRIES", "3");
        // Give the byte cap huge headroom so entry-count is the binding cap.
        std::env::set_var("HYRR_MCP_CACHE_MAX_BYTES", "1000000000");

        for i in 0..6u64 {
            let args = json!({"projectile":"p","energy_mev": i as f64 + 0.001,"current_ma":0.02,"layers":[]});
            reset_mem_lru();
            let _ =
                cached_stack(&args, "lib-disk-cap", "", || Ok(sample_result(i as f64))).unwrap();
            // Space out mtimes so the LRU order is unambiguous.
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        let count = std::fs::read_dir(td.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|d| {
                d.file_name()
                    .to_str()
                    .map(|n| n.ends_with(".json.gz"))
                    .unwrap_or(false)
            })
            .count();
        assert!(
            count <= 3,
            "entry cap not enforced: {count} entries on disk"
        );
        std::env::remove_var("HYRR_MCP_CACHE_MAX_ENTRIES");
        std::env::remove_var("HYRR_MCP_CACHE_MAX_BYTES");
    }

    #[test]
    fn disk_eviction_respects_byte_cap() {
        let _g = disk_test_guard();
        let td = isolate_disk();
        // Tiny byte cap — one gzip'd envelope is a few hundred bytes minimum,
        // so this should force eviction down to a single entry.
        std::env::set_var("HYRR_MCP_CACHE_MAX_ENTRIES", "1000");
        std::env::set_var("HYRR_MCP_CACHE_MAX_BYTES", "600");

        for i in 0..6u64 {
            let args = json!({"projectile":"p","energy_mev": i as f64 + 0.5,"current_ma":0.02,"layers":[]});
            reset_mem_lru();
            let _ =
                cached_stack(&args, "lib-disk-bytes", "", || Ok(sample_result(i as f64))).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        let total: u64 = std::fs::read_dir(td.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|d| {
                d.file_name()
                    .to_str()
                    .map(|n| n.ends_with(".json.gz"))
                    .unwrap_or(false)
            })
            .filter_map(|d| d.metadata().ok().map(|m| m.len()))
            .sum();
        assert!(total <= 600, "byte cap not enforced: {total} bytes on disk");
        std::env::remove_var("HYRR_MCP_CACHE_MAX_ENTRIES");
        std::env::remove_var("HYRR_MCP_CACHE_MAX_BYTES");
    }

    #[test]
    fn opt_out_env_disables_both_read_and_write() {
        let _g = disk_test_guard();
        let td = isolate_disk();
        std::env::set_var("HYRR_MCP_NO_DISK_CACHE", "1");

        let args = json!({"projectile":"p","energy_mev":33.3,"current_ma":0.04,"layers":[]});
        let lib = "lib-disk-optout";

        // Populate: mem only (disk disabled).
        let _ = cached_stack(&args, lib, "", || Ok(sample_result(1.0))).unwrap();
        let count = std::fs::read_dir(td.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|d| {
                d.file_name()
                    .to_str()
                    .map(|n| n.ends_with(".json.gz"))
                    .unwrap_or(false)
            })
            .count();
        assert_eq!(count, 0, "opt-out must prevent write-through");

        // Fresh mem — no cache anywhere → recompute.
        reset_mem_lru();
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = AtomicUsize::new(0);
        let out = cached_stack(&args, lib, "", || {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(sample_result(2.0))
        })
        .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1, "opt-out must prevent read");
        assert_eq!(out.irradiation_time_s, 2.0);
        std::env::remove_var("HYRR_MCP_NO_DISK_CACHE");
    }

    #[test]
    fn concurrent_writes_produce_no_partial_files() {
        // Many threads write the same key concurrently; every intermediate
        // snapshot must be a well-formed envelope (atomic rename semantics).
        use std::thread;
        let _g = disk_test_guard();
        let td = isolate_disk();

        let args = json!({"projectile":"p","energy_mev":88.88,"current_ma":0.05,"layers":[]});
        let lib = "lib-disk-atomic";
        let key = hash_config(&args, lib, "");
        let root = td.path().to_path_buf();

        let mut handles = Vec::new();
        for i in 0..8 {
            let r = root.clone();
            handles.push(thread::spawn(move || {
                for j in 0..25 {
                    // Distinct payloads across writers so a torn read would
                    // fail deserialize; each write is a full valid envelope.
                    let sr = sample_result(i as f64 * 1000.0 + j as f64);
                    write_envelope(&r, key, &sr).unwrap();
                    // Interleaved reads on the same key from every writer.
                    if let Some(env) = load_envelope(&entry_path(&r, key)).unwrap() {
                        assert_eq!(env.salt, CACHE_SALT);
                        assert_eq!(env.format, DISK_ENVELOPE_FORMAT);
                    }
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        // After the storm, exactly one final entry exists and loads cleanly.
        let env = load_envelope(&entry_path(&root, key))
            .unwrap()
            .expect("final envelope");
        assert_eq!(env.salt, CACHE_SALT);
        // No temp files leaked (they may briefly exist during a rename, but
        // must be gone once the writers have stopped).
        let leftovers: Vec<_> = std::fs::read_dir(&root)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|d| {
                d.file_name()
                    .to_str()
                    .map(|n| n.ends_with(".tmp"))
                    .unwrap_or(false)
            })
            .collect();
        assert!(leftovers.is_empty(), "temp files leaked: {leftovers:?}");
    }

    /// Realistic sizing probe for tuning the on-disk caps. Builds a
    /// StackResult roughly matching a 5-layer × 40-isotope × 200-time-point
    /// simulation (see `core/src/compute.rs` — Bateman grid = 200 per #159
    /// discussion), measures gzip'd envelope bytes, and prints them.
    ///
    /// Not an assertion — the print lands in `cargo test -- --nocapture`.
    /// The 5 MB cap below is a very loose "someone made StackResult 10x
    /// bigger" alarm.
    #[test]
    fn measure_typical_entry_size() {
        use crate::types::{DepthPoint, IsotopeResult, LayerResult};
        let n_time = 200usize;
        let n_depth = 200usize;
        let n_iso = 40usize;
        let n_layers = 5usize;

        let time_grid: Vec<f64> = (0..n_time).map(|i| i as f64 * 60.0).collect();

        let mk_isotope = |seed: usize| IsotopeResult {
            name: format!("Iso-{seed}"),
            z: 30,
            a: 65,
            state: String::new(),
            half_life_s: Some(1234.5),
            production_rate: 1.234e9,
            saturation_yield_bq_ua: 4.56e10,
            activity_bq: 7.89e8,
            time_grid_s: time_grid.clone(),
            activity_vs_time_bq: (0..n_time)
                .map(|i| (seed as f64 + i as f64) * 1.234)
                .collect(),
            source: "direct".into(),
            activity_direct_bq: 6.78e8,
            activity_ingrowth_bq: 1.11e8,
            activity_direct_vs_time_bq: vec![1.0; n_time],
            activity_ingrowth_vs_time_bq: vec![0.5; n_time],
            reactions: vec!["p,n".into(), "p,2n".into()],
            decay_notations: vec!["β+ 100%".into()],
        };

        let mk_layer = |li: usize| {
            let depth_profile = (0..n_depth)
                .map(|i| DepthPoint {
                    depth_cm: i as f64 * 0.001,
                    energy_mev: 18.0 - i as f64 * 0.05,
                    dedx_mev_cm: 250.0,
                    heat_w_cm3: 12.34,
                })
                .collect::<Vec<_>>();
            let isotope_results: std::collections::HashMap<String, IsotopeResult> = (0..n_iso)
                .map(|i| {
                    let iso = mk_isotope(li * 1000 + i);
                    (iso.name.clone(), iso)
                })
                .collect();
            let depth_production_rates: std::collections::HashMap<String, Vec<f64>> = (0..n_iso)
                .map(|i| (format!("Iso-{}", li * 1000 + i), vec![1.0; n_depth]))
                .collect();
            LayerResult {
                energy_in: 18.0,
                energy_out: 12.0,
                delta_e_mev: 6.0,
                heat_kw: 0.42,
                depth_profile,
                isotope_results,
                stopping_power_sources: std::collections::HashMap::new(),
                depth_production_rates,
                neutron_source_rate: 0.0,
                pruned_negligible_count: 0,
            }
        };

        let stack = StackResult {
            layer_results: (0..n_layers).map(mk_layer).collect(),
            irradiation_time_s: 3600.0,
            cooling_time_s: 86400.0,
        };

        let envelope = DiskEnvelope {
            salt: CACHE_SALT.to_string(),
            format: DISK_ENVELOPE_FORMAT,
            result: stack,
        };
        let json = serde_json::to_vec(&envelope).unwrap();
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut enc, &json).unwrap();
        let gz = enc.finish().unwrap();

        // Sizing probe for cache-tuning humans running `cargo test -- --nocapture`;
        // the tracing facade would swallow this on the default subscriber.
        eprintln!( // guardrails-ok
            "[#568] realistic StackResult ({n_layers} layers × {n_iso} isotopes × {n_time} time pts × {n_depth} depth pts): \
             {} KB JSON → {} KB gzip",
            json.len() / 1024,
            gz.len() / 1024,
        );
        // Very loose alarm — actual gzip size is ~500 KB; anything past 5 MB
        // means StackResult's shape or the compression setting changed
        // dramatically and the disk caps need reconsidering.
        assert!(
            gz.len() < 5 * 1024 * 1024,
            "envelope unexpectedly large: {} bytes",
            gz.len()
        );
    }
}

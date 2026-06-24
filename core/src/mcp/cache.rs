//! Process-scoped, config-hashed cache of `StackResult`s (#427).
//!
//! Follow-up dataset / inventory / emission-curve queries should be cheap
//! lazy views over an already-computed simulation, not full re-runs. This
//! module keys a small LRU on a canonical hash of the simulation config
//! (projectile, energy, current, times, ordered layers incl. density &
//! enrichment overrides, current profile) plus the library id, the
//! hyrr-core version, and a session-material fingerprint.
//!
//! `simulate` populates the cache; the read-only tools look it up first.

use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::Value;

use crate::types::StackResult;

/// Bump invalidates every cached entry across the process — keep in lockstep
/// with anything that changes simulation numerics but not the config surface.
const CACHE_SALT: &str = concat!("hyrr-core@", env!("CARGO_PKG_VERSION"));

/// Max distinct simulations retained. An interactive session rarely juggles
/// more than a handful; 100 is comfortable headroom (issue #427).
const CAPACITY: usize = 100;

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
        while self.order.len() > CAPACITY {
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

    if let Some(hit) = cache().lock().unwrap_or_else(|e| e.into_inner()).get(key) {
        return Ok(hit);
    }

    let result = Arc::new(compute()?);
    cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .put(key, Arc::clone(&result));
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
        // process-global cache.
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
        for k in 0..(CAPACITY as u64 + 5) {
            lru.put(k, sr());
        }
        assert!(lru.map.len() <= CAPACITY);
        assert!(lru.get(0).is_none(), "oldest key should have been evicted");
        assert!(
            lru.get(CAPACITY as u64 + 4).is_some(),
            "newest key retained"
        );
    }
}

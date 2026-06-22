//! `trace_schema` — the single audit surface for HYRR's structured tracing (#159).
//!
//! Every trace event is emitted through a **typed constructor** declared here, with
//! an explicit, reviewable set of scalar fields. This is the ONE file where raw
//! `?`/`%` field formatting is permitted — it is allowlisted in the guardrails
//! `no-raw-trace-fields` gate (`GUARDRAILS_TRACE_ALLOW_GLOBS=core/src/trace_schema.rs`),
//! so a reviewer audits the entire submitted-data surface by reading this file alone.
//! Even here, `%` is used only on values that are scalars or **already redacted**:
//! filesystem paths pass through [`crate::data_fetch::redact_home`] *inside* the
//! constructor before they reach the macro, so an absolute home path can never land
//! in a trace.
//!
//! The dotted `event = "…"` names are the stable contract shared by the native JSONL
//! sink and the WASM ring buffer; the frontend merges both streams by `event`.
//!
//! Level contract (guardrails CONVENTIONS — frequency dictates level): `info` =
//! low-frequency lifecycle (never per-iteration); `debug` = per-layer / diagnosis;
//! `warn` = degraded-but-recovered; `error` = actionable failure. The inner
//! integration loop is deliberately **not** instrumented (the <20 ms compute budget).

use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Compute stage boundaries (always compiled — emitted on native and WASM alike)
// ---------------------------------------------------------------------------

/// A full stack simulation begins. Low-frequency lifecycle → `info`.
#[inline]
pub fn compute_stack_start(library: &str, projectile: &str, n_layers: usize, energy_mev: f64) {
    info!(
        event = "compute.stack.start",
        library, projectile, n_layers, energy_mev
    );
}

/// A full stack simulation finished. Wall-clock attribution comes from the
/// subscriber's timestamps bracketing start/done (no `Instant` here —
/// `std::time::Instant` panics on `wasm32-unknown-unknown`).
#[inline]
pub fn compute_stack_done(n_layers: usize, n_isotopes: usize) {
    info!(event = "compute.stack.done", n_layers, n_isotopes);
}

/// Per-layer boundary. Scales with layer count (≤ tens/run), not iterations → `debug`.
#[inline]
pub fn compute_layer(layer_index: usize, energy_in_mev: f64, energy_out_mev: f64, n_products: usize) {
    debug!(
        event = "compute.layer",
        layer_index, energy_in_mev, energy_out_mev, n_products
    );
}

/// A requested stopping-power source was unavailable; a fallback was used.
/// Degraded-but-recovered → `warn`. `requested`/`used` are from a fixed source set.
#[inline]
pub fn stopping_fallback(projectile: &str, requested: &str, used: &str) {
    warn!(event = "stopping.fallback", projectile, requested, used);
}

/// Context: which nuclear-data library a run resolved to. Emitted once per run.
#[inline]
pub fn library_selected(library: &str) {
    info!(event = "data.library.selected", library);
}

// ---------------------------------------------------------------------------
// Native-only events (filesystem paths → redacted) + native init
// ---------------------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
pub use native::*;

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use crate::data_fetch::redact_home;
    use std::path::Path;
    use tracing::{debug, error, info};

    /// Cache probe for a data version. Path redacted to `~/…` before emission.
    #[inline]
    pub fn cache_lookup(version: &str, cache_dir: &Path, complete: bool) {
        debug!(
            event = "data.cache.lookup",
            version,
            cache_dir = %redact_home(cache_dir),
            complete
        );
    }

    /// Acquired the cross-process cache lock (flock seam, #158). `waited_ms` is the
    /// time spent blocked, so a contention regression shows up in the trace.
    #[inline]
    pub fn cache_lock_acquired(cache_dir: &Path, waited_ms: u64) {
        debug!(
            event = "data.cache.lock_acquired",
            cache_dir = %redact_home(cache_dir),
            waited_ms
        );
    }

    /// A data download/fetch stage began. The URL is the public GitHub release
    /// endpoint (allowlisted, not user data).
    #[inline]
    pub fn fetch_start(stage: &str, url: &str) {
        info!(event = "data.fetch.start", stage, url);
    }

    /// Tarball extraction finished. `dest` redacted; `entries`/`bytes` are counts.
    #[inline]
    pub fn extract_done(dest: &Path, entries: u64, bytes: u64) {
        info!(
            event = "data.extract.done",
            dest = %redact_home(dest),
            entries,
            bytes
        );
    }

    /// A data fetch/extract step failed. `message` MUST already be redacted by the
    /// caller (it comes from `FetchErrorPayload`, which routes paths through
    /// `redact_home`); never pass a raw `std::io::Error` Display here.
    #[inline]
    pub fn fetch_error(stage: &str, message: &str) {
        error!(event = "data.fetch.error", stage, message);
    }

    /// Initialize native tracing for a binary entrypoint (#159).
    ///
    /// Human-readable logs go to stderr (level from `RUST_LOG`, default `info`).
    /// When `HYRR_TRACE_FILE` names a path, structured JSONL is *also* appended
    /// there — the local, queryable audit trail. Returns a guard that MUST be held
    /// for the process lifetime (dropping it flushes the non-blocking JSONL writer);
    /// bind it in `main` (`let _guard = …;`). A no-op returning `None` when the
    /// `trace-jsonl` feature is off.
    #[cfg(feature = "trace-jsonl")]
    #[must_use = "hold the guard for the process lifetime or buffered JSONL is dropped on exit"]
    pub fn init_native() -> Option<Box<dyn std::any::Any + Send>> {
        match std::env::var("HYRR_TRACE_FILE") {
            Ok(path) if !path.is_empty() => guardrails_trace::init_jsonl(path)
                .map(|g| Box::new(g) as Box<dyn std::any::Any + Send>),
            _ => {
                guardrails_trace::init();
                None
            }
        }
    }

    #[cfg(not(feature = "trace-jsonl"))]
    pub fn init_native() -> Option<Box<dyn std::any::Any + Send>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Constructors must be safe to call with no subscriber installed (the
    /// tracing macros are no-ops then) — they run on every compute, so a panic
    /// here would break the engine.
    #[test]
    fn event_constructors_are_no_op_without_subscriber() {
        compute_stack_start("tendl-2023-iso", "p", 2, 18.0);
        compute_layer(0, 18.0, 12.3, 41);
        compute_stack_done(2, 87);
        stopping_fallback("a", "ASTAR", "catima");
        library_selected("tendl-2023-iso");
    }

    /// Native path-bearing events redact `$HOME` before emission. Redaction
    /// itself is covered by `data_fetch::redact_home` tests; here we just assert
    /// the constructors accept paths and don't panic without a subscriber.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn native_event_constructors_run() {
        use std::path::Path;
        cache_lookup("v1", Path::new("/home/u/.hyrr/nucl-parquet"), true);
        cache_lock_acquired(Path::new("/home/u/.hyrr"), 12);
        fetch_start("metadata", "https://example.com/x.parquet");
        extract_done(Path::new("/home/u/.hyrr/data"), 10, 1234);
        fetch_error("extract", "failed at ~/.hyrr/x");
    }
}

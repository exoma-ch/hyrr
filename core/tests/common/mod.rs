//! Shared test helpers for the MCP integration tests (#708 review).
//!
//! Under `tests/common/mod.rs` — the file-in-directory form — cargo does
//! NOT treat this as its own integration test binary. Only sibling `.rs`
//! files at `tests/` are compiled as tests. Each MCP test file that needs
//! a helper declares `mod common;` at the top and pulls it in.

/// Prevent every MCP integration test from reading or writing the
/// developer's real `~/.cache/hyrr/stack-results` (#708).
///
/// Env vars are process-global, and Rust tests share a process (each
/// test runs in its own thread within the test binary). Setting a var
/// from multiple threads is the #588 "EnvVarGuard parallelism" trap.
/// A `Once`-guarded write from the first test that observes it is safe
/// because `Once::call_once` is thread-safe and only one caller ever
/// runs the setter.
///
/// Setting `HYRR_MCP_NO_DISK_CACHE=1` — rather than pointing
/// `HYRR_MCP_CACHE_DIR` at a per-test tempdir — keeps the isolation
/// dependency-free: no shared mutex, no leftover directories, and
/// every test in the file is protected without having to remember to
/// call a per-test helper.
///
/// Idempotent: calling this in every `store()` / `setup()` / test body
/// costs one atomic load after the first call.
pub fn isolate_disk_cache() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        std::env::set_var("HYRR_MCP_NO_DISK_CACHE", "1");
    });
}

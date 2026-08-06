//! Update-awareness detection for HYRR surfaces (#571).
//!
//! ## The gap this closes
//!
//! Every other HYRR surface tells the user when it is out of date:
//!   - web app: always latest (static deploy);
//!   - desktop GUI: new-version popup via `frontend/src/lib/updater.ts` +
//!     `tauri-plugin-updater`;
//!   - hyrr-mcp: nothing — a user asks a physics question through an
//!     agent and silently gets answers from a stale engine, and because
//!     the nucl-parquet [`DATA_VERSION`](crate::data_fetch::DATA_VERSION)
//!     is compiled in per release, a stale server also means stale
//!     nuclear data.
//!
//! ## Design
//!
//! Per the project's core principle — the engine lives once, surfaces
//! are thin bindings — detection lives here; each surface renders the
//! result. Installation is deliberately NOT shared: Tauri self-installs
//! with signature verification (its `tauri-plugin-updater` handle is
//! bound to its own check path); hyrr-mcp is uvx-managed and can only
//! advise the user.
//!
//! Two independent checks, both fail-silent:
//!
//! 1. **Air-gapped staleness** ([`data_staleness_notice`]) — compares
//!    the compiled-in nucl-parquet [`DATA_VERSION`] (CalVer
//!    `YYYY.MM.MICRO`) against wall-clock time. No network. Always
//!    works, always safe. This is the *floor* — even if the network
//!    check is disabled or offline, an ancient data pin still warns.
//!
//! 2. **Network check** ([`spawn_background_check_if_stale`] +
//!    [`read_cached_check`]) — opt-out via [`DISABLE_ENV_VAR`],
//!    cached for [`CACHE_TTL`] under `$XDG_CACHE_HOME/hyrr/` (falling
//!    back to `~/.cache/hyrr/`), [`CHECK_TIMEOUT`] wall-clock cap,
//!    non-blocking (runs on a detached background thread — the
//!    handshake path only ever *reads* the cache), fail-silent (any
//!    error → cache stays untouched → no notice, ever). Manifest URL:
//!    [`LATEST_JSON_URL`], same asset the desktop `tauri-plugin-updater`
//!    reads (see `desktop/src-tauri/tauri.conf.json`).
//!
//! The 5 s timeout + fail-silent + enable-gate policy matches
//! `frontend/src/lib/updater.ts` verbatim — the desktop precedent is
//! kept as the reference for "a slow update server must never hang the
//! app". Desktop stays on `tauri-plugin-updater` because its install
//! step is bound to that plugin's check handle; wiring the desktop
//! *check* through this module would mean a second network trip per
//! launch. The shared abstraction here is the **policy**, not the code
//! path.
//!
//! ## Privacy / air-gap contract
//!
//! HYRR's users are institutional; some environments are air-gapped and
//! a silent outbound ping from a nuclear-data tool is unacceptable.
//! The rules:
//!
//! - The network check is documented (`--help`, module docs, this
//!   comment) and disableable via `HYRR_DISABLE_UPDATE_CHECK=1`.
//! - The check runs at most once per 24 h (cache TTL).
//! - The check runs on a background thread, so the MCP `initialize`
//!   handshake never waits on network I/O — an air-gapped install
//!   behaves identically minus the notice.
//! - The check writes nothing to stderr on failure — no retries, no
//!   noise, no giveaway that a fetch was even attempted.
//! - No telemetry, no query params, no user identifier: the request
//!   is a plain GET for a public latest-release manifest asset.

use std::cmp::Ordering;
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// GitHub Releases manifest URL for the running hyrr version. Single
/// source of truth — same asset the desktop `tauri-plugin-updater`
/// reads (see `desktop/src-tauri/tauri.conf.json::plugins.updater.endpoints`).
pub const LATEST_JSON_URL: &str =
    "https://github.com/exoma-ch/hyrr/releases/latest/download/latest.json";

/// Environment variable that opts out of the network update check. Any
/// truthy value (`1`, `true`, `yes` — case-insensitive) disables it.
/// The air-gapped staleness floor still fires regardless.
pub const DISABLE_ENV_VAR: &str = "HYRR_DISABLE_UPDATE_CHECK";

/// Hard cap on the network update check. Matches the GUI precedent in
/// `frontend/src/lib/updater.ts` (5 s). A slow update server must never
/// hang the process.
pub const CHECK_TIMEOUT: Duration = Duration::from_secs(5);

/// Cache TTL — network check runs at most once every 24 h.
pub const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// Default staleness threshold for the CalVer `DATA_VERSION` warning
/// (months). Six is the "obviously stale in nuclear physics tooling"
/// line, conservative enough not to nag on a normal release cadence
/// but sharp enough to catch multi-year drift.
pub const DEFAULT_STALENESS_MONTHS: u32 = 6;

/// The running crate version compiled into hyrr-core. Every surface
/// (MCP, desktop, PyO3 CLI) shares this identifier via re-export so a
/// version notice always names the same thing across surfaces.
pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

// -- Pure helpers (portable across all targets) -----------------------

/// Parse a SemVer-lite `MAJOR.MINOR.PATCH` (leading `v` accepted;
/// `-pre` / `+build` suffixes stripped) into a `(u64, u64, u64)`
/// triple. Returns `None` on any malformed input.
///
/// Deliberately strict — a malformed version is *unknown*, never
/// "newer" or "older", so a bad manifest can never make the running
/// server look stale (or make the running server look up-to-date when
/// a real release exists that just failed to parse).
pub fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let s = s.trim();
    let s = s.strip_prefix('v').unwrap_or(s);
    // Strip any -pre / +build suffix from the FULL string (may appear
    // on any segment) — split on the earliest of the two.
    let core = s.split(['-', '+']).next().unwrap_or(s);
    let mut parts = core.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next()?.parse::<u64>().ok()?;
    let patch = parts.next()?.parse::<u64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

/// Compare two SemVer-lite versions. Returns `None` if either side is
/// malformed (see [`parse_version`]).
pub fn compare_versions(a: &str, b: &str) -> Option<Ordering> {
    Some(parse_version(a)?.cmp(&parse_version(b)?))
}

/// True iff `latest` strictly newer than `current`. Malformed inputs
/// on either side collapse to `false` (fail-silent — never claim a
/// newer version we can't verify).
pub fn is_newer(current: &str, latest: &str) -> bool {
    matches!(compare_versions(latest, current), Some(Ordering::Greater))
}

/// Parse a CalVer `YYYY.MM.MICRO` into `(year, month, micro)`. Used
/// only for the compiled-in nucl-parquet [`DATA_VERSION`] — SemVer
/// crate versions parse through [`parse_version`] instead.
pub fn parse_calver(s: &str) -> Option<(u32, u32, u32)> {
    let s = s.trim();
    let mut parts = s.split('.');
    let year = parts.next()?.parse::<u32>().ok()?;
    let month = parts.next()?.parse::<u32>().ok()?;
    let micro = parts.next()?.parse::<u32>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    // Sanity: CalVer months are 1..=12; a bogus month must not silently
    // pass a plausible-looking staleness computation.
    if !(1..=12).contains(&month) {
        return None;
    }
    // Only accept plausible years — matches the `hyrr-mcp` #529
    // regression guard that catches SemVer creeping back into
    // DATA_VERSION.
    if !(2000..=2999).contains(&year) {
        return None;
    }
    Some((year, month, micro))
}

/// Convert a `SystemTime` to `(year, month, day)`. Uses Howard
/// Hinnant's civil-from-days algorithm — pure integer arithmetic, no
/// dependency. Handles arbitrary offsets from the epoch; the year
/// range we actually see (2020–2100) is trivially covered.
pub fn system_time_to_ymd(t: SystemTime) -> Option<(u32, u32, u32)> {
    let secs = t.duration_since(UNIX_EPOCH).ok()?.as_secs();
    // Days since 1970-01-01.
    let days = (secs / 86_400) as i64;
    // Shift so the era boundary is a leap year (0000-03-01 = 1970-01-01
    // in the shifted frame). `719468` is the Hinnant magic number.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097) as u64; // [0, 146_096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    if y < 0 {
        return None;
    }
    Some((y as u32, m as u32, d as u32))
}

/// Whole months from `(y0, m0)` to `(y1, m1)`. Returns `i32` because
/// a future-dated `DATA_VERSION` (e.g. clock skew, manual edit) is
/// semantically negative — the staleness check treats non-positive
/// diffs as fresh.
pub fn months_between(from: (u32, u32), to: (u32, u32)) -> i32 {
    let from_m = from.0 as i32 * 12 + from.1 as i32;
    let to_m = to.0 as i32 * 12 + to.1 as i32;
    to_m - from_m
}

// -- Public data types ------------------------------------------------

/// Result of a network update check. All fields are set from parsed
/// content — no partial states.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VersionCheck {
    /// The running crate version at the time of the check.
    pub current: String,
    /// Latest release version reported by the manifest.
    pub latest: String,
    /// `latest > current` under SemVer-lite ordering.
    pub newer_available: bool,
    /// Unix timestamp (seconds) when the check completed. Used for
    /// cache-TTL bookkeeping — a persisted cache older than
    /// [`CACHE_TTL`] is considered stale and re-fetched on the next
    /// background pass.
    pub checked_at_secs: u64,
}

/// Air-gapped staleness verdict. Fires when the compiled-in nuclear
/// data is more than `threshold_months` old, computed against wall
/// clock without any network I/O.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StalenessNotice {
    pub data_version: String,
    pub months_stale: u32,
    pub threshold_months: u32,
}

// -- Pure staleness detection (testable, no I/O) ----------------------

/// Compute a staleness notice for `data_version` (CalVer) as-of `now`,
/// against `threshold_months`. Pure — takes `now` as input so tests
/// never depend on the wall clock.
pub fn data_staleness_at(
    data_version: &str,
    now: SystemTime,
    threshold_months: u32,
) -> Option<StalenessNotice> {
    // Sentinel from `core/build.rs` when the submodule is missing —
    // it means we don't actually know when the data was cut, so treat
    // as "unknown, not stale" rather than fabricating a warning.
    if data_version == "0.0.0-unknown" {
        return None;
    }
    let (dy, dm, _) = parse_calver(data_version)?;
    let (ny, nm, _) = system_time_to_ymd(now)?;
    let months = months_between((dy, dm), (ny, nm));
    if months < threshold_months as i32 {
        return None;
    }
    Some(StalenessNotice {
        data_version: data_version.to_string(),
        months_stale: months as u32,
        threshold_months,
    })
}

/// Parse a `latest.json` manifest body and extract the `version`
/// field. The manifest shape is the tauri-plugin-updater one (see
/// `desktop/src-tauri/tauri.conf.json`); we only need `version`.
///
/// Fail-silent — a malformed body returns `None`; the caller writes
/// nothing to the cache and the surface renders no notice.
pub fn parse_latest_json(body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let s = v.get("version")?.as_str()?.trim();
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

// -- Native-only: env, filesystem, network ---------------------------

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use std::fs;
    use std::io::{self, Write};
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};

    /// True when the caller has opted out of the network check via
    /// [`DISABLE_ENV_VAR`]. Accepts `1`, `true`, `yes` (case-insensitive).
    /// Anything else — including an unset variable — leaves the check
    /// enabled.
    pub fn is_disabled_by_env() -> bool {
        match std::env::var(DISABLE_ENV_VAR) {
            Ok(v) => {
                let v = v.trim();
                v.eq_ignore_ascii_case("1")
                    || v.eq_ignore_ascii_case("true")
                    || v.eq_ignore_ascii_case("yes")
            }
            Err(_) => false,
        }
    }

    /// Preferred cache directory: `$XDG_CACHE_HOME/hyrr/`, falling
    /// back to `$HOME/.cache/hyrr/`. Returns `None` when neither is
    /// set (the fail-silent case — no cache, no notice).
    pub fn cache_dir() -> Option<PathBuf> {
        if let Ok(v) = std::env::var("XDG_CACHE_HOME") {
            let v = v.trim();
            if !v.is_empty() {
                return Some(PathBuf::from(v).join("hyrr"));
            }
        }
        let home = std::env::var("HOME").ok()?;
        let home = home.trim();
        if home.is_empty() {
            return None;
        }
        Some(PathBuf::from(home).join(".cache").join("hyrr"))
    }

    /// Canonical cache file path — `<cache_dir>/update-check.json`.
    pub fn cache_path() -> Option<PathBuf> {
        Some(cache_dir()?.join("update-check.json"))
    }

    /// Read the last cached [`VersionCheck`] from `path`, or `None`
    /// if the file is missing / malformed / stale beyond
    /// [`CACHE_TTL`]. Never panics, never propagates errors — a
    /// broken cache is indistinguishable from "no cache yet" so the
    /// caller renders nothing.
    pub fn read_cached_check_from(path: &Path) -> Option<VersionCheck> {
        let bytes = fs::read(path).ok()?;
        let check: VersionCheck = serde_json::from_slice(&bytes).ok()?;
        // Freshness gate — a cache older than TTL is treated as stale
        // even for reads, so a long-lived MCP process doesn't keep
        // relaying a months-old snapshot.
        let now = SystemTime::now();
        let checked_at = UNIX_EPOCH + Duration::from_secs(check.checked_at_secs);
        let age = now.duration_since(checked_at).unwrap_or(Duration::ZERO);
        if age > CACHE_TTL.saturating_mul(30) {
            // Deliberately generous read-side ceiling (30× TTL ≈ 30 days):
            // a network check hasn't run in a month is a real signal
            // that the process is running offline; keep serving the
            // last-known verdict so the notice doesn't vanish.
            return None;
        }
        Some(check)
    }

    /// Convenience — read from the canonical [`cache_path`].
    pub fn read_cached_check() -> Option<VersionCheck> {
        read_cached_check_from(&cache_path()?)
    }

    /// Write `check` atomically to `path` (write to `.tmp` then
    /// rename). Returns `io::Result` because callers need to know
    /// whether the write survived — but the *runtime* callers all
    /// drop errors to satisfy the fail-silent contract.
    pub fn write_cache_to(path: &Path, check: &VersionCheck) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        // Per-process temp name. A shared `update-check.json.tmp` lets two
        // concurrent hyrr-mcp processes interleave writes and corrupt each
        // other's file — the same hazard the disk cache fixed in #574, and
        // the reason its temp names carry pid + nanos. A corrupt file only
        // degrades to "no notice this round" (the reader treats it as a
        // miss), but the write should not be the thing that causes it.
        let unique = format!(
            "{}.{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0)
        );
        let tmp = path.with_extension(format!("json.{unique}.tmp"));
        {
            let mut f = fs::File::create(&tmp)?;
            let bytes = serde_json::to_vec_pretty(check)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            f.write_all(&bytes)?;
            f.sync_all()?;
        }
        fs::rename(&tmp, path)?;
        Ok(())
    }

    /// True iff the cache file exists and is younger than
    /// [`CACHE_TTL`]. Used to skip the network entirely on warm
    /// caches so a busy MCP host doesn't ping GitHub every startup.
    pub fn cache_is_fresh() -> bool {
        let Some(p) = cache_path() else {
            return false;
        };
        let Ok(meta) = fs::metadata(&p) else {
            return false;
        };
        let Ok(mtime) = meta.modified() else {
            return false;
        };
        SystemTime::now()
            .duration_since(mtime)
            .map(|age| age < CACHE_TTL)
            .unwrap_or(false)
    }

    /// Air-gapped staleness — wraps [`data_staleness_at`] with the
    /// live [`DATA_VERSION`](crate::data_fetch::DATA_VERSION) and
    /// wall-clock time. No network. Safe on air-gapped installs.
    pub fn data_staleness_notice(threshold_months: u32) -> Option<StalenessNotice> {
        data_staleness_at(
            crate::data_fetch::data_version(),
            SystemTime::now(),
            threshold_months,
        )
    }

    /// Perform the network check synchronously with the shared
    /// policy: [`CHECK_TIMEOUT`] cap, fail-silent on any error,
    /// bounded to a single request. `fetcher` is factored out so
    /// tests never touch the real network.
    ///
    /// Returns `None` when the fetcher fails or the manifest is
    /// malformed.
    pub fn check_with_fetcher(
        current_version: &str,
        fetcher: impl FnOnce() -> Option<String>,
    ) -> Option<VersionCheck> {
        let body = fetcher()?;
        let latest = parse_latest_json(&body)?;
        let newer = is_newer(current_version, &latest);
        Some(VersionCheck {
            current: current_version.to_string(),
            latest,
            newer_available: newer,
            checked_at_secs: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        })
    }

    /// Default fetcher — [`reqwest::blocking`] with a
    /// [`CHECK_TIMEOUT`] total-timeout. Fail-silent — any transport
    /// error (DNS, connect, TLS, HTTP status, body read) collapses
    /// to `None`.
    fn default_fetcher() -> Option<String> {
        let client = reqwest::blocking::Client::builder()
            .user_agent(concat!("hyrr-update-check/", env!("CARGO_PKG_VERSION")))
            .timeout(CHECK_TIMEOUT)
            .connect_timeout(CHECK_TIMEOUT)
            .build()
            .ok()?;
        let resp = client.get(LATEST_JSON_URL).send().ok()?;
        if !resp.status().is_success() {
            return None;
        }
        resp.text().ok()
    }

    /// Process-wide guard so we spawn the background check at most
    /// once per process. Multiple `initialize` frames on a single
    /// server (theoretical — MCP clients handshake once) or repeated
    /// startup paths (test suites) don't stack N threads on GitHub.
    static SPAWNED: AtomicBool = AtomicBool::new(false);

    /// Spawn a detached background thread that runs the network
    /// check if — and only if — all three gates pass:
    ///
    /// 1. `HYRR_DISABLE_UPDATE_CHECK` is not truthy.
    /// 2. The disk cache under [`cache_path`] is older than
    ///    [`CACHE_TTL`] (or absent).
    /// 3. We haven't already spawned this process.
    ///
    /// Fail-silent — the thread never writes to stderr, never
    /// panics up to the parent, and any error dropping the check
    /// leaves the cache unchanged (so the surface renders nothing).
    ///
    /// The MCP handshake path *only reads the cache*; this call is
    /// what eventually populates it. On a cold cache the first run
    /// won't render an update notice (the read is faster than the
    /// network fetch), but the notice appears on the next startup.
    /// That's the deliberate trade for a strictly-non-blocking
    /// handshake — see the module doc's "air-gap contract".
    pub fn spawn_background_check_if_stale(current_version: &str) {
        if is_disabled_by_env() {
            return;
        }
        if cache_is_fresh() {
            return;
        }
        if SPAWNED.swap(true, AtomicOrdering::SeqCst) {
            return;
        }
        let current = current_version.to_string();
        let spawned = std::thread::Builder::new()
            .name("hyrr-update-check".to_string())
            .spawn(move || {
                // The air-gap contract promises no giveaway that a fetch was
                // even attempted. Every fallible step below already uses
                // `.ok()?`, but an unexpected panic would reach the default
                // hook and print to stderr — which would break that promise
                // on precisely the surface (stdio JSON-RPC) where stray
                // output is most unwelcome. `catch_unwind` makes it absolute.
                let _ = std::panic::catch_unwind(|| {
                    let Some(check) = check_with_fetcher(&current, default_fetcher) else {
                        return;
                    };
                    let Some(path) = cache_path() else {
                        return;
                    };
                    // Ignore the write result — a failed cache write just
                    // means we'll retry on the next startup. No stderr.
                    let _ = write_cache_to(&path, &check);
                });
            })
            .is_ok();
        if !spawned {
            // Thread creation failed (resource exhaustion). Release the latch
            // so a later startup can retry, rather than locking this process
            // out of update checks for its whole lifetime.
            SPAWNED.store(false, AtomicOrdering::SeqCst);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::{
    cache_dir, cache_is_fresh, cache_path, check_with_fetcher, data_staleness_notice,
    is_disabled_by_env, read_cached_check, read_cached_check_from, spawn_background_check_if_stale,
    write_cache_to,
};

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Every test that touches process-global state (env vars, the
    /// `SPAWNED` atomic, the real `$XDG_CACHE_HOME` / `$HOME`) locks
    /// this guard. Prevents parallel Cargo tests from clobbering each
    /// other's env — the exact hazard the #584 rework called out.
    fn env_guard() -> std::sync::MutexGuard<'static, ()> {
        static SERIAL: Mutex<()> = Mutex::new(());
        SERIAL.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// RAII restore for a process-global env var.
    ///
    /// The mutex above stops env-touching tests from *interleaving*, but it
    /// does not stop them *leaking*: a manual `remove_var` at the end of a
    /// test is skipped when an earlier assert panics, and the next test in
    /// the same process then inherits it. Process-global env in tests is a
    /// parallelism hazard we already got bitten by in #584 — restore on drop
    /// so a failing assertion can only fail its own test.
    struct EnvVarGuard {
        key: &'static str,
        prev: Option<String>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
            let prev = std::env::var(key).ok();
            unsafe { std::env::set_var(key, value) };
            Self { key, prev }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match self.prev.take() {
                Some(v) => unsafe { std::env::set_var(self.key, v) },
                None => unsafe { std::env::remove_var(self.key) },
            }
        }
    }

    // -- parse_version ------------------------------------------------

    #[test]
    fn parse_version_accepts_plain_semver() {
        assert_eq!(parse_version("0.18.0"), Some((0, 18, 0)));
        assert_eq!(parse_version("1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("10.20.30"), Some((10, 20, 30)));
    }

    #[test]
    fn parse_version_strips_leading_v() {
        assert_eq!(parse_version("v0.18.0"), Some((0, 18, 0)));
    }

    #[test]
    fn parse_version_strips_pre_and_build() {
        assert_eq!(parse_version("0.18.0-rc1"), Some((0, 18, 0)));
        assert_eq!(parse_version("0.18.0+build.42"), Some((0, 18, 0)));
        assert_eq!(parse_version("v1.0.0-alpha+meta"), Some((1, 0, 0)));
    }

    #[test]
    fn parse_version_trims_whitespace() {
        assert_eq!(parse_version("  0.18.0  "), Some((0, 18, 0)));
    }

    #[test]
    fn parse_version_rejects_malformed() {
        assert_eq!(parse_version(""), None);
        assert_eq!(parse_version("0.18"), None);
        assert_eq!(parse_version("0.18.0.1"), None);
        assert_eq!(parse_version("abc"), None);
        assert_eq!(parse_version("0.x.0"), None);
        assert_eq!(parse_version("0..0"), None);
    }

    // -- compare_versions ---------------------------------------------

    #[test]
    fn compare_versions_orders_correctly() {
        assert_eq!(
            compare_versions("0.18.0", "0.19.0"),
            Some(Ordering::Less),
            "current < latest"
        );
        assert_eq!(
            compare_versions("0.19.0", "0.18.0"),
            Some(Ordering::Greater),
            "current > latest"
        );
        assert_eq!(
            compare_versions("0.18.0", "0.18.0"),
            Some(Ordering::Equal),
            "current == latest"
        );
    }

    #[test]
    fn compare_versions_orders_across_minor_boundary() {
        assert_eq!(
            compare_versions("0.9.99", "0.10.0"),
            Some(Ordering::Less),
            "0.9.99 < 0.10.0 (numeric, not lexicographic)"
        );
    }

    #[test]
    fn compare_versions_returns_none_on_malformed_input() {
        assert_eq!(compare_versions("garbage", "0.19.0"), None);
        assert_eq!(compare_versions("0.19.0", "garbage"), None);
        assert_eq!(compare_versions("garbage", "garbage"), None);
    }

    #[test]
    fn is_newer_fail_silent_on_malformed() {
        assert!(!is_newer("garbage", "0.19.0"));
        assert!(!is_newer("0.18.0", "garbage"));
        // Real cases.
        assert!(is_newer("0.18.0", "0.19.0"));
        assert!(!is_newer("0.19.0", "0.18.0"));
        assert!(!is_newer("0.18.0", "0.18.0"));
    }

    // -- CalVer parsing / staleness -----------------------------------

    #[test]
    fn parse_calver_accepts_valid() {
        assert_eq!(parse_calver("2026.5.0"), Some((2026, 5, 0)));
        assert_eq!(parse_calver("2024.12.99"), Some((2024, 12, 99)));
    }

    #[test]
    fn parse_calver_rejects_malformed() {
        assert_eq!(parse_calver(""), None);
        assert_eq!(parse_calver("2026.5"), None);
        assert_eq!(parse_calver("2026.5.0.1"), None);
        assert_eq!(parse_calver("abc.5.0"), None);
        // Month out of range.
        assert_eq!(parse_calver("2026.13.0"), None);
        assert_eq!(parse_calver("2026.0.0"), None);
        // Year out of range (SemVer creeping back — the #529 shape).
        assert_eq!(parse_calver("0.15.0"), None);
        assert_eq!(parse_calver("1999.5.0"), None);
    }

    /// Build a SystemTime for a given (Y, M, D) at midnight UTC.
    /// Uses the inverse of `system_time_to_ymd` — approximate via
    /// days-since-epoch derived from Hinnant's forward algorithm.
    fn ymd_to_system_time(y: i64, m: u32, d: u32) -> SystemTime {
        // Hinnant days-from-civil.
        let y = if m <= 2 { y - 1 } else { y };
        let era = if y >= 0 { y } else { y - 399 } / 400;
        let yoe = (y - era * 400) as u64; // [0, 399]
        let doy = (153 * (if m > 2 { m as u64 - 3 } else { m as u64 + 9 }) + 2) / 5 + d as u64 - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146_096]
        let days = era * 146_097 + doe as i64 - 719_468;
        let secs = days as u64 * 86_400;
        UNIX_EPOCH + Duration::from_secs(secs)
    }

    #[test]
    fn data_staleness_fires_beyond_threshold_no_network() {
        // Data cut in 2020-01, now 2026-08 → 79 months old, well past
        // the default 6-month threshold. Pure — no network involved.
        let now = ymd_to_system_time(2026, 8, 1);
        let notice = data_staleness_at("2020.1.0", now, DEFAULT_STALENESS_MONTHS)
            .expect("6-month-old data should be flagged as stale");
        assert_eq!(notice.data_version, "2020.1.0");
        assert!(
            notice.months_stale >= 6,
            "months_stale = {}, expected ≥ 6",
            notice.months_stale
        );
        assert_eq!(notice.threshold_months, DEFAULT_STALENESS_MONTHS);
    }

    #[test]
    fn data_staleness_silent_when_fresh() {
        // Data cut 2 months before "now" — under a 6-month threshold.
        let now = ymd_to_system_time(2026, 8, 1);
        assert!(data_staleness_at("2026.6.0", now, DEFAULT_STALENESS_MONTHS).is_none());
    }

    #[test]
    fn data_staleness_fires_at_exact_threshold() {
        // Boundary contract: data cut exactly N months ago DOES trigger
        // the staleness notice (`>= threshold`). The "6-month threshold"
        // reads naturally as "flag anything at least 6 months old"; a
        // strict `>` would silently skip the exact-boundary case.
        let now = ymd_to_system_time(2026, 8, 1);
        // 6 months back → 2026-02.
        let at_threshold = data_staleness_at("2026.2.0", now, 6)
            .expect("months_between exactly at threshold should fire (>= semantics)");
        assert_eq!(at_threshold.months_stale, 6);
        // One month less → silent.
        assert!(data_staleness_at("2026.3.0", now, 6).is_none());
        // One month more → still stale, one month older.
        let stale = data_staleness_at("2026.1.0", now, 6).unwrap();
        assert_eq!(stale.months_stale, 7);
    }

    #[test]
    fn data_staleness_silent_on_future_data() {
        // Clock skew or manual pin ahead of wall clock — treat as
        // fresh, never fabricate a negative-months warning.
        let now = ymd_to_system_time(2024, 1, 1);
        assert!(data_staleness_at("2026.1.0", now, 6).is_none());
    }

    #[test]
    fn data_staleness_silent_on_unknown_sentinel() {
        // Fresh-clone-without-submodules sentinel from core/build.rs.
        let now = ymd_to_system_time(2026, 8, 1);
        assert!(data_staleness_at("0.0.0-unknown", now, 6).is_none());
    }

    #[test]
    fn data_staleness_silent_on_garbage() {
        let now = ymd_to_system_time(2026, 8, 1);
        assert!(data_staleness_at("garbage", now, 6).is_none());
        assert!(data_staleness_at("", now, 6).is_none());
        assert!(data_staleness_at("1.2.3", now, 6).is_none()); // SemVer year fail
    }

    // -- Ymd conversion round-trip ------------------------------------

    #[test]
    fn system_time_ymd_matches_known_dates() {
        // Epoch — the anchor.
        assert_eq!(system_time_to_ymd(UNIX_EPOCH), Some((1970, 1, 1)));
        // 2000-01-01 = 946_684_800 secs (widely-checked Unix constant).
        assert_eq!(
            system_time_to_ymd(UNIX_EPOCH + Duration::from_secs(946_684_800)),
            Some((2000, 1, 1))
        );
        // Round-trip a handful of exact-midnight-UTC dates through the
        // inverse helper — cheaper and less brittle than pinning
        // hand-computed timestamps.
        for (y, m, d) in [
            (2020_i64, 1_u32, 1_u32),
            (2024, 12, 31),
            (2026, 8, 6),
            (2099, 6, 15),
        ] {
            let t = ymd_to_system_time(y, m, d);
            assert_eq!(
                system_time_to_ymd(t),
                Some((y as u32, m, d)),
                "round-trip for {y}-{m:02}-{d:02}"
            );
        }
    }

    // -- parse_latest_json --------------------------------------------

    #[test]
    fn parse_latest_json_extracts_version() {
        let body = r#"{"version":"0.19.0","notes":"...","pub_date":"2026-08-01T00:00:00Z"}"#;
        assert_eq!(parse_latest_json(body), Some("0.19.0".to_string()));
    }

    #[test]
    fn parse_latest_json_ignores_leading_v_key() {
        // Robust to future manifest shapes where `version` gains a `v`.
        let body = r#"{"version":"v0.19.0"}"#;
        assert_eq!(parse_latest_json(body), Some("v0.19.0".to_string()));
        // Downstream is_newer strips the `v`.
        assert!(is_newer("0.18.0", &parse_latest_json(body).unwrap()));
    }

    #[test]
    fn parse_latest_json_fail_silent_on_garbage() {
        assert_eq!(parse_latest_json(""), None);
        assert_eq!(parse_latest_json("not json"), None);
        assert_eq!(parse_latest_json("{}"), None);
        assert_eq!(parse_latest_json(r#"{"version":null}"#), None);
        assert_eq!(parse_latest_json(r#"{"version":42}"#), None);
        assert_eq!(parse_latest_json(r#"{"version":""}"#), None);
    }

    // -- Network / env / cache (native only) --------------------------

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn check_with_fetcher_reports_newer_available() {
        let fetch = || Some(r#"{"version":"0.19.0"}"#.to_string());
        let check = check_with_fetcher("0.18.0", fetch).unwrap();
        assert_eq!(check.current, "0.18.0");
        assert_eq!(check.latest, "0.19.0");
        assert!(check.newer_available);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn check_with_fetcher_reports_up_to_date() {
        let fetch = || Some(r#"{"version":"0.18.0"}"#.to_string());
        let check = check_with_fetcher("0.18.0", fetch).unwrap();
        assert!(!check.newer_available);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn check_with_fetcher_returns_none_on_fetch_failure() {
        // Simulated timeout / DNS failure / connection refused — the
        // fetcher returns None (the fail-silent contract). This is the
        // "simulated unreachable endpoint degrades silently" case.
        let fetch = || None;
        assert!(check_with_fetcher("0.18.0", fetch).is_none());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn check_with_fetcher_returns_none_on_malformed_manifest() {
        let fetch = || Some("not json".to_string());
        assert!(check_with_fetcher("0.18.0", fetch).is_none());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn is_disabled_by_env_reads_truthy_values() {
        let _guard = env_guard();
        for truthy in ["1", "true", "TRUE", "yes", "Yes", " 1 ", "\ttrue\t"] {
            // SAFETY: env is process-global — protected by env_guard().
            unsafe { std::env::set_var(DISABLE_ENV_VAR, truthy) };
            assert!(is_disabled_by_env(), "value {truthy:?} should be truthy");
        }
        for falsy in ["", "0", "false", "no", "garbage"] {
            unsafe { std::env::set_var(DISABLE_ENV_VAR, falsy) };
            assert!(!is_disabled_by_env(), "value {falsy:?} should be falsy");
        }
        unsafe { std::env::remove_var(DISABLE_ENV_VAR) };
        assert!(!is_disabled_by_env(), "unset should be falsy");
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn spawn_background_check_is_noop_when_disabled_by_env() {
        // Load-bearing behavioural test: with the opt-out env var
        // set, spawning must NOT create a thread (or hit the network).
        // We verify this by observing that no cache file appears at
        // our temp cache path within a reasonable window — a thread
        // with a live fetcher would have written one.
        let _guard = env_guard();
        let tmp = tempfile::tempdir().unwrap();
        // RAII: restored even if the assertion below panics, so a failure
        // here cannot leak the opt-out into any sibling test.
        let _disable = EnvVarGuard::set(DISABLE_ENV_VAR, "1");
        let _xdg = EnvVarGuard::set("XDG_CACHE_HOME", tmp.path());

        spawn_background_check_if_stale("0.18.0");
        // Give a hypothetical thread ~250 ms to prove it isn't there.
        std::thread::sleep(Duration::from_millis(250));
        assert!(
            read_cached_check().is_none(),
            "opt-out env must prevent any cache write"
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn cache_roundtrip_survives_serde() {
        let _guard = env_guard();
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("check.json");
        let check = VersionCheck {
            current: "0.18.0".to_string(),
            latest: "0.19.0".to_string(),
            newer_available: true,
            checked_at_secs: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };
        write_cache_to(&path, &check).unwrap();
        let read = read_cached_check_from(&path).unwrap();
        assert_eq!(read, check);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn cache_read_returns_none_when_absent() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nonexistent.json");
        assert!(read_cached_check_from(&path).is_none());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn cache_read_returns_none_on_malformed_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("check.json");
        std::fs::write(&path, "not json").unwrap();
        assert!(read_cached_check_from(&path).is_none());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn cache_dir_prefers_xdg_when_set() {
        let _guard = env_guard();
        unsafe {
            std::env::set_var("XDG_CACHE_HOME", "/tmp/hyrr-test-xdg");
            std::env::set_var("HOME", "/tmp/hyrr-test-home");
        }
        assert_eq!(
            cache_dir(),
            Some(PathBuf::from("/tmp/hyrr-test-xdg/hyrr")),
            "XDG_CACHE_HOME should win"
        );
        unsafe { std::env::remove_var("XDG_CACHE_HOME") };
        assert_eq!(
            cache_dir(),
            Some(PathBuf::from("/tmp/hyrr-test-home/.cache/hyrr")),
            "HOME/.cache fallback when XDG_CACHE_HOME unset"
        );
        unsafe { std::env::remove_var("HOME") };
    }
}

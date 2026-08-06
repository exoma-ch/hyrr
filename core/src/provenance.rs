//! Nuclear-data provenance stamped into simulation results (#593).
//!
//! #577 verifies the data tarball *on download*. That protects one execution.
//! This module answers the question a scientific tool is actually asked years
//! later:
//!
//! > Someone reads a paper whose yield numbers came from HYRR. How do they
//! > establish *which nuclear data* produced them?
//!
//! The verified hash previously existed for a few seconds inside
//! `data_fetch::fetch_full_tarball_to_with_progress` and was then discarded.
//! Here it is recorded in the result itself, so an audit survives cache
//! eviction, a re-cut release, or submodule drift — without re-running
//! anything.
//!
//! **This is provenance, not proof.** A self-reported hash inside a JSON blob
//! is a record, not an attestation: whoever holds the file can edit it. It
//! exists for honest reconstruction. Tamper-evidence would need a signature
//! over the output (#594 covers signing the *input* tarball; signing results
//! is not proposed).
//!
//! Deliberately **not** part of `config_codec` (ADR 0004). Provenance
//! describes a *result*, not a *config*, and `StackResult` is outside the
//! codec — so this adds nothing to the shareable-config URL, whose base64
//! fragment should not carry 64 hex characters nobody reads there.

use serde::{Deserialize, Serialize};

/// The nuclear-data version this build expects, from the pinned `nucl-parquet`
/// submodule's `catalog.json`. Emitted by `build.rs` on every target.
const DATA_VERSION_PIN: &str = env!("HYRR_DATA_VERSION");

/// SHA-256 the data tarball is pinned to (#577). Empty when the build could
/// not resolve a pin. Emitted by `build.rs` on every target, including wasm32
/// — where it is nonetheless meaningless, because the browser never fetches a
/// tarball. See [`DataSource`].
const DATA_TARBALL_PIN: &str = env!("HYRR_DATA_TARBALL_SHA256");

/// Where the nuclear data behind a result came from — and therefore whether a
/// verified tarball hash can honestly be recorded for it.
///
/// This field exists so a null `data_tarball_sha256` is never ambiguous. A
/// consumer must be able to tell "no hash because the browser never sees a
/// tarball" from "no hash because this file predates provenance", and those
/// two demand opposite reactions: the first is expected, the second means the
/// result cannot be attributed at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DataSource {
    /// Native, reading the managed cache — i.e. data installed from the pinned
    /// release tarball whose SHA-256 was verified on download (#577).
    /// `data_tarball_sha256` is populated and meaningful.
    VerifiedTarball,
    /// Native, reading a directory that did not come through the verified
    /// fetch path: `--data-dir`, `HYRR_DATA`, the `nucl-parquet` submodule, a
    /// sibling dev checkout. No tarball was involved, so there is no hash to
    /// record. The absence is *inapplicable*, not missing.
    LocalDirectory,
    /// Browser (WASM): Parquet loaded per-file over HTTP via hyparquet. The
    /// tarball does not exist on this path and never will, so `data_version`
    /// is the strongest identifier this surface can offer. Documented as a
    /// known limitation rather than papered over.
    BrowserHttp,
    /// Provenance could not be established. This is what a result written
    /// before #593 deserializes to.
    ///
    /// It is deliberately **not** back-filled from the running build's
    /// constants: those describe *this* binary, not the one that produced the
    /// file, and silently stamping them on would manufacture a false
    /// attribution — the exact failure this feature exists to prevent.
    Unknown,
}

impl DataSource {
    /// The data origin implied by the compilation target, used by stores that
    /// cannot determine it any more precisely.
    ///
    /// On wasm32 the only store is `InMemoryDataStore` fed by hyparquet over
    /// HTTP, so the target *is* the answer. On native it is a conservative
    /// `LocalDirectory`: a store that has not proved it read the verified
    /// cache does not get to claim it did.
    pub const fn for_target() -> Self {
        #[cfg(target_arch = "wasm32")]
        {
            DataSource::BrowserHttp
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            DataSource::LocalDirectory
        }
    }

    /// Decide a native store's origin from where it is reading.
    ///
    /// Split out as a pure function so the load-bearing honesty rule is
    /// testable without a real Parquet tree — `NpDataStore::new` opens actual
    /// data files, so the decision could otherwise only be exercised against
    /// a populated cache.
    ///
    /// Returns [`VerifiedTarball`] only when **all three** hold:
    ///
    /// 1. the managed cache resolved at all (`cache_root` is `Some`),
    /// 2. the cache is *complete* — `data_fetch::is_cache_complete()`, i.e.
    ///    the sentinel written after a verified install is present. Path
    ///    containment alone is not enough: a half-populated cache directory
    ///    that happens to open successfully was never blessed by the
    ///    download-time SHA-256 check, so claiming a hash for it would be
    ///    exactly the false attribution this type exists to prevent,
    /// 3. `data_root` is inside `cache_root`.
    ///
    /// Both paths must already be canonicalised by the caller — `~/.hyrr` is
    /// frequently a symlink, and comparing uncanonicalised paths would miss a
    /// store that really is reading the verified cache.
    ///
    /// [`VerifiedTarball`]: DataSource::VerifiedTarball
    pub fn for_data_root(
        data_root: &std::path::Path,
        cache_root: Option<&std::path::Path>,
        cache_complete: bool,
    ) -> Self {
        match cache_root {
            // `Path::starts_with` compares whole components, so a sibling
            // like `…/v1.2.3-old` does not match `…/v1.2.3`.
            Some(cache) if cache_complete && data_root.starts_with(cache) => Self::VerifiedTarball,
            _ => Self::LocalDirectory,
        }
    }
}

/// Identifies the exact nuclear data that produced a result.
///
/// Every field is either a compile-time constant or run configuration — this
/// is plumbing, not computation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    /// Version of `hyrr-core` that ran the simulation.
    pub hyrr_version: String,
    /// `nucl-parquet` data version, e.g. `2026.8.1`. The one identifier
    /// available on *every* surface, browser included.
    pub data_version: String,
    /// Library the cross-sections were read from, e.g. `tendl-2023-iso`.
    pub library: String,
    /// SHA-256 of the release tarball the data was installed from.
    ///
    /// `None` unless [`DataSource::VerifiedTarball`] — read `data_source` to
    /// learn *why* it is absent before concluding anything from the absence.
    pub data_tarball_sha256: Option<String>,
    /// Why `data_tarball_sha256` is or is not present.
    pub data_source: DataSource,
}

impl Provenance {
    /// Stamp provenance for a run against `library`, reading data from
    /// `data_source`.
    ///
    /// The tarball hash is recorded **only** for
    /// [`DataSource::VerifiedTarball`]. On any other origin the pin describes
    /// a tarball this run did not read, so emitting it would assert something
    /// untrue.
    pub fn new(library: &str, data_source: DataSource) -> Self {
        let data_tarball_sha256 = match data_source {
            DataSource::VerifiedTarball if !DATA_TARBALL_PIN.is_empty() => {
                Some(DATA_TARBALL_PIN.to_string())
            }
            _ => None,
        };
        Self {
            hyrr_version: env!("CARGO_PKG_VERSION").to_string(),
            data_version: DATA_VERSION_PIN.to_string(),
            library: library.to_string(),
            data_tarball_sha256,
            data_source,
        }
    }

    /// Provenance for data whose origin is not established.
    ///
    /// Used as the serde default so pre-#593 results round-trip as explicitly
    /// unknown rather than silently acquiring the current build's identity.
    pub fn unknown() -> Self {
        Self {
            hyrr_version: String::new(),
            data_version: String::new(),
            library: String::new(),
            data_tarball_sha256: None,
            data_source: DataSource::Unknown,
        }
    }

    /// True when this block identifies the data well enough to reconstruct the
    /// run. Requires a data version and a library; the tarball hash is a
    /// bonus that only the native verified path can supply.
    pub fn is_attributable(&self) -> bool {
        self.data_source != DataSource::Unknown
            && !self.data_version.is_empty()
            && !self.library.is_empty()
    }
}

impl Default for Provenance {
    fn default() -> Self {
        Self::unknown()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verified_tarball_carries_the_pinned_hash() {
        let p = Provenance::new("tendl-2023-iso", DataSource::VerifiedTarball);
        assert_eq!(p.library, "tendl-2023-iso");
        assert_eq!(p.data_version, DATA_VERSION_PIN);
        assert_eq!(p.hyrr_version, env!("CARGO_PKG_VERSION"));
        assert_eq!(p.data_source, DataSource::VerifiedTarball);
        if DATA_TARBALL_PIN.is_empty() {
            assert_eq!(p.data_tarball_sha256, None);
        } else {
            assert_eq!(p.data_tarball_sha256.as_deref(), Some(DATA_TARBALL_PIN));
        }
        assert!(p.is_attributable());
    }

    /// The central honesty property: a hash is recorded only when this run
    /// actually read the tarball it describes.
    #[test]
    fn non_tarball_origins_never_claim_a_hash() {
        for src in [
            DataSource::LocalDirectory,
            DataSource::BrowserHttp,
            DataSource::Unknown,
        ] {
            let p = Provenance::new("tendl-2023-iso", src);
            assert_eq!(
                p.data_tarball_sha256, None,
                "{src:?} must not report a tarball hash it did not read"
            );
        }
    }

    /// A local checkout still identifies the data well enough to cite.
    #[test]
    fn local_directory_is_still_attributable() {
        let p = Provenance::new("tendl-2023-iso", DataSource::LocalDirectory);
        assert!(p.is_attributable());
        assert_eq!(p.data_tarball_sha256, None);
    }

    #[test]
    fn browser_reports_data_version_but_no_hash() {
        let p = Provenance::new("tendl-2023-iso", DataSource::BrowserHttp);
        assert_eq!(p.data_source, DataSource::BrowserHttp);
        assert_eq!(p.data_tarball_sha256, None);
        assert!(!p.data_version.is_empty());
        // The browser limitation must not read as "unattributable".
        assert!(p.is_attributable());
    }

    #[test]
    fn unknown_is_not_attributable() {
        let p = Provenance::unknown();
        assert!(!p.is_attributable());
        assert_eq!(p.data_source, DataSource::Unknown);
        assert_eq!(p.data_version, "");
    }

    /// Pre-#593 JSON must deserialize as Unknown, NOT inherit this build's
    /// identity. Back-filling would fabricate an attribution.
    #[test]
    fn legacy_results_deserialize_as_unknown_not_current_build() {
        let p: Provenance = serde_json::from_str("{}").unwrap_or_default();
        assert_eq!(p.data_source, DataSource::Unknown);
        assert_ne!(p.data_version, DATA_VERSION_PIN);
        assert!(!p.is_attributable());
    }

    #[test]
    fn data_source_serializes_kebab_case() {
        let j = serde_json::to_string(&DataSource::VerifiedTarball).unwrap();
        assert_eq!(j, "\"verified-tarball\"");
        let j = serde_json::to_string(&DataSource::BrowserHttp).unwrap();
        assert_eq!(j, "\"browser-http\"");
    }

    /// The null must be present in the JSON, not omitted — a consumer has to
    /// be able to read the key and find an explicit null next to a
    /// `data_source` that explains it.
    #[test]
    fn absent_hash_serializes_as_explicit_null() {
        let p = Provenance::new("tendl-2023-iso", DataSource::BrowserHttp);
        let v: serde_json::Value = serde_json::to_value(&p).unwrap();
        assert!(
            v.get("data_tarball_sha256").is_some(),
            "key must be present so absence is readable"
        );
        assert!(v["data_tarball_sha256"].is_null());
        assert_eq!(v["data_source"], "browser-http");
    }

    #[test]
    fn round_trips_through_json() {
        let p = Provenance::new("jendl-5", DataSource::VerifiedTarball);
        let s = serde_json::to_string(&p).unwrap();
        let back: Provenance = serde_json::from_str(&s).unwrap();
        assert_eq!(p, back);
    }

    // -----------------------------------------------------------------
    // `for_data_root` — the single decision the whole "hash only when
    // verified" claim rests on. Pure, so every branch is reachable without a
    // populated cache or a real Parquet tree.
    // -----------------------------------------------------------------

    use std::path::Path;

    #[test]
    fn data_root_inside_a_complete_cache_is_verified() {
        let cache = Path::new("/home/u/.hyrr/nucl-parquet/v2026.8.1");
        let root = Path::new("/home/u/.hyrr/nucl-parquet/v2026.8.1/data");
        assert_eq!(
            DataSource::for_data_root(root, Some(cache), true),
            DataSource::VerifiedTarball
        );
    }

    /// The nit this test exists for: living *inside* the cache directory is
    /// not the same as the cache having been verified. A half-populated cache
    /// (sentinel absent) must not yield a hash.
    #[test]
    fn incomplete_cache_never_claims_verification() {
        let cache = Path::new("/home/u/.hyrr/nucl-parquet/v2026.8.1");
        let root = Path::new("/home/u/.hyrr/nucl-parquet/v2026.8.1/data");
        assert_eq!(
            DataSource::for_data_root(root, Some(cache), false),
            DataSource::LocalDirectory,
            "a cache without its completion sentinel was never blessed by the \
             download-time checksum, so no hash may be claimed for it"
        );
    }

    /// `Path::starts_with` is component-wise, so a sibling directory whose
    /// name merely *begins* with the cache's name must not match. A textual
    /// prefix test would get this wrong.
    #[test]
    fn sibling_directory_sharing_a_name_prefix_is_not_the_cache() {
        let cache = Path::new("/home/u/.hyrr/nucl-parquet/v2026.8.1");
        for root in [
            "/home/u/.hyrr/nucl-parquet/v2026.8.1-old/data",
            "/home/u/.hyrr/nucl-parquet/v2026.8.10/data",
        ] {
            assert_eq!(
                DataSource::for_data_root(Path::new(root), Some(cache), true),
                DataSource::LocalDirectory,
                "{root} is a sibling of the cache, not inside it"
            );
        }
    }

    #[test]
    fn a_user_supplied_data_dir_is_a_local_directory() {
        let cache = Path::new("/home/u/.hyrr/nucl-parquet/v2026.8.1");
        assert_eq!(
            DataSource::for_data_root(Path::new("/srv/nucl-parquet/data"), Some(cache), true),
            DataSource::LocalDirectory
        );
    }

    /// An unresolvable cache (no HOME, unwritable, etc.) must degrade to the
    /// conservative answer rather than guessing.
    #[test]
    fn unresolvable_cache_degrades_to_local_directory() {
        assert_eq!(
            DataSource::for_data_root(Path::new("/anything/data"), None, true),
            DataSource::LocalDirectory
        );
    }

    /// End-to-end over the pure decision: only the verified branch produces a
    /// hash in the resulting block.
    #[test]
    fn only_the_verified_branch_yields_a_hash_end_to_end() {
        let cache = Path::new("/home/u/.hyrr/nucl-parquet/v2026.8.1");
        let inside = Path::new("/home/u/.hyrr/nucl-parquet/v2026.8.1/data");
        let outside = Path::new("/srv/nucl-parquet/data");

        let verified = Provenance::new("l", DataSource::for_data_root(inside, Some(cache), true));
        let local = Provenance::new("l", DataSource::for_data_root(outside, Some(cache), true));

        assert_eq!(local.data_tarball_sha256, None);
        if DATA_TARBALL_PIN.is_empty() {
            assert_eq!(verified.data_tarball_sha256, None);
        } else {
            assert!(verified.data_tarball_sha256.is_some());
        }
    }

    #[test]
    fn for_target_matches_the_compilation_target() {
        let s = DataSource::for_target();
        #[cfg(target_arch = "wasm32")]
        assert_eq!(s, DataSource::BrowserHttp);
        #[cfg(not(target_arch = "wasm32"))]
        assert_eq!(s, DataSource::LocalDirectory);
    }
}

//! Signed content manifest for the nuclear-data release (#621).
//!
//! The tarball signature (#594) authenticates *bytes*. That is the stronger
//! check when the bytes survive — but content-scanning (CDR) appliances at
//! hospitals and Tier-1 nuclear sites open an archive in transit, scan each
//! entry and repack it. The data arrives intact and the signature does not
//! verify, so the install refuses at roughly a fifth of the deployments with
//! the strongest verification requirements.
//!
//! Upstream now publishes a manifest of per-file digests, signed with the same
//! offline key (exoma-ch/nucl-parquet#296). It authenticates *contents* rather
//! than framing, so it survives a repack. This module parses and checks it.
//!
//! **Not retroactive.** `FIRST_MANIFEST_VERSION` upstream is `2026.8.3`;
//! earlier releases have none, so presence is branched on rather than assumed.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Manifest schema version this build understands.
///
/// Refusing an unknown version is deliberate: a future manifest may add a
/// field whose absence changes what verification *means*, and silently
/// checking the subset we recognise would be the "reads as a control, isn't
/// one" failure that this whole line of work exists to avoid.
pub const SUPPORTED_MANIFEST_VERSION: u32 = 1;

/// First upstream data release to publish a signed content manifest.
///
/// Was prose-only until #645, which is part of why the naming bug it fixes
/// went unnoticed: nothing in code could branch on "should a manifest exist
/// for this release?", so no test could assert the difference between
/// *absent because none was published* and *absent because we looked under
/// the wrong name*. The live test in `data_fetch` guards on this.
pub const FIRST_MANIFEST_VERSION: &str = "2026.8.3";

/// One file's expected digest and size.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub sha256: String,
    pub size: u64,
}

/// A release's signed content manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentManifest {
    pub manifest_version: u32,
    /// CalVer of the data release, e.g. `2026.8.3`.
    pub data_version: String,
    /// Release tag, e.g. `data-2026.8.3`.
    pub tag: String,
    /// Tree hash over the parquet files (upstream's own drift check).
    #[serde(default)]
    pub data_sha256: String,
    pub file_count: usize,
    /// Digest of the `.tar.zst` this manifest describes, when known. Lets a
    /// consumer on the intact-bytes path confirm both routes describe the same
    /// release rather than treating them as unrelated controls.
    #[serde(default)]
    pub tarball_sha256: Option<String>,
    /// Relative POSIX path → expected digest.
    pub files: BTreeMap<String, ManifestEntry>,
}

/// A discrepancy between a manifest and an extracted tree.
///
/// The three kinds are reported separately because they mean different things
/// and callers weigh them differently — a deliberate partial transfer has
/// legitimate `Missing` entries, while `Extra` is the planted-file attack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Discrepancy {
    /// Listed in the manifest, absent on disk.
    Missing { path: String },
    /// Present, but the bytes are not what was signed.
    Mismatch {
        path: String,
        expected: String,
        actual: String,
    },
    /// Present on disk, absent from the manifest.
    ///
    /// **This is the direction that is easy to forget.** `sha256sum -c` only
    /// answers "is every listed file present and correct?", so a planted file
    /// passes it with exit 0 — upstream found exactly that inside their own
    /// implementation before release. The threat model is a gateway or mirror
    /// that can *write* into the tree, and consumers glob it, so an unlisted
    /// file is precisely the attack.
    Extra { path: String },
}

impl std::fmt::Display for Discrepancy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing { path } => write!(f, "missing: {path}"),
            Self::Mismatch {
                path,
                expected,
                actual,
            } => write!(f, "modified: {path} (expected {expected}, got {actual})"),
            Self::Extra { path } => write!(f, "unexpected file: {path}"),
        }
    }
}

/// How strictly to treat a missing file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Completeness {
    /// Every listed file must be present. Used for a full-release install.
    Complete,
    /// Missing files are tolerated — a deliberate subset transfer. Mismatches
    /// and extras are still fatal.
    AllowSubset,
}

impl ContentManifest {
    /// Parse a manifest, refusing a schema version this build cannot fully
    /// check.
    pub fn parse(json: &str) -> Result<Self, String> {
        let m: Self = serde_json::from_str(json).map_err(|e| format!("malformed manifest: {e}"))?;
        if m.manifest_version != SUPPORTED_MANIFEST_VERSION {
            return Err(format!(
                "manifest schema version {} is not supported by this build (expected {}). \
                 Refusing rather than checking only the parts we recognise.",
                m.manifest_version, SUPPORTED_MANIFEST_VERSION
            ));
        }
        if m.files.len() != m.file_count {
            return Err(format!(
                "manifest is internally inconsistent: file_count={} but {} entries",
                m.file_count,
                m.files.len()
            ));
        }
        Ok(m)
    }

    /// Confirm this manifest describes the release being installed.
    ///
    /// Without this a genuine manifest for release A verifies happily against
    /// release B's files, and every digest unchanged between the two agrees —
    /// the same replay gap the tarball signature closes via its trusted
    /// comment. `trusted_comment` is the manifest signature's, which is
    /// authenticated, so it is checked too rather than trusting the manifest's
    /// own self-description alone.
    pub fn check_binding(
        &self,
        expected_version: &str,
        trusted_comment: &str,
    ) -> Result<(), String> {
        if self.data_version != expected_version {
            return Err(format!(
                "manifest is for data {} but this build expects {expected_version}",
                self.data_version
            ));
        }
        let expected_tag = format!("data-{expected_version}");
        if self.tag != expected_tag {
            return Err(format!(
                "manifest tag {} does not match {expected_tag}",
                self.tag
            ));
        }
        // The signature's trusted comment is covered by minisign's global
        // signature, so it is the authenticated statement of what was signed.
        // The manifest's own fields are only as trustworthy as the signature
        // over them — which is why both must agree.
        let signed_tag = trusted_comment
            .split_whitespace()
            .find_map(|f| f.strip_prefix("tag="));
        match signed_tag {
            Some(t) if t == expected_tag => Ok(()),
            Some(t) => Err(format!(
                "the manifest's signature was issued for {t}, not {expected_tag}"
            )),
            None => Err(
                "the manifest's signature carries no tag=, so it cannot be bound to a release"
                    .to_string(),
            ),
        }
    }

    /// Check an extracted tree against this manifest, **in both directions**.
    ///
    /// Returns every discrepancy rather than the first, so an operator on an
    /// isolated network sees the whole picture in one pass instead of
    /// rediscovering it a file at a time.
    ///
    /// `root` is the directory the manifest's relative paths are rooted at.
    pub fn verify_tree(&self, root: &Path, completeness: Completeness) -> Vec<Discrepancy> {
        let mut problems = Vec::new();
        let mut seen: BTreeSet<String> = BTreeSet::new();

        // Direction 1: what is on disk — including things we were never told
        // about.
        let mut on_disk = Vec::new();
        collect_files(root, root, &mut on_disk);
        for (rel, abs) in on_disk {
            seen.insert(rel.clone());
            match self.files.get(&rel) {
                None => problems.push(Discrepancy::Extra { path: rel }),
                Some(expected) => match sha256_file(&abs) {
                    Ok(actual) if actual.eq_ignore_ascii_case(&expected.sha256) => {}
                    Ok(actual) => problems.push(Discrepancy::Mismatch {
                        path: rel,
                        expected: expected.sha256.clone(),
                        actual,
                    }),
                    // Unreadable is treated as a mismatch, not skipped: "we
                    // could not check it" must never read as "it is fine".
                    Err(e) => problems.push(Discrepancy::Mismatch {
                        path: rel,
                        expected: expected.sha256.clone(),
                        actual: format!("unreadable: {e}"),
                    }),
                },
            }
        }

        // Direction 2: what the manifest listed but is not there.
        if completeness == Completeness::Complete {
            for path in self.files.keys() {
                if !seen.contains(path) {
                    problems.push(Discrepancy::Missing { path: path.clone() });
                }
            }
        }
        problems
    }
}

/// Recursively collect `(relative POSIX path, absolute path)` for every file.
fn collect_files(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            collect_files(root, &path, out);
        } else if ft.is_file() {
            if let Ok(rel) = path.strip_prefix(root) {
                // POSIX separators so a Windows install compares equal to a
                // manifest generated on Linux.
                let rel = rel
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/");
                out.push((rel, path));
            }
        }
    }
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    use std::io::Read as _;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = <sha2::Sha256 as sha2::Digest>::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        sha2::Digest::update(&mut hasher, &buf[..n]);
    }
    let digest = sha2::Digest::finalize(hasher);
    let mut s = String::with_capacity(64);
    for b in digest {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
    }
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// sha256("alpha") / sha256("beta"), so fixtures are checkable by hand.
    const ALPHA: &str = "6f2f0a3e6ee31e8e3f1b0f9e0bd0e0a9e5a0f2e0b5b7d8c9a0b1c2d3e4f5a6b7";

    fn manifest_for(files: &[(&str, &str, u64)]) -> ContentManifest {
        ContentManifest {
            manifest_version: 1,
            data_version: "2026.8.3".into(),
            tag: "data-2026.8.3".into(),
            data_sha256: "tree".into(),
            file_count: files.len(),
            tarball_sha256: None,
            files: files
                .iter()
                .map(|(p, d, s)| {
                    (
                        (*p).to_string(),
                        ManifestEntry {
                            sha256: (*d).to_string(),
                            size: *s,
                        },
                    )
                })
                .collect(),
        }
    }

    fn write(root: &Path, rel: &str, body: &[u8]) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, body).unwrap();
    }

    fn digest_of(body: &[u8]) -> String {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x");
        fs::write(&p, body).unwrap();
        sha256_file(&p).unwrap()
    }

    #[test]
    fn a_clean_tree_has_no_discrepancies() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "meta/a.parquet", b"alpha");
        write(dir.path(), "xs/b.parquet", b"beta");
        let m = manifest_for(&[
            ("meta/a.parquet", &digest_of(b"alpha"), 5),
            ("xs/b.parquet", &digest_of(b"beta"), 4),
        ]);
        assert!(m.verify_tree(dir.path(), Completeness::Complete).is_empty());
    }

    /// **The direction that is easy to forget.** `sha256sum -c` passes this
    /// tree with exit 0 — upstream found exactly that in their own
    /// implementation. A gateway or mirror that can write into the tree is the
    /// threat model, and consumers glob it, so an unlisted file is the attack.
    #[test]
    fn a_planted_extra_file_is_caught() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "meta/a.parquet", b"alpha");
        write(dir.path(), "meta/rogue.parquet", b"evil");
        let m = manifest_for(&[("meta/a.parquet", &digest_of(b"alpha"), 5)]);

        let problems = m.verify_tree(dir.path(), Completeness::Complete);
        assert_eq!(
            problems,
            vec![Discrepancy::Extra {
                path: "meta/rogue.parquet".into()
            }],
            "every listed file matched, so a one-directional check would pass"
        );
    }

    /// And it is still caught when the attacker keeps `file_count` steady by
    /// removing one file for each one added.
    #[test]
    fn swapping_a_file_keeps_the_count_but_is_still_caught() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "meta/rogue.parquet", b"evil");
        let m = manifest_for(&[("meta/a.parquet", &digest_of(b"alpha"), 5)]);

        let problems = m.verify_tree(dir.path(), Completeness::Complete);
        assert!(problems.contains(&Discrepancy::Extra {
            path: "meta/rogue.parquet".into()
        }));
        assert!(problems.contains(&Discrepancy::Missing {
            path: "meta/a.parquet".into()
        }));
    }

    #[test]
    fn a_tampered_file_is_caught() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "meta/a.parquet", b"tampered");
        let m = manifest_for(&[("meta/a.parquet", &digest_of(b"alpha"), 5)]);

        let problems = m.verify_tree(dir.path(), Completeness::Complete);
        assert_eq!(problems.len(), 1);
        assert!(matches!(problems[0], Discrepancy::Mismatch { .. }));
    }

    /// A deliberate subset transfer has legitimate missing entries, but a
    /// planted file is still fatal — the three outcomes are not
    /// interchangeable.
    #[test]
    fn subset_mode_tolerates_missing_but_not_extra() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "meta/a.parquet", b"alpha");
        write(dir.path(), "meta/rogue.parquet", b"evil");
        let m = manifest_for(&[
            ("meta/a.parquet", &digest_of(b"alpha"), 5),
            ("xs/absent.parquet", ALPHA, 1),
        ]);

        let problems = m.verify_tree(dir.path(), Completeness::AllowSubset);
        assert_eq!(
            problems,
            vec![Discrepancy::Extra {
                path: "meta/rogue.parquet".into()
            }]
        );
    }

    #[test]
    fn nested_directories_use_posix_relative_paths() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "a/b/c/deep.parquet", b"alpha");
        let m = manifest_for(&[("a/b/c/deep.parquet", &digest_of(b"alpha"), 5)]);
        assert!(m.verify_tree(dir.path(), Completeness::Complete).is_empty());
    }

    /// "We could not read it" must never read as "it is fine".
    #[test]
    fn a_directory_where_a_file_was_expected_is_not_silently_ok() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("meta/a.parquet")).unwrap();
        let m = manifest_for(&[("meta/a.parquet", ALPHA, 5)]);
        let problems = m.verify_tree(dir.path(), Completeness::Complete);
        assert_eq!(
            problems,
            vec![Discrepancy::Missing {
                path: "meta/a.parquet".into()
            }],
            "a directory is not the signed file"
        );
    }

    // --- parsing / binding -------------------------------------------------

    #[test]
    fn parses_the_upstream_shape() {
        let json = r#"{"manifest_version":1,"data_version":"2026.8.3",
            "tag":"data-2026.8.3","data_sha256":"abc","file_count":1,
            "tarball_sha256":"def",
            "files":{"meta/a.parquet":{"sha256":"aa","size":3}}}"#;
        let m = ContentManifest::parse(json).unwrap();
        assert_eq!(m.data_version, "2026.8.3");
        assert_eq!(m.tarball_sha256.as_deref(), Some("def"));
        assert_eq!(m.files["meta/a.parquet"].size, 3);
    }

    /// A future schema may add a field whose absence changes what verification
    /// *means*; checking only the parts we recognise would be a control that
    /// isn't one.
    #[test]
    fn an_unsupported_schema_version_is_refused() {
        let json = r#"{"manifest_version":99,"data_version":"2026.8.3",
            "tag":"data-2026.8.3","file_count":0,"files":{}}"#;
        let err = ContentManifest::parse(json).unwrap_err();
        assert!(err.contains("not supported"), "{err}");
    }

    #[test]
    fn an_inconsistent_file_count_is_refused() {
        let json = r#"{"manifest_version":1,"data_version":"2026.8.3",
            "tag":"data-2026.8.3","file_count":9,
            "files":{"a":{"sha256":"aa","size":1}}}"#;
        assert!(ContentManifest::parse(json)
            .unwrap_err()
            .contains("file_count"));
    }

    /// Replay: a genuine manifest for release A must not verify release B.
    #[test]
    fn a_manifest_for_another_release_is_refused() {
        let m = manifest_for(&[]);
        let err = m
            .check_binding(
                "2026.8.2",
                "nucl-parquet manifest 2026.8.3 tag=data-2026.8.3",
            )
            .unwrap_err();
        assert!(
            err.contains("2026.8.3") && err.contains("2026.8.2"),
            "{err}"
        );
    }

    /// The manifest's own fields are only as trustworthy as the signature over
    /// them, so the authenticated trusted comment must agree too.
    #[test]
    fn a_signature_issued_for_another_release_is_refused() {
        let m = manifest_for(&[]);
        let err = m
            .check_binding(
                "2026.8.3",
                "nucl-parquet manifest 2026.8.1 tag=data-2026.8.1",
            )
            .unwrap_err();
        assert!(err.contains("issued for data-2026.8.1"), "{err}");
    }

    #[test]
    fn a_signature_without_a_tag_cannot_bind() {
        let m = manifest_for(&[]);
        let err = m.check_binding("2026.8.3", "no tag here").unwrap_err();
        assert!(err.contains("no tag="), "{err}");
    }

    #[test]
    fn a_matching_release_binds() {
        let m = manifest_for(&[]);
        m.check_binding(
            "2026.8.3",
            "nucl-parquet manifest 2026.8.3 tag=data-2026.8.3 sha256=xy",
        )
        .unwrap();
    }
}

//! Enumerate what nuclear data is actually on disk (#653, epic #649).
//!
//! Nothing in the codebase could answer "which (library, projectile, target)
//! triples exist?" — `DatabaseProtocol` resolves a *requested* target to a file
//! but cannot list them, so a caller had to `read_dir` the xs directory itself.
//! That gap is why the target axis was never swept and why `catalog.json` could
//! claim projectiles it does not ship without anything noticing.
//!
//! This module answers it from the filesystem, and reconciles the answer
//! against the catalog's claims. It is the input to the coverage sweep (#654)
//! and the natural data source for a library explorer (#657).
//!
//! Deliberately filesystem-level and free of `DatabaseProtocol`: the point is to
//! see what is *there*, including the files the resolver currently cannot reach.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// One library as declared in `catalog.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogLibrary {
    pub id: String,
    /// Projectiles the catalog claims this library provides.
    pub projectiles: Vec<String>,
    /// `cross_sections`, `production_cross_sections`, `dose_coefficients`, …
    pub data_type: String,
    /// Declared path relative to the data root, e.g. `tendl-2023-iso/xs/`.
    pub path: String,
}

impl CatalogLibrary {
    /// Whether this library holds per-(projectile, target) cross-section files.
    /// Regulatory tables and structure data live under other `data_type`s and
    /// are not part of the `<lib>/xs/` universe.
    pub fn is_cross_section_library(&self) -> bool {
        self.data_type.contains("cross_sections")
    }

    /// Whether a user could select this library for a *production* calculation,
    /// i.e. whether the data store is expected to reach it at all.
    ///
    /// `experimental_cross_sections` (EXFOR measurements),
    /// `transport_cross_sections` (channel data) and
    /// `total_reaction_cross_sections` are consumed differently or not yet
    /// consumed; the store not reaching them is not by itself a defect.
    pub fn is_store_selectable(&self) -> bool {
        matches!(
            self.data_type.as_str(),
            "cross_sections" | "production_cross_sections"
        )
    }
}

/// A single cross-section file on disk.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct XsFile {
    pub library: String,
    /// Filename projectile stem: `p`, `d`, `n`, `g`, `c12`, `ar40`, …
    pub projectile: String,
    /// Element symbol for symbol-named files (`p_Cu.parquet`).
    pub symbol: Option<String>,
    /// Atomic number for Z-named files (`p_Z88.parquet`), which nucl-parquet
    /// uses for elements with no standard symbol mapping upstream — including
    /// mid-table Tc (43) and Pm (61), not only the actinides.
    pub z: Option<u32>,
    pub file_name: String,
}

/// Parse `{projectile}_{Symbol|Z<n>}.parquet`.
///
/// Returns `None` for anything that doesn't match, so the census can report
/// unexpected filenames rather than silently skipping them.
pub fn parse_xs_filename(file_name: &str) -> Option<(String, Option<String>, Option<u32>)> {
    let stem = file_name.strip_suffix(".parquet")?;
    let (projectile, target) = stem.split_once('_')?;
    if projectile.is_empty() || target.is_empty() {
        return None;
    }
    // `Z88` → atomic number; anything else is treated as an element symbol.
    if let Some(digits) = target.strip_prefix('Z') {
        if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
            return Some((projectile.to_string(), None, digits.parse().ok()));
        }
    }
    Some((projectile.to_string(), Some(target.to_string()), None))
}

/// Read and parse `catalog.json` from a data root.
pub fn load_catalog(data_root: &Path) -> Result<Vec<CatalogLibrary>, String> {
    let path = data_root.join("catalog.json");
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let val: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("cannot parse {}: {e}", path.display()))?;

    let libs = val
        .get("libraries")
        .and_then(|l| l.as_object())
        .ok_or_else(|| format!("{} has no `libraries` object", path.display()))?;

    let mut out = Vec::new();
    for (id, entry) in libs {
        out.push(CatalogLibrary {
            id: id.clone(),
            projectiles: entry
                .get("projectiles")
                .and_then(|p| p.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
            data_type: entry
                .get("data_type")
                .and_then(|d| d.as_str())
                .unwrap_or_default()
                .to_string(),
            path: entry
                .get("path")
                .and_then(|p| p.as_str())
                .unwrap_or_default()
                .to_string(),
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

/// Where a library's xs files actually live, honouring the catalog `path`.
///
/// The store hardcodes `{data_root}/{library}/xs`, which is why `exfor` and
/// `exfor-channels` — whose catalog `path` has no `/xs/` component — are
/// unreachable through it. The census resolves the declared path so it can
/// *see* those files and report the mismatch instead of inheriting the bug.
pub fn resolve_xs_dir(data_root: &Path, lib: &CatalogLibrary) -> Option<PathBuf> {
    [
        data_root.join(lib.path.trim_end_matches('/')),
        data_root.join(&lib.id).join("xs"),
        data_root.join(&lib.id),
    ]
    .into_iter()
    .find(|candidate| candidate.is_dir())
}

/// Every cross-section file for one library.
pub fn scan_library(data_root: &Path, lib: &CatalogLibrary) -> Vec<XsFile> {
    let Some(dir) = resolve_xs_dir(data_root, lib) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.ends_with(".parquet") {
            continue;
        }
        if let Some((projectile, symbol, z)) = parse_xs_filename(&name) {
            out.push(XsFile {
                library: lib.id.clone(),
                projectile,
                symbol,
                z,
                file_name: name,
            });
        }
    }
    out.sort();
    out
}

/// Filenames under a library's xs dir that don't match either naming
/// convention. Reported rather than skipped — an unparsed name is a file no
/// engine can reach.
pub fn unparsable_files(data_root: &Path, lib: &CatalogLibrary) -> Vec<String> {
    let Some(dir) = resolve_xs_dir(data_root, lib) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out: Vec<String> = entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".parquet") && parse_xs_filename(n).is_none())
        .collect();
    out.sort();
    out
}

/// A contradiction between what `catalog.json` claims and what is on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CensusProblem {
    /// The catalog claims a projectile for which no file exists. A user
    /// selecting it gets silence.
    ClaimedProjectileHasNoFiles { library: String, projectile: String },
    /// Files exist for a projectile the catalog does not advertise.
    UnclaimedProjectileHasFiles { library: String, projectile: String },
    /// The declared `path` resolves nowhere, so the library is unreachable.
    PathDoesNotResolve { library: String, path: String },
    /// A `.parquet` matching neither naming convention.
    UnparsableFilename { library: String, file_name: String },
    /// The files exist at the catalog's declared path, but not where the data
    /// store looks. `NpDataStore::ensure_xs` hardcodes `{data_root}/{lib}/xs`,
    /// so a library whose declared `path` omits the `/xs/` component is
    /// unreachable from every engine no matter what the catalog says.
    UnreachableByStore {
        library: String,
        declared_path: String,
        store_expects: String,
    },
}

impl CensusProblem {
    /// Whether this is an unambiguous contradiction (fail CI) rather than an
    /// observation worth printing (report only).
    ///
    /// `UnreachableByStore` is soft on purpose: for a library the store is not
    /// expected to serve, a path mismatch is a design choice, not a bug. The
    /// scan only raises it for store-selectable libraries, but even then it can
    /// be a deliberate staging step, so it is reported rather than fatal.
    pub fn is_hard_error(&self) -> bool {
        !matches!(self, Self::UnreachableByStore { .. })
    }

    pub fn message(&self) -> String {
        match self {
            Self::ClaimedProjectileHasNoFiles { library, projectile } => format!(
                "{library}: catalog claims projectile '{projectile}' but ships no files for it"
            ),
            Self::UnclaimedProjectileHasFiles { library, projectile } => format!(
                "{library}: has files for projectile '{projectile}' which the catalog does not claim"
            ),
            Self::PathDoesNotResolve { library, path } => {
                format!("{library}: declared path '{path}' resolves to no directory")
            }
            Self::UnparsableFilename { library, file_name } => format!(
                "{library}: '{file_name}' matches neither {{proj}}_{{Symbol}}.parquet nor {{proj}}_Z{{Z}}.parquet"
            ),
            Self::UnreachableByStore {
                library,
                declared_path,
                store_expects,
            } => format!(
                "{library}: files live at '{declared_path}' but the data store only looks in \
                 '{store_expects}' — this library is unreachable from every engine"
            ),
        }
    }
}

/// The full picture: what exists, and where it contradicts the catalog.
#[derive(Debug, Clone)]
pub struct Census {
    pub files: Vec<XsFile>,
    pub problems: Vec<CensusProblem>,
    pub libraries: Vec<CatalogLibrary>,
}

impl Census {
    /// Walk every cross-section library in the catalog.
    pub fn scan(data_root: &Path) -> Result<Self, String> {
        let libraries = load_catalog(data_root)?;
        let mut files = Vec::new();
        let mut problems = Vec::new();

        for lib in libraries.iter().filter(|l| l.is_cross_section_library()) {
            if resolve_xs_dir(data_root, lib).is_none() {
                // A library absent from a sparse checkout is not a defect — only
                // one whose *declared path* cannot resolve even though the
                // library directory is present.
                if data_root.join(&lib.id).is_dir() {
                    problems.push(CensusProblem::PathDoesNotResolve {
                        library: lib.id.clone(),
                        path: lib.path.clone(),
                    });
                }
                continue;
            }

            // The store's fixed `{lib}/xs` convention vs the catalog's declared
            // path. When they disagree the census can still read the files, but
            // no engine can.
            if lib.is_store_selectable() && !data_root.join(&lib.id).join("xs").is_dir() {
                problems.push(CensusProblem::UnreachableByStore {
                    library: lib.id.clone(),
                    declared_path: lib.path.clone(),
                    store_expects: format!("{}/xs/", lib.id),
                });
            }

            let lib_files = scan_library(data_root, lib);
            for file_name in unparsable_files(data_root, lib) {
                problems.push(CensusProblem::UnparsableFilename {
                    library: lib.id.clone(),
                    file_name,
                });
            }

            let present: BTreeSet<&str> = lib_files.iter().map(|f| f.projectile.as_str()).collect();
            let claimed: BTreeSet<&str> = lib.projectiles.iter().map(String::as_str).collect();

            for p in claimed.difference(&present) {
                problems.push(CensusProblem::ClaimedProjectileHasNoFiles {
                    library: lib.id.clone(),
                    projectile: (*p).to_string(),
                });
            }
            for p in present.difference(&claimed) {
                problems.push(CensusProblem::UnclaimedProjectileHasFiles {
                    library: lib.id.clone(),
                    projectile: (*p).to_string(),
                });
            }

            files.extend(lib_files);
        }

        Ok(Self {
            files,
            problems,
            libraries,
        })
    }

    /// Target-element coverage per (library, projectile). The ragged edges are
    /// normal and upstream — reported, never failed on.
    pub fn coverage(&self) -> BTreeMap<(String, String), BTreeSet<String>> {
        let mut m: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
        for f in &self.files {
            let target = f
                .symbol
                .clone()
                .unwrap_or_else(|| format!("Z{}", f.z.unwrap_or(0)));
            m.entry((f.library.clone(), f.projectile.clone()))
                .or_default()
                .insert(target);
        }
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_symbol_named_files() {
        assert_eq!(
            parse_xs_filename("p_Cu.parquet"),
            Some(("p".into(), Some("Cu".into()), None))
        );
        // Single-letter symbols must not be mistaken for anything else — the
        // absence of p_H/p_He in tendl is an upstream gap, not a parse failure.
        assert_eq!(
            parse_xs_filename("a_B.parquet"),
            Some(("a".into(), Some("B".into()), None))
        );
    }

    #[test]
    fn parses_z_named_files() {
        assert_eq!(
            parse_xs_filename("p_Z88.parquet"),
            Some(("p".into(), None, Some(88)))
        );
        assert_eq!(
            parse_xs_filename("t_Z43.parquet"),
            Some(("t".into(), None, Some(43)))
        );
    }

    #[test]
    fn parses_heavy_ion_and_neutron_stems() {
        assert_eq!(
            parse_xs_filename("c12_Cu.parquet"),
            Some(("c12".into(), Some("Cu".into()), None))
        );
        assert_eq!(
            parse_xs_filename("n_Ac.parquet"),
            Some(("n".into(), Some("Ac".into()), None))
        );
        assert_eq!(
            parse_xs_filename("g_Ag.parquet"),
            Some(("g".into(), Some("Ag".into()), None))
        );
    }

    #[test]
    fn rejects_names_that_are_not_xs_files() {
        assert_eq!(parse_xs_filename("manifest.json"), None);
        assert_eq!(parse_xs_filename("noseparator.parquet"), None);
        assert_eq!(parse_xs_filename("_Cu.parquet"), None);
        assert_eq!(parse_xs_filename("p_.parquet"), None);
        // `Zx` is not a Z-number, so it is a (weird) symbol, not a parse failure.
        assert_eq!(
            parse_xs_filename("p_Zn.parquet"),
            Some(("p".into(), Some("Zn".into()), None))
        );
    }
}

//! Core data types mirroring the Python/TypeScript data model.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

use crate::constants::{ELEMENTARY_CHARGE, LN2};
use crate::projectile::Projectile;

/// Projectile type identifier.
///
/// Serializes as a string: "p", "d", "t", "h", "a" for light ions,
/// "C-12", "O-16", "Ne-20" etc. for heavy ions.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProjectileType {
    Proton,
    Deuteron,
    Tritium,
    Helion,
    Alpha,
    /// Heavy ion with element symbol, Z, and A (e.g., C-12, O-16).
    HeavyIon {
        symbol: String,
        z: u32,
        a: u32,
    },
}

impl Serialize for ProjectileType {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.symbol_string())
    }
}

impl<'de> Deserialize<'de> for ProjectileType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown projectile: {s}")))
    }
}

impl ProjectileType {
    /// Canonical string representation.
    pub fn symbol_string(&self) -> String {
        match self {
            Self::Proton => "p".to_string(),
            Self::Deuteron => "d".to_string(),
            Self::Tritium => "t".to_string(),
            Self::Helion => "h".to_string(),
            Self::Alpha => "a".to_string(),
            Self::HeavyIon { symbol, a, .. } => format!("{}-{}", symbol, a),
        }
    }

    /// Short symbol for light ions, element symbol for heavy ions.
    pub fn symbol(&self) -> &str {
        match self {
            Self::Proton => "p",
            Self::Deuteron => "d",
            Self::Tritium => "t",
            Self::Helion => "h",
            Self::Alpha => "a",
            Self::HeavyIon { symbol, .. } => symbol,
        }
    }

    /// Filename stem used to look this projectile up in a cross-section library.
    ///
    /// Light ions keep their short symbol (`p_Cu.parquet`). Heavy ions use
    /// nucl-parquet's `{symbol_lowercase}{A}` convention — `C-12` becomes
    /// `c12`, giving `c12_Cu.parquet`.
    ///
    /// This exists because [`Self::symbol`] returns the bare element symbol for
    /// a heavy ion (`"C"`), and the compute path used that to build the lookup
    /// key — so it searched for `C_Cu.parquet`, which does not exist. Every
    /// heavy-ion run therefore produced zero isotopes, silently, in every engine
    /// (#659). `symbol()` has other callers that want the element, so this is a
    /// separate accessor rather than a change to it.
    pub fn xs_key(&self) -> String {
        match self {
            Self::HeavyIon { symbol, a, .. } => format!("{}{}", symbol.to_lowercase(), a),
            other => other.symbol().to_string(),
        }
    }

    /// Parse from string: "p", "d", "t", "h", "a", "C-12", "O-16", etc.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "p" => Some(Self::Proton),
            "d" => Some(Self::Deuteron),
            "t" => Some(Self::Tritium),
            "h" => Some(Self::Helion),
            "a" => Some(Self::Alpha),
            _ => {
                // Heavy ion notation: Symbol-A (e.g., C-12, O-16, Ne-20)
                let parts: Vec<&str> = s.splitn(2, '-').collect();
                if parts.len() != 2 {
                    return None;
                }
                let symbol = parts[0];
                let a: u32 = parts[1].parse().ok()?;
                let z = *crate::materials::SYMBOL_TO_Z_MAP.get(symbol)?;
                Some(Self::HeavyIon {
                    symbol: symbol.to_string(),
                    z,
                    a,
                })
            }
        }
    }

    pub fn projectile(&self) -> Projectile {
        Projectile::from_type(self)
    }

    pub fn z(&self) -> u32 {
        self.projectile().z
    }

    pub fn a(&self) -> u32 {
        self.projectile().a
    }
}

/// Incident beam specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Beam {
    pub projectile: ProjectileType,
    pub energy_mev: f64,
    pub current_ma: f64,
}

impl Beam {
    pub fn new(projectile: ProjectileType, energy_mev: f64, current_ma: f64) -> Self {
        Self {
            projectile,
            energy_mev,
            current_ma,
        }
    }

    pub fn particles_per_second(&self) -> f64 {
        let z = self.projectile.z() as f64;
        (self.current_ma * 1e-3) / (z * ELEMENTARY_CHARGE)
    }
}

/// Element with isotopic composition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Element {
    pub symbol: String,
    pub z: u32,
    /// A -> fractional abundance (sums to ~1.0).
    pub isotopes: HashMap<u32, f64>,
}

/// Single target layer specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer {
    pub density_g_cm3: f64,
    /// (Element, atom_fraction) pairs.
    pub elements: Vec<(Element, f64)>,
    pub thickness_cm: Option<f64>,
    pub areal_density_g_cm2: Option<f64>,
    pub energy_out_mev: Option<f64>,
    pub is_monitor: bool,
    /// NIST compound name for direct stopping-power table lookup.
    /// When set, `compound_dedx_with_nist` uses ICRU-measured data
    /// instead of Bragg additivity (e.g. "WATER_LIQUID").
    #[serde(skip)]
    pub nist_compound: Option<String>,
    /// Computed fields (set during simulation).
    #[serde(skip)]
    pub computed_energy_in: f64,
    #[serde(skip)]
    pub computed_energy_out: f64,
    #[serde(skip)]
    pub computed_thickness: f64,
}

impl Layer {
    pub fn average_atomic_mass(&self) -> f64 {
        let mut total = 0.0;
        for (elem, frac) in &self.elements {
            let mut elem_mass = 0.0;
            for (&a, &ab) in &elem.isotopes {
                elem_mass += a as f64 * ab;
            }
            total += frac * elem_mass;
        }
        total
    }
}

/// Time-varying beam current during irradiation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentProfile {
    /// Monotonic times starting at 0 [s].
    pub times_s: Vec<f64>,
    /// Piecewise-constant currents [mA].
    pub currents_ma: Vec<f64>,
}

impl CurrentProfile {
    /// Construct from time and current arrays with validation.
    ///
    /// # Errors
    /// Returns `Err` if lengths mismatch, arrays are empty,
    /// times are non-monotonic, or any current is negative.
    pub fn from_values(times_s: Vec<f64>, currents_ma: Vec<f64>) -> Result<Self, String> {
        if times_s.len() != currents_ma.len() {
            return Err(format!(
                "times_s and currents_ma must have the same length, got {} and {}",
                times_s.len(),
                currents_ma.len()
            ));
        }
        if times_s.is_empty() {
            return Err("CurrentProfile must have at least one entry".into());
        }
        if times_s.iter().chain(currents_ma.iter()).any(|v| v.is_nan()) {
            return Err("times_s and currents_ma must not contain NaN".into());
        }
        for w in times_s.windows(2) {
            if w[1] < w[0] {
                return Err(format!(
                    "times_s must be monotonically increasing, got {} followed by {}",
                    w[0], w[1]
                ));
            }
        }
        if currents_ma.iter().any(|&c| c < 0.0) {
            return Err("currents_ma must be non-negative".into());
        }
        Ok(Self {
            times_s,
            currents_ma,
        })
    }

    /// Return intervals as [(t_start, t_end, current_mA)].
    pub fn intervals(&self, t_end: f64) -> Vec<(f64, f64, f64)> {
        let mut result = Vec::new();
        for i in 0..self.times_s.len() {
            let t_start = self.times_s[i];
            if t_start >= t_end {
                break;
            }
            let t_end_i = if i + 1 < self.times_s.len() {
                self.times_s[i + 1].min(t_end)
            } else {
                t_end
            };
            result.push((t_start, t_end_i, self.currents_ma[i]));
        }
        if !result.is_empty() && result[0].0 > 0.0 {
            result.insert(0, (0.0, result[0].0, self.currents_ma[0]));
        } else if result.is_empty() {
            result.push((0.0, t_end, self.currents_ma[0]));
        }
        result
    }
}

/// Ordered stack of target layers traversed by the beam.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetStack {
    pub beam: Beam,
    pub layers: Vec<Layer>,
    pub irradiation_time_s: f64,
    pub cooling_time_s: f64,
    pub area_cm2: f64,
    pub current_profile: Option<CurrentProfile>,
}

/// Cross-section data for one reaction channel.
#[derive(Debug, Clone)]
pub struct CrossSectionData {
    pub target_a: u32,
    pub residual_z: u32,
    pub residual_a: u32,
    pub state: String,
    pub energies_mev: Vec<f64>,
    pub xs_mb: Vec<f64>,
}

/// Single decay mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecayMode {
    pub mode: String,
    pub daughter_z: Option<u32>,
    pub daughter_a: Option<u32>,
    pub daughter_state: String,
    pub branching: f64,
}

/// Decay data for a nuclide.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecayData {
    pub z: u32,
    pub a: u32,
    pub state: String,
    pub half_life_s: Option<f64>,
    pub decay_modes: Vec<DecayMode>,
}

/// Single point in a depth profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepthPoint {
    pub depth_cm: f64,
    pub energy_mev: f64,
    pub dedx_mev_cm: f64,
    pub heat_w_cm3: f64,
}

/// Production result for a single isotope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsotopeResult {
    pub name: String,
    pub z: u32,
    pub a: u32,
    pub state: String,
    pub half_life_s: Option<f64>,
    pub production_rate: f64,
    pub saturation_yield_bq_ua: f64,
    pub activity_bq: f64,
    pub time_grid_s: Vec<f64>,
    pub activity_vs_time_bq: Vec<f64>,
    pub source: String,
    pub activity_direct_bq: f64,
    pub activity_ingrowth_bq: f64,
    pub activity_direct_vs_time_bq: Vec<f64>,
    pub activity_ingrowth_vs_time_bq: Vec<f64>,
    pub reactions: Vec<String>,
    pub decay_notations: Vec<String>,
}

/// One element as the engine actually resolved it (#666).
///
/// `isotopes` is the resolved abundance vector, which is what distinguishes a
/// natural target from an enriched one. Carrying only the symbol would make
/// those two indistinguishable in a shared result while their yields differ by
/// orders of magnitude.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ElementProvenance {
    pub symbol: String,
    pub z: u32,
    /// Fraction of the layer's atoms that are this element.
    pub atom_fraction: f64,
    /// A → fractional abundance, after enrichment is applied.
    pub isotopes: BTreeMap<u32, f64>,
}

/// What was actually irradiated, recorded at compute time (#666).
///
/// # Why this lives on the result rather than in the exported config
///
/// A shared result carries its answers in full and, until this, barely carried
/// its question: the viewer artifact (ADR 0008) embedded a *material name*
/// (`"havar"`) and nothing about what that name resolved to. Material names are
/// resolved through py-materials/MorePET, so the same name can resolve
/// differently between versions — identical inputs, different outputs, and
/// nothing in the file to reveal why. That is the failure class #593/#601
/// closed for nuclear data, left open for the target.
///
/// Recording it here, at the moment the layer is computed, is what makes it
/// *provenance* rather than a second copy of the input: it cannot describe a
/// layer other than the one that produced these numbers, it cannot be a stale
/// re-resolution, and every surface (browser, desktop, MCP, plain JSON API)
/// gets the same fields without three separate mappings to keep in step.
///
/// `Default` is the honest empty value for the zero-production early-returns
/// where no composition was resolved — not a fabricated one.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct LayerProvenance {
    /// Density used for stopping power [g/cm³] — the resolved value, which may
    /// be a material default rather than anything the caller supplied.
    pub density_g_cm3: f64,
    /// Thickness the simulation actually used [cm]. This is `computed_thickness`,
    /// not the requested one: an areal-density or energy-out spec produces a
    /// thickness the caller never stated.
    pub thickness_cm: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub areal_density_g_cm2: Option<f64>,
    pub is_monitor: bool,
    /// NIST compound name when ICRU-measured stopping data replaced Bragg
    /// additivity (#542). Its presence changes the stopping model, so its
    /// absence has to be distinguishable from its presence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nist_compound: Option<String>,
    pub elements: Vec<ElementProvenance>,
}

impl LayerProvenance {
    /// Snapshot a resolved layer. Reads `computed_thickness`, so it is only
    /// meaningful after the layer has been computed.
    pub fn from_layer(layer: &Layer) -> Self {
        Self {
            density_g_cm3: layer.density_g_cm3,
            thickness_cm: layer.computed_thickness,
            areal_density_g_cm2: layer.areal_density_g_cm2,
            is_monitor: layer.is_monitor,
            nist_compound: layer.nist_compound.clone(),
            elements: layer
                .elements
                .iter()
                .map(|(el, frac)| ElementProvenance {
                    symbol: el.symbol.clone(),
                    z: el.z,
                    atom_fraction: *frac,
                    isotopes: el.isotopes.iter().map(|(a, ab)| (*a, *ab)).collect(),
                })
                .collect(),
        }
    }
}

/// Full result for a single layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerResult {
    pub energy_in: f64,
    pub energy_out: f64,
    pub delta_e_mev: f64,
    pub heat_kw: f64,
    /// What was irradiated to produce this (#666). `#[serde(default)]` so
    /// results written before it still deserialize — they land on the empty
    /// value, which reports "not recorded" rather than inventing a target.
    #[serde(default)]
    pub provenance: LayerProvenance,
    pub depth_profile: Vec<DepthPoint>,
    pub isotope_results: HashMap<String, IsotopeResult>,
    pub stopping_power_sources: HashMap<u32, String>,
    /// Per-isotope production rate at each depth_profile point [atoms/s/cm].
    /// Keyed by the same isotope name as isotope_results. Summed across target channels.
    #[serde(default)]
    pub depth_production_rates: HashMap<String, Vec<f64>>,
    /// Free neutrons produced per second in this layer by (x,n)-type charged
    /// reactions — the source term for secondary neutron activation (ADR-0003
    /// Phase 2). Zero on the neutron-source and stopping-only paths.
    #[serde(default)]
    pub neutron_source_rate: f64,
    /// How many produced isotopes were pruned as numerical dust before the
    /// chain solver ran (issue #533). Non-zero values are surfaced so the
    /// filter is never silent — the pruned entries had activity below the
    /// relative floor *and* production rate below the relative rate floor
    /// (long-lived, low-yield minor-component products are exempt from the
    /// activity floor and reach the reported inventory).
    #[serde(default)]
    pub pruned_negligible_count: usize,
}

/// How loudly a [`Diagnostic`] should be presented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticSeverity {
    /// The result is wrong or empty for a reason the user must see. Render at
    /// error prominence even though `compute_stack` returned `Ok`.
    Error,
    /// The result is usable but less complete or less certain than it looks.
    Warning,
}

/// A machine-readable reason a result is emptier than it looks (#650, epic #649).
///
/// The failure users report as "some elements do not work" is a simulation that
/// completes successfully and renders nothing. Every path that can produce that
/// outcome now records *why*, so "no data for this target" is distinguishable
/// from "computed, genuinely zero yield" at every layer — engine, wire, and UI.
///
/// `#[non_exhaustive]` is load-bearing: it forces every consumer's `match` to be
/// updated when a variant is added, so a new kind cannot ship without a
/// renderer. `pruned_negligible_count` on [`LayerResult`] is the cautionary
/// precedent — a diagnostic channel with no forcing function that acquired zero
/// consumers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum DiagnosticKind {
    /// No cross-section data for this (projectile, target) pair. The dominant
    /// cause of a silently empty result: a missing parquet, a target the
    /// library does not cover, or a naming mismatch.
    NoCrossSectionData {
        projectile: String,
        target_z: u32,
        target_symbol: String,
        target_a: u32,
    },
    /// The element resolved with no isotopes at all, so it contributes zero
    /// mass. Hits every element with no naturally-occurring isotopes (Tc, Pm,
    /// Po, At, Rn, Fr, Ra, Ac, Pa, …) unless enrichment was supplied.
    EmptyIsotopeComposition { symbol: String, z: u32 },
    /// Cross-sections exist for this target, but every channel is tabulated
    /// outside the energy the beam actually spans in this layer, so all of them
    /// interpolate to zero.
    ///
    /// Found by the coverage sweep (#654): jendl-5's d + Cu channels cover
    /// 130–200 MeV only, so a 20 MeV deuteron run produced nothing and said
    /// nothing — the data was there, just not where the beam was.
    ReactionOutsideEnergyRange {
        symbol: String,
        data_min_mev: f64,
        data_max_mev: f64,
        beam_min_mev: f64,
        beam_max_mev: f64,
    },
}

impl DiagnosticKind {
    /// Human-readable rendering. Kept as a method rather than a struct field so
    /// the text cannot drift from the data, and so non-UI surfaces (CLI, MCP)
    /// get the same wording for free.
    pub fn message(&self) -> String {
        match self {
            Self::NoCrossSectionData {
                projectile,
                target_symbol,
                target_a,
                ..
            } => format!(
                "No cross-section data for {projectile} + {target_symbol}-{target_a} \
                 in this library — that target isotope produced nothing."
            ),
            Self::EmptyIsotopeComposition { symbol, .. } => format!(
                "{symbol} has no naturally-occurring isotopes, so it contributes no \
                 target mass. Specify an enrichment to use it as a target."
            ),
            Self::ReactionOutsideEnergyRange {
                symbol,
                data_min_mev,
                data_max_mev,
                beam_min_mev,
                beam_max_mev,
            } => format!(
                "Cross-sections for {symbol} are tabulated from {data_min_mev:.3} to \
                 {data_max_mev:.3} MeV, but the beam only spans {beam_min_mev:.3}–\
                 {beam_max_mev:.3} MeV in this layer — no channel overlaps, so nothing \
                 is produced. Try a different beam energy or library."
            ),
        }
    }

    /// Default severity for this kind.
    pub fn severity(&self) -> DiagnosticSeverity {
        match self {
            Self::NoCrossSectionData { .. } => DiagnosticSeverity::Error,
            Self::EmptyIsotopeComposition { .. } => DiagnosticSeverity::Error,
            Self::ReactionOutsideEnergyRange { .. } => DiagnosticSeverity::Error,
        }
    }
}

/// A diagnostic attached to a [`StackResult`], optionally scoped to a layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Diagnostic {
    #[serde(flatten)]
    pub kind: DiagnosticKind,
    pub severity: DiagnosticSeverity,
    /// 0-based index into `StackResult::layer_results`, when the diagnostic is
    /// attributable to one layer.
    pub layer_index: Option<usize>,
    /// Pre-rendered text, so consumers that cannot match on the enum (JS, the
    /// shared-results HTML) still show something useful.
    pub message: String,
}

impl Diagnostic {
    pub fn new(kind: DiagnosticKind, layer_index: Option<usize>) -> Self {
        Self {
            severity: kind.severity(),
            message: kind.message(),
            kind,
            layer_index,
        }
    }
}

/// Full result for all layers in a stack.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackResult {
    pub layer_results: Vec<LayerResult>,
    pub irradiation_time_s: f64,
    pub cooling_time_s: f64,
    /// Which nuclear data produced this result (#593) — version, library, and
    /// the verified tarball hash where one applies.
    ///
    /// `#[serde(default)]` so results written before #593 still deserialize;
    /// they land on [`Provenance::unknown`], which reports
    /// [`DataSource::Unknown`](crate::provenance::DataSource::Unknown) rather
    /// than inheriting the running build's identity.
    #[serde(default)]
    pub provenance: crate::provenance::Provenance,
    /// Why this result is emptier than it looks (#650). Empty on a clean run.
    ///
    /// `#[serde(default)]` so results serialized before #650 still deserialize.
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
}

/// A single radiation emission line for a decaying nuclide (#427).
///
/// Sourced from the parent-keyed `meta/ensdf/emissions/{Symbol}.parquet`
/// dataset: each row is one γ / X-ray / Auger / conversion-electron / β± /
/// annihilation line emitted *per decay of the parent* `(z, a, state)`.
/// `intensity_per_decay` is absolute (the raw `intensity_pct / 100`), so a
/// 511 keV annihilation pair reads `2.0` and Co-60's 1173 keV γ reads `0.9986`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmissionLine {
    /// `gamma` | `xray` | `auger` | `ce` | `beta-` | `beta+` | `annihilation`.
    pub rad_type: String,
    pub energy_kev: f64,
    /// Photons/particles emitted per parent decay (absolute, not %).
    pub intensity_per_decay: f64,
    /// Feeding decay channel: `EC` | `beta-` | `beta+` | `IT` |
    /// `KshellEC` / `LshellEC` / `MshellEC`. `None` when unfiled.
    pub decay_mode: Option<String>,
    pub daughter_z: Option<u32>,
    pub daughter_a: Option<u32>,
    /// Total internal-conversion coefficient (γ lines only).
    pub icc_total: Option<f64>,
    /// Line subtype, e.g. `Kα1` (X-ray) or `KLL` (Auger). `None` for γ.
    pub rad_subtype: Option<String>,
}

/// Chain isotope used in the coupled ODE solver.
#[derive(Debug, Clone)]
pub struct ChainIsotope {
    pub z: u32,
    pub a: u32,
    pub state: String,
    pub half_life_s: Option<f64>,
    pub production_rate: f64,
    pub decay_modes: Vec<DecayMode>,
}

impl ChainIsotope {
    pub fn key(&self) -> String {
        format!("{}-{}-{}", self.z, self.a, self.state)
    }

    pub fn is_stable(&self) -> bool {
        match self.half_life_s {
            None => true,
            Some(t) => t <= 0.0,
        }
    }

    pub fn lambda(&self) -> f64 {
        match self.half_life_s {
            Some(t) if t > 0.0 => LN2 / t,
            _ => 0.0,
        }
    }
}

/// Solution from the coupled decay chain solver.
#[derive(Debug, Clone)]
pub struct ChainSolution {
    pub isotopes: Vec<ChainIsotope>,
    pub time_grid_s: Vec<f64>,
    /// Abundances: [n_isotopes][n_times].
    pub abundances: Vec<Vec<f64>>,
    /// Activities: [n_isotopes][n_times].
    pub activities: Vec<Vec<f64>>,
    pub activities_direct: Vec<Vec<f64>>,
    pub activities_ingrowth: Vec<Vec<f64>>,
    /// For each isotope in `isotopes`, the list of parents that feed into it
    /// via decay. Each entry is `(parent_key, branching_ratio, decay_mode)`
    /// where `parent_key` has the form `"{Z}-{A}-{state}"` (same as
    /// `ChainIsotope::key`) and `decay_mode` is the raw `DecayMode::mode`
    /// string from the nuclear data (e.g. "β-", "EC", "IT").
    pub parent_info: Vec<Vec<(String, f64, String)>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_values_valid() {
        let cp = CurrentProfile::from_values(vec![0.0, 1.0, 2.0], vec![0.05, 0.05, 0.04]);
        assert!(cp.is_ok());
        let cp = cp.unwrap();
        assert_eq!(cp.times_s.len(), 3);
        assert_eq!(cp.currents_ma.len(), 3);
    }

    #[test]
    fn from_values_single_entry() {
        let cp = CurrentProfile::from_values(vec![0.0], vec![0.05]);
        assert!(cp.is_ok());
    }

    #[test]
    fn from_values_length_mismatch() {
        let cp = CurrentProfile::from_values(vec![0.0, 1.0], vec![0.05]);
        assert!(cp.is_err());
        assert!(cp.unwrap_err().contains("same length"));
    }

    #[test]
    fn from_values_empty() {
        let cp = CurrentProfile::from_values(vec![], vec![]);
        assert!(cp.is_err());
        assert!(cp.unwrap_err().contains("at least one"));
    }

    #[test]
    fn from_values_non_monotonic() {
        let cp = CurrentProfile::from_values(vec![0.0, 2.0, 1.0], vec![0.05, 0.05, 0.04]);
        assert!(cp.is_err());
        assert!(cp.unwrap_err().contains("monotonically increasing"));
    }

    #[test]
    fn from_values_negative_current() {
        let cp = CurrentProfile::from_values(vec![0.0, 1.0], vec![0.05, -0.01]);
        assert!(cp.is_err());
        assert!(cp.unwrap_err().contains("non-negative"));
    }

    #[test]
    fn from_values_zero_current_allowed() {
        let cp = CurrentProfile::from_values(vec![0.0, 1.0], vec![0.05, 0.0]);
        assert!(cp.is_ok());
    }

    #[test]
    fn from_values_nan_rejected() {
        let cp = CurrentProfile::from_values(vec![0.0, f64::NAN], vec![0.05, 0.05]);
        assert!(cp.is_err());
        assert!(cp.unwrap_err().contains("NaN"));

        let cp = CurrentProfile::from_values(vec![0.0, 1.0], vec![f64::NAN, 0.05]);
        assert!(cp.is_err());
        assert!(cp.unwrap_err().contains("NaN"));
    }
}

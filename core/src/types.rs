//! Core data types mirroring the Python/TypeScript data model.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::constants::{ELEMENTARY_CHARGE, LN2};
use crate::projectile::Projectile;

/// Projectile type identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProjectileType {
    #[serde(rename = "p")]
    Proton,
    #[serde(rename = "d")]
    Deuteron,
    #[serde(rename = "t")]
    Tritium,
    #[serde(rename = "h")]
    Helion,
    #[serde(rename = "a")]
    Alpha,
}

impl ProjectileType {
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Proton => "p",
            Self::Deuteron => "d",
            Self::Tritium => "t",
            Self::Helion => "h",
            Self::Alpha => "a",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "p" => Some(Self::Proton),
            "d" => Some(Self::Deuteron),
            "t" => Some(Self::Tritium),
            "h" => Some(Self::Helion),
            "a" => Some(Self::Alpha),
            _ => None,
        }
    }

    pub fn projectile(self) -> Projectile {
        Projectile::from_type(self)
    }

    pub fn z(self) -> u32 {
        self.projectile().z
    }

    pub fn a(self) -> u32 {
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

/// Full result for a single layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerResult {
    pub energy_in: f64,
    pub energy_out: f64,
    pub delta_e_mev: f64,
    pub heat_kw: f64,
    pub depth_profile: Vec<DepthPoint>,
    pub isotope_results: HashMap<String, IsotopeResult>,
    pub stopping_power_sources: HashMap<u32, String>,
}

/// Full result for all layers in a stack.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackResult {
    pub layer_results: Vec<LayerResult>,
    pub irradiation_time_s: f64,
    pub cooling_time_s: f64,
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
}

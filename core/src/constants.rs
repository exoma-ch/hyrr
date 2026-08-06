//! Physical constants used across compute modules.

/// Avogadro's number [mol⁻¹].
pub const AVOGADRO: f64 = 6.022_140_76e23;

/// ln(2).
pub const LN2: f64 = core::f64::consts::LN_2;

/// 1 barn in cm².
pub const BARN_CM2: f64 = 1e-24;

/// 1 millibarn in cm².
pub const MILLIBARN_CM2: f64 = 1e-27;

/// Elementary charge [C].
pub const ELEMENTARY_CHARGE: f64 = 1.602_176_634e-19;

/// MeV to Joule conversion factor.
pub const MEV_TO_JOULE: f64 = 1.602_176_634e-13;

/// Minimum peak cross-section (mb) on the integration grid to consider active.
pub const MIN_PEAK_XS_MB: f64 = 1e-6;

/// Dust threshold for production rate [atoms/s] — anything strictly below this
/// is treated as numerical noise (subnormal / underflow residue) and pruned
/// from the isotope inventory before the chain solver runs. This is not a
/// relevance filter — real production rates span many orders of magnitude and
/// the caller decides what "small" means via the reporting-layer
/// `activity_floor_bq` (issue #567). `f64::MIN_POSITIVE` (~2.2e-308) is the
/// smallest normal positive f64; smaller values are subnormal and cannot
/// meaningfully accumulate in downstream arithmetic.
///
/// Deliberately **unit-agnostic**: the prune applies it to a production rate
/// (atoms/s) *and* to activity samples (Bq). It is a floating-point-magnitude
/// floor, not a physical quantity, so one constant is honest for both — naming
/// it after either unit would not be.
pub const DUST_MAGNITUDE_THRESHOLD: f64 = f64::MIN_POSITIVE;

/// Half-life threshold (s) above which isotopes are "geologically long-lived".
pub const LONG_HALFLIFE_THRESHOLD_S: f64 = 1e10;

/// Max chain size for matrix exponential solver.
pub const MAX_CHAIN_SIZE: usize = 40;

/// Beam energy below which the projectile is treated as stopped [MeV].
///
/// PSTAR/ASTAR tables end near 1 keV; catima varies but is comparable. Below
/// this floor the residual range is sub-µm for any physical target (nanometers
/// for a keV proton in metal), and — more importantly — a stopping-power lookup
/// underneath the table minimum returns `EnergyOutOfRange` and aborts the whole
/// run. So `compute_energy_out` treats "energy stepped below MIN_TRACKED" the
/// same as "energy stepped below 0" (beam stopped; downstream layers see zero
/// flux), and `compute_thickness_from_energy` clamps the integration lower
/// bound so a user-configured `energy_out = 0` degrader still computes a
/// finite thickness instead of panicking on sub-keV midpoints (issue #527).
///
/// Also aligned with the `energy_out.max(0.01)` inline clamps in `compute_layer`
/// and `compute_production_rate` — same "stopped" floor, one canonical name.
pub const MIN_TRACKED_ENERGY_MEV: f64 = 0.01;

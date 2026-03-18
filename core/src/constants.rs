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

/// Activity cutoff fraction relative to peak EOB activity.
pub const ACTIVITY_CUTOFF_FRACTION: f64 = 1e-6;

/// Half-life threshold (s) above which isotopes are "geologically long-lived".
pub const LONG_HALFLIFE_THRESHOLD_S: f64 = 1e10;

/// Max chain size for matrix exponential solver.
pub const MAX_CHAIN_SIZE: usize = 40;

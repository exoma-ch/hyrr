//! hyrr-core: Rust physics core for HYRR.
//!
//! Pure Rust implementation of isotope production rate calculations
//! for stacked target assemblies under charged-particle bombardment.

pub mod bateman;
pub mod chains;
pub mod compute;
pub mod constants;
pub mod data_dir;
pub mod db;
pub mod formula;
pub mod interpolation;
pub mod materials;
pub mod math;
#[cfg(feature = "mcp")]
pub mod mcp;
pub mod matrix_exp;
pub mod production;
pub mod projectile;
pub mod stopping;
pub mod types;

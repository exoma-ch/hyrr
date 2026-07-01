//! hyrr-core: Rust physics core for HYRR.
//!
//! Pure Rust implementation of isotope production rate calculations
//! for stacked target assemblies under charged-particle bombardment.

pub mod bateman;
pub mod chains;
pub mod compute;
pub mod config_url;
pub mod constants;
pub mod data_dir;
#[cfg(not(target_arch = "wasm32"))]
pub mod data_fetch;
pub mod db;
pub mod formula;
pub mod interpolation;
pub mod materials;
pub mod math;
pub mod matrix_exp;
#[cfg(feature = "mcp")]
pub mod mcp;
pub mod neutron;
pub mod production;
pub mod projectile;
pub mod stopping;
pub mod trace_schema;
pub mod types;

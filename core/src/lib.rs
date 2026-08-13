//! hyrr-core: Rust physics core for HYRR.
//!
//! Pure Rust implementation of isotope production rate calculations
//! for stacked target assemblies under charged-particle bombardment.

/// The one version identity for everything HYRR ships (#603).
///
/// ADR 0001 made `hyrr-core` the single source of truth for MCP tool logic,
/// with thin entry points that never reimplement. Version identity was left
/// out of that rule, and each shim ended up reporting its OWN crate version:
/// the published wheel told users `0.1.0` for four releases while every
/// protocol surface correctly said `0.19.0`, because `py-mcp/Cargo.toml` is
/// not in release-please's `extra-files` and nobody noticed (#599).
///
/// So the version is now core's, exactly like the physics is. Entry points
/// read this; they must not read their own `CARGO_PKG_VERSION`, and a
/// guardrail hook enforces that (see `.pre-commit-config.yaml`).
///
/// The remaining `env!("CARGO_PKG_VERSION")` uses inside this crate are all
/// inside `concat!`, which requires a literal and cannot take a const.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod bateman;
pub mod chains;
pub mod compute;
pub mod config_codec;
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
pub mod provenance;
pub mod release_notes;
pub mod stopping;
pub mod trace_schema;
pub mod types;
#[cfg(not(target_arch = "wasm32"))]
pub mod update_check;
pub mod viewer;

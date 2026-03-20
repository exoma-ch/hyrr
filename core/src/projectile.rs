//! Projectile definitions and resolution.

use crate::types::ProjectileType;

/// Resolved projectile definition.
#[derive(Debug, Clone)]
pub struct Projectile {
    pub z: u32,
    pub a: u32,
    pub charge_state: u32,
}

impl Projectile {
    pub fn from_type(pt: &ProjectileType) -> Self {
        match pt {
            ProjectileType::Proton => Self {
                z: 1,
                a: 1,
                charge_state: 1,
            },
            ProjectileType::Deuteron => Self {
                z: 1,
                a: 2,
                charge_state: 1,
            },
            ProjectileType::Tritium => Self {
                z: 1,
                a: 3,
                charge_state: 1,
            },
            ProjectileType::Helion => Self {
                z: 2,
                a: 3,
                charge_state: 2,
            },
            ProjectileType::Alpha => Self {
                z: 2,
                a: 4,
                charge_state: 2,
            },
            ProjectileType::HeavyIon { z, a, .. } => Self {
                z: *z,
                a: *a,
                charge_state: *z, // fully stripped
            },
        }
    }
}

/// Resolve a projectile name to its definition.
pub fn resolve_projectile(name: &str) -> Option<Projectile> {
    ProjectileType::from_str(name).map(|pt| Projectile::from_type(&pt))
}

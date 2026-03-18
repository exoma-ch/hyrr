//! Projectile definitions and resolution.

use crate::types::ProjectileType;

/// Resolved projectile definition.
#[derive(Debug, Clone, Copy)]
pub struct Projectile {
    pub symbol: &'static str,
    pub z: u32,
    pub a: u32,
    pub charge_state: u32,
}

impl Projectile {
    pub fn from_type(pt: ProjectileType) -> Self {
        match pt {
            ProjectileType::Proton => Self {
                symbol: "p",
                z: 1,
                a: 1,
                charge_state: 1,
            },
            ProjectileType::Deuteron => Self {
                symbol: "d",
                z: 1,
                a: 2,
                charge_state: 1,
            },
            ProjectileType::Tritium => Self {
                symbol: "t",
                z: 1,
                a: 3,
                charge_state: 1,
            },
            ProjectileType::Helion => Self {
                symbol: "h",
                z: 2,
                a: 3,
                charge_state: 2,
            },
            ProjectileType::Alpha => Self {
                symbol: "a",
                z: 2,
                a: 4,
                charge_state: 2,
            },
        }
    }
}

/// Resolve a projectile name to its definition.
pub fn resolve_projectile(name: &str) -> Option<Projectile> {
    ProjectileType::from_str(name).map(Projectile::from_type)
}

//! Tests for runtime material registry and the removal of the 5.0 g/cm³
//! fallback density.

#![cfg(feature = "embed-data")]

use hyrr_core::db::EmbeddedDataStore;
use hyrr_core::materials::{resolve_material, MaterialRegistry, RuntimeMaterial};
use std::collections::HashMap;

fn db() -> EmbeddedDataStore {
    EmbeddedDataStore::new("tendl-2023-iso").unwrap()
}

#[test]
fn registry_material_resolves_with_correct_density() {
    let db = db();

    let mut registry: MaterialRegistry = HashMap::new();
    let mut fracs = HashMap::new();
    fracs.insert("Ni".to_string(), 0.60);
    fracs.insert("Cr".to_string(), 0.22);
    fracs.insert("Fe".to_string(), 0.18);
    registry.insert(
        "inconel".to_string(),
        RuntimeMaterial {
            density_g_cm3: 8.19,
            mass_fractions: fracs,
            nist_compound: None,
        },
    );

    let res = resolve_material(&db, "Inconel", None, Some(&registry), None).unwrap();
    assert!(
        (res.density - 8.19).abs() < 1e-6,
        "expected 8.19, got {}",
        res.density
    );
    assert_eq!(res.elements.len(), 3);
}

#[test]
fn registry_overrides_static_catalog() {
    let db = db();

    // Define a custom "havar" with different density
    let mut registry: MaterialRegistry = HashMap::new();
    let mut fracs = HashMap::new();
    fracs.insert("Co".to_string(), 0.50);
    fracs.insert("Cr".to_string(), 0.50);
    registry.insert(
        "havar".to_string(),
        RuntimeMaterial {
            density_g_cm3: 9.99,
            mass_fractions: fracs,
            nist_compound: None,
        },
    );

    let res = resolve_material(&db, "havar", None, Some(&registry), None).unwrap();
    assert!(
        (res.density - 9.99).abs() < 1e-6,
        "session havar should override static catalog, got density={}",
        res.density
    );
    assert_eq!(
        res.elements.len(),
        2,
        "should have 2 elements from session definition"
    );
}

#[test]
fn unknown_compound_returns_error_not_fallback() {
    let db = db();

    let result = resolve_material(&db, "NbSn3", None, None, None);
    assert!(
        result.is_err(),
        "unknown compound should return Err, not a 5.0 g/cm³ fallback"
    );
    let msg = result.unwrap_err();
    assert!(
        msg.contains("No density known"),
        "error should mention missing density, got: {msg}"
    );
}

#[test]
fn known_element_still_resolves() {
    let db = db();
    let res = resolve_material(&db, "Cu", None, None, None).unwrap();
    assert!(
        (res.density - 8.96).abs() < 0.01,
        "Cu should resolve to ~8.96, got {}",
        res.density
    );
}

#[test]
fn known_compound_still_resolves() {
    let db = db();
    let res = resolve_material(&db, "H2O", None, None, None).unwrap();
    assert!(
        (res.density - 1.0).abs() < 0.01,
        "H2O should resolve to ~1.0, got {}",
        res.density
    );
}

#[test]
fn density_override_with_unknown_compound_via_registry() {
    let db = db();

    // Without registry: NbSn should fail
    assert!(resolve_material(&db, "NbSn", None, None, None).is_err());

    // With registry: NbSn should resolve
    let mut registry: MaterialRegistry = HashMap::new();
    let mut fracs = HashMap::new();
    fracs.insert("Nb".to_string(), 0.65);
    fracs.insert("Sn".to_string(), 0.35);
    registry.insert(
        "nbsn".to_string(),
        RuntimeMaterial {
            density_g_cm3: 6.5,
            mass_fractions: fracs,
            nist_compound: None,
        },
    );

    let res = resolve_material(&db, "NbSn", None, Some(&registry), None).unwrap();
    assert!((res.density - 6.5).abs() < 1e-6);
}

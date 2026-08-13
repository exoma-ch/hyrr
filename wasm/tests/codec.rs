//! wasm-bindgen tests for the config-codec binding (#539 increment 3a).
//!
//! Run with `wasm-pack test --node wasm/` (or `--headless --firefox`). These
//! exercise the *real* JsValue marshalling path (`serde-wasm-bindgen`) that the
//! browser hits — encode a `CodecConfig` object → hash, decode the hash back to
//! an object, and prove:
//!   1. the round-trip preserves the full canonical config,
//!   2. a `current_profile` float decodes bit-identically to JS `JSON.parse`
//!      (the `float_roundtrip` gotcha, proven across the actual wasm boundary),
//!   3. a malformed hash surfaces as a thrown JS error — never a wasm panic
//!      (which would poison the store).
#![cfg(target_arch = "wasm32")]

use hyrr_core::config_codec::{
    Beam, CodecConfig, CurrentProfile, CustomMaterial, Item, Layer, SizePolicy,
};
use hyrr_wasm::{decode_config, encode_config};
use serde_wasm_bindgen::Serializer;
use std::collections::BTreeMap;
use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

/// Serialize a Rust value to a JS object exactly as the frontend would build it
/// (plain objects, not JS `Map`s) — the input shape `encode_config` expects.
fn to_js<T: serde::Serialize>(v: &T) -> JsValue {
    v.serialize(&Serializer::json_compatible()).unwrap()
}

fn from_js<T: serde::de::DeserializeOwned>(v: &JsValue) -> T {
    serde_wasm_bindgen::from_value(v.clone()).unwrap()
}

/// Read the `hash` field off an `EncodeOutcome` JS object (it's Serialize-only
/// on the Rust side, so we pull the one field we need via Reflect).
fn outcome_hash(outcome: &JsValue) -> String {
    js_sys::Reflect::get(outcome, &JsValue::from_str("hash"))
        .unwrap()
        .as_string()
        .unwrap()
}

fn sample_config() -> CodecConfig {
    let mut fractions = BTreeMap::new();
    fractions.insert("Ni".to_string(), 0.58);
    fractions.insert("Cr".to_string(), 0.22);
    CodecConfig {
        beam: Beam {
            projectile: "p".to_string(),
            energy_mev: 28.0,
            current_ma: 0.2,
        },
        items: vec![
            Item::Layer(Layer {
                material: "poc-inconel".to_string(),
                thickness_cm: Some(0.15),
                custom: Some(CustomMaterial {
                    density_g_cm3: 8.44,
                    mass_fractions: Some(fractions),
                    formula: Some("Ni58Cr22".to_string()),
                    enrichment: None,
                    nist_compound: None,
                }),
                ..Default::default()
            }),
            Item::Layer(Layer {
                material: "Cu".to_string(),
                energy_out_mev: Some(0.0),
                ..Default::default()
            }),
        ],
        irradiation_s: 604800.0,
        cooling_s: 86400.0,
        neutron_flux: None,
        secondary_neutron: false,
        current_profile: None,
    }
}

#[wasm_bindgen_test]
fn wasm_encode_decode_round_trips() {
    let cfg = sample_config();
    let outcome = encode_config(to_js(&cfg), to_js(&SizePolicy::Url { budget_bytes: 2000 }))
        .expect("encode should succeed");
    let hash = outcome_hash(&outcome);
    assert!(hash.starts_with("#config=1:"), "hash was {hash}");

    let decoded_js = decode_config(&hash).expect("decode should succeed");
    let decoded: CodecConfig = from_js(&decoded_js);
    assert_eq!(
        decoded, cfg,
        "full canonical config must round-trip via wasm"
    );
}

#[wasm_bindgen_test]
fn wasm_decode_float_equals_js_json_parse() {
    // The inc1 gotcha, proven across the real wasm boundary: a `current_profile`
    // value must decode to the exact same f64 JS `JSON.parse` yields.
    let v = 0.05_f64 + 3e-4;
    let mut cfg = sample_config();
    cfg.current_profile = Some(CurrentProfile {
        times_s: vec![0.0, 60.0],
        currents_ma: vec![v, 0.1],
    });

    let outcome = encode_config(to_js(&cfg), to_js(&SizePolicy::File)).expect("encode");
    let hash = outcome_hash(&outcome);
    let decoded: CodecConfig = from_js(&decode_config(&hash).expect("decode"));
    let decoded_v = decoded.current_profile.expect("profile").currents_ma[0];

    // What JS would parse from the shortest (ryu) decimal serde_json emits.
    let ryu = serde_json::to_string(&v).unwrap();
    let js_v = js_sys::JSON::parse(&ryu).unwrap().as_f64().unwrap();

    assert_eq!(
        decoded_v.to_bits(),
        v.to_bits(),
        "wasm decode drifted from the original f64 (float_roundtrip inactive?)"
    );
    assert_eq!(
        decoded_v.to_bits(),
        js_v.to_bits(),
        "wasm decode must equal JS JSON.parse of the same literal"
    );
}

#[wasm_bindgen_test]
fn wasm_decode_bad_hash_throws_not_panics() {
    // Not a v2 hash → a thrown JS value carrying a clear message, not a panic.
    let err = decode_config("totally-not-a-config-hash").unwrap_err();
    let msg = err
        .as_string()
        .expect("error should carry a string message");
    assert!(msg.contains("decode failed"), "message was {msg}");

    // Garbage after the v2 prefix (bad base64 / not inflatable) also errors
    // cleanly rather than poisoning the store with a panic.
    let err2 = decode_config("#config=1:!!!!not-base64!!!!").unwrap_err();
    assert!(
        err2.as_string().unwrap().contains("decode failed"),
        "expected a clean decode error"
    );
}

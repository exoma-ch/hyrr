//! Config codec — the **B′ (codec-only)** proof-of-concept for issue #539.
//!
//! Today HYRR has three forked config serializers (`.hyrr.json`, the `#config=`
//! share URL, and this Rust MCP-link encoder in `config_url.rs`). #531 is that
//! the Rust encoder silently drops the inline custom-material composition that
//! the TS decoder (`frontend/src/lib/config-url-v2.ts`, `expandLayer`) already
//! knows how to read (`cl.x`). This module proves the fix: a Rust codec that
//!
//!  1. **emits** the enriched compact payload — including the `x`
//!     InlineComposition object for custom alloys — so the existing TS decoder
//!     round-trips it with **zero TS changes**, and
//!  2. **decodes** a v2 hash back to a canonical config (net-new: `config_url.rs`
//!     is encode-only today), with the security caps the #539 review panel
//!     required (compressed-input cap, streaming decompression cap, item/float/
//!     map bounds, `deny_unknown_fields` on the security-sensitive
//!     `InlineComposition`).
//!
//! Wire format (unchanged, must match `config-url-v2.ts` byte-for-byte
//! semantics): `#config=1:<base64url(rawDEFLATE(compact-json))>`.
//!
//! Scope discipline (per the panel's "codec-only B′" verdict): this is the
//! codec and its canonical type only — NOT the full domain-model migration, no
//! ts-rs, no WASM encode path, no session-io rewrite.

use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::{Read, Write};

// ─── Security caps (panel-mandated) ──────────────────────────────────────────

/// Reject a `#config=` payload whose *compressed* bytes exceed this. A share URL
/// is never legitimately this large; anything bigger is abuse or corruption.
const MAX_COMPRESSED_BYTES: usize = 8 * 1024;
/// Cap the *decompressed* stream — the decompression-bomb guard. Enforced by a
/// streaming `take(MAX + 1)`, so we never allocate more than this even for a
/// hostile input that inflates to gigabytes.
const MAX_DECOMPRESSED_BYTES: u64 = 1024 * 1024; // 1 MiB
/// Max top-level items (layers + groups). Mirrors the TS `MAX_URL_ITEMS`.
const MAX_ITEMS: usize = 30;
/// Bound the size of any embedded map (mass fractions, enrichment element set).
const MAX_MAP_ENTRIES: usize = 512;
/// Bound a formula / free-text string length.
const MAX_FORMULA_LEN: usize = 256;
/// Bound the length of a current-profile sample array.
const MAX_PROFILE_SAMPLES: usize = 100_000;

const V2_PREFIX: &str = "1:";

// ─── Errors ──────────────────────────────────────────────────────────────────

/// Codec failure modes. Decode errors are deliberately structured so a caller
/// (MCP tool, future WASM binding) can distinguish "malformed link" from "abuse".
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CodecError {
    #[error("not a v2 config hash (missing `1:` prefix)")]
    NotV2,
    #[error("base64url decode failed")]
    Base64,
    #[error("compressed payload too large: {0} bytes > {MAX_COMPRESSED_BYTES}")]
    CompressedTooLarge(usize),
    #[error("decompressed payload exceeds {MAX_DECOMPRESSED_BYTES} bytes (decompression bomb?)")]
    DecompressedTooLarge,
    #[error("inflate failed")]
    Inflate,
    #[error("compact JSON parse failed: {0}")]
    Json(String),
    #[error("too many items: {0} > {MAX_ITEMS}")]
    TooManyItems(usize),
    #[error("map too large: {0} entries > {MAX_MAP_ENTRIES}")]
    MapTooLarge(usize),
    #[error("string too long: {0} > {MAX_FORMULA_LEN}")]
    StringTooLong(usize),
    #[error("profile too long: {0} samples > {MAX_PROFILE_SAMPLES}")]
    ProfileTooLong(usize),
    #[error("non-finite float in `{0}`")]
    NonFinite(&'static str),
}

// ─── Canonical domain model (Rust-owned, per B′) ─────────────────────────────

/// The canonical simulation config. In full B′ this becomes the single owner of
/// the config type (shared by file / URL / MCP via WASM). Here it is the
/// round-trip type for the codec.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub beam: Beam,
    pub items: Vec<Item>,
    pub irradiation_s: f64,
    pub cooling_s: f64,
    /// Neutron spectrum (ADR-0003) — opaque passthrough for the PoC.
    pub neutron_flux: Option<Value>,
    pub secondary_neutron: bool,
    /// Time-varying beam current. Large; subject to `SizePolicy` for URLs.
    pub current_profile: Option<CurrentProfile>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Beam {
    pub projectile: String,
    pub energy_mev: f64,
    pub current_ma: f64,
}

/// A top-level item is either a single layer or an authoring group.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Layer(Layer),
    Group(Group),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Layer {
    pub material: String,
    pub thickness_cm: Option<f64>,
    pub areal_density_g_cm2: Option<f64>,
    pub energy_out_mev: Option<f64>,
    pub enrichment: Option<Value>,
    pub is_monitor: bool,
    pub density_g_cm3: Option<f64>,
    /// The #531 payload: an embedded custom-material definition (→ compact `x`).
    pub custom: Option<CustomMaterial>,
}

/// Inline custom-material definition (a custom alloy that must travel with the
/// link). This is what #531 drops and what makes the loss *silent AND
/// physics-altering* (density changes stopping power → wrong yield).
#[derive(Debug, Clone, PartialEq)]
pub struct CustomMaterial {
    pub density_g_cm3: f64,
    pub mass_fractions: Option<BTreeMap<String, f64>>,
    pub formula: Option<String>,
    pub enrichment: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub layers: Vec<Layer>,
    pub mode: Option<String>,
    pub count: Option<f64>,
    pub energy_threshold: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CurrentProfile {
    pub times_s: Vec<f64>,
    pub currents_ma: Vec<f64>,
}

// ─── Size policy ─────────────────────────────────────────────────────────────

/// Where the encoded config is going, which decides the `currentProfile` budget.
///
/// * `File` — a `.hyrr.json` download: embed everything, no budget.
/// * `Url { profile_budget_bytes }` — a share/MCP link: transport/CDN/QR limits
///   bind ~2 KB for the whole hash. If embedding `currentProfile` would blow the
///   budget, we DROP it and report it in `EncodeOutcome.dropped` — never
///   silently (the panel's non-negotiable).
#[derive(Debug, Clone, Copy)]
pub enum SizePolicy {
    File,
    Url { profile_budget_bytes: usize },
}

/// The result of an encode: the hash plus a structured list of anything the
/// size policy forced us to drop, so the caller can surface a visible warning
/// and offer a lossless fallback.
#[derive(Debug, Clone, PartialEq)]
pub struct EncodeOutcome {
    pub hash: String,
    pub dropped: Vec<&'static str>,
}

// ─── Compact wire structs (serde-typed; compact keys) ────────────────────────
//
// These are the on-the-wire shape. serde field renames give us the compact keys
// that `config-url-v2.ts` reads; `skip_serializing_if = Option::is_none` keeps
// absent fields out of the JSON exactly like the TS `compactLayer`.

#[derive(Serialize, Deserialize, Debug)]
struct CompactConfig {
    b: CompactBeam,
    l: Vec<CompactItem>,
    i: f64,
    c: f64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    nf: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    sn: Option<bool>,
    /// currentProfile — NEW compact key. Emitted only under `SizePolicy::File`
    /// (or a URL that fits budget). NOTE: the TS decoder does not read `cp` yet
    /// (it ignores unknown top-level keys), so profile round-trips only within
    /// the Rust/file path for now — full B′ TODO: teach the TS decoder `cp`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    cp: Option<CompactProfile>,
}

#[derive(Serialize, Deserialize, Debug)]
struct CompactBeam {
    p: String,
    e: f64,
    c: f64,
}

/// Untagged: a group carries `g: true`; a layer never does. Group is tried
/// first, so a bare layer (no `g`) falls through to `Layer`.
#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
enum CompactItem {
    Group(CompactGroup),
    Layer(CompactLayer),
}

#[derive(Serialize, Deserialize, Debug)]
struct CompactGroup {
    g: bool,
    l: Vec<CompactLayer>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    d: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    k: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    h: Option<f64>,
}

#[derive(Serialize, Deserialize, Debug)]
struct CompactLayer {
    m: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    t: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    a: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    o: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    n: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    f: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    d: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    x: Option<CompactInlineComposition>,
}

/// The security-sensitive one — the only struct with `deny_unknown_fields`, so a
/// malformed/hostile share link can't smuggle extra keys into the custom-mat
/// definition (panel requirement).
#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct CompactInlineComposition {
    d: f64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    e: Option<BTreeMap<String, f64>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    f: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    n: Option<Value>,
}

#[derive(Serialize, Deserialize, Debug)]
struct CompactProfile {
    t: Vec<f64>,
    c: Vec<f64>,
}

// ─── Canonical → compact ─────────────────────────────────────────────────────

impl From<&Layer> for CompactLayer {
    fn from(l: &Layer) -> Self {
        CompactLayer {
            m: l.material.clone(),
            t: l.thickness_cm,
            a: l.areal_density_g_cm2,
            o: l.energy_out_mev,
            n: l.enrichment.clone(),
            f: if l.is_monitor { Some(true) } else { None },
            d: l.density_g_cm3,
            x: l.custom.as_ref().map(|cm| CompactInlineComposition {
                d: cm.density_g_cm3,
                e: cm.mass_fractions.clone(),
                f: cm.formula.clone(),
                n: cm.enrichment.clone(),
            }),
        }
    }
}

impl CompactConfig {
    fn from_config(cfg: &Config, include_profile: bool) -> Self {
        let l = cfg
            .items
            .iter()
            .map(|item| match item {
                Item::Layer(layer) => CompactItem::Layer(layer.into()),
                Item::Group(g) => CompactItem::Group(CompactGroup {
                    g: true,
                    l: g.layers.iter().map(CompactLayer::from).collect(),
                    d: g.mode.clone(),
                    k: g.count,
                    h: g.energy_threshold,
                }),
            })
            .collect();
        CompactConfig {
            b: CompactBeam {
                p: cfg.beam.projectile.clone(),
                e: cfg.beam.energy_mev,
                c: cfg.beam.current_ma,
            },
            l,
            i: cfg.irradiation_s,
            c: cfg.cooling_s,
            nf: cfg.neutron_flux.clone(),
            sn: if cfg.secondary_neutron {
                Some(true)
            } else {
                None
            },
            cp: if include_profile {
                cfg.current_profile.as_ref().map(|p| CompactProfile {
                    t: p.times_s.clone(),
                    c: p.currents_ma.clone(),
                })
            } else {
                None
            },
        }
    }
}

// ─── Encode ──────────────────────────────────────────────────────────────────

/// Encode a canonical config to a `#config=1:…` hash under the given size policy.
///
/// Under `SizePolicy::Url`, the `currentProfile` is included only if the whole
/// hash still fits the budget; otherwise it is dropped and reported in
/// `EncodeOutcome.dropped`.
pub fn encode(cfg: &Config, policy: SizePolicy) -> EncodeOutcome {
    match policy {
        SizePolicy::File => {
            let hash = encode_hash(&CompactConfig::from_config(cfg, true));
            EncodeOutcome {
                hash,
                dropped: vec![],
            }
        }
        SizePolicy::Url {
            profile_budget_bytes,
        } => {
            let has_profile = cfg.current_profile.is_some();
            // Try to fit the profile; measure the full hash.
            if has_profile {
                let with = encode_hash(&CompactConfig::from_config(cfg, true));
                if with.len() <= profile_budget_bytes {
                    return EncodeOutcome {
                        hash: with,
                        dropped: vec![],
                    };
                }
            }
            // No profile, or it blew the budget → drop it, warn.
            let hash = encode_hash(&CompactConfig::from_config(cfg, false));
            EncodeOutcome {
                hash,
                dropped: if has_profile {
                    vec!["currentProfile"]
                } else {
                    vec![]
                },
            }
        }
    }
}

fn encode_hash(compact: &CompactConfig) -> String {
    let json = serde_json::to_vec(compact).expect("compact config is always serializable");
    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&json).expect("deflate write to Vec");
    let compressed = encoder.finish().expect("deflate finish");
    format!("#config={}{}", V2_PREFIX, base64url_encode(&compressed))
}

// ─── Decode ──────────────────────────────────────────────────────────────────

/// Decode a v2 config hash back to the canonical `Config`, applying every
/// security cap. Accepts a full URL, a bare `#config=1:…`/`config=1:…` hash, or
/// the raw `1:<payload>`.
pub fn decode(input: &str) -> Result<Config, CodecError> {
    let payload = extract_v2_payload(input).ok_or(CodecError::NotV2)?;

    let compressed = base64url_decode(payload).ok_or(CodecError::Base64)?;
    if compressed.len() > MAX_COMPRESSED_BYTES {
        return Err(CodecError::CompressedTooLarge(compressed.len()));
    }

    // Streaming decompression cap: never read more than MAX + 1 bytes.
    let mut decoder = DeflateDecoder::new(&compressed[..]).take(MAX_DECOMPRESSED_BYTES + 1);
    let mut json = Vec::new();
    decoder
        .read_to_end(&mut json)
        .map_err(|_| CodecError::Inflate)?;
    if json.len() as u64 > MAX_DECOMPRESSED_BYTES {
        return Err(CodecError::DecompressedTooLarge);
    }

    let compact: CompactConfig =
        serde_json::from_slice(&json).map_err(|e| CodecError::Json(e.to_string()))?;

    compact_to_config(compact)
}

/// Pull the base64url payload out of any accepted input shape.
fn extract_v2_payload(input: &str) -> Option<&str> {
    let hash = match input.find('#') {
        Some(i) => &input[i + 1..],
        None => input,
    };
    let after = hash.strip_prefix("config=").unwrap_or(hash);
    after.strip_prefix(V2_PREFIX)
}

fn compact_to_config(c: CompactConfig) -> Result<Config, CodecError> {
    if c.l.len() > MAX_ITEMS {
        return Err(CodecError::TooManyItems(c.l.len()));
    }
    check_finite(c.b.e, "beam.energy")?;
    check_finite(c.b.c, "beam.current")?;
    check_finite(c.i, "irradiation_s")?;
    check_finite(c.c, "cooling_s")?;

    let items =
        c.l.into_iter()
            .map(|item| match item {
                CompactItem::Layer(cl) => Ok(Item::Layer(compact_to_layer(cl)?)),
                CompactItem::Group(cg) => {
                    if cg.layers_len() > MAX_ITEMS {
                        return Err(CodecError::TooManyItems(cg.layers_len()));
                    }
                    if let Some(k) = cg.k {
                        check_finite(k, "group.count")?;
                    }
                    if let Some(h) = cg.h {
                        check_finite(h, "group.energyThreshold")?;
                    }
                    let layers =
                        cg.l.into_iter()
                            .map(compact_to_layer)
                            .collect::<Result<Vec<_>, _>>()?;
                    Ok(Item::Group(Group {
                        layers,
                        mode: cg.d,
                        count: cg.k,
                        energy_threshold: cg.h,
                    }))
                }
            })
            .collect::<Result<Vec<_>, _>>()?;

    let current_profile = match c.cp {
        Some(p) => {
            if p.t.len() > MAX_PROFILE_SAMPLES || p.c.len() > MAX_PROFILE_SAMPLES {
                return Err(CodecError::ProfileTooLong(p.t.len().max(p.c.len())));
            }
            for &v in p.t.iter().chain(p.c.iter()) {
                check_finite(v, "currentProfile")?;
            }
            Some(CurrentProfile {
                times_s: p.t,
                currents_ma: p.c,
            })
        }
        None => None,
    };

    Ok(Config {
        beam: Beam {
            projectile: c.b.p,
            energy_mev: c.b.e,
            current_ma: c.b.c,
        },
        items,
        irradiation_s: c.i,
        cooling_s: c.c,
        neutron_flux: c.nf,
        secondary_neutron: c.sn.unwrap_or(false),
        current_profile,
    })
}

impl CompactGroup {
    fn layers_len(&self) -> usize {
        self.l.len()
    }
}

fn compact_to_layer(cl: CompactLayer) -> Result<Layer, CodecError> {
    for (v, name) in [
        (cl.t, "thickness_cm"),
        (cl.a, "areal_density"),
        (cl.o, "energy_out"),
        (cl.d, "density"),
    ] {
        if let Some(x) = v {
            check_finite(x, name)?;
        }
    }
    let custom = match cl.x {
        Some(x) => Some(compact_to_custom(x)?),
        None => None,
    };
    Ok(Layer {
        material: cl.m,
        thickness_cm: cl.t,
        areal_density_g_cm2: cl.a,
        energy_out_mev: cl.o,
        enrichment: cl.n,
        is_monitor: cl.f.unwrap_or(false),
        density_g_cm3: cl.d,
        custom,
    })
}

fn compact_to_custom(x: CompactInlineComposition) -> Result<CustomMaterial, CodecError> {
    check_finite(x.d, "custom.density")?;
    if let Some(e) = &x.e {
        if e.len() > MAX_MAP_ENTRIES {
            return Err(CodecError::MapTooLarge(e.len()));
        }
        for &v in e.values() {
            check_finite(v, "custom.massFractions")?;
        }
    }
    if let Some(f) = &x.f {
        if f.len() > MAX_FORMULA_LEN {
            return Err(CodecError::StringTooLong(f.len()));
        }
    }
    Ok(CustomMaterial {
        density_g_cm3: x.d,
        mass_fractions: x.e,
        formula: x.f,
        enrichment: x.n,
    })
}

fn check_finite(v: f64, name: &'static str) -> Result<(), CodecError> {
    if v.is_finite() {
        Ok(())
    } else {
        Err(CodecError::NonFinite(name))
    }
}

// ─── base64url (RFC 4648 §5, no padding — matches config_url.rs / TS) ─────────

/// Base64url encode: `+`→`-`, `/`→`_`, no padding.
fn base64url_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        out.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(CHARS[(triple & 0x3F) as usize] as char);
        }
    }
    out
}

/// Base64url decode, tolerant of missing padding (TS strips `=`) and `+/`↔`-_`.
fn base64url_decode(input: &str) -> Option<Vec<u8>> {
    fn val(b: u8) -> Option<u32> {
        match b {
            b'A'..=b'Z' => Some((b - b'A') as u32),
            b'a'..=b'z' => Some((b - b'a' + 26) as u32),
            b'0'..=b'9' => Some((b - b'0' + 52) as u32),
            b'+' | b'-' => Some(62),
            b'/' | b'_' => Some(63),
            _ => None,
        }
    }
    let bytes: Vec<u8> = input
        .bytes()
        .filter(|&b| b != b'=' && !b.is_ascii_whitespace())
        .collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        if chunk.len() == 1 {
            return None; // invalid base64 length
        }
        let mut acc = 0u32;
        for &b in chunk {
            acc = (acc << 6) | val(b)?;
        }
        // Left-align to a full 24-bit triple.
        acc <<= 6 * (4 - chunk.len());
        out.push((acc >> 16) as u8);
        if chunk.len() > 2 {
            out.push((acc >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(acc as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The PoC alloy — a made-up "poc-inconel" with an explicit density and mass
    /// fractions. This is exactly the custom-material definition #531 silently
    /// drops on the MCP link path today.
    fn poc_inconel_config() -> Config {
        let mut fractions = BTreeMap::new();
        fractions.insert("Ni".to_string(), 0.58);
        fractions.insert("Cr".to_string(), 0.22);
        fractions.insert("Fe".to_string(), 0.05);
        fractions.insert("Mo".to_string(), 0.09);
        fractions.insert("Nb".to_string(), 0.036);
        fractions.insert("Ti".to_string(), 0.004);
        Config {
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
                        formula: Some("Ni58Cr22Fe5Mo9Nb3.6Ti0.4".to_string()),
                        enrichment: None,
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

    // (a) Round-trip a config-with-custom-alloy encode → decode; the alloy
    //     density + mass fractions must survive.
    #[test]
    fn roundtrip_custom_alloy_survives() {
        let cfg = poc_inconel_config();
        let outcome = encode(
            &cfg,
            SizePolicy::Url {
                profile_budget_bytes: 2000,
            },
        );
        assert!(outcome.dropped.is_empty());
        let decoded = decode(&outcome.hash).expect("decode");
        assert_eq!(decoded, cfg, "full canonical config must round-trip");

        // Explicit alloy assertions (the #531 payload).
        let Item::Layer(layer) = &decoded.items[0] else {
            panic!("expected layer");
        };
        let cm = layer.custom.as_ref().expect("custom material survives");
        assert_eq!(cm.density_g_cm3, 8.44);
        let mf = cm.mass_fractions.as_ref().unwrap();
        assert_eq!(mf.get("Ni"), Some(&0.58));
        assert_eq!(mf.get("Cr"), Some(&0.22));
        assert_eq!(cm.formula.as_deref(), Some("Ni58Cr22Fe5Mo9Nb3.6Ti0.4"));
    }

    /// Deterministic pseudo-random stream (SplitMix64) — for building
    /// *incompressible* test payloads without a `rand` dependency.
    fn lcg_stream(seed: u64, n: usize) -> Vec<u64> {
        let mut x = seed;
        (0..n)
            .map(|_| {
                x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
                let mut z = x;
                z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
                z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
                z ^ (z >> 31)
            })
            .collect()
    }

    // (b) URL-policy with a big currentProfile → dropped WITH a warning, and the
    //     hash stays under budget. Also measures a *realistic* profile's true
    //     byte cost, which turns out to fit the 2 KB budget (GOTCHA for #539).
    #[test]
    fn url_policy_drops_oversized_profile_with_warning() {
        const BUDGET: usize = 2000;

        // ── Realistic smooth ramp: 200 samples. DEFLATE crushes it — it FITS. ──
        let mut smooth = poc_inconel_config();
        let n = 200;
        smooth.current_profile = Some(CurrentProfile {
            times_s: (0..n).map(|i| i as f64 * 3600.0).collect(),
            currents_ma: (0..n).map(|i| 0.1 + (i as f64) * 1e-4).collect(),
        });
        let file = encode(&smooth, SizePolicy::File);
        assert!(file.dropped.is_empty());
        assert_eq!(
            decode(&file.hash)
                .unwrap()
                .current_profile
                .unwrap()
                .times_s
                .len(),
            n
        );
        // MEASURED: a realistic 200-sample smooth ramp compresses to ~1544 bytes
        // total — well under the 2 KB URL budget. (GOTCHA for #539: the "profile
        // is too large for a URL" assumption is false for realistic smooth ramps.)
        assert!(
            file.hash.len() < 2000,
            "realistic smooth profile hash was {} bytes",
            file.hash.len()
        );
        // Under URL policy this realistic profile actually fits → KEPT, no warning.
        let url_smooth = encode(
            &smooth,
            SizePolicy::Url {
                profile_budget_bytes: BUDGET,
            },
        );
        assert!(
            url_smooth.dropped.is_empty(),
            "a realistic smooth profile fits the budget and is kept"
        );
        assert!(decode(&url_smooth.hash).unwrap().current_profile.is_some());
        assert!(url_smooth.hash.len() <= BUDGET);

        // ── Large irregular profile: incompressible, clearly over budget → DROP. ──
        let mut big = poc_inconel_config();
        let m = 1500;
        let noise = lcg_stream(42, 2 * m);
        big.current_profile = Some(CurrentProfile {
            times_s: (0..m).map(|i| i as f64 * 60.0).collect(),
            currents_ma: noise[..m]
                .iter()
                .map(|&u| (u % 1_000_000) as f64 / 997.0)
                .collect(),
        });
        let file_big = encode(&big, SizePolicy::File);
        assert!(file_big.dropped.is_empty());
        // MEASURED: this large irregular profile inflates the File hash to
        // ~17 KB — far over any URL budget, so the URL path must drop it.
        assert!(
            file_big.hash.len() > BUDGET,
            "large irregular profile hash was {} bytes",
            file_big.hash.len()
        );

        let url = encode(
            &big,
            SizePolicy::Url {
                profile_budget_bytes: BUDGET,
            },
        );
        // The money assertion: dropped, WITH a structured warning, hash < budget.
        assert_eq!(url.dropped, vec!["currentProfile"]);
        assert!(
            url.hash.len() <= BUDGET,
            "URL hash {} must fit budget {} after dropping the profile",
            url.hash.len(),
            BUDGET
        );
        assert!(decode(&url.hash).unwrap().current_profile.is_none());
    }

    // (c) Decompression-bomb inputs are rejected — both caps proven.
    #[test]
    fn decompression_bomb_rejected() {
        // Cap #1 (compressed input): ~16 KB of incompressible pseudo-random bytes
        // barely shrinks under DEFLATE, so it trips the 8 KB compressed-input gate
        // before we ever inflate it.
        let noise: Vec<u8> = lcg_stream(7, 4096)
            .iter()
            .flat_map(|w| w.to_le_bytes())
            .collect(); // 32 KiB of high-entropy bytes
        let mut enc = DeflateEncoder::new(Vec::new(), Compression::default());
        enc.write_all(&noise).unwrap();
        let compressed = enc.finish().unwrap();
        assert!(
            compressed.len() > MAX_COMPRESSED_BYTES,
            "incompressible input must exceed the compressed cap ({} bytes)",
            compressed.len()
        );
        let hash = format!("#config=1:{}", base64url_encode(&compressed));
        assert!(
            matches!(
                decode(&hash).unwrap_err(),
                CodecError::CompressedTooLarge(_)
            ),
            "expected CompressedTooLarge"
        );

        // Cap #2 (decompressed stream): 4 MiB of zeros compresses to a few KB
        // (< 8 KB input cap) but inflates past the 1 MiB streaming take() → caught.
        let bomb = vec![0u8; 4 * 1024 * 1024];
        let mut enc2 = DeflateEncoder::new(Vec::new(), Compression::default());
        enc2.write_all(&bomb).unwrap();
        let compressed2 = enc2.finish().unwrap();
        assert!(
            compressed2.len() <= MAX_COMPRESSED_BYTES,
            "the zip-bomb's compressed form ({} bytes) is under the input cap, so \
             the decompressed cap is what must fire",
            compressed2.len()
        );
        let hash2 = format!("#config=1:{}", base64url_encode(&compressed2));
        assert_eq!(
            decode(&hash2).unwrap_err(),
            CodecError::DecompressedTooLarge
        );
    }

    // Extra hardening coverage: deny_unknown_fields on InlineComposition.
    #[test]
    fn unknown_field_in_inline_composition_rejected() {
        // Build a compact JSON with a rogue key inside `x`.
        let compact = json!({
            "b": { "p": "p", "e": 28.0, "c": 0.2 },
            "l": [ { "m": "x", "x": { "d": 8.0, "evil": 1 } } ],
            "i": 1.0, "c": 1.0
        });
        let json_bytes = serde_json::to_vec(&compact).unwrap();
        let mut enc = DeflateEncoder::new(Vec::new(), Compression::default());
        enc.write_all(&json_bytes).unwrap();
        let hash = format!("#config=1:{}", base64url_encode(&enc.finish().unwrap()));
        assert!(matches!(decode(&hash).unwrap_err(), CodecError::Json(_)));
    }

    #[test]
    fn out_of_range_float_rejected_at_parse() {
        // GOTCHA: serde_json rejects an out-of-range literal like `1e999` at PARSE
        // time (a `Json` error) — it never reaches our `is_finite` guard as an
        // Infinity, because JSON has no Infinity/NaN and overflow is a parse error.
        // So the codec rejects non-finite input either way; the `is_finite` guard
        // below is defense-in-depth (matters if we ever route through `f32`/`Value`).
        let raw = r#"{"b":{"p":"p","e":1e999,"c":0.2},"l":[],"i":1.0,"c":1.0}"#;
        let mut enc = DeflateEncoder::new(Vec::new(), Compression::default());
        enc.write_all(raw.as_bytes()).unwrap();
        let hash = format!("#config=1:{}", base64url_encode(&enc.finish().unwrap()));
        assert!(matches!(decode(&hash).unwrap_err(), CodecError::Json(_)));
    }

    #[test]
    fn finite_guard_rejects_infinity_and_nan() {
        // Direct coverage of the panel-required `is_finite()` validation.
        assert!(check_finite(f64::INFINITY, "x").is_err());
        assert!(check_finite(f64::NEG_INFINITY, "x").is_err());
        assert!(check_finite(f64::NAN, "x").is_err());
        assert!(check_finite(0.0, "x").is_ok());
        assert!(check_finite(-8.44, "x").is_ok());
    }

    // (d) Write the cross-language fixture: a config-with-alloy encoded to a hash,
    //     committed for the vitest cross-lang test to decode with the REAL TS
    //     decoder. Encoded under Url policy — the actual #531 MCP-link scenario.
    #[test]
    fn write_cross_lang_fixture() {
        let cfg = poc_inconel_config();
        let outcome = encode(
            &cfg,
            SizePolicy::Url {
                profile_budget_bytes: 2000,
            },
        );
        assert!(outcome.hash.starts_with("#config=1:"));

        let dir = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../frontend/src/lib/__fixtures__"
        );
        std::fs::create_dir_all(dir).expect("create fixture dir");
        let path = format!("{dir}/poc-rust-encoded.txt");
        // Trailing newline so the committed fixture is stable under the repo's
        // end-of-file-fixer hook. The TS reader `.trim()`s it.
        std::fs::write(&path, format!("{}\n", outcome.hash)).expect("write fixture");
    }

    #[test]
    fn base64url_roundtrip_tolerates_missing_padding() {
        for data in [&b"a"[..], b"ab", b"abc", b"abcd", b"\x00\xff\x10"] {
            let enc = base64url_encode(data);
            assert!(!enc.contains('='));
            assert_eq!(base64url_decode(&enc).unwrap(), data);
        }
    }

    #[test]
    fn group_survives_roundtrip() {
        let cfg = Config {
            beam: Beam {
                projectile: "p".to_string(),
                energy_mev: 18.0,
                current_ma: 0.15,
            },
            items: vec![
                Item::Layer(Layer {
                    material: "Al".to_string(),
                    thickness_cm: Some(0.05),
                    ..Default::default()
                }),
                Item::Group(Group {
                    layers: vec![
                        Layer {
                            material: "Cu".to_string(),
                            thickness_cm: Some(0.01),
                            ..Default::default()
                        },
                        Layer {
                            material: "Zn".to_string(),
                            thickness_cm: Some(0.01),
                            ..Default::default()
                        },
                    ],
                    mode: Some("count".to_string()),
                    count: Some(5.0),
                    energy_threshold: None,
                }),
            ],
            irradiation_s: 86400.0,
            cooling_s: 86400.0,
            neutron_flux: None,
            secondary_neutron: false,
            current_profile: None,
        };
        let outcome = encode(&cfg, SizePolicy::File);
        assert_eq!(decode(&outcome.hash).unwrap(), cfg);
    }
}

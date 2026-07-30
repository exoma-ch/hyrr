//! Config codec — the production **B′ (codec-only)** codec for issue #539.
//!
//! Today HYRR has three forked config serializers (`.hyrr.json`, the `#config=`
//! share URL, and the Rust MCP-link encoder). #531 was that the Rust encoder
//! silently dropped the inline custom-material composition that the TS decoder
//! (`frontend/src/lib/config-url-v2.ts`, `expandLayer`) already knows how to
//! read (`cl.x`), plus per-layer density overrides and the `secondary_neutron`
//! toggle. This module is the single canonical codec that
//!
//!  1. **encodes** a canonical [`CodecConfig`] to the enriched compact payload —
//!     including the `x` InlineComposition object for custom alloys, per-layer
//!     density overrides (`d`), the neutron spectrum (`nf`), the
//!     `secondary_neutron` toggle (`sn`), and (under [`SizePolicy`]) the
//!     current profile (`cp`) — so the existing TS decoder round-trips it, and
//!  2. **decodes** a v2 hash back to a [`CodecConfig`], with the security caps
//!     the #539 review panel required (compressed-input cap, streaming
//!     decompression cap, item/float/map bounds, `deny_unknown_fields` on the
//!     security-sensitive `InlineComposition`).
//!
//! The MCP/URL link path (`config_url::share_url`) is a thin adapter that maps
//! MCP simulate args → [`CodecConfig`] and calls [`encode`]; this is what closes
//! #531. The canonical [`CodecConfig`] type is Rust-owned, per the B′ verdict.
//!
//! Wire format (unchanged, must match `config-url-v2.ts` byte-for-byte
//! semantics): `#config=1:<base64url(rawDEFLATE(compact-json))>`.
//!
//! Scope discipline (increment 1 of the #539 staging plan): this is the codec,
//! its canonical type, and the MCP/URL rewire only — NOT the `.hyrr.json` file
//! self-containment (increment 2), the WASM encode path, ts-rs type-gen, or the
//! `config-url-v2.ts` collapse (increment 3).

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
/// Max top-level items (layers + groups) the decoder will accept — mirrors the
/// TS `MAX_URL_ITEMS` in `config-url-v2.ts`. The encoder uses it too, so a stack
/// too large to *decode* is never emitted as a "view in browser" link the
/// frontend would silently refuse to load (the panel's non-negotiable).
pub const MAX_ITEMS: usize = 30;
/// Bound the size of any embedded map (mass fractions, enrichment element set).
const MAX_MAP_ENTRIES: usize = 512;
/// Bound a formula / free-text string length.
const MAX_FORMULA_LEN: usize = 256;
/// Bound the length of a current-profile sample array.
const MAX_PROFILE_SAMPLES: usize = 100_000;

/// Default whole-hash byte budget for a share/MCP URL. Transport/CDN/QR limits
/// bind well before browsers do; the panel fixed ~2 KB as the safe budget.
pub const DEFAULT_URL_BUDGET_BYTES: usize = 2000;

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

/// The canonical simulation config — the single Rust-owned round-trip type for
/// the codec. In the #539 endgame (increment 3) this becomes the sole owner of
/// the config type, shared by file / URL / MCP via WASM.
///
/// # Cross-language marshalling shape
///
/// This type (and its nested types below) carry `Serialize`/`Deserialize` so the
/// WASM binding (`hyrr-wasm`, #539 increment 3a) can marshal a config across the
/// JS↔Rust boundary with `serde-wasm-bindgen`, and — under the `ts` feature — a
/// `ts_rs::TS` derive so the canonical TypeScript type is *generated from this
/// Rust struct* rather than hand-mirrored (the drift class #531 belongs to). The
/// TS derive is feature-gated (`ts`) and never enabled by the hermetic
/// `nix flake check`; a dedicated CI job regenerates + diffs the committed
/// `packages/compute/src/generated/config-codec.ts`.
///
/// Note this is the *canonical* (readable) shape, distinct from the compact
/// on-the-wire shape (`CompactConfig`, compact keys) that the codec deflates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CodecConfig {
    pub beam: Beam,
    pub items: Vec<Item>,
    pub irradiation_s: f64,
    pub cooling_s: f64,
    /// Neutron spectrum (ADR-0003) — opaque passthrough (the tagged FluxModel
    /// JSON), carried verbatim so a neutron run round-trips.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub neutron_flux: Option<Value>,
    #[serde(default)]
    pub secondary_neutron: bool,
    /// Time-varying beam current. Large; subject to `SizePolicy` for URLs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_profile: Option<CurrentProfile>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Beam {
    pub projectile: String,
    pub energy_mev: f64,
    pub current_ma: f64,
}

/// A top-level item is either a single layer or an authoring group. Adjacently
/// tagged (`{ "kind": "layer" | "group", "data": … }`) so the JS-facing shape
/// and the generated TS union are both unambiguous.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum Item {
    Layer(Layer),
    Group(Group),
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Layer {
    pub material: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thickness_cm: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub areal_density_g_cm2: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub energy_out_mev: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enrichment: Option<Value>,
    #[serde(default)]
    pub is_monitor: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub density_g_cm3: Option<f64>,
    /// The #531 payload: an embedded custom-material definition (→ compact `x`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom: Option<CustomMaterial>,
}

/// Inline custom-material definition (a custom alloy that must travel with the
/// link). This is what #531 drops and what makes the loss *silent AND
/// physics-altering* (density changes stopping power → wrong yield).
///
/// `nist_compound` (#542): NIST PSTAR compound name (e.g. `WATER_LIQUID`) when
/// the custom material is compound-backed. Carrying it lets the recipient use
/// ICRU-measured stopping tables instead of Bragg additivity. Compact key `c`.
/// A compound-only material (empty `mass_fractions`) can still travel because
/// this field alone is enough to identify the stopping model on the other side.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CustomMaterial {
    pub density_g_cm3: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mass_fractions: Option<BTreeMap<String, f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formula: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enrichment: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nist_compound: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Group {
    pub layers: Vec<Layer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub energy_threshold: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CurrentProfile {
    pub times_s: Vec<f64>,
    pub currents_ma: Vec<f64>,
}

// ─── Size policy ─────────────────────────────────────────────────────────────

/// Where the encoded config is going, which decides the `currentProfile` budget.
///
/// * `File` — a `.hyrr.json` download: embed everything, no budget.
/// * `Url { budget_bytes }` — a share/MCP link: transport/CDN/QR limits
///   bind ~2 KB for the whole hash. If embedding `currentProfile` would blow the
///   budget, we DROP it and report it in `EncodeOutcome.dropped` — never
///   silently (the panel's non-negotiable).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SizePolicy {
    File,
    Url { budget_bytes: usize },
}

/// The result of an encode: the hash plus everything the caller needs to warn
/// the user instead of ever losing state silently or handing them a dead link.
///
/// * `dropped` — structured names of state the size policy *deliberately* left
///   out (today: `"currentProfile"`), so the caller can offer a lossless path.
/// * `warnings` — human-readable notes for conditions where nothing specific was
///   dropped but the link is still not fully sound: the base config is *over
///   budget* (Fix 1) or the stack has more items than the decoder accepts (Fix
///   2). Never silent — the panel's non-negotiable.
/// * `link_unusable` — the config exceeds [`MAX_ITEMS`], so both the Rust and TS
///   decoders will refuse to load the hash. The caller MUST NOT present it as a
///   "view in browser" link; it is a dead link. Point at the lossless fallback
///   instead.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct EncodeOutcome {
    pub hash: String,
    // `&'static str` serializes fine (output-only type; never deserialized), but
    // ts-rs has no `TS` impl for it — pin the generated type explicitly.
    #[cfg_attr(feature = "ts", ts(type = "Array<string>"))]
    pub dropped: Vec<&'static str>,
    pub warnings: Vec<String>,
    pub link_unusable: bool,
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
    /// currentProfile — compact key. Emitted under `SizePolicy::File`, or under
    /// `SizePolicy::Url` when the whole hash still fits the budget
    /// (measure-and-keep). The TS decoder reads `cp` as of increment 1
    /// (`expandConfigSer`/`expandConfigFlat` → `currentProfile`), so the profile
    /// now round-trips cross-language, not just within the Rust/file path.
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
///
/// Compact keys: `d` density, `e` mass fractions, `f` formula, `n` enrichment,
/// `c` nist_compound (#542 — carries the ICRU-measured stopping-table identity).
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
    #[serde(skip_serializing_if = "Option::is_none", default)]
    c: Option<String>,
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
                c: cm.nist_compound.clone(),
            }),
        }
    }
}

impl CompactConfig {
    fn from_config(cfg: &CodecConfig, include_profile: bool) -> Self {
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
/// Under `SizePolicy::Url` the guarantees are: (1) `currentProfile` is included
/// only if the whole hash still fits the budget, else it is dropped and reported
/// in `EncodeOutcome.dropped`; (2) if the profile-less base config STILL exceeds
/// the budget it is reported via `EncodeOutcome.warnings` — never a silent
/// over-budget hash (Fix 1); (3) if the stack exceeds [`MAX_ITEMS`] the decoder
/// refuses to load the link, so `EncodeOutcome.link_unusable` is set and a
/// warning recorded rather than emitting a dead link (Fix 2).
pub fn encode(cfg: &CodecConfig, policy: SizePolicy) -> EncodeOutcome {
    match policy {
        SizePolicy::File => {
            let hash = encode_hash(&CompactConfig::from_config(cfg, true));
            EncodeOutcome {
                hash,
                dropped: vec![],
                warnings: vec![],
                link_unusable: false,
            }
        }
        SizePolicy::Url { budget_bytes } => {
            let mut dropped: Vec<&'static str> = vec![];
            let mut warnings: Vec<String> = vec![];

            // Measure-and-keep the profile: keep it only if the whole hash fits.
            let has_profile = cfg.current_profile.is_some();
            let hash = if has_profile {
                let with = encode_hash(&CompactConfig::from_config(cfg, true));
                if with.len() <= budget_bytes {
                    with
                } else {
                    dropped.push("currentProfile");
                    encode_hash(&CompactConfig::from_config(cfg, false))
                }
            } else {
                encode_hash(&CompactConfig::from_config(cfg, false))
            };

            // Fix 2: > MAX_ITEMS top-level items → both decoders (Rust
            // `compact_to_config`, TS `MAX_URL_ITEMS`) reject the hash. Never emit
            // a dead link; flag it and warn.
            let n_items = cfg.items.len();
            let link_unusable = n_items > MAX_ITEMS;
            if link_unusable {
                warnings.push(format!(
                    "stack too large to link ({n_items} > {MAX_ITEMS} layers); use a .hyrr.json export"
                ));
            } else if hash.len() > budget_bytes {
                // Fix 1: even with the profile dropped (or with no profile at
                // all), the base config can exceed the budget — a huge alloy
                // mass-fraction map, many layers, etc. Surface it; the panel's
                // non-negotiable is that this is never silent.
                warnings.push(format!(
                    "link is {} bytes, over the {budget_bytes}-byte URL budget — \
                     some transports/QR codes may reject it; use a .hyrr.json export \
                     for the full config",
                    hash.len()
                ));
            }

            EncodeOutcome {
                hash,
                dropped,
                warnings,
                link_unusable,
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

/// Decode a v2 config hash back to the canonical `CodecConfig`, applying every
/// security cap. Accepts a full URL, a bare `#config=1:…`/`config=1:…` hash, or
/// the raw `1:<payload>`.
pub fn decode(input: &str) -> Result<CodecConfig, CodecError> {
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

fn compact_to_config(c: CompactConfig) -> Result<CodecConfig, CodecError> {
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

    Ok(CodecConfig {
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
    if let Some(c) = &x.c {
        if c.len() > MAX_FORMULA_LEN {
            return Err(CodecError::StringTooLong(c.len()));
        }
    }
    Ok(CustomMaterial {
        density_g_cm3: x.d,
        mass_fractions: x.e,
        formula: x.f,
        enrichment: x.n,
        nist_compound: x.c,
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
    fn poc_inconel_config() -> CodecConfig {
        let mut fractions = BTreeMap::new();
        fractions.insert("Ni".to_string(), 0.58);
        fractions.insert("Cr".to_string(), 0.22);
        fractions.insert("Fe".to_string(), 0.05);
        fractions.insert("Mo".to_string(), 0.09);
        fractions.insert("Nb".to_string(), 0.036);
        fractions.insert("Ti".to_string(), 0.004);
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
                        formula: Some("Ni58Cr22Fe5Mo9Nb3.6Ti0.4".to_string()),
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

    // (a) Round-trip a config-with-custom-alloy encode → decode; the alloy
    //     density + mass fractions must survive.
    #[test]
    fn roundtrip_custom_alloy_survives() {
        let cfg = poc_inconel_config();
        let outcome = encode(&cfg, SizePolicy::Url { budget_bytes: 2000 });
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
                budget_bytes: BUDGET,
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
                budget_bytes: BUDGET,
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

    // (#542 nit 1) `nist_compound` round-trips through the codec. A compound-
    // backed custom (empty `mass_fractions`, only a compound identifier) is
    // preserved end-to-end so the recipient's stopping model matches the sender's.
    #[test]
    fn nist_compound_roundtrips_on_custom_material() {
        // Compound-only: no composition, just density + compound id.
        let cfg = CodecConfig {
            beam: Beam {
                projectile: "p".to_string(),
                energy_mev: 20.0,
                current_ma: 0.1,
            },
            items: vec![Item::Layer(Layer {
                material: "just-water".to_string(),
                thickness_cm: Some(0.1),
                custom: Some(CustomMaterial {
                    density_g_cm3: 1.0,
                    mass_fractions: None,
                    formula: None,
                    enrichment: None,
                    nist_compound: Some("WATER_LIQUID".to_string()),
                }),
                ..Default::default()
            })],
            irradiation_s: 3600.0,
            cooling_s: 3600.0,
            neutron_flux: None,
            secondary_neutron: false,
            current_profile: None,
        };
        let out = encode(&cfg, SizePolicy::File);
        let dec = decode(&out.hash).unwrap();
        assert_eq!(dec, cfg);
        let Item::Layer(layer) = &dec.items[0] else {
            panic!()
        };
        assert_eq!(
            layer.custom.as_ref().unwrap().nist_compound.as_deref(),
            Some("WATER_LIQUID")
        );
    }

    // A hostile `x.c` (nist_compound) that exceeds MAX_FORMULA_LEN is rejected
    // — proves the cap is wired for the new field, not just declared.
    #[test]
    fn oversized_nist_compound_rejected() {
        let long = "A".repeat(MAX_FORMULA_LEN + 1);
        let compact = json!({
            "b": { "p": "p", "e": 28.0, "c": 0.2 },
            "l": [ { "m": "x", "x": { "d": 1.0, "c": long } } ],
            "i": 1.0, "c": 1.0
        });
        let json_bytes = serde_json::to_vec(&compact).unwrap();
        let mut enc = DeflateEncoder::new(Vec::new(), Compression::default());
        enc.write_all(&json_bytes).unwrap();
        let hash = format!("#config=1:{}", base64url_encode(&enc.finish().unwrap()));
        assert!(matches!(
            decode(&hash).unwrap_err(),
            CodecError::StringTooLong(_)
        ));
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

    /// The config the cross-language vitest fixture is built from: a
    /// config-with-alloy PLUS a small `currentProfile`, encoded under Url policy
    /// (the actual #531 MCP-link scenario). The profile is small so
    /// measure-and-keep KEEPS it (nothing dropped), proving the TS decoder's
    /// `cp` support recovers the profile cross-language (increment 1).
    fn cross_lang_fixture_hash() -> String {
        let mut cfg = poc_inconel_config();
        // A small 5-sample beam-current ramp — comfortably under the URL budget.
        cfg.current_profile = Some(CurrentProfile {
            times_s: vec![0.0, 3600.0, 7200.0, 10800.0, 14400.0],
            currents_ma: vec![0.2, 0.2, 0.15, 0.15, 0.1],
        });
        let outcome = encode(&cfg, SizePolicy::Url { budget_bytes: 2000 });
        assert!(outcome.hash.starts_with("#config=1:"));
        // The profile fits → KEPT (the fixture must carry `cp` for the vitest).
        assert!(
            outcome.dropped.is_empty() && outcome.warnings.is_empty(),
            "fixture profile must fit the budget so `cp` is emitted"
        );
        outcome.hash
    }

    fn cross_lang_fixture_path() -> String {
        let dir = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../frontend/src/lib/__fixtures__"
        );
        format!("{dir}/poc-rust-encoded.txt")
    }

    // (d) Cross-language fixture DRIFT CHECK (Fix 3). The normal test run asserts
    //     the committed fixture equals freshly-encoded output — it never writes,
    //     so `cargo test` leaves the working tree clean. If the codec's wire
    //     output ever drifts from the committed fixture, this fails loudly.
    //     Regeneration is explicit and opt-in: `REGEN_FIXTURES=1 cargo test`.
    #[test]
    fn cross_lang_fixture_matches_encoder() {
        // Trailing newline so the committed fixture is stable under the repo's
        // end-of-file-fixer hook. The TS reader `.trim()`s it.
        let expected = format!("{}\n", cross_lang_fixture_hash());
        let path = cross_lang_fixture_path();

        if std::env::var_os("REGEN_FIXTURES").is_some() {
            let dir = std::path::Path::new(&path).parent().unwrap();
            std::fs::create_dir_all(dir).expect("create fixture dir");
            std::fs::write(&path, &expected).expect("write fixture");
            return;
        }

        let committed = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            // The hermetic core-only build (nix flake check / crane) has no
            // frontend/ tree, so the fixture is absent. The drift check is only
            // meaningful in a full checkout (local dev + the non-hermetic CI
            // jobs that check the whole repo) — skip cleanly rather than fail
            // the hermetic gate. Regenerate with `REGEN_FIXTURES=1`.
            Err(_) => return,
        };
        assert_eq!(
            committed, expected,
            "cross-lang fixture drifted from the encoder output; \
             regenerate with `REGEN_FIXTURES=1 cargo test cross_lang_fixture_matches_encoder`"
        );
    }

    // ─── ts-rs binding generation + drift gate (#539 increment 3a) ───────────
    //
    // The canonical TypeScript types are *generated from the Rust structs above*
    // (via `ts_rs::TS`), never hand-mirrored — that hand-mirror is the drift
    // class #531 belongs to. Generation is gated behind the `ts` cargo feature so
    // the hermetic `nix flake check` (which builds `core` with default features
    // in `rust-test`, and only *compiles* — never runs — tests under
    // `rust-clippy --all-features`) never triggers a file write into the
    // read-only sandbox. It's also write-gated behind `REGEN_TS`: the normal
    // `cargo test --features ts` run only *asserts* the committed file matches
    // (reads, never writes), exactly like `cross_lang_fixture_matches_encoder`.
    // CI regenerates (`REGEN_TS=1`) then `git diff --exit-code`s the path.

    /// Render every canonical codec type into one committed `.ts` module. Uses
    /// `TS::decl()` (declaration only, no `export`, no cross-file imports — all
    /// types live in this single file so their names resolve locally) and
    /// prepends `export ` so the frontend can import them.
    #[cfg(feature = "ts")]
    fn render_ts_bindings() -> String {
        use ts_rs::TS;
        // Fixed (non-env) config so the render is deterministic across machines.
        let cfg = ts_rs::Config::default();
        let mut out = String::new();
        out.push_str(
            "// Generated by `REGEN_TS=1 cargo test --features ts` \
             (see core/src/config_codec.rs). DO NOT EDIT BY HAND.\n\
             //\n\
             // Canonical config-codec types, generated from the Rust structs in\n\
             // `hyrr-core::config_codec`. A CI drift gate (`.github/workflows/ci.yml`,\n\
             // job `ts-bindings-sync`) regenerates this file and `git diff --exit-code`s\n\
             // it, so these types can never silently drift from the Rust source of\n\
             // truth (issue #539).\n\n",
        );
        // Order: leaves first so a human reads the file top-down without forward
        // references (TypeScript doesn't require it, but it's tidier). `JsonValue`
        // comes first — the `serde_json::Value` passthrough fields (neutron flux,
        // enrichment) reference it, and this single self-contained file must
        // define it locally rather than import it.
        let decls = [
            <Value as TS>::decl(&cfg),
            Beam::decl(&cfg),
            CustomMaterial::decl(&cfg),
            Layer::decl(&cfg),
            Group::decl(&cfg),
            Item::decl(&cfg),
            CurrentProfile::decl(&cfg),
            CodecConfig::decl(&cfg),
            SizePolicy::decl(&cfg),
            EncodeOutcome::decl(&cfg),
        ];
        for decl in decls {
            out.push_str("export ");
            out.push_str(&decl);
            out.push_str("\n\n");
        }
        // Normalize to hook-clean output so the repo's `end-of-file-fixer` and
        // `trailing-whitespace` pre-commit hooks never mutate the committed file
        // out from under the drift gate: ts-rs emits a trailing space before an
        // embedded doc comment, and the per-decl `\n\n` leaves a blank line at
        // EOF. Strip per-line trailing whitespace and end with exactly one
        // newline. (The generator is the SSoT, so the committed file must match
        // this byte-for-byte.)
        let mut cleaned = out
            .lines()
            .map(|line| line.trim_end())
            .collect::<Vec<_>>()
            .join("\n");
        while cleaned.ends_with('\n') {
            cleaned.pop();
        }
        cleaned.push('\n');
        cleaned
    }

    #[cfg(feature = "ts")]
    fn ts_bindings_path() -> String {
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../packages/compute/src/generated/config-codec.ts"
        )
        .to_string()
    }

    /// Drift gate: the committed generated TS must equal a fresh render. Never
    /// writes unless `REGEN_TS` is set (so it's safe even if the `ts` feature is
    /// ever compiled *and run* under a sandbox — it only reads). Regenerate with
    /// `REGEN_TS=1 cargo test --features ts ts_bindings`.
    #[cfg(feature = "ts")]
    #[test]
    fn ts_bindings_match_committed() {
        let generated = render_ts_bindings();
        let path = ts_bindings_path();

        if std::env::var_os("REGEN_TS").is_some() {
            let dir = std::path::Path::new(&path).parent().unwrap();
            std::fs::create_dir_all(dir).expect("create generated dir");
            std::fs::write(&path, &generated).expect("write ts bindings");
            return;
        }

        let committed = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "committed TS bindings {path} unreadable ({e}); regenerate with \
                 `REGEN_TS=1 cargo test --features ts ts_bindings_match_committed`"
            )
        });
        assert_eq!(
            committed, generated,
            "generated TS bindings drifted from the Rust source of truth; \
             regenerate with `REGEN_TS=1 cargo test --features ts ts_bindings_match_committed`"
        );
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
        let cfg = CodecConfig {
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

    // Full canonical state — the union of everything #531 dropped plus the
    // neutron fields — round-trips through the codec under the URL policy, and a
    // realistic profile is KEPT (measure-and-keep), nothing dropped.
    #[test]
    fn full_canonical_state_round_trips_and_keeps_realistic_profile() {
        let mut fractions = BTreeMap::new();
        fractions.insert("Ni".to_string(), 0.9);
        fractions.insert("Ti".to_string(), 0.1);
        let n = 200;
        let cfg = CodecConfig {
            beam: Beam {
                projectile: "p".to_string(),
                energy_mev: 24.0,
                current_ma: 0.05,
            },
            items: vec![
                Item::Layer(Layer {
                    material: "nitinol".to_string(),
                    thickness_cm: Some(0.2),
                    density_g_cm3: Some(6.45),
                    enrichment: Some(json!([{ "element": "Ni", "A": 58, "fraction": 0.99 }])),
                    custom: Some(CustomMaterial {
                        density_g_cm3: 6.45,
                        mass_fractions: Some(fractions),
                        formula: None,
                        enrichment: None,
                        nist_compound: None,
                    }),
                    ..Default::default()
                }),
                Item::Layer(Layer {
                    material: "H2O".to_string(),
                    thickness_cm: Some(2.0),
                    density_g_cm3: Some(1.0),
                    ..Default::default()
                }),
            ],
            irradiation_s: 3600.0,
            cooling_s: 86400.0,
            neutron_flux: Some(json!({ "kind": "thermal", "flux": 1e13, "kt_mev": 2.53e-8 })),
            secondary_neutron: true,
            current_profile: Some(CurrentProfile {
                times_s: (0..n).map(|i| i as f64 * 3600.0).collect(),
                currents_ma: (0..n).map(|i| 0.05 + i as f64 * 1e-4).collect(),
            }),
        };

        // Measure-and-keep: a realistic 200-sample profile FITS the URL budget.
        let url = encode(
            &cfg,
            SizePolicy::Url {
                budget_bytes: DEFAULT_URL_BUDGET_BYTES,
            },
        );
        assert!(
            url.dropped.is_empty(),
            "realistic profile fits {}-byte budget (hash {} bytes)",
            DEFAULT_URL_BUDGET_BYTES,
            url.hash.len()
        );
        assert!(url.hash.len() <= DEFAULT_URL_BUDGET_BYTES);
        assert_eq!(decode(&url.hash).unwrap(), cfg, "full state round-trips");
    }

    // (Fix 1) A profile-LESS config whose base config STILL exceeds the budget —
    // a huge, incompressible custom-alloy mass-fraction map — must be reported,
    // never silently emitted as an over-budget hash with `dropped == []`.
    #[test]
    fn over_budget_base_config_is_warned_not_silent() {
        let mut fractions = BTreeMap::new();
        // ~600 incompressible entries so DEFLATE can't crush the base config
        // under 2 KB. Keys are distinct; values high-entropy.
        let noise = lcg_stream(99, 600);
        for (i, &u) in noise.iter().enumerate() {
            fractions.insert(format!("E{i:04}"), (u % 1_000_000) as f64 / 997.0 + 1e-6);
        }
        let cfg = CodecConfig {
            beam: Beam {
                projectile: "p".to_string(),
                energy_mev: 20.0,
                current_ma: 0.1,
            },
            items: vec![Item::Layer(Layer {
                material: "huge-alloy".to_string(),
                thickness_cm: Some(0.1),
                custom: Some(CustomMaterial {
                    density_g_cm3: 5.0,
                    mass_fractions: Some(fractions),
                    formula: None,
                    enrichment: None,
                    nist_compound: None,
                }),
                ..Default::default()
            })],
            irradiation_s: 3600.0,
            cooling_s: 3600.0,
            neutron_flux: None,
            secondary_neutron: false,
            current_profile: None,
        };
        let out = encode(
            &cfg,
            SizePolicy::Url {
                budget_bytes: DEFAULT_URL_BUDGET_BYTES,
            },
        );
        // No profile → nothing to drop, but the base config blows the budget.
        assert!(out.dropped.is_empty(), "no profile to drop");
        assert!(
            out.hash.len() > DEFAULT_URL_BUDGET_BYTES,
            "test setup: base config must exceed the budget (was {} bytes)",
            out.hash.len()
        );
        assert!(!out.link_unusable, "one item — not an item-cap failure");
        // The money assertion: the over-budget condition is OBSERVABLE, not silent.
        assert_eq!(out.warnings.len(), 1, "warnings: {:?}", out.warnings);
        assert!(
            out.warnings[0].contains("over the"),
            "warning names the budget: {:?}",
            out.warnings
        );
    }

    // (Fix 2) A stack with more than MAX_ITEMS top-level items is flagged
    // `link_unusable` + warned — never handed back as a "view in browser" link
    // the decoder would refuse. Proven by decoding the emitted hash → rejected.
    #[test]
    fn oversized_stack_flagged_unusable_and_would_not_decode() {
        let items: Vec<Item> = (0..MAX_ITEMS + 1)
            .map(|_| {
                Item::Layer(Layer {
                    material: "Cu".to_string(),
                    thickness_cm: Some(0.01),
                    ..Default::default()
                })
            })
            .collect();
        let cfg = CodecConfig {
            beam: Beam {
                projectile: "p".to_string(),
                energy_mev: 18.0,
                current_ma: 0.1,
            },
            items,
            irradiation_s: 3600.0,
            cooling_s: 3600.0,
            neutron_flux: None,
            secondary_neutron: false,
            current_profile: None,
        };
        let out = encode(
            &cfg,
            SizePolicy::Url {
                budget_bytes: DEFAULT_URL_BUDGET_BYTES,
            },
        );
        assert!(out.link_unusable, "{} > {} items", MAX_ITEMS + 1, MAX_ITEMS);
        assert!(out.dropped.is_empty());
        assert_eq!(out.warnings.len(), 1, "warnings: {:?}", out.warnings);
        assert!(
            out.warnings[0].contains("stack too large"),
            "warning: {:?}",
            out.warnings
        );
        // Proof it's a dead link: the decoder rejects the very hash we produced.
        assert_eq!(
            decode(&out.hash).unwrap_err(),
            CodecError::TooManyItems(MAX_ITEMS + 1)
        );
    }
}

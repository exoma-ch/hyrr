//! Config URL encoding — generates shareable links to the hosted frontend.
//!
//! Produces `#config=1:<base64url-of-deflated-compact-json>` hashes
//! compatible with the frontend's `config-url-v2.ts` decoder.
//!
//! SSoT: this is the canonical encoder. The frontend TS version should
//! eventually import this via WASM.

use flate2::write::DeflateEncoder;
use flate2::Compression;
use serde_json::{json, Value};
use std::io::Write;

const FRONTEND_BASE: &str = "https://exoma-ch.github.io/hyrr/";

/// Build a share URL for a simulation config.
///
/// Takes the MCP tool arguments (projectile, energy_mev, current_ma,
/// irradiation_time_s, cooling_time_s, layers) and produces a full URL.
pub fn share_url(args: &Value) -> Option<String> {
    let hash = encode_config_v2(args)?;
    Some(format!("{}{}", FRONTEND_BASE, hash))
}

/// Encode MCP simulate args to v2 URL hash: `#config=1:<base64url>`.
///
/// Compatible with the frontend's `decodeConfigV2()`.
fn encode_config_v2(args: &Value) -> Option<String> {
    let compact = compact_config(args)?;
    let json_bytes = serde_json::to_vec(&compact).ok()?;

    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&json_bytes).ok()?;
    let compressed = encoder.finish().ok()?;

    let base64 = base64url_encode(&compressed);
    Some(format!("#config=1:{}", base64))
}

/// Convert MCP args to the compact v2 JSON format.
///
/// Compact keys: b(beam), l(layers), i(irradiation_s), c(cooling_s)
/// Beam: p(projectile), e(energy), c(current)
/// Layer: m(material), t(thickness), o(energy_out), n(enrichment), f(monitor)
fn compact_config(args: &Value) -> Option<Value> {
    let projectile = args.get("projectile")?.as_str()?;
    let energy = args.get("energy_mev")?.as_f64()?;
    let current = args.get("current_ma")?.as_f64()?;
    let irr = args.get("irradiation_time_s").and_then(|v| v.as_f64()).unwrap_or(86400.0);
    let cool = args.get("cooling_time_s").and_then(|v| v.as_f64()).unwrap_or(86400.0);
    let layers = args.get("layers")?.as_array()?;

    let compact_layers: Vec<Value> = layers
        .iter()
        .filter_map(|l| {
            let mut cl = json!({ "m": l.get("material")?.as_str()? });
            if let Some(t) = l.get("thickness_cm").and_then(|v| v.as_f64()) {
                cl["t"] = json!(t);
            }
            if let Some(o) = l.get("energy_out_mev").and_then(|v| v.as_f64()) {
                cl["o"] = json!(o);
            }
            if let Some(n) = l.get("enrichment") {
                if n.is_array() {
                    cl["n"] = n.clone();
                }
            }
            Some(cl)
        })
        .collect();

    Some(json!({
        "b": { "p": projectile, "e": energy, "c": current },
        "l": compact_layers,
        "i": irr,
        "c": cool,
    }))
}

/// Base64url encode (RFC 4648 §5): +→-, /→_, no padding.
fn base64url_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn roundtrip_simple_config() {
        let args = json!({
            "projectile": "p",
            "energy_mev": 28.0,
            "current_ma": 0.2,
            "irradiation_time_s": 604800.0,
            "cooling_time_s": 86400.0,
            "layers": [
                { "material": "Ga", "thickness_cm": 0.15 }
            ]
        });

        let url = share_url(&args).unwrap();
        assert!(url.starts_with("https://exoma-ch.github.io/hyrr/#config=1:"));
        assert!(url.len() < 300); // should be compact
    }

    #[test]
    fn base64url_no_padding_no_plus_no_slash() {
        let args = json!({
            "projectile": "p",
            "energy_mev": 16.0,
            "current_ma": 0.15,
            "layers": [
                { "material": "havar", "thickness_cm": 0.003 },
                { "material": "Mo-100", "thickness_cm": 0.01 },
                { "material": "Cu", "energy_out_mev": 0.0 }
            ]
        });

        let url = share_url(&args).unwrap();
        let hash = url.split("#config=1:").nth(1).unwrap();
        assert!(!hash.contains('+'));
        assert!(!hash.contains('/'));
        assert!(!hash.contains('='));
    }

    #[test]
    fn missing_fields_returns_none() {
        let args = json!({ "projectile": "p" }); // missing energy, layers
        assert!(share_url(&args).is_none());
    }

    #[test]
    fn rust_encoded_url_decodes_to_valid_compact_json() {
        use flate2::read::DeflateDecoder;
        use std::io::Read;

        let args = json!({
            "projectile": "p",
            "energy_mev": 16.0,
            "current_ma": 0.15,
            "irradiation_time_s": 86400.0,
            "cooling_time_s": 86400.0,
            "layers": [
                { "material": "havar", "thickness_cm": 0.003 },
                { "material": "Mo-100", "thickness_cm": 0.01 },
                { "material": "Cu", "energy_out_mev": 0.0 }
            ]
        });

        let url = share_url(&args).unwrap();
        let hash = url.split("#config=1:").nth(1).unwrap();

        // Decode base64url → bytes
        let base64 = hash.replace('-', "+").replace('_', "/");
        // Pad to multiple of 4
        let padded = match base64.len() % 4 {
            2 => format!("{}==", base64),
            3 => format!("{}=", base64),
            _ => base64,
        };
        let compressed = base64_decode(&padded);

        // Inflate
        let mut decoder = DeflateDecoder::new(&compressed[..]);
        let mut json_str = String::new();
        decoder.read_to_string(&mut json_str).unwrap();

        // Parse and verify compact keys
        let compact: Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(compact["b"]["p"], "p");
        assert_eq!(compact["b"]["e"], 16.0);
        assert_eq!(compact["b"]["c"], 0.15);
        assert_eq!(compact["i"], 86400.0);
        assert_eq!(compact["c"], 86400.0);
        let layers = compact["l"].as_array().unwrap();
        assert_eq!(layers.len(), 3);
        assert_eq!(layers[0]["m"], "havar");
        assert_eq!(layers[0]["t"], 0.003);
        assert_eq!(layers[1]["m"], "Mo-100");
        assert_eq!(layers[2]["m"], "Cu");
        assert_eq!(layers[2]["o"], 0.0);
    }

    fn base64_decode(input: &str) -> Vec<u8> {
        const TABLE: &[u8; 128] = &{
            let mut t = [255u8; 128];
            let chars = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            let mut i = 0;
            while i < 64 {
                t[chars[i] as usize] = i as u8;
                i += 1;
            }
            t
        };
        let bytes: Vec<u8> = input.bytes().filter(|&b| b != b'=' && b != b'\n').collect();
        let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
        for chunk in bytes.chunks(4) {
            let n = chunk.len();
            let b = |i: usize| if i < n { TABLE[chunk[i] as usize] as u32 } else { 0 };
            let triple = (b(0) << 18) | (b(1) << 12) | (b(2) << 6) | b(3);
            out.push((triple >> 16) as u8);
            if n > 2 { out.push((triple >> 8) as u8); }
            if n > 3 { out.push(triple as u8); }
        }
        out
    }
}

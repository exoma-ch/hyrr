//! Gamma dose-rate helpers for the MCP dose tools (#440 / #441).
//!
//! The load-bearing physics is already in hyrr-core: each nuclide's specific
//! gamma constant `k` is served by
//! [`DatabaseProtocol::get_dose_constant`](crate::db::DatabaseProtocol::get_dose_constant)
//! in units of µSv·m²·MBq⁻¹·h⁻¹ (ENSDF-derived, matches the web app's
//! `dose-constants.ts`). This module wires `k_i × A_i / r²` into one place so
//! `get_dose_rate` and `get_dose_rate_breakdown` share the same summation and
//! the same treatment of missing-k isotopes (they are surfaced, never hidden).

use crate::db::DatabaseProtocol;
use crate::types::{IsotopeResult, StackResult};

/// A dose-rate contribution from one produced isotope. `dose_rate_usv_h` is at
/// the requested distance; `k_usv_m2_per_mbq_h` is the raw specific gamma
/// constant (at 1 m). `source` is the k provenance string
/// (`ensdf` / `it-approx` / …) as returned by the database. `activity_bq`
/// is the isotope's activity at the requested cooling time — usually the
/// StackResult's `iso.activity_bq` (end of cooling).
#[derive(Debug, Clone)]
pub struct DoseContribution {
    pub isotope: String,
    pub z: u32,
    pub a: u32,
    pub state: String,
    pub activity_bq: f64,
    /// `None` when the database has no dose constant for this nuclide — the
    /// contribution is reported as 0 and the caller must surface the miss so
    /// the total is never silently under-reported.
    pub k_usv_m2_per_mbq_h: Option<f64>,
    pub source: Option<String>,
    pub dose_rate_usv_h: f64,
}

/// Summed dose from a stack at `distance_cm`. `missing_k` is every produced
/// isotope with non-negligible activity but no dose constant — reported so the
/// caller can warn instead of silently zeroing them out.
#[derive(Debug, Clone)]
pub struct StackDose {
    pub distance_cm: f64,
    pub total_dose_rate_usv_h: f64,
    pub total_activity_bq: f64,
    pub contributions: Vec<DoseContribution>,
    pub missing_k: Vec<String>,
}

/// Convert `k` (at 1 m) + activity + distance to a dose rate.
///
/// `k` is µSv·m²·MBq⁻¹·h⁻¹, so `dose = k · (A / 1e6) / r_m²`. Anything below
/// `MIN_DISTANCE_M` returns `f64::INFINITY` rather than a nonsense large value
/// — the near-field approximation of a point source is invalid there; the
/// tool surfaces the ∞ so a caller doesn't quietly report an astronomical
/// bare-source number.
pub const MIN_DISTANCE_M: f64 = 0.01; // 1 cm

pub fn dose_rate_at(k_usv_m2_per_mbq_h: f64, activity_bq: f64, distance_cm: f64) -> f64 {
    let r_m = distance_cm / 100.0;
    if r_m < MIN_DISTANCE_M {
        return f64::INFINITY;
    }
    (activity_bq / 1.0e6) * k_usv_m2_per_mbq_h / (r_m * r_m)
}

/// Aggregate every distinct produced isotope in the stack (summed across
/// layers) into one dose contribution. Layer-summed activity is `iso.activity_bq`
/// — the end-of-cooling activity that the caller asked for by picking
/// `cooling_time_s`; there is no separate interpolation step needed.
///
/// `activity_floor_bq` filters trivial numerical dust before the dose sum;
/// pass `0.0` to include everything.
pub fn compute_stack_dose(
    db: &dyn DatabaseProtocol,
    result: &StackResult,
    distance_cm: f64,
    activity_floor_bq: f64,
) -> StackDose {
    use std::collections::HashMap;

    // Sum activity across layers per (z, a, state).
    let mut agg: HashMap<(u32, u32, String), (String, f64)> = HashMap::new();
    for lr in &result.layer_results {
        for iso in lr.isotope_results.values() {
            let key = (iso.z, iso.a, iso.state.clone());
            let entry = agg.entry(key).or_insert_with(|| (iso.name.clone(), 0.0));
            entry.1 += iso.activity_bq;
        }
    }

    let mut contributions = Vec::with_capacity(agg.len());
    let mut missing_k = Vec::new();
    let mut total_dose = 0.0;
    let mut total_activity = 0.0;

    for ((z, a, state), (name, activity_bq)) in agg {
        if activity_bq < activity_floor_bq {
            continue;
        }
        total_activity += activity_bq;

        let (k, source) = match db.get_dose_constant(z, a, &state) {
            Some((k, src)) => (Some(k), Some(src)),
            None => {
                missing_k.push(name.clone());
                (None, None)
            }
        };
        let dose = k
            .map(|k| dose_rate_at(k, activity_bq, distance_cm))
            .unwrap_or(0.0);
        total_dose += dose;

        contributions.push(DoseContribution {
            isotope: name,
            z,
            a,
            state,
            activity_bq,
            k_usv_m2_per_mbq_h: k,
            source,
            dose_rate_usv_h: dose,
        });
    }

    // Sort by dose contribution descending; missing-k isotopes float to the
    // end where they can't be lost in a truncated table.
    contributions.sort_by(|x, y| {
        y.dose_rate_usv_h
            .partial_cmp(&x.dose_rate_usv_h)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| x.isotope.cmp(&y.isotope))
    });
    missing_k.sort();
    missing_k.dedup();

    StackDose {
        distance_cm,
        total_dose_rate_usv_h: total_dose,
        total_activity_bq: total_activity,
        contributions,
        missing_k,
    }
}

/// Convenience: compute a single isotope's dose rate at `distance_cm` given its
/// activity. Returns `None` when the database has no dose constant.
pub fn iso_dose_rate(
    db: &dyn DatabaseProtocol,
    iso: &IsotopeResult,
    distance_cm: f64,
) -> Option<(f64, f64, String)> {
    db.get_dose_constant(iso.z, iso.a, &iso.state)
        .map(|(k, src)| (dose_rate_at(k, iso.activity_bq, distance_cm), k, src))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dose_scales_inverse_square() {
        // 1 MBq at 1 m with k = 0.100 µSv·m²/(MBq·h) → 0.100 µSv/h.
        // At 2 m → 0.025 µSv/h (quartered).
        let d1 = dose_rate_at(0.100, 1.0e6, 100.0);
        let d2 = dose_rate_at(0.100, 1.0e6, 200.0);
        assert!((d1 - 0.100).abs() < 1e-9, "1 m dose {d1}");
        assert!((d2 - 0.025).abs() < 1e-9, "2 m dose {d2}");
        assert!((d1 / d2 - 4.0).abs() < 1e-9, "inverse-square");
    }

    #[test]
    fn dose_zero_activity_is_zero() {
        assert_eq!(dose_rate_at(0.143, 0.0, 100.0), 0.0);
    }

    #[test]
    fn dose_below_min_distance_is_infinite() {
        // 0.5 cm < MIN_DISTANCE_M (1 cm) → refuse a near-field number.
        assert!(dose_rate_at(0.143, 1.0e6, 0.5).is_infinite());
    }

    #[test]
    fn f18_hand_calc() {
        // F-18 k ≈ 0.143 µSv·m²/(MBq·h); 12.6 MBq at 1 m → ~1.80 µSv/h.
        let dose = dose_rate_at(0.143, 12.6e6, 100.0);
        assert!((dose - 1.802).abs() < 0.01, "F-18 12.6 MBq at 1 m → {dose}");
    }
}

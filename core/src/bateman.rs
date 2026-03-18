//! Bateman equations: time-dependent activity, daughter ingrowth.

use crate::constants::LN2;
use crate::interpolation::linspace;

/// Result of Bateman activity computation.
pub struct BatemanResult {
    pub time_grid: Vec<f64>,
    pub activity: Vec<f64>,
}

/// Compute time-dependent activity via Bateman equations.
///
/// During irradiation: A(t) = R * (1 - exp(-λt))
/// During cooling: A(t) = A(T_irr) * exp(-λ(t - T_irr))
pub fn bateman_activity(
    production_rate: f64,
    half_life_s: Option<f64>,
    irradiation_time_s: f64,
    cooling_time_s: f64,
    n_time_points: usize,
) -> BatemanResult {
    let n_irr = n_time_points / 2;
    let n_cool = n_time_points - n_irr;

    let t_irr = linspace(0.0, irradiation_time_s, n_irr);
    let t_cool_full = linspace(
        irradiation_time_s,
        irradiation_time_s + cooling_time_s,
        n_cool + 1,
    );
    // Exclude duplicate point at irradiation_time_s
    let t_cool = &t_cool_full[1..];

    let mut time_grid = Vec::with_capacity(n_irr + n_cool);
    time_grid.extend_from_slice(&t_irr);
    time_grid.extend_from_slice(t_cool);

    let mut activity = vec![0.0; time_grid.len()];

    let half_life = match half_life_s {
        Some(t) if t > 0.0 => t,
        _ => {
            return BatemanResult {
                time_grid,
                activity,
            }
        }
    };

    let lambda = LN2 / half_life;

    // Irradiation phase
    for (i, &t) in time_grid.iter().enumerate() {
        if t <= irradiation_time_s {
            activity[i] = production_rate * (1.0 - (-lambda * t).exp());
        }
    }

    // Activity at end of irradiation
    let a_eoi = production_rate * (1.0 - (-lambda * irradiation_time_s).exp());

    // Cooling phase
    for (i, &t) in time_grid.iter().enumerate() {
        if t > irradiation_time_s {
            let dt_cool = t - irradiation_time_s;
            activity[i] = a_eoi * (-lambda * dt_cool).exp();
        }
    }

    BatemanResult {
        time_grid,
        activity,
    }
}

/// Compute daughter activity from parent decay during cooling (Bateman ingrowth).
pub fn daughter_ingrowth(
    parent_activity_eoi_bq: f64,
    parent_half_life_s: f64,
    daughter_half_life_s: Option<f64>,
    branching_ratio: f64,
    cooling_times_s: &[f64],
) -> Vec<f64> {
    let mut result = vec![0.0; cooling_times_s.len()];

    let daughter_hl = match daughter_half_life_s {
        Some(t) if t > 0.0 => t,
        _ => return result,
    };

    let lambda_p = LN2 / parent_half_life_s;
    let lambda_d = LN2 / daughter_hl;

    // Degenerate case: lambda_P ≈ lambda_D
    if (lambda_d - lambda_p).abs() < 1e-30 * lambda_d.max(lambda_p) {
        for (i, &t) in cooling_times_s.iter().enumerate() {
            result[i] =
                branching_ratio * parent_activity_eoi_bq * lambda_d * t * (-lambda_d * t).exp();
        }
        return result;
    }

    let coeff = (branching_ratio * parent_activity_eoi_bq * lambda_d) / (lambda_d - lambda_p);
    for (i, &t) in cooling_times_s.iter().enumerate() {
        result[i] = coeff * ((-lambda_p * t).exp() - (-lambda_d * t).exp());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bateman_saturation() {
        // At saturation (100 half-lives), activity ≈ production rate
        let rate = 1e6;
        let half_life = 3600.0;
        let irr_time = 100.0 * half_life;
        let result = bateman_activity(rate, Some(half_life), irr_time, 0.0, 200);
        let a_eoi = *result
            .activity
            .iter()
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap();
        assert!((a_eoi - rate).abs() / rate < 1e-6);
    }

    #[test]
    fn test_bateman_stable() {
        let result = bateman_activity(1e6, None, 3600.0, 3600.0, 200);
        assert!(result.activity.iter().all(|&a| a == 0.0));
    }

    #[test]
    fn test_saturation_yield() {
        use crate::production::saturation_yield;
        let yield_val = saturation_yield(1e6, Some(3600.0), 1.0);
        assert!((yield_val - 1000.0).abs() < 1e-6);
    }
}

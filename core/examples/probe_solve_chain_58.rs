//! Empirically reproduce #58 step-function symptom by calling solve_chain
//! directly on a 1-isotope chain (¹⁵O), then a 2-isotope chain (Mo-99 → Tc-99m).
//!
//! solve_chain is currently bypassed in apply_chain_solver_by_component
//! (use_bateman_fallback = true). This probe sidesteps the bypass.

use hyrr_core::chains::solve_chain;
use hyrr_core::types::{ChainIsotope, DecayMode};

fn rel_err(a: f64, b: f64) -> f64 {
    let d = (a - b).abs();
    let s = b.abs().max(1.0);
    d / s
}

fn run_o15() {
    println!("=== Experiment 1: ¹⁵O direct, single isotope ===");
    let half_life = 122.24;
    let irr = 7200.0;
    let cool = 3600.0;
    let n_pts = 200;
    let r = 1.0e9_f64; // atoms/s

    let chain = vec![ChainIsotope {
        z: 8,
        a: 15,
        state: String::new(),
        half_life_s: Some(half_life),
        production_rate: r,
        decay_modes: vec![DecayMode {
            mode: "β+".into(),
            daughter_z: Some(7),
            daughter_a: Some(15),
            daughter_state: String::new(),
            branching: 1.0,
        }],
    }];

    let sol = solve_chain(&chain, irr, cool, 0.0, n_pts, None, 1.0);

    let lam = std::f64::consts::LN_2 / half_life;
    let mut max_rel = 0.0_f64;
    let mut max_t_at = 0.0;
    let mut sample_lines = Vec::new();

    for (k, &t) in sol.time_grid_s.iter().enumerate() {
        let act_solver = sol.activities[0][k];
        let act_ref = if t <= irr {
            r * (1.0 - (-lam * t).exp())
        } else {
            r * (1.0 - (-lam * irr).exp()) * (-lam * (t - irr)).exp()
        };
        let re = rel_err(act_solver, act_ref);
        if re > max_rel {
            max_rel = re;
            max_t_at = t;
        }
        if k % 25 == 0 || k < 5 || k == sol.time_grid_s.len() - 1 {
            sample_lines.push(format!(
                "  t={:8.1}s  solver={:11.4e}  bateman={:11.4e}  rel_err={:.3e}",
                t, act_solver, act_ref, re
            ));
        }
    }

    for l in sample_lines {
        println!("{}", l);
    }
    println!(
        "  --> max_rel_err = {:.3e} at t = {:.1}s",
        max_rel, max_t_at
    );
    if max_rel > 1e-3 {
        println!("  *** STEP-FUNCTION REGRESSION CONFIRMED on 1-isotope chain ***");
    } else {
        println!("  --> 1-isotope chain matches Bateman; bug needs ≥2 isotopes");
    }
}

/// Numerical reference: RK4-integrate the coupled ODEs for Mo-99 → Tc-99m
/// independently of solve_chain. This is a ground-truth check that both
/// validates the analytical Bateman formula AND tests solve_chain.
fn rk4_mo99(
    r_mo: f64,
    lam_mo: f64,
    lam_tc: f64,
    br: f64,
    irr_s: f64,
    cool_s: f64,
    n_steps_per_phase: usize,
) -> Vec<(f64, f64, f64)> {
    let mut out = Vec::new();
    let dt_irr = irr_s / n_steps_per_phase as f64;
    let dt_cool = cool_s / n_steps_per_phase as f64;

    let f = |n_mo: f64, n_tc: f64, irradiating: bool| {
        let r = if irradiating { r_mo } else { 0.0 };
        let dn_mo = r - lam_mo * n_mo;
        let dn_tc = lam_mo * br * n_mo - lam_tc * n_tc;
        (dn_mo, dn_tc)
    };

    let mut n_mo = 0.0;
    let mut n_tc = 0.0;
    out.push((0.0, 0.0, 0.0));
    let mut t = 0.0;

    // Irradiation
    for _ in 0..n_steps_per_phase {
        let (k1_mo, k1_tc) = f(n_mo, n_tc, true);
        let (k2_mo, k2_tc) = f(n_mo + 0.5 * dt_irr * k1_mo, n_tc + 0.5 * dt_irr * k1_tc, true);
        let (k3_mo, k3_tc) = f(n_mo + 0.5 * dt_irr * k2_mo, n_tc + 0.5 * dt_irr * k2_tc, true);
        let (k4_mo, k4_tc) = f(n_mo + dt_irr * k3_mo, n_tc + dt_irr * k3_tc, true);
        n_mo += dt_irr * (k1_mo + 2.0 * k2_mo + 2.0 * k3_mo + k4_mo) / 6.0;
        n_tc += dt_irr * (k1_tc + 2.0 * k2_tc + 2.0 * k3_tc + k4_tc) / 6.0;
        t += dt_irr;
        out.push((t, lam_mo * n_mo, lam_tc * n_tc));
    }
    // Cooling
    for _ in 0..n_steps_per_phase {
        let (k1_mo, k1_tc) = f(n_mo, n_tc, false);
        let (k2_mo, k2_tc) =
            f(n_mo + 0.5 * dt_cool * k1_mo, n_tc + 0.5 * dt_cool * k1_tc, false);
        let (k3_mo, k3_tc) =
            f(n_mo + 0.5 * dt_cool * k2_mo, n_tc + 0.5 * dt_cool * k2_tc, false);
        let (k4_mo, k4_tc) =
            f(n_mo + dt_cool * k3_mo, n_tc + dt_cool * k3_tc, false);
        n_mo += dt_cool * (k1_mo + 2.0 * k2_mo + 2.0 * k3_mo + k4_mo) / 6.0;
        n_tc += dt_cool * (k1_tc + 2.0 * k2_tc + 2.0 * k3_tc + k4_tc) / 6.0;
        t += dt_cool;
        out.push((t, lam_mo * n_mo, lam_tc * n_tc));
    }
    out
}

fn run_mo99() {
    println!("\n=== Experiment 2: ⁹⁹Mo → ⁹⁹ᵐTc (BR 0.876) ===");
    // Mo-99 t½ = 65.94 h; Tc-99m t½ = 6.0067 h.
    let mo_half = 65.94 * 3600.0;
    let tc_half = 6.0067 * 3600.0;
    let br_to_meta = 0.876;

    let irr = 24.0 * 3600.0; // 24h irradiation
    let cool = 24.0 * 3600.0; // 24h cool
    let n_pts = 400;
    let r_mo = 1.0e9; // atoms/s into Mo-99 (direct cyclotron production)

    let chain = vec![
        ChainIsotope {
            z: 42,
            a: 99,
            state: String::new(),
            half_life_s: Some(mo_half),
            production_rate: r_mo,
            decay_modes: vec![DecayMode {
                mode: "β-".into(),
                daughter_z: Some(43),
                daughter_a: Some(99),
                daughter_state: "m".into(),
                branching: br_to_meta,
            }],
        },
        ChainIsotope {
            z: 43,
            a: 99,
            state: "m".into(),
            half_life_s: Some(tc_half),
            production_rate: 0.0,
            decay_modes: vec![DecayMode {
                mode: "IT".into(),
                daughter_z: Some(43),
                daughter_a: Some(99),
                daughter_state: String::new(),
                branching: 1.0,
            }],
        },
    ];

    let sol = solve_chain(&chain, irr, cool, 0.0, n_pts, None, 1.0);

    // Analytical: Mo-99 obeys single-isotope Bateman (loss to ground BR=0.124
    // not in chain → no extra sink in our 2-iso model; Mo decay rate = full λ_Mo)
    // Activity_Mo(t) = R_Mo * (1 - exp(-λ_Mo·t)) during irr,
    //                  A_Mo(EOI) * exp(-λ_Mo·(t-T_irr)) during cooling.
    // Activity_Tc(t) = ingrowth from Mo · BR + decay.
    //
    // Two-isotope Bateman during irradiation (parent A_p = R(1-e^{-λ_p t}),
    // daughter ingrowth from constant production at rate R_p · BR plus chain
    // ingrowth from decaying parent stockpile).
    //
    // Closed form for Tc-99m from Mo-99 (constant R_Mo, zero direct R_Tc):
    // A_Tc(t≤T_irr) = BR · R_Mo · λ_Tc/(λ_Tc - λ_Mo) ·
    //                  [(1 - exp(-λ_Tc·t)) - λ_Mo/λ_Tc · (1 - exp(-λ_Mo·t))]
    let lam_mo = std::f64::consts::LN_2 / mo_half;
    let lam_tc = std::f64::consts::LN_2 / tc_half;

    let a_tc_irr = |t: f64| -> f64 {
        let r = r_mo * br_to_meta;
        let bracket = (1.0 - (-lam_tc * t).exp())
            - (lam_mo / lam_tc) * (1.0 - (-lam_mo * t).exp());
        r * lam_tc / (lam_tc - lam_mo) * bracket
    };

    let mo_eoi_ref = r_mo * (1.0 - (-lam_mo * irr).exp());
    println!(
        "  EOI activities: Mo-99 solver={:.4e} ref={:.4e} | Tc-99m solver={:.4e} ref={:.4e}",
        sol.activities[0][n_pts / 2 - 1],
        mo_eoi_ref,
        sol.activities[1][n_pts / 2 - 1],
        a_tc_irr(irr),
    );

    // Independent RK4 reference
    let rk4 = rk4_mo99(r_mo, lam_mo, lam_tc, br_to_meta, irr, cool, 50_000);

    // Sample Tc-99m at several irradiation times
    println!("  Tc-99m through irradiation: comparing solver vs analytical-Bateman vs RK4");
    println!("    {:>5}  {:>11}  {:>11}  {:>11}", "t/h", "solver", "bateman_fn", "rk4_truth");
    for &t_h in &[1.0_f64, 4.0, 8.0, 12.0, 24.0] {
        let t = t_h * 3600.0;
        let idx = sol
            .time_grid_s
            .iter()
            .position(|&x| x >= t)
            .unwrap_or(sol.time_grid_s.len() - 1);
        let rk4_idx = rk4
            .iter()
            .position(|&(tt, _, _)| tt >= t)
            .unwrap_or(rk4.len() - 1);
        let act_solver = sol.activities[1][idx];
        let act_bat = a_tc_irr(t);
        let act_rk4 = rk4[rk4_idx].2;
        println!(
            "    {:5.1}  {:11.4e}  {:11.4e}  {:11.4e}",
            t_h, act_solver, act_bat, act_rk4
        );
    }
}

fn main() {
    run_o15();
    run_mo99();
}

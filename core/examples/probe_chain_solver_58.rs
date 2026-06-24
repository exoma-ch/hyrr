//! Probe for hyrr issue #58: does `solve_chain` produce step-function activities
//! for short-lived isotopes because `matrix_exp` on the augmented 2×2 matrix
//! `[[-λ·dt, dt], [0, 0]]` returns garbage?
//!
//! Three experiments:
//!   (1) For ¹⁵O at dt = 72 s, compare e_aug[1] (top-right entry) against the
//!       closed-form φ₁(-λ·dt)·dt = (exp(-λ·dt) − 1) / (-λ).
//!   (2) Sweep dt ∈ {0.5, 5, 50, 500, 5000} s — find at what scale (if any)
//!       the augmented-matrix exponential breaks down.
//!   (3) Drive a 1-isotope "chain" by stepping the ODE dN/dt = -λN + R via the
//!       same `step_irradiation` path used in `chains.rs::solve_chain`, and
//!       compare A(t) = λ·N(t) against the analytical Bateman curve at every
//!       grid point. Report max relative error.

use hyrr_core::bateman::bateman_activity;
use hyrr_core::matrix_exp::{mat_vec_mul, matrix_exp};

const LN2: f64 = std::f64::consts::LN_2;

/// Re-implementation of `chains.rs::step_irradiation` (private there) so the
/// probe exercises the exact same call sequence: scaled A, matrix_exp(A·dt),
/// then matrix_exp on the 2n×2n augmented matrix to recover ∫exp(A·s)R ds.
fn step_irradiation(a: &[f64], r: &[f64], n_state: &[f64], dt: f64, n: usize) -> Vec<f64> {
    let a_dt: Vec<f64> = a.iter().map(|&x| x * dt).collect();
    let ea = matrix_exp(&a_dt, n);
    let mut result = mat_vec_mul(&ea, n_state, n);

    let m = 2 * n;
    let mut aug_m = vec![0.0; m * m];
    for i in 0..n {
        for j in 0..n {
            aug_m[i * m + j] = a[i * n + j] * dt;
        }
    }
    for i in 0..n {
        aug_m[i * m + (n + i)] = dt;
    }

    let e_aug = matrix_exp(&aug_m, m);

    for i in 0..n {
        for j in 0..n {
            result[i] += e_aug[i * m + (n + j)] * r[j];
        }
    }

    result
}

/// Build the 2×2 augmented matrix [[-λ·dt, dt], [0, 0]] used inside
/// step_irradiation for a 1-isotope chain.
fn aug_2x2(lambda: f64, dt: f64) -> Vec<f64> {
    vec![-lambda * dt, dt, 0.0, 0.0]
}

/// Closed form for φ₁(-λ·dt)·dt = (1 − exp(-λ·dt)) / λ.
fn phi1_dt_closed(lambda: f64, dt: f64) -> f64 {
    if lambda == 0.0 {
        dt
    } else {
        (1.0 - (-lambda * dt).exp()) / lambda
    }
}

fn norm1(m: &[f64], n: usize) -> f64 {
    let mut max_col = 0.0_f64;
    for j in 0..n {
        let mut s = 0.0;
        for i in 0..n {
            s += m[i * n + j].abs();
        }
        if s > max_col {
            max_col = s;
        }
    }
    max_col
}

fn print_aug_result(label: &str, lambda: f64, dt: f64) {
    let aug = aug_2x2(lambda, dt);
    let n1 = norm1(&aug, 2);
    let e = matrix_exp(&aug, 2);
    let expected_top_right = phi1_dt_closed(lambda, dt);
    let expected_top_left = (-lambda * dt).exp();

    let rel_err_top_right = if expected_top_right.abs() > 0.0 {
        (e[1] - expected_top_right).abs() / expected_top_right.abs()
    } else {
        e[1].abs()
    };
    let rel_err_top_left = if expected_top_left.abs() > 0.0 {
        (e[0] - expected_top_left).abs() / expected_top_left.abs()
    } else {
        e[0].abs()
    };

    println!("{label}  λ·dt={:.4e}  ‖aug‖₁={:.3e}", lambda * dt, n1);
    println!(
        "  e_aug = [[{:.6e}, {:.6e}], [{:.6e}, {:.6e}]]",
        e[0], e[1], e[2], e[3]
    );
    println!(
        "  expected top-left  exp(-λ·dt)        = {:.6e}   got {:.6e}   rel_err={:.2e}",
        expected_top_left, e[0], rel_err_top_left
    );
    println!(
        "  expected top-right (1-exp(-λ·dt))/λ = {:.6e}   got {:.6e}   rel_err={:.2e}",
        expected_top_right, e[1], rel_err_top_right
    );
    println!(
        "  expected bottom-row [0, 1]; got [{:.3e}, {:.6e}]",
        e[2], e[3]
    );
}

fn main() {
    let half_life = 122.24_f64; // O-15
    let lambda = LN2 / half_life;

    println!("=== EXPERIMENT 1: augmented 2×2 at dt = 72 s (O-15) ===");
    print_aug_result("[O-15, dt=72]", lambda, 72.0);

    println!("\n=== EXPERIMENT 2: dt sweep on the augmented 2×2 (O-15) ===");
    for &dt in &[0.5_f64, 5.0, 50.0, 500.0, 5000.0] {
        print_aug_result(&format!("[dt={dt}]"), lambda, dt);
    }

    println!("\n=== EXPERIMENT 3: 1-isotope step-loop vs analytical Bateman ===");
    let r = 1.294e11_f64;
    let irr = 7200.0_f64; // 2 h
    let cool = 3600.0_f64; // 1 h
    let n_time_points = 200_usize;

    // Replicate the linspace half-half grid used by solve_chain.
    let n_irr = n_time_points / 2;
    let n_cool = n_time_points - n_irr;
    let t_irr: Vec<f64> = (0..n_irr)
        .map(|i| i as f64 * irr / (n_irr - 1) as f64)
        .collect();
    let t_cool: Vec<f64> = (1..=n_cool)
        .map(|i| irr + i as f64 * cool / n_cool as f64)
        .collect();
    let mut time_grid = Vec::with_capacity(n_irr + n_cool);
    time_grid.extend_from_slice(&t_irr);
    time_grid.extend_from_slice(&t_cool);

    // 1-isotope decay matrix and production rate (units: atoms / s).
    let a_mat = vec![-lambda];
    let r_vec = vec![r];

    let mut n_state = vec![0.0_f64];
    let mut abundances = vec![0.0_f64; time_grid.len()];

    // Irradiation phase
    for ti in 1..n_irr {
        let dt = t_irr[ti] - t_irr[ti - 1];
        n_state = step_irradiation(&a_mat, &r_vec, &n_state, dt, 1);
        if n_state[0] < 0.0 {
            n_state[0] = 0.0;
        }
        abundances[ti] = n_state[0];
    }
    // Cooling phase: pure decay (matches solve_chain's cooling branch).
    for ti in 0..n_cool {
        let t_prev = if ti == 0 { irr } else { t_cool[ti - 1] };
        let dt = t_cool[ti] - t_prev;
        let a_dt = vec![-lambda * dt];
        let ea = matrix_exp(&a_dt, 1);
        n_state[0] *= ea[0];
        if n_state[0] < 0.0 {
            n_state[0] = 0.0;
        }
        abundances[n_irr + ti] = n_state[0];
    }

    // Compare to bateman_activity on the same grid.
    let bat = bateman_activity(r, Some(half_life), irr, cool, n_time_points);

    let mut max_abs_err = 0.0_f64;
    let mut max_rel_err = 0.0_f64;
    let mut max_rel_idx = 0usize;
    println!(
        "  {:>3}  {:>9}  {:>12}  {:>12}  {:>10}",
        "i", "t (s)", "A_chain", "A_bateman", "rel_err"
    );
    for i in 0..time_grid.len() {
        let a_chain = lambda * abundances[i];
        let a_ref = bat.activity[i];
        let abs_err = (a_chain - a_ref).abs();
        let rel = if a_ref.abs() > 1e-30 {
            abs_err / a_ref.abs()
        } else {
            0.0
        };
        if abs_err > max_abs_err {
            max_abs_err = abs_err;
        }
        if rel > max_rel_err {
            max_rel_err = rel;
            max_rel_idx = i;
        }
        if [0_usize, 1, 2, 5, 10, 50, 98, 99, 100, 101, 150, 199].contains(&i) {
            println!(
                "  {:>3}  {:>9.2}  {:>12.4e}  {:>12.4e}  {:>10.2e}",
                i, time_grid[i], a_chain, a_ref, rel
            );
        }
    }
    println!(
        "  max_abs_err = {:.4e}   max_rel_err = {:.4e}  at i={} t={:.2}s",
        max_abs_err, max_rel_err, max_rel_idx, time_grid[max_rel_idx]
    );
}

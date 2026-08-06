//! Decay chain discovery and coupled ODE solver.
//!
//! BFS chain discovery, topological sort, matrix exponential solution
//! for coupled decay+production equations, piecewise current profiles.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

use crate::interpolation::linspace;
use crate::matrix_exp::{mat_vec_mul, matrix_exp};
use crate::types::{ChainIsotope, ChainSolution, CurrentProfile};

use crate::db::DatabaseProtocol;

/// Discover full decay chains from directly-produced isotopes via BFS.
/// Returns isotopes in topological order (parents before daughters).
pub fn discover_chains(
    db: &dyn DatabaseProtocol,
    direct_isotopes: &[(u32, u32, String, f64)],
    max_depth: usize,
) -> Vec<ChainIsotope> {
    let mut isotope_map: HashMap<String, ChainIsotope> = HashMap::new();
    let mut queue: Vec<(u32, u32, String, usize)> = Vec::new();

    // Seed with directly-produced isotopes
    for (z, a, state, rate) in direct_isotopes {
        let key = format!("{}-{}-{}", z, a, state);
        if let Some(existing) = isotope_map.get_mut(&key) {
            existing.production_rate += rate;
        } else {
            let decay = db.get_decay_data(*z, *a, state);
            let (half_life, modes) = match decay {
                Some(d) => (d.half_life_s, d.decay_modes.clone()),
                None => (None, Vec::new()),
            };
            isotope_map.insert(
                key,
                ChainIsotope {
                    z: *z,
                    a: *a,
                    state: state.clone(),
                    half_life_s: half_life,
                    production_rate: *rate,
                    decay_modes: modes,
                },
            );
            queue.push((*z, *a, state.clone(), 0));
        }
    }

    // BFS through daughters
    let mut qi = 0;
    while qi < queue.len() {
        let (z, a, state, depth) = queue[qi].clone();
        qi += 1;
        if depth >= max_depth {
            continue;
        }

        let parent_key = format!("{}-{}-{}", z, a, state);
        let parent = isotope_map.get(&parent_key).unwrap().clone();
        if parent.is_stable() {
            continue;
        }

        for mode in &parent.decay_modes {
            if mode.daughter_z.is_none() || mode.daughter_a.is_none() {
                continue;
            }
            if mode.mode == "stable" {
                continue;
            }

            let dz = mode.daughter_z.unwrap();
            let da = mode.daughter_a.unwrap();
            let ds = &mode.daughter_state;
            let dkey = format!("{}-{}-{}", dz, da, ds);

            if let std::collections::hash_map::Entry::Vacant(e) = isotope_map.entry(dkey) {
                let decay = db.get_decay_data(dz, da, ds);
                let (half_life, modes) = match decay {
                    Some(d) => (d.half_life_s, d.decay_modes.clone()),
                    None => (None, Vec::new()),
                };
                e.insert(ChainIsotope {
                    z: dz,
                    a: da,
                    state: ds.clone(),
                    half_life_s: half_life,
                    production_rate: 0.0,
                    decay_modes: modes,
                });
                queue.push((dz, da, ds.clone(), depth + 1));
            }
        }
    }

    topological_sort(&isotope_map)
}

fn topological_sort(isotope_map: &HashMap<String, ChainIsotope>) -> Vec<ChainIsotope> {
    let mut children: HashMap<String, HashSet<String>> = HashMap::new();
    let mut in_degree: HashMap<String, usize> = HashMap::new();

    for key in isotope_map.keys() {
        children.insert(key.clone(), HashSet::new());
        in_degree.insert(key.clone(), 0);
    }

    for (key, iso) in isotope_map {
        for mode in &iso.decay_modes {
            if mode.daughter_z.is_none() || mode.daughter_a.is_none() {
                continue;
            }
            let dkey = format!(
                "{}-{}-{}",
                mode.daughter_z.unwrap(),
                mode.daughter_a.unwrap(),
                mode.daughter_state
            );
            if isotope_map.contains_key(&dkey) {
                children.get_mut(key).unwrap().insert(dkey.clone());
                *in_degree.get_mut(&dkey).unwrap() += 1;
            }
        }
    }

    // Kahn's algorithm
    let mut queue: VecDeque<String> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(k, _)| k.clone())
        .collect();

    let mut result: Vec<ChainIsotope> = Vec::new();
    while let Some(key) = queue.pop_front() {
        result.push(isotope_map.get(&key).unwrap().clone());
        for child in children.get(&key).unwrap() {
            let deg = in_degree.get_mut(child).unwrap();
            *deg -= 1;
            if *deg == 0 {
                queue.push_back(child.clone());
            }
        }
    }

    // Cycle fallback
    if result.len() < isotope_map.len() {
        for iso in isotope_map.values() {
            if !result.iter().any(|r| r.key() == iso.key()) {
                result.push(iso.clone());
            }
        }
    }

    result
}

/// Step the irradiation ODE: dN/dt = A*N + R for time dt.
///
/// Uses augmented matrix approach for numerical stability.
fn step_irradiation(a: &[f64], r: &[f64], n_state: &[f64], dt: f64, n: usize) -> Vec<f64> {
    let a_dt = scale_flat(a, dt, n);
    let ea = matrix_exp(&a_dt, n);

    // Decay existing atoms: N_decay = exp(A*dt) * N
    let mut result = mat_vec_mul(&ea, n_state, n);

    // Production integral via augmented matrix
    let m = 2 * n;
    let mut aug_m = vec![0.0; m * m];
    // Top-left: A*dt
    for i in 0..n {
        for j in 0..n {
            aug_m[i * m + j] = a[i * n + j] * dt;
        }
    }
    // Top-right: I*dt
    for i in 0..n {
        aug_m[i * m + (n + i)] = dt;
    }

    let e_aug = matrix_exp(&aug_m, m);

    // Extract phi₁(A*dt)*dt from top-right block and multiply by R
    for i in 0..n {
        for j in 0..n {
            result[i] += e_aug[i * m + (n + j)] * r[j];
        }
    }

    result
}

fn scale_flat(m: &[f64], t: f64, _n: usize) -> Vec<f64> {
    m.iter().map(|&x| x * t).collect()
}

/// Solve coupled decay chain equations using matrix exponential.
///
/// Wrapper for [`solve_chain_at_times`] that builds the classic linspace grid
/// (half the points across irradiation, half across cooling). The point-query
/// path (`get_activity_at`, #570) calls `solve_chain_at_times` directly with a
/// caller-supplied time set, so both surfaces run through the same solver and
/// a query at a grid time equals the curve value there by construction.
pub fn solve_chain(
    chain: &[ChainIsotope],
    irradiation_time_s: f64,
    cooling_time_s: f64,
    _beam_particles_per_s: f64,
    n_time_points: usize,
    current_profile: Option<&CurrentProfile>,
    nominal_current_ma: f64,
) -> ChainSolution {
    if chain.is_empty() {
        return ChainSolution {
            isotopes: Vec::new(),
            time_grid_s: Vec::new(),
            abundances: Vec::new(),
            activities: Vec::new(),
            activities_direct: Vec::new(),
            activities_ingrowth: Vec::new(),
            parent_info: Vec::new(),
        };
    }

    // Build the classic split-linspace grid: n_irr points from 0 → irr, then
    // n_cool points strictly after irr → irr+cool. Identical to the pre-#570
    // layout; the last irradiation-phase sample is exactly `irradiation_time_s`.
    // linspace(0, irr, n) can overshoot the endpoint by 1 ULP; snap the last
    // irradiation sample to exactly `irradiation_time_s` so the new
    // time-based split rule in solve_chain_at_times lands it in the
    // irradiation phase (matches the old index-based bucketing).
    let n_irr = n_time_points / 2;
    let n_cool = n_time_points - n_irr;
    let mut t_irr = linspace(0.0, irradiation_time_s, n_irr);
    if let Some(last) = t_irr.last_mut() {
        *last = irradiation_time_s;
    }
    let t_cool_full = linspace(
        irradiation_time_s,
        irradiation_time_s + cooling_time_s,
        n_cool + 1,
    );
    let t_cool: Vec<f64> = t_cool_full[1..].to_vec();

    let mut all_times = Vec::with_capacity(n_irr + n_cool);
    all_times.extend_from_slice(&t_irr);
    all_times.extend_from_slice(&t_cool);

    solve_chain_at_times(
        chain,
        irradiation_time_s,
        &all_times,
        current_profile,
        nominal_current_ma,
    )
}

/// Solve the coupled decay chain at caller-specified output times (#570).
///
/// This is the general form — [`solve_chain`] delegates here after building a
/// linspace grid. A point query on the same chain returns the exact Bateman
/// value at each requested `t`, matching the curve's value at grid times to
/// f64 round-off; a query between grid points is the *analytic* value, not an
/// interpolation of the coarse grid.
///
/// `output_times_s` may contain any non-negative times (irradiation OR cooling),
/// in any order, with duplicates allowed. The returned per-isotope arrays are
/// aligned index-for-index to `output_times_s`. Times inside irradiation are
/// solved through the piecewise current-profile walker; times after are decayed
/// from the end-of-irradiation state via `exp(A · Δt)`.
pub fn solve_chain_at_times(
    chain: &[ChainIsotope],
    irradiation_time_s: f64,
    output_times_s: &[f64],
    current_profile: Option<&CurrentProfile>,
    nominal_current_ma: f64,
) -> ChainSolution {
    let n = chain.len();
    let n_t = output_times_s.len();
    if n == 0 {
        return ChainSolution {
            isotopes: Vec::new(),
            time_grid_s: output_times_s.to_vec(),
            abundances: Vec::new(),
            activities: Vec::new(),
            activities_direct: Vec::new(),
            activities_ingrowth: Vec::new(),
            parent_info: Vec::new(),
        };
    }

    // Build index map
    let idx: HashMap<String, usize> = chain
        .iter()
        .enumerate()
        .map(|(i, iso)| (iso.key(), i))
        .collect();

    // Identify isotopes whose half-life is so short that including them in
    // the matrix-exp would blow up the conditioning. The chain discovery
    // can pull in nuclear-prompt nuclides (e.g. ¹⁶F at t½ ≈ 1×10⁻²⁰ s — a
    // β-delayed-proton precursor of ¹⁵O); for any reasonable physical
    // timestep these have λ·dt ≫ 1, scaling-and-squaring needs > 60
    // squarings, and the surviving slow-mode entries are numerical
    // garbage. Treat them as instantaneous feed-through: their entire
    // production-rate flux is redirected to their daughters before the
    // matrix is built. Their reported activity is the analytical
    // saturation value (which is just R during irradiation, ~0 during
    // cooling — they decay in << one timestep).
    //
    // Threshold: 1 ms. Catches all nuclear-prompt species (10⁻²⁰ s …
    // µs scale) without sweeping up legitimate sub-second daughters
    // like ⁸Li (t½=0.84 s) or ⁸B (t½=0.77 s) which the matrix-exp
    // handles correctly.
    const INSTANTANEOUS_THRESHOLD_S: f64 = 1.0e-3;
    let is_instantaneous: Vec<bool> = chain
        .iter()
        .map(|iso| matches!(iso.half_life_s, Some(t) if t > 0.0 && t < INSTANTANEOUS_THRESHOLD_S))
        .collect();

    // Effective production-rate vector: redistribute each instantaneous
    // isotope's production into its daughters via the branching ratio.
    // The chain is topologically sorted, so iterating in order ensures
    // that an instantaneous isotope feeding another instantaneous
    // isotope is fully cascaded.
    let mut r_nominal: Vec<f64> = chain.iter().map(|iso| iso.production_rate).collect();
    let r_original_for_instant: Vec<f64> = r_nominal.clone();
    for i in 0..n {
        if !is_instantaneous[i] {
            continue;
        }
        let r_i = r_nominal[i];
        if r_i <= 0.0 {
            continue;
        }
        for mode in &chain[i].decay_modes {
            let (Some(dz), Some(da)) = (mode.daughter_z, mode.daughter_a) else {
                continue;
            };
            let dkey = format!("{}-{}-{}", dz, da, mode.daughter_state);
            if let Some(&j) = idx.get(&dkey) {
                r_nominal[j] += r_i * mode.branching;
            }
        }
        r_nominal[i] = 0.0;
    }

    // Build decay matrix A (n×n flat row-major). Skip instantaneous
    // isotopes — their rows and columns stay zero, which keeps them out
    // of the matrix-exp entirely while preserving their position in the
    // output vectors so downstream indexing is unaffected.
    let mut a_mat = vec![0.0; n * n];
    for (i, iso) in chain.iter().enumerate() {
        if iso.is_stable() || is_instantaneous[i] {
            continue;
        }
        let lam = iso.lambda();
        a_mat[i * n + i] = -lam;
        for mode in &iso.decay_modes {
            if mode.daughter_z.is_none() || mode.daughter_a.is_none() {
                continue;
            }
            let dkey = format!(
                "{}-{}-{}",
                mode.daughter_z.unwrap(),
                mode.daughter_a.unwrap(),
                mode.daughter_state
            );
            if let Some(&j) = idx.get(&dkey) {
                a_mat[j * n + i] += lam * mode.branching;
            }
        }
    }

    // Split output times into irradiation and cooling phases. `t == irr` goes
    // in the irradiation bucket to match solve_chain's pre-#570 semantics
    // (linspace(0, irr, n_irr) puts the last sample exactly at irr).
    let mut irr_out_times: Vec<f64> = Vec::new();
    let mut irr_out_slots: Vec<usize> = Vec::new();
    let mut cool_out_slots: Vec<(usize, f64)> = Vec::new();
    for (i, &t) in output_times_s.iter().enumerate() {
        if t <= irradiation_time_s {
            irr_out_times.push(t);
            irr_out_slots.push(i);
        } else {
            cool_out_slots.push((i, t));
        }
    }

    let mut abundances: Vec<Vec<f64>> = (0..n).map(|_| vec![0.0; n_t]).collect();
    let mut activities: Vec<Vec<f64>> = (0..n).map(|_| vec![0.0; n_t]).collect();

    // --- Irradiation phase ---
    let (n_eoi, irr_abund) = walk_irradiation_at_times(
        &a_mat,
        &r_nominal,
        n,
        irradiation_time_s,
        &irr_out_times,
        current_profile,
        nominal_current_ma,
    );
    for (k, &out_idx) in irr_out_slots.iter().enumerate() {
        for i in 0..n {
            abundances[i][out_idx] = irr_abund[i][k];
        }
    }

    // --- Cooling phase --- decay each requested time independently from n_eoi.
    // No production term, so state at t is `exp(A · (t - t_irr)) · n_eoi`;
    // computing per-request keeps the code obvious and lets the caller ask
    // for out-of-order or duplicate cooling times without extra machinery.
    for &(out_idx, t) in &cool_out_slots {
        let dt = t - irradiation_time_s;
        if dt <= 0.0 {
            for i in 0..n {
                abundances[i][out_idx] = n_eoi[i].max(0.0);
            }
        } else {
            let a_dt = scale_flat(&a_mat, dt, n);
            let ea = matrix_exp(&a_dt, n);
            let n_state = mat_vec_mul(&ea, &n_eoi, n);
            for i in 0..n {
                abundances[i][out_idx] = n_state[i].max(0.0);
            }
        }
    }

    // Compute activities. The previous implementation clamped abundances
    // to a global ceiling (Σ R · t_irr) and a per-daughter analytical
    // transient-equilibrium ceiling — both bandaids for a chain-solver
    // that produced numerical garbage when nuclear-prompt isotopes
    // dragged ‖A·dt‖ to ~10²¹. With those isotopes now collapsed to
    // instantaneous feed-through above, the matrix-exp output matches
    // analytical Bateman / RK4 to f64 round-off and the ceilings only
    // suppress correct results (they'd cap Tc-99m below transient-eq
    // build-up before the chain reaches secular equilibrium).
    for (i, iso) in chain.iter().enumerate() {
        if iso.is_stable() {
            continue;
        }
        if is_instantaneous[i] {
            // Instantaneous isotope: activity ≡ R during irradiation
            // (saturation reached in << dt), 0 during cooling. Abundance
            // stays at its analytical equilibrium value R/λ — which is
            // negligible for any nuclear-prompt species but mathematically
            // consistent so downstream code doesn't see a NaN/zero
            // surprise. Compare against irradiation_time_s directly on the
            // caller's own time — no linspace-slop indexing.
            let r_orig = r_original_for_instant[i];
            let lam = iso.lambda();
            let n_eq = if lam > 0.0 { r_orig / lam } else { 0.0 };
            for (k, &t) in output_times_s.iter().enumerate() {
                if t <= irradiation_time_s {
                    abundances[i][k] = n_eq;
                    activities[i][k] = r_orig;
                } else {
                    abundances[i][k] = 0.0;
                    activities[i][k] = 0.0;
                }
            }
            continue;
        }
        let lam = iso.lambda();
        for t in 0..n_t {
            activities[i][t] = lam * abundances[i][t];
        }
    }

    // --- Direct component ---
    let activities_direct = compute_direct_component_at_times(
        chain,
        output_times_s,
        irradiation_time_s,
        current_profile,
        nominal_current_ma,
    );

    // Ingrowth = total - direct (clamp >= 0)
    let activities_ingrowth: Vec<Vec<f64>> = (0..n)
        .map(|i| {
            (0..n_t)
                .map(|t| (activities[i][t] - activities_direct[i][t]).max(0.0))
                .collect()
        })
        .collect();

    // Build parent-info: for each daughter index, record every parent in the
    // chain that feeds into it via a decay mode, together with the branching
    // ratio and the raw decay-mode label. Outer index matches `chain[i]`.
    let mut parent_info: Vec<Vec<(String, f64, String)>> = vec![Vec::new(); n];
    for parent in chain.iter() {
        for mode in &parent.decay_modes {
            let (Some(dz), Some(da)) = (mode.daughter_z, mode.daughter_a) else {
                continue;
            };
            if mode.mode == "stable" {
                continue;
            }
            let dkey = format!("{}-{}-{}", dz, da, mode.daughter_state);
            if let Some(&di) = idx.get(&dkey) {
                parent_info[di].push((parent.key(), mode.branching, mode.mode.clone()));
            }
        }
    }

    ChainSolution {
        isotopes: chain.to_vec(),
        time_grid_s: output_times_s.to_vec(),
        abundances,
        activities,
        activities_direct,
        activities_ingrowth,
        parent_info,
    }
}

/// Walk the irradiation phase to caller-specified output times, always stepping
/// fully to `irradiation_time_s` so `n_eoi` is exact. Merges intervals from the
/// (optional) current profile with output times so the piecewise-constant
/// production stays honest across the whole window; a `None` profile is treated
/// as a single interval at nominal current (scale=1 everywhere).
///
/// Returns `(n_eoi, abundances[iso][output_slot])`, where `abundances[i][k]` is
/// the concentration of isotope `i` at `times_in_irr[k]`.
fn walk_irradiation_at_times(
    a: &[f64],
    r_nominal: &[f64],
    n: usize,
    irradiation_time_s: f64,
    times_in_irr: &[f64],
    current_profile: Option<&CurrentProfile>,
    nominal_current_ma: f64,
) -> (Vec<f64>, Vec<Vec<f64>>) {
    let n_out = times_in_irr.len();
    let mut abundances: Vec<Vec<f64>> = (0..n).map(|_| vec![0.0; n_out]).collect();

    // Map requested time → output slot(s) via bit-key so equal f64s collide.
    let mut output_idx: HashMap<u64, Vec<usize>> = HashMap::new();
    for (ti, &t) in times_in_irr.iter().enumerate() {
        output_idx.entry(t.to_bits()).or_default().push(ti);
    }

    let intervals = current_profile.map(|cp| cp.intervals(irradiation_time_s));

    // Boundary set: 0, irr_time, profile boundaries, and every requested
    // time in [0, irr_time]. Sorted + deduplicated via BTreeSet<u64>.
    let mut boundary_set: BTreeSet<u64> = BTreeSet::new();
    boundary_set.insert(0.0_f64.to_bits());
    boundary_set.insert(irradiation_time_s.to_bits());
    if let Some(iv) = &intervals {
        for &(s, e, _) in iv {
            boundary_set.insert(s.to_bits());
            boundary_set.insert(e.to_bits());
        }
    }
    for &t in times_in_irr {
        if t >= 0.0 && t <= irradiation_time_s {
            boundary_set.insert(t.to_bits());
        }
    }
    let all_times: Vec<f64> = boundary_set.iter().map(|&b| f64::from_bits(b)).collect();

    let mut n_state = vec![0.0; n];

    // Record t=0 for any output slot requesting it (n_state is zero here).
    if let Some(indices) = output_idx.get(&0.0_f64.to_bits()) {
        for &ti in indices {
            for i in 0..n {
                abundances[i][ti] = 0.0;
            }
        }
    }

    let mut iv_idx = 0usize;
    let mut prev_t = 0.0;

    for &t_next in &all_times {
        if t_next <= 0.0 {
            continue;
        }
        let dt = t_next - prev_t;
        if dt <= 0.0 {
            prev_t = t_next;
            continue;
        }

        let scale = match &intervals {
            Some(iv) => {
                while iv_idx < iv.len() - 1 && iv[iv_idx].1 <= prev_t {
                    iv_idx += 1;
                }
                if nominal_current_ma > 0.0 {
                    iv[iv_idx].2 / nominal_current_ma
                } else {
                    0.0
                }
            }
            None => 1.0,
        };

        let r_scaled: Vec<f64> = r_nominal.iter().map(|&r| r * scale).collect();
        n_state = step_irradiation(a, &r_scaled, &n_state, dt, n);

        if let Some(indices) = output_idx.get(&t_next.to_bits()) {
            for &ti in indices {
                for i in 0..n {
                    abundances[i][ti] = n_state[i].max(0.0);
                }
            }
        }

        prev_t = t_next;
    }

    (n_state, abundances)
}

/// Direct component (no chain coupling) at caller-specified output times.
///
/// For each isotope with a non-zero production rate, computes what its
/// activity would be if it were the *only* isotope in the chain — i.e.
/// production against its own decay, no daughters/parents. Then
/// `activities_ingrowth = activities - activities_direct` splits chain-fed
/// contributions cleanly.
///
/// Constant-current (None profile) uses the closed-form Bateman formula
/// evaluated pointwise at each requested `t`. The profile branch walks the
/// per-isotope ODE `dN/dt = R(t) - λN` across (profile boundaries ∪ requested
/// irradiation times), recording direct activity at each requested time; the
/// cooling phase decays analytically from `A(t_irr)`.
fn compute_direct_component_at_times(
    chain: &[ChainIsotope],
    output_times_s: &[f64],
    irradiation_time_s: f64,
    current_profile: Option<&CurrentProfile>,
    nominal_current_ma: f64,
) -> Vec<Vec<f64>> {
    let n = chain.len();
    let n_t = output_times_s.len();
    let mut activities_direct: Vec<Vec<f64>> = (0..n).map(|_| vec![0.0; n_t]).collect();

    for (i, iso) in chain.iter().enumerate() {
        if iso.production_rate <= 0.0 || iso.is_stable() {
            continue;
        }
        let lam = iso.lambda();

        if current_profile.is_none() {
            // Analytical Bateman (constant current) — evaluated directly at each
            // requested `t`, no grid interpolation.
            let a_eoi = iso.production_rate * (1.0 - (-lam * irradiation_time_s).exp());
            for (t_idx, &t) in output_times_s.iter().enumerate() {
                if t <= irradiation_time_s {
                    activities_direct[i][t_idx] = iso.production_rate * (1.0 - (-lam * t).exp());
                } else {
                    let dt_cool = t - irradiation_time_s;
                    activities_direct[i][t_idx] = a_eoi * (-lam * dt_cool).exp();
                }
            }
        } else {
            let profile = current_profile.unwrap();
            let intervals = profile.intervals(irradiation_time_s);

            // Per-request slots for this isotope's irradiation-phase output.
            // Sorted by time so we can walk boundaries once.
            let mut irr_outputs: Vec<(f64, usize)> = output_times_s
                .iter()
                .enumerate()
                .filter(|(_, &t)| t <= irradiation_time_s)
                .map(|(idx, &t)| (t, idx))
                .collect();
            irr_outputs.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal));

            // Merge boundaries: 0, irr_time, profile boundaries, requested times.
            let mut boundary_set: BTreeSet<u64> = BTreeSet::new();
            boundary_set.insert(0.0_f64.to_bits());
            boundary_set.insert(irradiation_time_s.to_bits());
            for &(s, e, _) in &intervals {
                boundary_set.insert(s.to_bits());
                boundary_set.insert(e.to_bits());
            }
            for &(t, _) in &irr_outputs {
                boundary_set.insert(t.to_bits());
            }
            let all_times: Vec<f64> = boundary_set.iter().map(|&b| f64::from_bits(b)).collect();

            let mut out_map: HashMap<u64, Vec<usize>> = HashMap::new();
            for &(t, ti) in &irr_outputs {
                out_map.entry(t.to_bits()).or_default().push(ti);
            }

            let mut n_val = 0.0_f64;
            let mut t_now = 0.0;
            let mut iv_idx = 0usize;

            for &t_next in &all_times {
                if t_next <= t_now {
                    continue;
                }
                let dt = t_next - t_now;
                if dt <= 0.0 {
                    continue;
                }

                while iv_idx < intervals.len() - 1 && intervals[iv_idx].1 <= t_now {
                    iv_idx += 1;
                }
                let scale = if nominal_current_ma > 0.0 {
                    intervals[iv_idx].2 / nominal_current_ma
                } else {
                    0.0
                };
                let r_t = iso.production_rate * scale;

                let exp_l_dt = (-lam * dt).exp();
                n_val = n_val * exp_l_dt + (r_t / lam) * (1.0 - exp_l_dt);
                t_now = t_next;

                if let Some(indices) = out_map.get(&t_now.to_bits()) {
                    for &ti in indices {
                        activities_direct[i][ti] = lam * n_val;
                    }
                }
            }

            // Cooling phase — analytical decay from EOI direct activity.
            let a_eoi = lam * n_val;
            for (t_idx, &t) in output_times_s.iter().enumerate() {
                if t > irradiation_time_s {
                    let dt_cool = t - irradiation_time_s;
                    activities_direct[i][t_idx] = a_eoi * (-lam * dt_cool).exp();
                }
            }
        }
    }

    activities_direct
}

/// Split discovered chain into connected components via undirected BFS.
pub fn split_components(chain: &[ChainIsotope]) -> Vec<Vec<ChainIsotope>> {
    let mut adj: HashMap<String, HashSet<String>> = HashMap::new();
    let mut iso_by_key: HashMap<String, &ChainIsotope> = HashMap::new();

    for iso in chain {
        let k = iso.key();
        iso_by_key.insert(k.clone(), iso);
        adj.entry(k.clone()).or_default();
        for mode in &iso.decay_modes {
            if mode.daughter_z.is_none() || mode.daughter_a.is_none() {
                continue;
            }
            let dk = format!(
                "{}-{}-{}",
                mode.daughter_z.unwrap(),
                mode.daughter_a.unwrap(),
                mode.daughter_state
            );
            adj.entry(dk.clone()).or_default();
            adj.get_mut(&k).unwrap().insert(dk.clone());
            adj.get_mut(&dk).unwrap().insert(k.clone());
        }
    }

    let mut visited = HashSet::new();
    let mut components = Vec::new();

    for iso in chain {
        let k = iso.key();
        if visited.contains(&k) {
            continue;
        }

        let mut component = Vec::new();
        let mut queue = VecDeque::new();
        queue.push_back(k.clone());
        visited.insert(k.clone());

        while let Some(cur) = queue.pop_front() {
            if let Some(&iso_ref) = iso_by_key.get(&cur) {
                component.push(iso_ref.clone());
            }
            if let Some(neighbors) = adj.get(&cur) {
                for neighbor in neighbors {
                    if !visited.contains(neighbor) {
                        visited.insert(neighbor.clone());
                        queue.push_back(neighbor.clone());
                    }
                }
            }
        }

        if !component.is_empty() {
            components.push(component);
        }
    }

    components
}

#[cfg(test)]
mod tests_at_times {
    //! Tests for `solve_chain_at_times` (#570). Data-free — synthetic chains
    //! and analytic Bateman as the ground truth.

    use super::*;
    use crate::types::DecayMode;

    /// A one-isotope "chain": production against its own decay. Analytic
    /// Bateman is the ground truth so we can hand-check every requested time.
    fn one_iso_chain(rate_per_s: f64, half_life_s: f64) -> Vec<ChainIsotope> {
        vec![ChainIsotope {
            z: 26, // Fe (label only; the solver never looks it up)
            a: 55,
            state: String::new(),
            half_life_s: Some(half_life_s),
            production_rate: rate_per_s,
            decay_modes: Vec::new(), // stable end
        }]
    }

    fn bateman_analytic(rate: f64, half_life: f64, irr: f64, t: f64) -> f64 {
        let lam = std::f64::consts::LN_2 / half_life;
        if t <= irr {
            rate * (1.0 - (-lam * t).exp())
        } else {
            let a_eoi = rate * (1.0 - (-lam * irr).exp());
            a_eoi * (-lam * (t - irr)).exp()
        }
    }

    #[test]
    fn point_query_matches_curve_at_grid_times() {
        // solve_chain builds a linspace grid; solve_chain_at_times evaluated
        // at that exact grid must agree elementwise (the whole "exactness"
        // guarantee — the curve and the point query share one solver).
        let chain = one_iso_chain(1.0e6, 3600.0);
        let irr = 7200.0;
        let cool = 3600.0;
        let curve = solve_chain(&chain, irr, cool, 0.0, 200, None, 1.0);
        let point = solve_chain_at_times(&chain, irr, &curve.time_grid_s, None, 1.0);
        for i in 0..curve.time_grid_s.len() {
            let a = curve.activities[0][i];
            let b = point.activities[0][i];
            let rel = if a.max(b) > 0.0 {
                (a - b).abs() / a.max(b)
            } else {
                0.0
            };
            assert!(
                rel < 1e-12,
                "grid time {i}: curve={a:.6e} point={b:.6e} rel={rel:.3e}",
            );
        }
    }

    #[test]
    fn point_query_returns_exact_analytic_bateman_between_grid_points() {
        // The whole point of the tool: reading a value between grid
        // points from an interpolated curve is only as good as the grid. The
        // point query evaluates Bateman analytically at any `t`.
        //
        // Fe-55 half-life ≈ 2.744 years — chosen so 200-grid interpolation
        // would clearly lose precision on a between-point query.
        let rate = 5.0e8;
        let hl = 2.744 * 365.25 * 86400.0;
        let chain = one_iso_chain(rate, hl);
        let irr = 86400.0 * 30.0; // 30 d irradiation
        let _cool = 86400.0 * 365.0; // 1 year cooling (kept for readability)

        // A gnarly non-grid time — 137.42 days into cooling.
        let t_probe = irr + 86400.0 * 137.42;
        let ref_activity = bateman_analytic(rate, hl, irr, t_probe);

        let sol = solve_chain_at_times(&chain, irr, &[t_probe], None, 1.0);
        let a = sol.activities[0][0];
        let rel = (a - ref_activity).abs() / ref_activity.max(1.0);
        assert!(
            rel < 1e-10,
            "arbitrary-time query must be analytic: got {a:.6e}, want {ref_activity:.6e} (rel {rel:.3e})",
        );

        // Sanity: for a one-isotope chain with no ingrowth path, direct and
        // total agree. Tolerance is generous — the matrix-exp uses scale-and-
        // square, the analytical formula uses one `expm1`, and the two paths
        // can differ by a handful of ULPs.
        let direct = sol.activities_direct[0][0];
        let rel_dt = (direct - a).abs() / a.max(direct).max(1.0);
        assert!(
            rel_dt < 1e-8,
            "one-isotope chain: direct ({direct:.6e}) vs total ({a:.6e}) rel {rel_dt:.3e}",
        );
        // Ingrowth for a one-isotope chain is definitionally zero (bounded by
        // (total - direct).max(0) = 0 to ~ULP of `a`).
        assert!(
            sol.activities_ingrowth[0][0] / a < 1e-8,
            "one-isotope chain: no ingrowth path — got {} (rel {})",
            sol.activities_ingrowth[0][0],
            sol.activities_ingrowth[0][0] / a,
        );
    }

    #[test]
    fn out_of_order_and_duplicate_times_are_honoured() {
        // The caller must be free to pass unsorted times with duplicates;
        // output stays aligned index-for-index to input.
        let chain = one_iso_chain(1.0e6, 1800.0);
        let irr = 3600.0;
        let times = vec![7200.0, 0.0, 3600.0, 7200.0, 1800.0];
        let sol = solve_chain_at_times(&chain, irr, &times, None, 1.0);
        // At t=0 activity is exactly 0; the two t=7200 samples must agree
        // (deterministic pure-decay evaluation).
        assert_eq!(sol.activities[0][1], 0.0);
        assert!(
            (sol.activities[0][0] - sol.activities[0][3]).abs() < 1e-12,
            "duplicate cooling times must produce identical activities",
        );
    }

    #[test]
    fn eob_query_at_exact_irradiation_time_gives_saturation_bateman() {
        // t == irradiation_time_s must count as EOB (not decayed) — matches
        // the split rule (`t <= irr` → irradiation phase). Regression for
        // the linspace-overshoot bug we fixed by snapping the last t_irr
        // sample; the point-query API doesn't have linspace so it just needs
        // the `<=` boundary to include equality.
        let rate = 2.0e6;
        let hl = 1000.0;
        let irr = 4000.0;
        let chain = one_iso_chain(rate, hl);
        let sol = solve_chain_at_times(&chain, irr, &[irr], None, 1.0);
        let want = bateman_analytic(rate, hl, irr, irr);
        let got = sol.activities[0][0];
        assert!(
            (got - want).abs() / want < 1e-10,
            "at-EOB: got {got:.6e}, want {want:.6e}",
        );
    }

    #[test]
    fn empty_output_times_returns_empty_series() {
        let chain = one_iso_chain(1.0e6, 3600.0);
        let sol = solve_chain_at_times(&chain, 3600.0, &[], None, 1.0);
        assert_eq!(sol.time_grid_s.len(), 0);
        assert_eq!(sol.activities[0].len(), 0);
    }

    #[test]
    fn empty_chain_returns_empty_series() {
        let sol = solve_chain_at_times(&[], 3600.0, &[0.0, 100.0], None, 1.0);
        assert_eq!(sol.isotopes.len(), 0);
        assert_eq!(sol.activities.len(), 0);
        // time_grid_s is echoed back so a downstream consumer can zip its
        // own bookkeeping without recomputing indexes.
        assert_eq!(sol.time_grid_s, vec![0.0, 100.0]);
    }

    #[test]
    fn chain_feeds_daughter_ingrowth_at_arbitrary_times() {
        // Parent (t½ = 60 s) → Daughter (t½ = 600 s), branching = 1.0. Only
        // the parent is directly produced; the daughter grows in via decay.
        // At an arbitrary cooling time we must see nonzero daughter activity
        // AND it must be reported as `activities_ingrowth`, not direct.
        let irr = 300.0;
        let chain = vec![
            ChainIsotope {
                z: 1,
                a: 1,
                state: String::new(),
                half_life_s: Some(60.0),
                production_rate: 1.0e6,
                decay_modes: vec![DecayMode {
                    mode: "beta-".into(),
                    daughter_z: Some(1),
                    daughter_a: Some(2),
                    daughter_state: String::new(),
                    branching: 1.0,
                }],
            },
            ChainIsotope {
                z: 1,
                a: 2,
                state: String::new(),
                half_life_s: Some(600.0),
                production_rate: 0.0,
                decay_modes: Vec::new(),
            },
        ];
        // Probe well into cooling so the parent is dead and the daughter
        // has non-trivial residual activity.
        let t = irr + 1800.0;
        let sol = solve_chain_at_times(&chain, irr, &[t], None, 1.0);
        let daughter_activity = sol.activities[1][0];
        let daughter_ingrowth = sol.activities_ingrowth[1][0];
        let daughter_direct = sol.activities_direct[1][0];
        assert!(daughter_activity > 0.0, "daughter must have activity");
        assert_eq!(
            daughter_direct, 0.0,
            "daughter has no direct production (production_rate = 0)",
        );
        assert!(
            (daughter_ingrowth - daughter_activity).abs() < 1e-12,
            "daughter activity must be reported as ingrowth (all of it)",
        );
    }
}

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

            if !isotope_map.contains_key(&dkey) {
                let decay = db.get_decay_data(dz, da, ds);
                let (half_life, modes) = match decay {
                    Some(d) => (d.half_life_s, d.decay_modes.clone()),
                    None => (None, Vec::new()),
                };
                isotope_map.insert(
                    dkey,
                    ChainIsotope {
                        z: dz,
                        a: da,
                        state: ds.clone(),
                        half_life_s: half_life,
                        production_rate: 0.0,
                        decay_modes: modes,
                    },
                );
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
pub fn solve_chain(
    chain: &[ChainIsotope],
    irradiation_time_s: f64,
    cooling_time_s: f64,
    _beam_particles_per_s: f64,
    n_time_points: usize,
    current_profile: Option<&CurrentProfile>,
    nominal_current_ma: f64,
) -> ChainSolution {
    let n = chain.len();
    if n == 0 {
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

    // Build index map
    let idx: HashMap<String, usize> = chain
        .iter()
        .enumerate()
        .map(|(i, iso)| (iso.key(), i))
        .collect();

    // Build decay matrix A (n×n flat row-major)
    let mut a_mat = vec![0.0; n * n];
    for (i, iso) in chain.iter().enumerate() {
        if iso.is_stable() {
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

    // Nominal production rate vector
    let r_nominal: Vec<f64> = chain.iter().map(|iso| iso.production_rate).collect();

    // Time grid
    let n_irr = n_time_points / 2;
    let n_cool = n_time_points - n_irr;
    let t_irr = linspace(0.0, irradiation_time_s, n_irr);
    let t_cool_full = linspace(
        irradiation_time_s,
        irradiation_time_s + cooling_time_s,
        n_cool + 1,
    );
    let t_cool: Vec<f64> = t_cool_full[1..].to_vec();

    let mut time_grid = Vec::with_capacity(n_irr + n_cool);
    time_grid.extend_from_slice(&t_irr);
    time_grid.extend_from_slice(&t_cool);
    let n_t = time_grid.len();

    let mut abundances: Vec<Vec<f64>> = (0..n).map(|_| vec![0.0; n_t]).collect();
    let mut activities: Vec<Vec<f64>> = (0..n).map(|_| vec![0.0; n_t]).collect();

    // --- Irradiation phase ---
    let n_eoi = if current_profile.is_some() {
        solve_irradiation_piecewise(
            &a_mat,
            &r_nominal,
            &t_irr,
            n,
            &mut abundances,
            current_profile.unwrap(),
            nominal_current_ma,
            irradiation_time_s,
        )
    } else {
        let mut n_state = vec![0.0; n];
        for ti in 0..n_irr {
            if ti == 0 {
                // already zero
            } else {
                let dt = t_irr[ti] - t_irr[ti - 1];
                n_state = step_irradiation(&a_mat, &r_nominal, &n_state, dt, n);
                for i in 0..n {
                    if n_state[i] < 0.0 {
                        n_state[i] = 0.0;
                    }
                    abundances[i][ti] = n_state[i];
                }
            }
        }

        // Step to exact EOI
        let last_irr_t = t_irr[n_irr - 1];
        if last_irr_t < irradiation_time_s {
            let dt_final = irradiation_time_s - last_irr_t;
            n_state = step_irradiation(&a_mat, &r_nominal, &n_state, dt_final, n);
            for i in 0..n {
                if n_state[i] < 0.0 {
                    n_state[i] = 0.0;
                }
            }
        }
        n_state
    };

    // --- Cooling phase ---
    {
        let mut n_state = n_eoi.clone();
        for ti in 0..n_cool {
            let t_prev = if ti == 0 {
                irradiation_time_s
            } else {
                t_cool[ti - 1]
            };
            let dt = t_cool[ti] - t_prev;
            if dt <= 0.0 {
                for i in 0..n {
                    abundances[i][n_irr + ti] = n_state[i];
                }
            } else {
                let a_dt = scale_flat(&a_mat, dt, n);
                let ea = matrix_exp(&a_dt, n);
                n_state = mat_vec_mul(&ea, &n_state, n);
                for i in 0..n {
                    if n_state[i] < 0.0 {
                        n_state[i] = 0.0;
                    }
                    abundances[i][n_irr + ti] = n_state[i];
                }
            }
        }
    }

    // Compute activities with ceilings
    let total_production_atoms: f64 =
        chain.iter().map(|iso| iso.production_rate).sum::<f64>() * irradiation_time_s;

    for (i, iso) in chain.iter().enumerate() {
        if iso.is_stable() {
            continue;
        }
        let lam = iso.lambda();

        // Global ceiling
        for t in 0..n_t {
            if abundances[i][t] > total_production_atoms {
                abundances[i][t] = total_production_atoms;
            }
        }

        // Per-isotope analytical ceiling for daughters
        if iso.production_rate == 0.0 {
            let parent_indices: Vec<(usize, f64)> = chain
                .iter()
                .enumerate()
                .filter(|(_, p)| !p.is_stable())
                .flat_map(|(p_idx, p)| {
                    p.decay_modes
                        .iter()
                        .filter(|mode| {
                            mode.daughter_z == Some(iso.z)
                                && mode.daughter_a == Some(iso.a)
                                && mode.daughter_state == iso.state
                        })
                        .map(move |mode| (p_idx, mode.branching))
                })
                .collect();

            if !parent_indices.is_empty() {
                for t in 0..n_t {
                    let mut max_n = 0.0;
                    for &(p, br) in &parent_indices {
                        let lam_p = chain[p].lambda();
                        let np = abundances[p][t];
                        if lam > lam_p && lam_p > 0.0 {
                            max_n += np * lam_p * br / (lam - lam_p);
                        } else {
                            max_n += np * br;
                        }
                    }
                    if abundances[i][t] > max_n && max_n > 0.0 {
                        abundances[i][t] = max_n;
                    }
                }
            }
        }

        for t in 0..n_t {
            activities[i][t] = lam * abundances[i][t];
        }
    }

    // --- Direct component ---
    let activities_direct = compute_direct_component(
        chain,
        &time_grid,
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
        time_grid_s: time_grid,
        abundances,
        activities,
        activities_direct,
        activities_ingrowth,
        parent_info,
    }
}

fn solve_irradiation_piecewise(
    a: &[f64],
    r_nominal: &[f64],
    t_irr: &[f64],
    n: usize,
    abundances: &mut [Vec<f64>],
    current_profile: &CurrentProfile,
    nominal_current_ma: f64,
    irradiation_time_s: f64,
) -> Vec<f64> {
    let intervals = current_profile.intervals(irradiation_time_s);

    // Build output index map
    let mut output_idx: HashMap<u64, Vec<usize>> = HashMap::new();
    for (ti, &t) in t_irr.iter().enumerate() {
        let key = t.to_bits();
        output_idx.entry(key).or_default().push(ti);
    }

    // Merge boundaries + output times
    let mut boundary_set: BTreeSet<u64> = BTreeSet::new();
    boundary_set.insert(0.0_f64.to_bits());
    boundary_set.insert(irradiation_time_s.to_bits());
    for &(s, e, _) in &intervals {
        boundary_set.insert(s.to_bits());
        boundary_set.insert(e.to_bits());
    }
    for &t in t_irr {
        boundary_set.insert(t.to_bits());
    }
    let all_times: Vec<f64> = boundary_set.iter().map(|&b| f64::from_bits(b)).collect();

    let mut n_state = vec![0.0; n];

    // Record t=0
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

        while iv_idx < intervals.len() - 1 && intervals[iv_idx].1 <= prev_t {
            iv_idx += 1;
        }
        let i_current = intervals[iv_idx].2;
        let scale = if nominal_current_ma > 0.0 {
            i_current / nominal_current_ma
        } else {
            0.0
        };

        let r_scaled: Vec<f64> = r_nominal.iter().map(|&r| r * scale).collect();
        n_state = step_irradiation(a, &r_scaled, &n_state, dt, n);

        if let Some(indices) = output_idx.get(&t_next.to_bits()) {
            for &ti in indices {
                for i in 0..n {
                    abundances[i][ti] = n_state[i];
                }
            }
        }

        prev_t = t_next;
    }

    // Ensure EOI stored
    for i in 0..n {
        abundances[i][t_irr.len() - 1] = n_state[i];
    }

    n_state
}

fn compute_direct_component(
    chain: &[ChainIsotope],
    time_grid: &[f64],
    irradiation_time_s: f64,
    current_profile: Option<&CurrentProfile>,
    nominal_current_ma: f64,
) -> Vec<Vec<f64>> {
    let n = chain.len();
    let n_t = time_grid.len();
    let mut activities_direct: Vec<Vec<f64>> = (0..n).map(|_| vec![0.0; n_t]).collect();

    for (i, iso) in chain.iter().enumerate() {
        if iso.production_rate <= 0.0 || iso.is_stable() {
            continue;
        }
        let lam = iso.lambda();

        if current_profile.is_none() {
            // Analytical Bateman (constant current)
            let a_eoi = iso.production_rate * (1.0 - (-lam * irradiation_time_s).exp());
            for (t_idx, &t) in time_grid.iter().enumerate() {
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

            let mut n_val = 0.0_f64;
            let mut t_now = 0.0;
            let mut iv_idx = 0usize;

            // Collect irradiation output times
            let irr_outputs: Vec<(f64, usize)> = time_grid
                .iter()
                .enumerate()
                .filter(|(_, &t)| t <= irradiation_time_s)
                .map(|(idx, &t)| (t, idx))
                .collect();

            // Merge boundaries
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

            // Cooling phase
            let a_eoi = lam * n_val;
            for (t_idx, &t) in time_grid.iter().enumerate() {
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

//! Math utilities: Gauss-Hermite quadrature nodes/weights.

/// 12-point Gauss-Hermite quadrature nodes (precomputed).
pub const GH_NODES_12: [f64; 12] = [
    -3.889_724_897_869_782,
    -3.020_637_025_120_890,
    -2.279_507_080_501_060,
    -1.597_682_635_152_605,
    -0.947_788_391_240_164,
    -0.314_240_376_254_359,
    0.314_240_376_254_359,
    0.947_788_391_240_164,
    1.597_682_635_152_605,
    2.279_507_080_501_060,
    3.020_637_025_120_890,
    3.889_724_897_869_782,
];

/// 12-point Gauss-Hermite quadrature weights (normalized by 1/sqrt(pi)).
pub const GH_WEIGHTS_12: [f64; 12] = [
    2.658_551_684_492_21e-7,
    8.573_687_043_587_87e-5,
    3.905_390_584_629_06e-3,
    5.160_798_561_588_39e-2,
    2.604_923_102_641_61e-1,
    5.701_352_362_624_80e-1,
    5.701_352_362_624_80e-1,
    2.604_923_102_641_61e-1,
    5.160_798_561_588_39e-2,
    3.905_390_584_629_06e-3,
    8.573_687_043_587_87e-5,
    2.658_551_684_492_21e-7,
];

/// Gauss-Hermite convolution of a cross-section function with energy spread.
///
/// `<sigma> = sum_k w_k * sigma(E_mean + sqrt(2) * sigma_E * x_k)`
pub fn gauss_hermite_convolved_xs<F>(xs_fn: &F, e_mean: &[f64], sigma_e: &[f64]) -> Vec<f64>
where
    F: Fn(&[f64]) -> Vec<f64>,
{
    let n = e_mean.len();
    let mut result = vec![0.0; n];

    for (k, (&node, &weight)) in GH_NODES_12.iter().zip(GH_WEIGHTS_12.iter()).enumerate() {
        let _ = k;
        let shifted: Vec<f64> = e_mean
            .iter()
            .zip(sigma_e.iter())
            .map(|(&e, &s)| {
                if s < 1e-6 {
                    e
                } else {
                    (e + std::f64::consts::SQRT_2 * s * node).max(0.01)
                }
            })
            .collect();
        let xs_vals = xs_fn(&shifted);
        for i in 0..n {
            result[i] += weight * xs_vals[i];
        }
    }

    result
}

//! Matrix exponential via Padé [13/13] approximation with scaling-and-squaring.
//!
//! Flat row-major `Vec<f64>` representation for n×n matrices.

/// Padé [13/13] coefficients (from scipy.linalg._expm_pade).
const B: [f64; 14] = [
    64764752532480000.0,
    32382376266240000.0,
    7771770303897600.0,
    1187353796428800.0,
    129060195264000.0,
    10559470521600.0,
    670442572800.0,
    33522128640.0,
    1323241920.0,
    40840800.0,
    960960.0,
    16380.0,
    182.0,
    1.0,
];

/// Scaling threshold θ₁₃.
const THETA_13: f64 = 5.371_920_351_148_152;

/// Multiply two n×n matrices stored as flat Vec<f64>.
fn mat_mul(a: &[f64], b: &[f64], n: usize) -> Vec<f64> {
    let mut c = vec![0.0; n * n];
    for i in 0..n {
        for k in 0..n {
            let aik = a[i * n + k];
            if aik == 0.0 {
                continue;
            }
            for j in 0..n {
                c[i * n + j] += aik * b[k * n + j];
            }
        }
    }
    c
}

fn mat_add(a: &[f64], b: &[f64]) -> Vec<f64> {
    a.iter().zip(b.iter()).map(|(&x, &y)| x + y).collect()
}

fn mat_sub(a: &[f64], b: &[f64]) -> Vec<f64> {
    a.iter().zip(b.iter()).map(|(&x, &y)| x - y).collect()
}

fn mat_scale(a: &[f64], s: f64) -> Vec<f64> {
    a.iter().map(|&x| x * s).collect()
}

fn mat_eye(n: usize) -> Vec<f64> {
    let mut m = vec![0.0; n * n];
    for i in 0..n {
        m[i * n + i] = 1.0;
    }
    m
}

/// 1-norm of matrix (max column sum of absolute values).
fn mat_norm1(a: &[f64], n: usize) -> f64 {
    let mut max_col = 0.0_f64;
    for j in 0..n {
        let mut col_sum = 0.0;
        for i in 0..n {
            col_sum += a[i * n + j].abs();
        }
        max_col = max_col.max(col_sum);
    }
    max_col
}

/// Solve linear system A*X = B (Gaussian elimination with partial pivoting).
fn mat_solve(a_in: &[f64], b_in: &[f64], n: usize) -> Vec<f64> {
    let mut a = a_in.to_vec();
    let mut b = b_in.to_vec();

    // Forward elimination with partial pivoting
    for col in 0..n {
        let mut max_val = a[col * n + col].abs();
        let mut max_row = col;
        for row in (col + 1)..n {
            let v = a[row * n + col].abs();
            if v > max_val {
                max_val = v;
                max_row = row;
            }
        }

        if max_row != col {
            for j in 0..n {
                a.swap(col * n + j, max_row * n + j);
                b.swap(col * n + j, max_row * n + j);
            }
        }

        let pivot = a[col * n + col];
        if pivot.abs() < 1e-300 {
            continue;
        }

        for row in (col + 1)..n {
            let factor = a[row * n + col] / pivot;
            for j in col..n {
                a[row * n + j] -= factor * a[col * n + j];
            }
            for j in 0..n {
                b[row * n + j] -= factor * b[col * n + j];
            }
        }
    }

    // Back substitution
    let mut x = vec![0.0; n * n];
    for col in (0..n).rev() {
        let pivot = a[col * n + col];
        if pivot.abs() < 1e-300 {
            continue;
        }
        for j in 0..n {
            let mut sum = b[col * n + j];
            for k in (col + 1)..n {
                sum -= a[col * n + k] * x[k * n + j];
            }
            x[col * n + j] = sum / pivot;
        }
    }

    x
}

/// Matrix exponential of an n×n matrix using Padé [13/13] with scaling-and-squaring.
///
/// `m` is a flat row-major `Vec<f64>` of size n*n.
pub fn matrix_exp(m: &[f64], n: usize) -> Vec<f64> {
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![m[0].exp()];
    }

    // Scaling: find s such that ||M/2^s|| < θ₁₃
    let norm = mat_norm1(m, n);
    let s = if norm > THETA_13 {
        (norm / THETA_13).log2().ceil().max(0.0) as u32
    } else {
        0
    };

    let a = if s > 0 {
        mat_scale(m, 1.0 / (1u64 << s) as f64)
    } else {
        m.to_vec()
    };

    // Compute powers: A2, A4, A6
    let a2 = mat_mul(&a, &a, n);
    let a4 = mat_mul(&a2, &a2, n);
    let a6 = mat_mul(&a2, &a4, n);
    let eye = mat_eye(n);

    // U = A @ (A6 @ (b[13]*A6 + b[11]*A4 + b[9]*A2) + b[7]*A6 + b[5]*A4 + b[3]*A2 + b[1]*I)
    let mut inner = mat_add(&mat_scale(&a6, B[13]), &mat_scale(&a4, B[11]));
    inner = mat_add(&inner, &mat_scale(&a2, B[9]));
    let mut u_body = mat_mul(&a6, &inner, n);
    u_body = mat_add(&u_body, &mat_scale(&a6, B[7]));
    u_body = mat_add(&u_body, &mat_scale(&a4, B[5]));
    u_body = mat_add(&u_body, &mat_scale(&a2, B[3]));
    u_body = mat_add(&u_body, &mat_scale(&eye, B[1]));
    let u = mat_mul(&a, &u_body, n);

    // V = A6 @ (b[12]*A6 + b[10]*A4 + b[8]*A2) + b[6]*A6 + b[4]*A4 + b[2]*A2 + b[0]*I
    let mut v_inner = mat_add(&mat_scale(&a6, B[12]), &mat_scale(&a4, B[10]));
    v_inner = mat_add(&v_inner, &mat_scale(&a2, B[8]));
    let mut v = mat_mul(&a6, &v_inner, n);
    v = mat_add(&v, &mat_scale(&a6, B[6]));
    v = mat_add(&v, &mat_scale(&a4, B[4]));
    v = mat_add(&v, &mat_scale(&a2, B[2]));
    v = mat_add(&v, &mat_scale(&eye, B[0]));

    // Padé approximant: expm = (V - U)^{-1} * (V + U)
    let vmu = mat_sub(&v, &u);
    let vpu = mat_add(&v, &u);

    let mut result = mat_solve(&vmu, &vpu, n);

    // Squaring phase
    for _ in 0..s {
        result = mat_mul(&result, &result, n);
    }

    result
}

/// Matrix-vector multiply: y = M * x, where M is n×n flat row-major.
pub fn mat_vec_mul(m: &[f64], x: &[f64], n: usize) -> Vec<f64> {
    let mut y = vec![0.0; n];
    for i in 0..n {
        let mut sum = 0.0;
        for j in 0..n {
            sum += m[i * n + j] * x[j];
        }
        y[i] = sum;
    }
    y
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exp_zero() {
        let m = vec![0.0; 4];
        let result = matrix_exp(&m, 2);
        // Should be identity
        assert!((result[0] - 1.0).abs() < 1e-12);
        assert!(result[1].abs() < 1e-12);
        assert!(result[2].abs() < 1e-12);
        assert!((result[3] - 1.0).abs() < 1e-12);
    }

    #[test]
    fn test_exp_diagonal() {
        // exp(diag(1, 2)) = diag(e, e²)
        let m = vec![1.0, 0.0, 0.0, 2.0];
        let result = matrix_exp(&m, 2);
        let e = std::f64::consts::E;
        assert!((result[0] - e).abs() < 1e-10);
        assert!(result[1].abs() < 1e-10);
        assert!(result[2].abs() < 1e-10);
        assert!((result[3] - e * e).abs() < 1e-10);
    }

    #[test]
    fn test_exp_inverse() {
        // exp(A) * exp(-A) = I
        let a = vec![0.1, 0.2, 0.3, 0.4];
        let neg_a: Vec<f64> = a.iter().map(|&x| -x).collect();
        let exp_a = matrix_exp(&a, 2);
        let exp_neg_a = matrix_exp(&neg_a, 2);
        let product = mat_mul(&exp_a, &exp_neg_a, 2);
        assert!((product[0] - 1.0).abs() < 1e-8);
        assert!(product[1].abs() < 1e-8);
        assert!(product[2].abs() < 1e-8);
        assert!((product[3] - 1.0).abs() < 1e-8);
    }

    #[test]
    fn test_two_isotope_decay() {
        // Parent -> daughter, dt = 2 half-lives
        // After 2 half-lives: parent = 25%, daughter = 75%
        let ln2 = std::f64::consts::LN_2;
        let half_life = 3600.0;
        let lambda = ln2 / half_life;
        let dt = 2.0 * half_life;

        let a = vec![
            -lambda, 0.0, lambda, 0.0, // daughter is stable
        ];
        let a_dt = mat_scale(&a, dt);
        let exp_a = matrix_exp(&a_dt, 2);

        let n0 = vec![1.0, 0.0];
        let n_final = mat_vec_mul(&exp_a, &n0, 2);

        assert!((n_final[0] - 0.25).abs() < 1e-6);
        assert!((n_final[1] - 0.75).abs() < 1e-6);
    }
}

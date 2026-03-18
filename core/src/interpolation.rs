//! Numerical utilities: interpolation, integration, linspace.

/// Generate evenly spaced values (like numpy.linspace).
pub fn linspace(start: f64, stop: f64, num: usize) -> Vec<f64> {
    if num == 0 {
        return Vec::new();
    }
    if num == 1 {
        return vec![start];
    }
    let step = (stop - start) / (num - 1) as f64;
    (0..num).map(|i| start + i as f64 * step).collect()
}

/// Trapezoidal integration of y over x.
pub fn trapezoid(y: &[f64], x: &[f64]) -> f64 {
    assert_eq!(y.len(), x.len());
    let mut sum = 0.0;
    for i in 1..x.len() {
        sum += 0.5 * (y[i - 1] + y[i]) * (x[i] - x[i - 1]);
    }
    sum
}

/// Linear interpolation (equivalent to np.interp).
/// `x` must be sorted ascending. Values outside range get left/right fill values.
pub fn interp(x_new: &[f64], x: &[f64], y: &[f64], left: f64, right: f64) -> Vec<f64> {
    let n = x.len();
    x_new
        .iter()
        .map(|&xv| {
            if xv <= x[0] {
                return left;
            }
            if xv >= x[n - 1] {
                return right;
            }
            // Binary search
            let mut lo = 0usize;
            let mut hi = n - 1;
            while hi - lo > 1 {
                let mid = (lo + hi) >> 1;
                if x[mid] <= xv {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            let t = (xv - x[lo]) / (x[hi] - x[lo]);
            y[lo] + t * (y[hi] - y[lo])
        })
        .collect()
}

/// Log-log linear interpolation for stopping power data.
/// Returns a closure that interpolates in log-log space with extrapolation at boundaries.
pub fn make_log_log_interpolator(
    energies_mev: &[f64],
    dedx: &[f64],
) -> Box<dyn Fn(&[f64]) -> Vec<f64> + Send + Sync> {
    let n = energies_mev.len();
    let log_e: Vec<f64> = energies_mev.iter().map(|&e| e.ln()).collect();
    let log_s: Vec<f64> = dedx.iter().map(|&s| s.ln()).collect();

    Box::new(move |energies: &[f64]| {
        energies
            .iter()
            .map(|&energy| {
                let le = energy.ln();

                if le <= log_e[0] {
                    if n < 2 {
                        return log_s[0].exp();
                    }
                    let slope = (log_s[1] - log_s[0]) / (log_e[1] - log_e[0]);
                    return (log_s[0] + slope * (le - log_e[0])).exp();
                }
                if le >= log_e[n - 1] {
                    if n < 2 {
                        return log_s[n - 1].exp();
                    }
                    let slope = (log_s[n - 1] - log_s[n - 2]) / (log_e[n - 1] - log_e[n - 2]);
                    return (log_s[n - 1] + slope * (le - log_e[n - 1])).exp();
                }

                // Binary search
                let mut lo = 0usize;
                let mut hi = n - 1;
                while hi - lo > 1 {
                    let mid = (lo + hi) >> 1;
                    if log_e[mid] <= le {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }

                let t = (le - log_e[lo]) / (log_e[hi] - log_e[lo]);
                (log_s[lo] + t * (log_s[hi] - log_s[lo])).exp()
            })
            .collect()
    })
}

/// Scalar version of log-log interpolation.
pub fn log_log_interp_scalar(energies_mev: &[f64], dedx: &[f64], energy: f64) -> f64 {
    let n = energies_mev.len();
    let le = energy.ln();

    let log_e_first = energies_mev[0].ln();
    let log_s_first = dedx[0].ln();

    if le <= log_e_first {
        if n < 2 {
            return log_s_first.exp();
        }
        let log_e_1 = energies_mev[1].ln();
        let log_s_1 = dedx[1].ln();
        let slope = (log_s_1 - log_s_first) / (log_e_1 - log_e_first);
        return (log_s_first + slope * (le - log_e_first)).exp();
    }

    let log_e_last = energies_mev[n - 1].ln();
    let log_s_last = dedx[n - 1].ln();

    if le >= log_e_last {
        if n < 2 {
            return log_s_last.exp();
        }
        let log_e_prev = energies_mev[n - 2].ln();
        let log_s_prev = dedx[n - 2].ln();
        let slope = (log_s_last - log_s_prev) / (log_e_last - log_e_prev);
        return (log_s_last + slope * (le - log_e_last)).exp();
    }

    // Binary search
    let mut lo = 0usize;
    let mut hi = n - 1;
    while hi - lo > 1 {
        let mid = (lo + hi) >> 1;
        if energies_mev[mid].ln() <= le {
            lo = mid;
        } else {
            hi = mid;
        }
    }

    let log_e_lo = energies_mev[lo].ln();
    let log_e_hi = energies_mev[hi].ln();
    let log_s_lo = dedx[lo].ln();
    let log_s_hi = dedx[hi].ln();
    let t = (le - log_e_lo) / (log_e_hi - log_e_lo);
    (log_s_lo + t * (log_s_hi - log_s_lo)).exp()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linspace() {
        let v = linspace(0.0, 10.0, 5);
        assert_eq!(v.len(), 5);
        let expected = [0.0, 2.5, 5.0, 7.5, 10.0];
        for (a, b) in v.iter().zip(expected.iter()) {
            assert!((a - b).abs() < 1e-12);
        }
    }

    #[test]
    fn test_trapezoid_constant() {
        let y = vec![5.0, 5.0, 5.0, 5.0];
        let x = vec![0.0, 1.0, 2.0, 3.0];
        assert!((trapezoid(&y, &x) - 15.0).abs() < 1e-12);
    }

    #[test]
    fn test_log_log_power_law() {
        // y = x^2 -> log-log interpolation should be exact
        let x: Vec<f64> = vec![1.0, 2.0, 4.0, 8.0, 16.0];
        let y: Vec<f64> = x.iter().map(|&v| v * v).collect();
        let interp_fn = make_log_log_interpolator(&x, &y);
        let result = interp_fn(&[5.0]);
        assert!((result[0] - 25.0).abs() < 1e-6);
    }

    #[test]
    fn test_interp_basic() {
        let x = vec![0.0, 1.0, 2.0, 3.0];
        let y = vec![0.0, 10.0, 20.0, 30.0];
        let result = interp(&[0.5, 1.5, 2.5], &x, &y, 0.0, 0.0);
        assert!((result[0] - 5.0).abs() < 1e-12);
        assert!((result[1] - 15.0).abs() < 1e-12);
        assert!((result[2] - 25.0).abs() < 1e-12);
    }
}

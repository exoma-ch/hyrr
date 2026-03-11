/**
 * Numerical utilities: interpolation, integration, linspace.
 */

/**
 * Log-log linear interpolation for stopping power data.
 * Returns a function that interpolates in log-log space.
 * Extrapolation is used at boundaries.
 */
export function makeLogLogInterpolator(
  energiesMeV: Float64Array,
  dedx: Float64Array,
): (energy: number | Float64Array) => number | Float64Array {
  const n = energiesMeV.length;
  const logE = new Float64Array(n);
  const logS = new Float64Array(n);
  for (let i = 0; i < n; i++) {
    logE[i] = Math.log(energiesMeV[i]);
    logS[i] = Math.log(dedx[i]);
  }

  function lookupScalar(energy: number): number {
    const le = Math.log(energy);

    // Binary search for interval
    let lo = 0;
    let hi = n - 1;

    if (le <= logE[0]) {
      // Extrapolate below
      if (n < 2) return Math.exp(logS[0]);
      const slope = (logS[1] - logS[0]) / (logE[1] - logE[0]);
      return Math.exp(logS[0] + slope * (le - logE[0]));
    }
    if (le >= logE[n - 1]) {
      // Extrapolate above
      if (n < 2) return Math.exp(logS[n - 1]);
      const slope =
        (logS[n - 1] - logS[n - 2]) / (logE[n - 1] - logE[n - 2]);
      return Math.exp(logS[n - 1] + slope * (le - logE[n - 1]));
    }

    // Binary search
    while (hi - lo > 1) {
      const mid = (lo + hi) >> 1;
      if (logE[mid] <= le) lo = mid;
      else hi = mid;
    }

    // Linear interpolation in log-log
    const t = (le - logE[lo]) / (logE[hi] - logE[lo]);
    return Math.exp(logS[lo] + t * (logS[hi] - logS[lo]));
  }

  return (energy: number | Float64Array): number | Float64Array => {
    if (typeof energy === "number") {
      return lookupScalar(energy);
    }
    const result = new Float64Array(energy.length);
    for (let i = 0; i < energy.length; i++) {
      result[i] = lookupScalar(energy[i]);
    }
    return result;
  };
}

/**
 * Trapezoidal integration of y over x.
 */
export function trapezoid(
  y: Float64Array,
  x: Float64Array,
): number {
  let sum = 0;
  for (let i = 1; i < x.length; i++) {
    sum += 0.5 * (y[i - 1] + y[i]) * (x[i] - x[i - 1]);
  }
  return sum;
}

/**
 * Linear interpolation (equivalent to np.interp).
 * x must be sorted ascending.
 * Values outside range get left/right fill values.
 */
export function interp(
  xNew: Float64Array,
  x: Float64Array,
  y: Float64Array,
  left: number = 0,
  right: number = 0,
): Float64Array {
  const result = new Float64Array(xNew.length);
  const n = x.length;

  for (let i = 0; i < xNew.length; i++) {
    const xv = xNew[i];
    if (xv <= x[0]) {
      result[i] = left;
    } else if (xv >= x[n - 1]) {
      result[i] = right;
    } else {
      // Binary search
      let lo = 0;
      let hi = n - 1;
      while (hi - lo > 1) {
        const mid = (lo + hi) >> 1;
        if (x[mid] <= xv) lo = mid;
        else hi = mid;
      }
      const t = (xv - x[lo]) / (x[hi] - x[lo]);
      result[i] = y[lo] + t * (y[hi] - y[lo]);
    }
  }

  return result;
}

/**
 * Generate evenly spaced values (like numpy.linspace).
 */
export function linspace(
  start: number,
  stop: number,
  num: number,
): Float64Array {
  const arr = new Float64Array(num);
  if (num === 1) {
    arr[0] = start;
    return arr;
  }
  const step = (stop - start) / (num - 1);
  for (let i = 0; i < num; i++) {
    arr[i] = start + i * step;
  }
  return arr;
}

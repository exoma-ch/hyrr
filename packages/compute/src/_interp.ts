/**
 * Simple linear interpolation utility (np.interp equivalent).
 * Kept for cross-section plotting in the frontend.
 */

/**
 * Linear interpolation. x must be sorted ascending.
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

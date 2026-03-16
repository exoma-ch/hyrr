/**
 * Matrix exponential via Padé [13/13] approximation with scaling-and-squaring.
 *
 * Flat Float64Array row-major representation for n×n matrices.
 * Matrices are small (5-30 isotopes), so no external library needed.
 */

// Padé [13/13] coefficients (from scipy.linalg._expm_pade)
const b = [
  64764752532480000,
  32382376266240000,
  7771770303897600,
  1187353796428800,
  129060195264000,
  10559470521600,
  670442572800,
  33522128640,
  1323241920,
  40840800,
  960960,
  16380,
  182,
  1,
];

/** Multiply two n×n matrices stored as flat Float64Array. */
function matMul(a: Float64Array, b: Float64Array, n: number): Float64Array {
  const c = new Float64Array(n * n);
  for (let i = 0; i < n; i++) {
    for (let k = 0; k < n; k++) {
      const aik = a[i * n + k];
      if (aik === 0) continue;
      for (let j = 0; j < n; j++) {
        c[i * n + j] += aik * b[k * n + j];
      }
    }
  }
  return c;
}

/** Add two n×n matrices. */
function matAdd(a: Float64Array, b: Float64Array): Float64Array {
  const c = new Float64Array(a.length);
  for (let i = 0; i < a.length; i++) c[i] = a[i] + b[i];
  return c;
}

/** Subtract: a - b. */
function matSub(a: Float64Array, b: Float64Array): Float64Array {
  const c = new Float64Array(a.length);
  for (let i = 0; i < a.length; i++) c[i] = a[i] - b[i];
  return c;
}

/** Scale matrix by scalar. */
function matScale(a: Float64Array, s: number): Float64Array {
  const c = new Float64Array(a.length);
  for (let i = 0; i < a.length; i++) c[i] = a[i] * s;
  return c;
}

/** Identity matrix. */
function matEye(n: number): Float64Array {
  const I = new Float64Array(n * n);
  for (let i = 0; i < n; i++) I[i * n + i] = 1;
  return I;
}

/** 1-norm of matrix (max column sum of absolute values). */
function matNorm1(a: Float64Array, n: number): number {
  let maxCol = 0;
  for (let j = 0; j < n; j++) {
    let colSum = 0;
    for (let i = 0; i < n; i++) {
      colSum += Math.abs(a[i * n + j]);
    }
    if (colSum > maxCol) maxCol = colSum;
  }
  return maxCol;
}

/**
 * Solve linear system A*X = B where A and B are n×n, in-place (Gaussian elimination).
 * Returns X. Modifies A and B.
 */
function matSolve(A: Float64Array, B: Float64Array, n: number): Float64Array {
  // Work on copies
  const a = new Float64Array(A);
  const b = new Float64Array(B);

  // Forward elimination with partial pivoting
  for (let col = 0; col < n; col++) {
    // Find pivot
    let maxVal = Math.abs(a[col * n + col]);
    let maxRow = col;
    for (let row = col + 1; row < n; row++) {
      const v = Math.abs(a[row * n + col]);
      if (v > maxVal) {
        maxVal = v;
        maxRow = row;
      }
    }

    // Swap rows
    if (maxRow !== col) {
      for (let j = 0; j < n; j++) {
        const tmpA = a[col * n + j];
        a[col * n + j] = a[maxRow * n + j];
        a[maxRow * n + j] = tmpA;
        const tmpB = b[col * n + j];
        b[col * n + j] = b[maxRow * n + j];
        b[maxRow * n + j] = tmpB;
      }
    }

    const pivot = a[col * n + col];
    if (Math.abs(pivot) < 1e-300) continue;

    // Eliminate
    for (let row = col + 1; row < n; row++) {
      const factor = a[row * n + col] / pivot;
      for (let j = col; j < n; j++) {
        a[row * n + j] -= factor * a[col * n + j];
      }
      for (let j = 0; j < n; j++) {
        b[row * n + j] -= factor * b[col * n + j];
      }
    }
  }

  // Back substitution
  const X = new Float64Array(n * n);
  for (let col = n - 1; col >= 0; col--) {
    const pivot = a[col * n + col];
    if (Math.abs(pivot) < 1e-300) continue;
    for (let j = 0; j < n; j++) {
      let sum = b[col * n + j];
      for (let k = col + 1; k < n; k++) {
        sum -= a[col * n + k] * X[k * n + j];
      }
      X[col * n + j] = sum / pivot;
    }
  }

  return X;
}

/**
 * Matrix exponential of an n×n matrix using Padé [13/13] with scaling-and-squaring.
 *
 * @param M - Flat row-major Float64Array of size n*n
 * @param n - Matrix dimension
 * @returns exp(M) as flat Float64Array
 */
export function matrixExp(M: Float64Array, n: number): Float64Array {
  if (n === 0) return new Float64Array(0);
  if (n === 1) return Float64Array.of(Math.exp(M[0]));

  // Scaling: find s such that ||M/2^s|| < 5.4
  const norm = matNorm1(M, n);
  let s = 0;
  const theta13 = 5.371920351148152;
  if (norm > theta13) {
    s = Math.max(0, Math.ceil(Math.log2(norm / theta13)));
  }

  const A = s > 0 ? matScale(M, 1 / (1 << s)) : new Float64Array(M);

  // Compute powers of A: A2, A4, A6
  const A2 = matMul(A, A, n);
  const A4 = matMul(A2, A2, n);
  const A6 = matMul(A2, A4, n);
  const I = matEye(n);

  // scipy formula:
  // U = A @ (A6 @ (b[13]*A6 + b[11]*A4 + b[9]*A2) + b[7]*A6 + b[5]*A4 + b[3]*A2 + b[1]*I)
  // V = A6 @ (b[12]*A6 + b[10]*A4 + b[8]*A2) + b[6]*A6 + b[4]*A4 + b[2]*A2 + b[0]*I

  // Inner part of U: A6 @ (b[13]*A6 + b[11]*A4 + b[9]*A2)
  let inner = matAdd(matScale(A6, b[13]), matScale(A4, b[11]));
  inner = matAdd(inner, matScale(A2, b[9]));
  let Ubody = matMul(A6, inner, n);
  // + b[7]*A6 + b[5]*A4 + b[3]*A2 + b[1]*I
  Ubody = matAdd(Ubody, matScale(A6, b[7]));
  Ubody = matAdd(Ubody, matScale(A4, b[5]));
  Ubody = matAdd(Ubody, matScale(A2, b[3]));
  Ubody = matAdd(Ubody, matScale(I, b[1]));
  const U = matMul(A, Ubody, n);

  // V: A6 @ (b[12]*A6 + b[10]*A4 + b[8]*A2)
  let Vinner = matAdd(matScale(A6, b[12]), matScale(A4, b[10]));
  Vinner = matAdd(Vinner, matScale(A2, b[8]));
  let V = matMul(A6, Vinner, n);
  // + b[6]*A6 + b[4]*A4 + b[2]*A2 + b[0]*I
  V = matAdd(V, matScale(A6, b[6]));
  V = matAdd(V, matScale(A4, b[4]));
  V = matAdd(V, matScale(A2, b[2]));
  V = matAdd(V, matScale(I, b[0]));

  // Padé approximant: expm = (V - U)^{-1} * (V + U)
  const VmU = matSub(V, U);
  const VpU = matAdd(V, U);

  let result = matSolve(VmU, VpU, n);

  // Squaring phase
  for (let i = 0; i < s; i++) {
    result = matMul(result, result, n);
  }

  return result;
}

/**
 * Matrix-vector multiply: y = M * x, where M is n×n flat row-major.
 */
export function matVecMul(
  M: Float64Array,
  x: Float64Array,
  n: number,
): Float64Array {
  const y = new Float64Array(n);
  for (let i = 0; i < n; i++) {
    let sum = 0;
    for (let j = 0; j < n; j++) {
      sum += M[i * n + j] * x[j];
    }
    y[i] = sum;
  }
  return y;
}

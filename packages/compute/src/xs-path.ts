/**
 * Cross-section file resolution — shared between browser (`DataStore`) and
 * node (`NodeDataStore`) so both stores try the same set of candidate
 * filenames in the same order.
 *
 * nucl-parquet ships cross-section files under two naming conventions:
 *
 *   - `{projectile}_{Symbol}.parquet` — the historical convention every
 *     element used to follow (p_Cu.parquet, a_Bi.parquet, …).
 *   - `{projectile}_Z{Z}.parquet` — the fallback nucl-parquet uses for the
 *     17 elements whose IUPAC symbol was not settled at data-build time or
 *     which have no stable natural isotopes (Tc 43, Pm 61, Po 84, Rn 86,
 *     Ra 88, Ac 89, Pa 91, Np 93, Pu 94, Am 95, Cm 96, Bk 97, Cf 98, Es 99,
 *     Fm 100, Md 101, Db 105).
 *
 * The stores previously only tried the symbol form, so ²²⁶Ra(p,x) silently
 * produced zero isotopes in the browser (blocking the ²²⁶Ra(p,2n)²²⁵Ac
 * clinical route). This mirrors the Rust fix in PR #555
 * (`core/src/db.rs::resolve_xs_path`) — try symbol form first, Z-form as
 * fallback. Pure function so the fallback logic is exercised by a unit
 * test without a real data tree.
 *
 * Refs: #488.
 */

/**
 * Return the candidate on-disk / on-server paths for a given cross-section
 * lookup, in probe order (symbol form first, Z-form fallback).
 *
 * Callers are expected to try each candidate in turn and use the first that
 * loads successfully. When every candidate fails they should emit
 * `logMissingXs` so the previously-silent "no data → zero isotopes" failure
 * shows up in the browser console (#488).
 *
 * @param subdir     — subdirectory holding the xs files, relative to a data
 *                     root (`"xs"` for charged particles, `"neutron-xs"` for
 *                     neutrons — see `DataStore.ensureCrossSections`).
 * @param projectile — projectile code (`p`, `d`, `t`, `h`, `a`, or a
 *                     heavy-ion string like `C-12`). Passed through verbatim.
 * @param targetZ    — target proton number, used to build the Z-form
 *                     fallback path. Required — the whole point of #488 is
 *                     that Z is what the fallback keys on.
 * @param symbol     — target element symbol, used for the primary path.
 */
export function xsPathCandidates(
  subdir: string,
  projectile: string,
  targetZ: number,
  symbol: string,
): string[] {
  return [
    `${subdir}/${projectile}_${symbol}.parquet`,
    `${subdir}/${projectile}_Z${targetZ}.parquet`,
  ];
}

/**
 * Emit a one-line console warning that both candidate cross-section files
 * were absent for `(projectile, target)`. Prefer this over a bare
 * `console.warn` at the call site so message format stays uniform across
 * `DataStore` and `NodeDataStore` — mirrors the Rust `data.xs.missing`
 * trace event added in PR #555.
 */
export function logMissingXs(
  projectile: string,
  targetZ: number,
  symbol: string,
): void {
  console.warn(
    `[DataStore] no cross-section data for ${projectile} on ${symbol} ` +
      `(Z=${targetZ}): tried both {proj}_{Symbol}.parquet and ` +
      `{proj}_Z{Z}.parquet. Isotope yields for this target will be zero. ` +
      `(#488)`,
  );
}

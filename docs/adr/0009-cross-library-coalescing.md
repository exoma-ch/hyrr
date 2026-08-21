# ADR 0009 — Cross-library cross-section coalescing: curve-level, priority-ordered, opt-in

- **Status**: accepted (2026-08-19) — decision only; implementation tracked on #658
- **Date**: 2026-08-19
- **Relates to**: #658 (this decision), #657 (library picker + explorer),
  \#649 (silent-empty epic), #505 (selectable neutron library),
  \#252 (isomeric dedup), #659 (per-projectile library routing)

## Context

No single nuclear data library is complete, and their gaps are **complementary
rather than nested**. Measured against the shipped `nucl-parquet` submodule
(data `2026.8.2`, 22 libraries, 4,613 cross-section files):

- `tendl-2023-iso` — the default. 5 projectiles × ~98 elements. Ships **no H and
  no He for any projectile**, no `Li` for p or d, no `As` for t, and nothing
  above Z=101. This is an upstream gap, not a lookup bug: `p_B.parquet` and
  `p_C.parquet` resolve fine.
- `endfb-8.1` — covers **exactly** those light targets (`p_H`, `p_He`, `p_Li`,
  `d_H`, `d_He`, `t_H`, `t_He`) and almost nothing else (33 files total).
- `tendl-2025` — wider element coverage and a wider target-isotope range, but
  **dropped the isomeric g/m split entirely**.

So a user asking for p + Li gets nothing today while the answer sits in two other
libraries on disk. The natural request is "merge them, with a priority order".

That request is reasonable, and the naive implementation is dangerous. This ADR
records why, and what shape is safe.

## What the data actually looks like

Measured, not assumed:

1. **The schema is byte-identical across all 22 libraries** — the same 18
   columns (`library, kind, projectile, proj_Z, proj_A, target_Z, target_A, MT,
   residual_Z, residual_A, state, energy_MeV, xs_mb, energy_err_MeV, xs_err_mb,
   source_entry, author, year`). Structurally, merging is trivial.

2. **`MT` is NULL in every `kind="production"` file.** The ENDF reaction number
   — the canonical channel identifier — is not populated. There is therefore no
   channel-level key. The de-facto key is
   `(target_A, residual_Z, residual_A, state)`: a residual product, already
   summed over channels.

3. **`state` means different things in different libraries.** `tendl-2023-iso`
   ships `['', 'g', 'm']`; `tendl-2025` ships `['']` only. Verified numerically:
   `state=''` is the **total**, and `g + m` sums to it exactly (ratio 1.0000 at
   every sampled energy, on 38 of 300 residuals for p+Cu alone).

4. **HYRR already deduplicates this within a library.**
   `dedup_xs_prefer_resolved` (`core/src/db.rs`, from #252) drops the totals when
   state-resolved entries exist for the same residual.

5. **Overlap between libraries is small and asymmetric.** For p+Cu:
   `tendl-2023-iso` has 376 reaction keys, `tendl-2025` has 314, and only **53
   are shared** (~14%). `endfb-8.1`, `jendl-5` and `jeff-4.0` carry **2 keys
   each** — sparse monitor-reaction evaluations.

6. **"Same element" does not mean the same target isotopes.** `tendl-2023-iso`
   covers Cu-63/65/**67**; `tendl-2025` covers Cu-**58–65**. Neither contains the
   other.

7. **Energy supports differ and do not nest.** tendl-2023-iso starts at
   0.125 MeV, tendl-2025 at 0.25, `endfb-8.1` spans only 1–30 MeV, jendl-5 and
   jeff-4.0 span 2.25–200.

## Decision

**Coalesce whole curves in a user-visible priority order. Do not blend.**

1. **Curve-level, never point-level.** For each
   `(target_A, residual_Z, residual_A)` key, take the *entire* curve from the
   highest-priority library that has it. Splicing points across evaluations
   produces a curve no evaluator ever published and violates the internal
   consistency of both — see fact 7: the supports do not even nest.

2. **Dedup the `state` axis per-library, then coalesce across libraries.**
   Order matters and reversing it is the sharpest hazard here. Facts 3 and 4
   together mean that coalescing a g/m library with a totals-only library and
   *then* running `dedup_xs_prefer_resolved` makes the existing rule see "a
   state-resolved entry exists" and drop the other library's total — silently
   preferring an evaluation **by accident of encoding rather than by the user's
   priority order**.

3. **Priority is an explicit ordered list**, surfaced in the UI, defaulting to a
   curated order. Not a silent heuristic, and not "whichever library happens to
   answer first".

4. **Provenance becomes per-reaction.** Today one library is recorded per run.
   Under coalescing every isotope row must carry which evaluation produced it.
   Without that, results stop being reproducible or citeable — for a scientific
   tool that is a worse defect than the missing data this feature exists to fix.

5. **Coalescing is opt-in, and single-library remains the default** for anything
   headed for publication. Mixing evaluations is scientifically contentious; the
   tool should not do it behind the user's back.

6. **Energy coverage gets a diagnostic from day one.** Coalescing *creates* a new
   silent-zero mode: if a sparse library wins a key but covers only 1–30 MeV
   while the beam is at 100 MeV, that reaction contributes zero. `#650`'s
   `ReactionOutsideEnergyRange` already exists for exactly this shape — it was
   added because the coverage sweep found jendl-5's d+Cu tabulated 130–200 MeV
   producing nothing at 20 MeV, silently. Coalescing must emit it.

## Alternatives considered

**Point-level merge (take the highest-fidelity point at each energy).** Rejected
on fact 7. Different evaluations disagree systematically, not randomly; stitching
them per-point manufactures discontinuities and an uncertainty that means
nothing.

**Automatic best-library selection per reaction.** Rejected: there is no
defensible ranking without the evaluated uncertainties, and `xs_err_mb`
conventions differ across libraries. It would also make results depend on a
heuristic the user cannot see or cite.

**Do nothing; make users pick one library.** This is the status quo and remains
the default. Rejected as the *only* option because it leaves p + Li dead while
the data sits on disk, and users have no way to discover that another library has
it — which is what #657's explorer addresses.

**Merge upstream in nucl-parquet.** Tempting, but it moves a HYRR policy decision
into the data layer and makes provenance harder, not easier. The libraries should
stay as their evaluators published them.

## Consequences

- The data store gains a coalescing layer over an ordered library list; the
  per-projectile routing added in #659 (`library_for_projectile`) is the natural
  place for it, and generalising that hardcode was a prerequisite.
- `Provenance` grows per-reaction detail, which increases result payload size —
  relevant to the shared-results HTML (ADR-0008) and the MCP structured export.
- The test matrix multiplies by priority order. #654's sweep should stay
  structural and cheap; coalescing wants targeted fixtures instead.
- If upstream ever populates `MT`, the reaction key changes from residual-level
  to channel-level. The implementation should not assume the current key is
  permanent.
- Neutrons bypass library selection entirely (`NEUTRON_LIBRARY`), so coalescing
  must not appear to affect them until #505 lands.

## References

- Measurements above are reproducible from `nucl-parquet` data `2026.8.2` via
  `hyrr_core::census` (#653) and the parquet schema.
- `dedup_xs_prefer_resolved` — `core/src/db.rs` (#252).
- `DiagnosticKind::ReactionOutsideEnergyRange` — `core/src/types.rs` (#650).
- Ragged-coverage measurements — epic #649, issue #658.

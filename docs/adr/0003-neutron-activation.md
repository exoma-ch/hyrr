# ADR 0003 — Neutron activation: source spectra + secondary (p,xn) neutrons

- **Status**: accepted (2026-07-01) — see "Decisions" below
- **Date**: 2026-07-01
- **Relates to**: #266 (neutron projectiles gated behind data availability), #444
  (production-channel attribution), the legacy `src/hyrr/neutrons.py` prototype
- **Supersedes**: nothing — this is the first neutron-activation design in the
  Rust core

## Context

HYRR models charged-particle activation: a beam (p/d/t/h/a) enters a 1D layer
stack, loses energy by `dE/dx` (PSTAR/ASTAR + Bragg), and at each depth the
reaction rate is `∫σ(E)/|dE/dx| dE`, fed into a Bateman/decay-chain solver
(`core/src/{compute,production,chains,bateman}.rs`). Two neutron use-cases are
out of scope today and repeatedly requested:

1. **Neutron-source irradiation** — a reactor/generator/accelerator neutron
   field entering the stack (thermal, epithermal, fast/fission, mono-energetic,
   or a user-defined spectrum) and activating it. 1D.
2. **Secondary neutron activation** — a charged-particle beam produces neutrons
   via `(p,xn)`/`(d,xn)` etc.; those neutrons form a volumetric source inside
   the stack, transport through it, and activate every layer. The user may toggle
   "model secondary neutron activation" on an otherwise-charged-particle run.
   (The same structure applies to photonuclear `(γ,n)` from bremsstrahlung — a
   future parallel axis, not covered here.)

Three facts make this tractable rather than a from-scratch physics project:

- **A legacy prototype already exists.** `src/hyrr/neutrons.py` (pre-Rust, not on
  the live path) implements the flux-spectrum models (`ThermalFlux` Maxwellian,
  `EpithermalFlux` 1/E, `MonoenergeticFlux`, `WeisskopfFlux` evaporation,
  `CompositeFlux`), a secondary-source builder (`neutron_multiplicity` +
  `compute_neutron_source`), and the flux-averaged cross-section
  `∫σφ dE / ∫φ dE`. It is the physics blueprint; this ADR ports and extends it.
- **The data exists.** `endfb-8.1` ships 97 `n_<element>` neutron reaction
  cross-sections (`target_A, residual_Z, residual_A, state, energy_MeV, xs_mb`),
  and `meta/neutron_total` ships `xs_total_mb, xs_elastic_mb` per nuclide — the
  `Σ_t` needed for attenuation. Both are already copied into the frontend by
  `scripts/copy-frontend-data.sh` (`endfb-8.1:neutron-xs`).
- **The back half is projectile-agnostic.** The Bateman/chain solver
  (`chains.rs`) consumes a production-rate vector `R` per nuclide. Neutron
  activation is "compute another `R` and add it to the same nuclides" — no
  changes to decay physics.

### Physics, and where it differs from charged particles

Neutrons carry no charge → **no Coulomb, no `dE/dx`, no Bragg peak.** Instead:

- Flux attenuates: `φ(x, E) = φ₀(E) · exp(−Σ_t(E)·x)`, `Σ_t = N·σ_total`.
- Reaction rate: `R = N · ∫ σ(E) · φ(E) dE` (a spectrum convolution, structurally
  the same numeric kernel as the Gauss–Hermite straggling convolution already in
  `production.rs`, with the flux spectrum as the kernel instead of a Gaussian).

So the depth↔energy coupling that dominates the charged-particle engine is
*absent*; the new machinery is (a) the flux spectrum `φ(E)`, (b) attenuation via
`Σ_t`, and — for the secondary case — (c) a neutron **source term** and how far it
is transported.

### The secondary source is anisotropic

At cyclotron energies `(p,xn)` neutrons are **forward-peaked** in the lab
(centre-of-mass anisotropy + kinematic forward boost), and the peaking grows with
proton energy. A 50/50 forward/backward split is wrong. The evaluated libraries
carry angular distributions (Legendre coefficients / double-differential
`dσ/dEdΩ`), but the **nucl-parquet parquet stores only angle-integrated `σ(E)`**
(schema confirmed: no angular columns). So true `dσ/dEdΩ` is unavailable on the
live data path today.

## Decision

Add neutron activation as a **second, projectile-agnostic production term** that
composes with the existing Bateman/chain solver, built in phases. Concretely:

### Model

- **Projectile / source.** Add a neutron source distinct from the charged-particle
  beam. A neutron source is `{ spectrum: FluxModel, magnitude }` where `FluxModel`
  is one of `Thermal(kT)`, `Epithermal(E_min,E_max)`, `Fast/Fission(Watt|Weisskopf
  T)`, `Monoenergetic(E)`, or `Custom(Vec<(E, φ)>)` (the "defined energies" case),
  plus `Composite(sum)`. This is a direct port of `neutrons.py`'s `NeutronFlux`
  hierarchy.
- **Attenuation.** New `Σ_t(E)` lookup from `meta/neutron_total` (analogue of the
  stopping-power lookup, but mean-free-path not `dE/dx`). Depth enters as
  `φ(z,E) = φ₀(E)·exp(−Σ_t(E)·z)` rather than through an energy-loss integral.
- **Activation rate.** `R(z) = N · ∫ σ_reaction(E) · φ(z,E) dE` per neutron
  reaction channel from `endfb-8.1/xs/n_<elem>`, summed into the same
  per-nuclide `R` the chain solver already consumes.
- **Secondary source term.** For a charged-particle run with the toggle on:
  per depth, infer the neutron yield from the `(x,n)`-type channels already
  integrated by the charged-particle pass (multiplicity from
  `A_target+Z_proj − A_residual` accounting, as `neutron_multiplicity` does), give
  it an evaporation spectrum (`Weisskopf T` from the reaction Q/energetics), and
  emit it as a **volumetric source** `S(z, E)`.
- **Secondary transport — 1D attenuation-only, bidirectional, anisotropic**
  (decided; see Alternatives for what was rejected):

  ```text
  φ(z, E) = Σ_z'  S(z', E) · [ f_fwd(E_p(z')) · e^(−Σ_t(E)·(z−z')) · 𝟙(z' < z)
                             + (1−f_fwd(E_p(z'))) · e^(−Σ_t(E)·(z'−z)) · 𝟙(z' > z) ]
  R(z, E) = N · ∫ σ(E) · φ(z, E) dE  → Bateman
  ```

  where `f_fwd(E_p)` is an **energy-dependent forward fraction** (`> ½`, growing
  with proton energy) capturing the forward-peaking. Because `dσ/dEdΩ` is not on
  the data path, `f_fwd` is a **parametrized approximation** (kinematic estimate
  or empirical fit), with `f_fwd = ½` (isotropic) as the degenerate default and a
  clean hook to swap in real angular data if/when nucl-parquet adds it. **No
  scatter / no moderation** (first-flight/ray approximation) — appropriate for
  the thin-to-moderate target geometry and clearly labeled as such in the UI and
  output.

### Provenance / the reaction table

The `IsotopeResult.reactions` field exists but is **never populated** today. To
show a nuclide's proton route, decay route, and (new) neutron route side by side,
tag each production contribution with its channel provenance
(`{projectile, target, product, channel-notation}`) as it is summed into `R`.
This is prerequisite plumbing for the display and independently useful (it also
closes the #444-class "which channel produced this?" gap).

### Phasing

- **Spike (prerequisite) — shared-core abstraction.** Design the DRY seam so
  charged-particle and neutron activation share one reaction-rate integral, one
  depth-stepping trait (dE/dx vs `Σ_t`), and one production→chain handoff. Output:
  the trait/function signatures and a thin proof-of-concept, before Phase 1 code.
- **Phase 0 — route provenance.** Populate `reactions` with channel provenance;
  render p-route + decay-route in the table. Small, visible, unblocks the display
  half, no neutron physics. *(Confirm order with maintainer.)*
- **Phase 1 — primary neutron source (case 1).** `FluxModel`, `Σ_t` attenuation,
  `∫σφ dE`, wired into the existing chain solver. Ships reactor/fast-neutron
  irradiation as a standalone, genuinely useful feature and builds ~80% of the
  infrastructure the secondary case reuses.
- **Phase 2 — secondary neutron activation (case 2).** `(x,n)` volumetric source
  + anisotropic bidirectional attenuation transport + the "model secondary neutron
  activation" config toggle; neutron routes appear in the table (built on Phase 0).
- **Phase 3+ (deferred).** Photonuclear `(γ,n)` (same architecture, new data);
  real `dσ/dEdΩ` angular data to replace parametrized `f_fwd`; time-dependent flux
  from decay neutrons (β-delayed n, spontaneous fission) coupling back into the
  Bateman solution (iterative; usually a negligible term — see below).

## Consequences

- **Positive.** Each phase ships value independently; the decay solver is
  untouched; the flux-convolution reuses an existing numeric pattern; the data is
  already present and deployed. Reactor/fast-neutron irradiation (Phase 1) is a
  useful feature on its own, de-risking the harder secondary case.
- **Approximations we accept (and must label).** No neutron scattering/moderation
  (first-flight only); parametrized (not data-driven) forward-peaking; secondary
  spectrum modeled as evaporation, not a true reaction-specific `dσ/dEdΩ`. These
  are order-of-magnitude-honest for the target geometry but are **not** a
  transport-code replacement, and the UI/output must say so.
- **Explicitly deferred.** Time-dependent flux from decay neutrons couples the
  neutron source to the Bateman solution (the source depends on the nuclide
  inventory, which depends on the source) → an iterative solve. It is almost
  always a tiny correction next to the prompt `(x,n)` source (which simply scales
  with instantaneous beam current, already handled by the current-profile path).
  Deferred to keep Phase 2 non-iterative.
- **New surface area.** A neutron source config, a `Σ_t` data path, a neutron
  activation module, and provenance on every production term. Each crate/binding
  (PyO3, WASM, Tauri, MCP) inherits the new config shape at its JSON boundary.

## Alternatives considered

- **0-D "well-mixed" secondary transport** (total yield → one uniform flux over
  the stack). Rejected as the *target* — it ignores geometry and the
  directionality the user specifically called out — but may serve as an internal
  sanity baseline.
- **Full 1-D transport with scattering/moderation** (diffusion or SN, multi-group,
  thermalization). Physically real for thick/moderating targets, but a
  research-grade effort (months) and disproportionate to the tool's fidelity
  elsewhere. Revisit only if a concrete moderated-target use-case demands it.
- **Isotropic (50/50) secondary emission.** Rejected: wrong at cyclotron energies;
  `f_fwd(E_p)` retains the 1D simplicity while respecting forward-peaking.

## Decisions (maintainer sign-off, 2026-07-01)

1. **Phase order — accepted as written** (routes → primary source → secondary),
   with an explicit emphasis: the charged-particle and neutron paths **must share
   the same core reaction-rate / production / Bateman functions** (DRY/SOLID), not
   fork into a parallel neutron pipeline. The shared abstraction is non-trivial —
   **run a `spike` first** to design the common seam (the `∫σ(E)·kernel(E) dE`
   reaction-rate integral, the depth-stepping trait, and the
   production-term→chain-solver handoff) before implementing Phase 1.
2. **`f_fwd(E_p)` — go angular from the start.** Do not ship an isotropic-first
   milestone; the secondary transport carries the energy-dependent forward-peaking
   from its first landed version. `f_fwd = ½` remains only the degenerate fallback,
   not a shipped phase. (Data is still angle-integrated, so `f_fwd` is the
   documented parametrized model — kinematic estimate first, refine to an empirical
   `(p,xn)` angular-yield fit.)
3. **Neutron data library — `endfb-8.1` is the default**, accessed through a
   **library wrapper/abstraction** so it can be swapped later without touching call
   sites. A follow-up issue tracks making the neutron library user-selectable
   (alongside `tendl-2023-iso`, which has no neutron sublibrary). *(Issue filed.)*
4. **Config surface — both cases, distinct shapes:**
   - **Case 1 (primary source):** selecting projectile = **neutron** swaps the beam
     inputs — instead of `(energy_MeV, current_mA)` the UI presents
     `(spectrum, flux/rate)`: a spectrum model (thermal/epithermal/fast-fission/
     mono/custom) + a source magnitude in flux units. Same `Beam`/source slot,
     different input fields driven by projectile type.
   - **Case 2 (secondary):** a plain **toggle** ("model secondary neutron
     activation") living with the charged-particle beam config; no separate source
     block. When on, the `(x,n)` volumetric source is derived from the same
     charged-particle pass.

   The two are independent: a run is *either* a neutron-source run *or* a
   charged-particle run (optionally with the secondary toggle) — they don't stack
   in one beam.

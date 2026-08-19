# Testing

How the suites are wired, and — more importantly — what each one actually
guarantees. Several of them guarantee less than their names suggest; where that
is true it is called out, because assuming otherwise is how the bugs in epic
\#649 shipped.

## Where the suites run

| Suite | Command | Gated on |
|---|---|---|
| Rust (default features) | `nix flake check` → `rust-test` | every PR |
| Rust (`mcp` feature) | `nix flake check` → `rust-test-mcp` | every PR |
| Rust (`embed-data` feature) | `nix flake check` → `rust-test-embed` | every PR |
| Python | `uv run pytest tests/ --ignore=tests/integration` | every PR (`ci.yml`) |
| Python integration | `uv run pytest tests/integration` | **nothing** — needs `just build-native` |
| Frontend vitest | `npm test` in `frontend/` | `frontend-check.yml` (**not** a required check) |
| Packages vitest | `npm run test:packages` at the repo root | `frontend-check.yml` (**not** a required check) |
| Shell-script tests | `just test-scripts` → `scripts/tests/run-all.sh` | `ci.yml` |
| Playwright e2e | `npx playwright test` in `frontend/` | PR: `--project=desktop-1280 --grep @smoke` only. Full matrix at `v*` tags |

Two of these deserve emphasis:

- **`frontend-check.yml` is not a required status check.** Its header explains
  why (a paths filter would block docs-only PRs). A red vitest run is advisory.
- **The PR e2e gate is 2 tests.** Everything else in `frontend/e2e/` runs only
  on tag. Issue \#559 is what that costs.

## Rust: the projectile matrix

`core/tests/projectile_matrix.rs` carries **two distinct guarantees**, and the
difference matters:

- `every_supported_projectile_computes_without_panic` — the **stopping-power**
  path works for all 11 projectiles (`p`, `d`, `t`, `h`, `a`, `C-12`, `O-16`,
  `Ne-20`, `Si-28`, `Ar-40`, `Fe-56`). It asserts only that no panic and no
  `Err` escaped. **It cannot see a silently-empty result** — a `compute_stack`
  returning `Ok` with zero isotopes passes it. That is the #137 regression guard,
  nothing more.
- `light_ion_projectiles_produce_isotopes_on_cu` — the **cross-section** path
  actually yields products, for `p`/`d`/`t`/`h`/`a` on Cu. This is the guard
  against the silent-zero class (epic \#649).

Both assertions cover all 11 projectiles. Heavy ions were excluded from the
production one until \#659, because `ProjectileType::symbol()` returns `"C"` for
C-12 — so the lookup built `C_Cu.parquet` while the file is `c12_Cu.parquet`,
and every heavy-ion run produced zero silently. `xs_key()` plus
`library_for_projectile` fixed it.

A sibling test asserts an unsupported heavy ion (`Cl-38`) returns a typed
`StoppingError::NoSourceTable` rather than panicking.

### Adding a new projectile

Add a row to `PROJECTILES` in `core/tests/projectile_matrix.rs`, and to
`LIGHT_ION_PROJECTILES` if the default library carries cross-sections for it.
The lists do not auto-discover `ProjectileType` variants — that is intentional;
the matrix is the forcing function for coverage.

### Running locally

These tests are `#[ignore]` by default because they need the bundled
nucl-parquet data:

```bash
HYRR_DATA="$PWD/nucl-parquet/data" cargo test --manifest-path core/Cargo.toml \
    --test projectile_matrix -- --include-ignored
```

Without `HYRR_DATA` they fall back to `../nucl-parquet/data` relative to the
cargo manifest. If neither resolves the test prints a skip message and exits
green, so a fresh checkout without the submodule still works.

## Data census and coverage sweep

Two tiers of epic \#649 that answer "does every (library, projectile, target)
triple actually work?".

**Census** (`core/tests/data_census.rs`, `hyrr_core::census`) reconciles
`catalog.json`'s claims against the files on disk — 22 libraries, 4,613 files,
0.05 s. Ragged coverage is never a failure: `tendl-2023-iso` genuinely ships no
H or He for any projectile and no Li for p or d. Only *contradictions* fail, and
known upstream ones live in a `KNOWN_PROBLEMS` baseline that itself fails when
an entry stops reproducing, so it cannot rot into a blanket suppression.

**Sweep** (`core/tests/coverage_sweep.rs`) runs a thin target per triple and
classifies the outcome:

| outcome | meaning | CI |
|---|---|---|
| `Produces` | isotopes came out | pass |
| `NoData` | nothing produced, and a diagnostic (\#650) explains why | report |
| `TypedError` | a typed error — loud, therefore fine | report |
| `SilentlyEmpty` | computed, produced nothing, explained nothing | **fail** |

The test energy is derived **per file** from the evaluation's own grid (the
first point above 1 mb), never hardcoded. A fixed energy would fail every
alpha-on-heavy-target row for entirely correct physics — α+Au needs ≥17 MeV
before any channel exceeds 1 mb, against 3 MeV for α+Li — and a suite that cries
wolf gets muted.

Scope is deliberately split, because each case costs a parquet decode:

- **PR** — a ~19-case deterministic sample inside `nix flake check` (~9 s):
  every charged projectile of the default library, plus the elements with no
  natural isotopes (Tc, Pm, Po, At, Rn, Fr, Ra, Ac, Pa). Those are Z-named on
  disk (`p_Z43.parquet`), so the sample matches on symbol **or** `Z<n>`.
- **Nightly + on submodule bump** — `HYRR_SWEEP=full` over every charged triple,
  via `.github/workflows/coverage-sweep.yml`. A failure opens or updates a
  tracking issue and a green closes it; nightly-only signal that nobody is paged
  for is how \#559 stayed red across releases.

Heavy ions ARE in scope since \#659 — the sweep maps census stems (`c12`) to
`ProjectileType` and routes them to `hi-xs-prod`, which took the full run from
1,747 to 3,277 cases. Still out of scope: `n` (neutron activation goes through
`compute_neutron_stack`) and `g` (photonuclear, unsupported).

Note the classifier is unit-tested separately
(`classifier_distinguishes_the_three_outcomes`). Constructing a genuinely silent
triple from real data is hard now that most failure modes are loud, which is the
desired state — but it would otherwise leave the headline assertion
unfalsifiable.

## Feature-gated tests: the allow-list trap

`nix flake check` runs three Rust derivations. `rust-test` uses the **default**
feature set, so anything behind `#[cfg(feature = "...")]` compiles to nothing
there and must be picked up by a dedicated derivation:

- `rust-test-mcp` enables `--features mcp` and runs the **whole** suite. No
  target allow-list, so `mcp_*.rs` files cannot be dropped.
- `rust-test-embed` enables `--features embed-data` and names each target
  explicitly with `--test`. **That list is an allow-list, and it was incomplete**:
  four files (`f18_production`, `nb_beam_stopper`, `compound_stopping`,
  `material_registry` — 10 tests) were gated on `embed-data` but not named, so
  they ran nowhere. `cargo test --test compound_stopping` on default features
  prints `running 0 tests` and exits 0, which is why nobody noticed.

`scripts/tests/test_feature_gated_tests_run.sh` now asserts the allow-list is
complete, and that `rust-test-mcp` never grows one. **Adding a new
`embed-data`-gated test means adding a `--test` line to `flake.nix`** — the guard
will tell you if you forget.

## Material catalog parity

The material catalog exists twice — `packages/compute/src/materials.ts` (TS) and
`core/src/materials.rs` (Rust) — and they must agree. A layer whose `material` is
a catalog key is sent to Rust **by name**; if Rust's catalog lacks the key it
cannot parse it as a formula either, and because the layer carries a catalog
density, `resolve_material` returns **`Ok` with an empty element list** instead
of an error. The result is a zero-mass layer that silently produces nothing.

That is not hypothetical: `o18-gas`, `xe124-gas` and `sr86-carbonate` shipped on
the TS side only (\#68/\#106) and resolved to zero elements in Rust.

Guards:

- `scripts/tests/test_material_catalog_parity.sh` — key parity in both
  directions, plus "exactly one TS `MATERIAL_CATALOG` definition exists" (a
  second copy diverges silently; there used to be one).
- `packages/compute/src/materials.test.ts` — schema invariants over every entry
  (mass fractions sum to 1 ±1e-6, known element symbols, no key colliding with
  the `Element-Mass` isotope form).
- `core/tests/material_registry.rs` — every catalog key resolves to non-empty
  elements, both with and without a density override, plus the same data
  invariants Rust-side.

## Frontend vs packages

`frontend/vite.config.ts` defines two vitest projects (`node` and `jsdom`) whose
includes are relative to `frontend/`: `src/**/*.test.ts` and
`scripts/**/*.test.ts`. That does **not** cover `packages/`.

So the 11 test files in `packages/compute` — 214 tests including
`xs-path.test.ts`, the regression guard for the \#488 Z-named cross-section
fallback — ran in no CI job at all. The root `vitest.config.ts` covers exactly
that gap and is wired into `frontend-check.yml` as `npm run test:packages`.

Keep the two configs' scopes disjoint. The root config cannot run frontend
modules: they use Svelte 5 runes and need the svelte plugin and `define` block
that only `frontend/vite.config.ts` supplies.

## Playwright

`frontend/playwright.config.ts` runs Chromium across four viewport projects
(`desktop-1280`, `iphone-se`, `iphone-14`, `ipad`). Two modes:

- default — builds and previews locally, hits `http://localhost:4173/hyrr/`
- `PLAYWRIGHT_BASE_URL` set — skips the preview server and runs against a live
  deploy; `@smoke`-tagged tests are the canonical subset for that

On CI it retries twice and keeps a trace on first retry, a screenshot on
failure, and video on failure. Locally there are no retries, so a flake stays
visible.

### Shared fixtures

Import `test`/`expect` from `frontend/e2e/fixtures.ts`, not `@playwright/test`.
It provides:

- **Strict console capture** — any `console.error`, `console.warn` or
  `pageerror` fails the test unless allow-listed. Warnings are deliberately not
  exempt: `logMissingXs` is a warning, and excluding warnings is how that signal
  was lost.
- **`CONSOLE_ALLOWLIST`** — typed entries carrying a pattern, ticket, reason and
  **expiry**. `console-allowlist.spec.ts` fails once an entry is past its
  expiry, so suppressions get re-reviewed instead of accumulating.
- **IndexedDB reset** per test, since Playwright reuses a context per worker and
  history / session / parquet-cache state otherwise leaks between specs.
- Helpers: `openPreset`, `waitForCompute`, `getIsotopeCount`.

Not every spec has been migrated to the fixture yet — the ones that haven't
still import `@playwright/test` directly and get no console checking.

### Presets and feeling-lucky

`presets-all.spec.ts` generates one case per entry in `PRESETS`, driven by
`#preset=<id>`. It replaced five hardcoded `#config=1:<base64>` URLs, three of
which were corrupt deflate streams (\#559). Generating from the registry means a
new preset is covered when it is added, not when someone remembers to write a
spec — which is why the four neutron presets previously had no coverage at all.

There is **no `@preset-heavy` tier**. Ge-68, At-211 and Ac-225 carried it
because they timed out at 5–13 minutes; via `#preset=` each finishes in ~1.5 s.
The app was never rendering, so the "slow physics" was a stalled page.

`lucky.spec.ts` clicks both `feelingLucky` call sites. Determinism comes from
`?seed=<n>` (`src/lib/lucky.ts`), gated to dev and automated browsers so a
production bundle ignores it. The seed→preset mapping is asserted in
`lucky.test.ts`, so an RNG change fails there first.

### Gating

PR runs `--project=desktop-1280 --grep @smoke|@preset` — 18 tests, ~29 s,
covering every preset, both lucky sites and allow-list hygiene. It was 2 tests
before \#656. The OS/viewport matrix stays at tag time.

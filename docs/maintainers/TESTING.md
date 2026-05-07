# Testing

## Projectile matrix smoke test (Tier 1 of #148)

`core/tests/projectile_matrix.rs` loops every supported `ProjectileType`
through `compute_stack` against a trivial Cu stack. It exists to catch
the #137 class of bug where one projectile has a bad lookup key and
silently breaks the per-projectile compute path.

The matrix currently covers: `p`, `d`, `t`, `h`, `a`, `C-12`, `O-16`,
`Ne-20`, `Si-28`, `Ar-40`, `Fe-56`. A sibling test asserts that an
unsupported heavy ion (`Cl-35`) returns a typed
`StoppingError::NoSourceTable` instead of panicking.

### Adding a new projectile

When you add a variant to `ProjectileType` (or extend the bundled
catima set in `nucl-parquet/data/stopping/`), add a row to the
`PROJECTILES` constant in `core/tests/projectile_matrix.rs`. The test
will not auto-discover new variants — that's intentional, the matrix
is the forcing function for coverage.

### Running locally

The test is `#[ignore]` by default because it needs the bundled
nucl-parquet data on disk. Run it explicitly:

```bash
HYRR_DATA=../nucl-parquet/data cargo test \
    --test projectile_matrix -- --include-ignored
```

Without `HYRR_DATA`, the test falls back to `../nucl-parquet/data`
relative to the cargo manifest (the standard sibling-submodule layout).
If neither path resolves, the test prints a skip message and exits
green — so a fresh `cargo test` on a checkout without the submodule
keeps working.

### CI

CI initialises the nucl-parquet submodule with sparse-checkout for
`data/meta`, `data/stopping`, and `data/tendl-2025/xs`, then runs
`cargo test --test projectile_matrix -- --include-ignored` from
`core/`. See `.github/workflows/ci.yml`.

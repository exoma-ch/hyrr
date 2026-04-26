# ADR 0001 — MCP single Rust SSoT and per-surface install channels

- **Status**: accepted
- **Date**: 2026-04-25
- **Implements**: #67 (PR #70 + PR #73)
- **Supersedes**: the Node `mcp/` implementation (deleted in #70)

## Context

By spring 2026 HYRR had four independent surfaces consuming the physics core:

- Tauri desktop app (`desktop/src-tauri/`)
- WASM frontend in the browser (`wasm/` + Svelte SPA)
- Python library (`src/hyrr/`, with PyO3 bindings in `py/`)
- An MCP server, originally in TypeScript (`mcp/`)

The Rust SSoT initiative (#36) consolidated physics into `hyrr-core` for the
first three surfaces, but the MCP layer was still TypeScript on a separate
codepath. Drift was already visible: `compare_results` existed only in Node;
the Rust MCP embedded in the Tauri binary lagged by one tool. The same
shape of failure that motivated the Rust SSoT in the first place was
recurring on the agent surface.

Separately, no install channel for the headless MCP existed: power users got
it bundled with the desktop app via `hyrr --mcp`, but anyone wanting the MCP
without a GUI installation had no path. `cargo install` would have required
a Rust toolchain (~80% of the target audience — physicists on Claude Code —
would bounce); npm wrappers bring node into the picture; `uvx` matched the
sibling `nucl-parquet-mcp` install idiom and the broader MCP ecosystem
(Anthropic reference servers, third-party Python MCPs).

## Decision

**One Rust source of truth for MCP tools, multiple thin entry points, one install channel per surface.**

### Code home

MCP tools, schemas, and dispatch live in `core/src/mcp/` behind a `mcp`
Cargo feature (default off). Every consumer imports from
`hyrr_core::mcp::transport::run_mcp_server` and never reimplements:

- **Desktop**: `hyrr --mcp` argv-branches in `desktop/src-tauri/src/main.rs`
  before the Tauri builder runs, calling the shared module.
- **Standalone Rust binary**: a sibling `hyrr-mcp/` crate that depends on
  `hyrr-core` with the `mcp` feature and nothing else (zero Tauri in
  `cargo tree`). Single `main.rs`, ~50 lines, parses `--data-dir` /
  `--version` / `--help`, then calls the same module.
- **Python wheel**: `py-mcp/` is a maturin/PyO3 crate exposing a
  three-line `_native.run(data_dir)` pyfunction. The Python wrapper
  (`hyrr_mcp/__init__.py`) parses CLI args and hands stdin/stdout to
  the native loop. ABI3 (`abi3-py311`) so one wheel covers Python ≥ 3.11.

A golden-fixture parity test (`scripts/mcp_parity_test.sh`) diffs the
stdio output of `hyrr --mcp` against `hyrr-mcp` on a JSON-RPC fixture
to guard against future divergence.

### Install channels (one per surface)

| Surface | Channel | Why |
|---|---|---|
| Desktop GUI | GitHub Releases (`.dmg` / `.msi` / `.deb` / `.AppImage`) | Native installers, code-signing, bundled ~68 MB Parquet data, OS integration, auto-update |
| MCP server (headless) | `uvx hyrr-mcp` (PyPI wheel, primary) | MCP-ecosystem norm; matches sibling `nucl-parquet-mcp`; no Rust toolchain required |
| MCP server (developer) | `cargo install hyrr-mcp` | Convenience for Rust contributors; not a public-facing recommendation |
| MCP server (already have desktop) | `hyrr --mcp` | Free with GUI install; reuses bundled data |
| Python library | `pip install hyrr` / `uv pip install hyrr` | Existing |
| Browser SPA | static GitHub Pages (WASM) | Existing |

### Tool surface frozen pre-publish

Eight tools, names locked before the first PyPI tag to avoid breaking
agent configs:

`simulate`, `list_materials`, `list_reaction_channels` (renamed from
`get_cross_sections` to avoid collision with `nucl-parquet-mcp`),
`get_decay_data`, `compare_simulations` (ported from Node
`compare_results`), `get_stack_energy_budget`, `get_stopping_power`,
`get_isotope_production_curve`.

Layer schemas use a flat `enrichment: [{element, A, fraction}]` shape
(LLM-friendlier than nested maps). Every response footer carries
`*Library: <id>*` so agents see which nuclear data fed the calculation
without trusting an invisible default.

## Alternatives considered

### Code home

- **Keep MCP in `desktop/src-tauri/src/mcp/` + feature-gate Tauri out
  for the headless binary.** Rejected. `tauri-build` is a
  `[build-dependencies]` entry that Cargo does *not* prune via
  `--no-default-features` unless wrapped in a feature itself; would
  silently pull Tauri on "headless" installs. `tauri::generate_context!`
  is a proc-macro requiring Tauri in scope, forcing module-level
  `#[cfg]`s that smear across `main.rs`. A reviewer 6 months out
  would have proposed exactly the lift we did.
- **Separate `hyrr-mcp-lib/` crate, MCP code outside `core/`.**
  Rejected as too much abstraction for too little benefit. `core/` was
  already the SSoT for physics; making the MCP tool contracts live
  next to the physics they expose was the smaller, more honest move.

### Distribution

- **`cargo install` as primary public channel.** Rejected. Effectively
  unused as a public MCP distribution channel in the ecosystem (no
  notable third-party MCP ships this way). Excludes ~80% of the target
  audience (physicists with no Rust toolchain).
- **`npx -y @hyrr/mcp` with prebuilt per-platform binaries.** Plausible
  alternative, ~1 day of release plumbing. Deferred but not rejected —
  if a node-native user base shows up, ship it as a second channel.
  Tracked in #71 as P2.
- **Hosted SPA-driven MCP.** Considered briefly. Dead end: the SPA is
  browser-WASM compute, not a server. A "router MCP" would have to
  either drive a headless browser or reimplement the physics
  server-side, both worse than just shipping the Rust binary.

### Tool surface

- **Freeze at 5 tools** (the original Node surface). Rejected. Forced
  `simulate` to be the universal hammer; agents had to re-run a full
  activation simulation to ask "will this stack stop the beam?". Three
  new first-class tools (`get_stack_energy_budget`,
  `get_stopping_power`, `get_isotope_production_curve`) widened the
  surface to 8 before name-freeze.

## Consequences

### Positive

- **Drift architecturally impossible.** Three entry points
  (`hyrr --mcp`, `hyrr-mcp`, `uvx hyrr-mcp`) all dispatch through the
  same `core::mcp::tools::call_tool`. Adding a tool means editing one
  file; all three surfaces pick it up on next build.
- **Install UX matches surface.** GUI users get a real installer,
  agent users get `uvx`, library users get pip. No one is asked to
  install a toolchain they don't already have.
- **Parity test in CI** catches drift if a future refactor reintroduces
  divergent codepaths.

### Negative

- **`mcp` feature flag** adds a small CI matrix dimension on
  `hyrr-core` (default vs `--no-default-features` vs `--features mcp`).
- **PyPI publish + cibuildwheel matrix** is real release-engineering
  work (#71). Not free, but amortized across all future releases.
- **Two MCP binaries on disk** for desktop users (`hyrr` and any
  separately-installed `hyrr-mcp`) — docs must pick one canonical
  invocation per scenario to avoid cargo-culting both.

### Empirical evidence

- **`hyrr-mcp` release binary**: 6 MB, zero Tauri in `cargo tree`.
- **PyO3 wheel**: ~10 MB ABI3 wheel, builds in ~7s with `maturin
  develop --release`.
- **Parity test**: `hyrr --mcp` and `hyrr-mcp` produce byte-identical
  stdio responses on a 5-request fixture (after a determinism fix in
  `list_materials` that the parity test itself caught).
- **Smoke test**: `get_stopping_power p/Cu [10,20,30]` returns
  ~27/16/12 MeV·cm²/g (NIST PSTAR within 6%); `get_stack_energy_budget
  p@20 MeV / 1 mm Cu` reports beam fully stopped with ~2020 W
  deposition (matches `P = E·I/q = 2000 W` for a 0.1 mA proton beam).

## References

- #67 — MCP consolidation parent issue
- #36 — Rust physics core SSoT
- #35 — `--mcp` flag on Tauri binary (predecessor)
- PR #70 — Rust-side consolidation
- PR #73 — PyO3/maturin wrapper
- #71 — PyPI publish + cibuildwheel + remaining follow-ups
- `core/src/mcp/` — tool definitions and dispatch
- `scripts/mcp_parity_test.sh` — drift guard

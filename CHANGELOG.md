# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.8.0] — 2026-05-12

The **Local-first** release. Three architectural shifts let HYRR run as an
honest offline-first tool: a lazy nuclear-data cache that no longer ships
the 380 MB blob inside the installer, a typed-error chain that surfaces
recoverable failures to the user instead of crashing the splash, and a
real Tauri desktop app with auto-updater + cross-platform Playwright
coverage. The MCP server is now a first-class entry point distributed via
`uvx`. ~80 merged PRs since v0.7.0.

### Added

- **Tauri desktop app** with auto-updater (minisign-signed, build attestations) and external-link routing through `tauri-plugin-opener`. Ships as `.dmg` / `.msi` / `.deb` / `.AppImage` via GitHub Releases (#125, #129, #116).
- **Lazy nuclear-data cache** — desktop and CLI fetch `nucl-parquet-data-v{V}.tar.zst` from GitHub Releases on first launch into `~/.hyrr/nucl-parquet/v{V}/`, with atomic install semantics (sentinel-last, partial-dir promotion, flock + process-mutex serialisation). The installer no longer bundles 380 MB of data (#52, #117, #121, #122, #123).
- **Splash-screen recovery UI** — when `ensure_data` fails on first launch, a typed `FetchErrorCard` surfaces Retry / Open URL / Install from local tarball / Use bundled-data-only with the actual GitHub URL inline. Push-based `FetchProgress` events drive a live progress bar (`Connecting → Downloading → Extracting → Verifying`) so users can distinguish a slow download from a hang (#118, #160).
- **Staging deploy slot** — every push to main lands at `https://exoma-ch.github.io/hyrr/tst/`. Promotion to prod (`/hyrr/`) is manual workflow_dispatch, gated by a post-deploy smoke test. Broken main commits can no longer reach the user-facing URL (#156, #187).
- **MCP server as first-class entry** — `hyrr-mcp` PyPI package distributed via `uvx`, wheel matrix across Linux/macOS/Windows, trusted PyPI publishing. Library override via `--library` / `HYRR_LIBRARY`, parity coverage with the Python core, stopping-only fast path for `get_stack_energy_budget` (#67, #70, #71, #73, #79, #80, #82, #83, #84).
- **Material-popup unified redesign** — rows-based DefineForm with paste-formula auto-detection (single-formula / mass-mixture / mole-mixture / by-atom), standalone PeriodicTable component, per-row enrichment "E" button, balance toggle, mode-switch undo strip, supplier links for enriched isotopes, gas-target catalog entries (#64 epic, #75, #77, #86, #88, #92, #93, #94, #95, #96, #97, #98).
- **PWA installability** — manifest + iOS meta tags for "Add to Home Screen" on the web frontend (#34, #115).
- **Bug-report modal Title field** with auto-derivation from description; "Open on GitHub" button greyed-with-hover-info until description has content; layer-stack clear-all × button (#144, #161-fix, #182).
- **CLI progress bar** — `hyrr fetch-data` shows tqdm progress on TTY, one-line-per-stage on non-TTY. PyO3 binding accepts an optional Python callable that's invoked with throttled `FetchProgress` events (#172).
- **Plot/table export** — CSV export on every plot, parquet session save/restore, save-icon dropdown unifying the surface.
- **Greek-symbol decay/reaction labels** in the activity table (α, β−, β+, EC, γ) for compactness (#131).
- **Display-layer value clamping** with configurable thresholds so the activity table doesn't render `1e-42 Bq` rows (#130).
- **RNP%-multi-select** with searchable picker and per-curve max markers; grouped/per-layer toggle on the activity table.
- **Test infrastructure tiers** — Tier 1 Rust projectile-matrix smoke (`core/tests/projectile_matrix.rs`), Tier 2 cross-platform Playwright E2E across Ubuntu/Windows/macOS + Linux distros (Arch/Fedora/Debian), `@testing-library/svelte` component-render coverage. 515 frontend tests, 30 Rust core lib tests (#148, #136, #169, #178).
- **CI guards** — `cargo check -p hyrr-py` (catches PyO3 binding drift), `data-fetch-ssot` grep guard (no hardcoded release URLs in docs/code), `supplier-catalog-freshness` warning at >90 days stale (#180/#181/#183).

### Changed

- **nucl-parquet** bumped to v0.10.x — TENDL-2025 default across all surfaces, hi-xs-prod heavy-ion data, catima-derived stopping for Z≥6 projectiles (#91, #113, #127, #128).
- **Default cross-section library** is `tendl-2025` everywhere; configurable via `--library`, `HYRR_LIBRARY`, or `DataStore(library=…)` (#91).
- **Typed `StoppingError` / `FetchError` chains** replace `Result<_, String>` at all IPC boundaries. Frontend `parseStoppingError` / `parseFetchError` decode the JSON wire format into discriminated unions; recovery cards render variant-specific titles (`Couldn't download nuclear data — HTTP 404`, `Energy out of table range`, etc.) (#142, #150).
- **Single source of truth for data-fetch constants** — `hyrr_core::data_fetch::{release_url,tarball_filename,cache_root_pattern,data_version}` plus mirror Tauri commands and frontend `data-fetch-meta.ts` getters. No more concrete `v0.10.0/nucl-parquet-data-v0.10.0.tar.zst` literals scattered across docs/CI/code (#118-contract, #157).
- **Shared `FetchProgressThrottle`** — desktop and PyO3 binding both use one canonical 100 ms / 256 KiB combinator in `hyrr_core::data_fetch`. Replaced two ~60-LOC inline copies (#180).
- **Activity-plot legend behaviour** — selection persists across `Plotly.react()` calls via a `Map<string, boolean | "legendonly">` instead of being clobbered on every re-render.
- **`init_data_store` and `FetchErrorPayload`** redact `$HOME` to `~/…` before crossing the IPC boundary, so bug-report attachments don't leak the OS username (#173, #176).

### Fixed

- **Heavy-ion crashes** — `compute_stack` for C-12 / O-16 / Ne-20 / Si-28 / Ar-40 / Fe-56 hit a catima parquet-key mismatch (`catima_O-16` vs `catima_O16`); fixed by stripping the dash in the source-key construction and adding a typed-error path when the table lookup misses (#137-fix, #141, #142, #150, #161).
- **Stale results displayed after compute failure** — the previous successful run's stats no longer mask a freshly-failed run; both `setResultErrored` and `setComputeError` fire from the scheduler's single funnel (#143).
- **Activity clamp too loose** — `R × λt` allowed 40× spurious peaks mid-irradiation; replaced with the analytical saturation envelope `R × (1 − exp(−λt_irr))` (closes #55).
- **Activity plot 1 trace per layer × N layers** — aggregated by isotope name; the per-layer view is now opt-in via a toggle (closes #54).
- **Save-menu invisible** — dropdown was clipped by `overflow: hidden` on `.header-bar`; window-event handler had a flip race with the toggle button.
- **External links no-op in WKWebView** — bug-report and GitHub buttons now route through `tauri-plugin-opener` instead of `window.open` (#129).
- **Stale WASM binary** — checked-in `hyrr_wasm_bg.wasm` was 5 weeks behind the Rust source; rebuild + CI guard so future drift is caught (#152).
- **Tarball extract refuses non-regular entries** — symlinks, hardlinks, devices, etc. are rejected before extraction (#122).
- **Decay chain solver** — bypassed broken matrix-exp on chains of mixed half-lives, falls back to analytical Bateman per isotope (closes #58).
- **`init_data_store` IPC error** no longer leaks the resolved data dir's absolute path (#176).
- **Bug-report "Open on GitHub" silently failed** when description was empty — getSerializableConfig fallthrough fixed, plus an enable-gate with hover info on the disabled state (#137-related).

### Removed

- **Bundled 380 MB nuclear data from the installer** — replaced by lazy fetch on first launch (#52, #117). Air-gapped installs use `hyrr fetch-data --offline-bundle <path>`.
- **Duplicate physics implementations in Python and TypeScript** — `hyrr_core` Rust crate is now the SSoT for stopping/compute/Bateman; PyO3 + WASM bindings expose it. The legacy Python core and TS port survived as compat shims through earlier releases; in 0.8 they're gone (#67-era rust-ssot-cutover).
- **`feat/v0.7.1-batch`-era inline material picker** — superseded by the unified material-form redesign (#92, #98).

### Security

- **Tarball extraction** rejects symlinks/hardlinks/devices, eliminating a class of path-traversal vectors when installing from an untrusted offline bundle (#122).
- **Tauri auto-updater** signs releases with minisign + verifies build attestations; the updater refuses unsigned releases (#116, #125).
- **IPC payload redaction** — `$HOME` stripped from cache paths in error payloads so bug-report attachments don't carry the OS username (#173, #176).

### Infrastructure

- **CI matrix** now covers: Python (pyright + ruff + pytest), Rust projectile-matrix (Tier 1), cross-platform Playwright E2E (Tier 2: Ubuntu 22/24, Windows 22/25, macOS 15, Linux distros Arch/Fedora/Debian), `cargo check -p hyrr-py` (PyO3 drift), data-fetch SSoT grep, supplier-catalog freshness, docs build validation. ~15 jobs per PR depending on touched paths.
- **Cross-platform Tauri builds** ship signed installers via GitHub Releases; minisign keys + auto-updater plumbing wired end-to-end.
- **Staging slot** at `/hyrr/tst/` decouples broken main commits from the user-facing prod URL. Manual promotion via workflow_dispatch.

## [0.7.0] — 2026-03-27

### Changed

- **nucl-parquet** bumped to v0.9.0 — isomeric state data (699 metastable nuclides), state-scoped radiation lines, updated dose constants covering isomeric pure-beta emitters

### Added

- **IsotopePopup: XS data loading** — cross-section parquet files are now lazily loaded when the popup opens, restoring XS plots, depth plots, theory/real toggle, and compare isotope dropdown
- **IsotopePopup: Real depth production rates** — "Real" mode now computes σ(E(x)) × abundance × number density × beam flux in the frontend, independent of backend; shows actual production rate (atoms/s/cm) per layer with correct density and isotopic scaling
- **Download links with OS detection** — Help modal shows an OS-aware "Download for macOS/Windows/Linux" button; footer shows "Desktop app (macOS)" etc.
- **Playwright e2e tests** for isotope depth plot theory/real toggle (3 tests)

### Fixed

- **IsotopePopup blank on open** — `getCrossSections()` returned empty because the scheduler's DataStore never had XS parquet files loaded; fixed by calling `ensureMultipleCrossSections()` before reading
- **Real mode depth plot was empty** — Rust WASM engine doesn't output `depth_production_rates`; replaced with frontend-computed rates using XS channels + depth preview + material composition
- **Playwright webServer timeout** — increased from 60s to 120s (build alone takes ~35s)

## [0.6.1] — 2026-03-24

### Added

- **Repeating layer groups** — wrap any set of layers into a group and repeat them N times or until beam energy drops below a threshold; groups are first-class in the UI and persist across reloads
- **Group persistence** — groups survive URL hash sharing and session tab restore (encoded as `{g:true, ...}` in compact config format v2)
- **Shared isotope filter bar** — filter panel extracted above both the activity plots and the activity table; filter state is shared between both views
- **Simulation mode toggle** — Auto/Manual button next to beam properties with live status dot (idle / busy / ready / error)
- **Undo/redo** — Cmd+Z / Cmd+Shift+Z (50-deep snapshot stack); also accessible via keyboard while any text field is not focused
- **Clear history** — "Clear all" button in the history panel with inline Yes/No confirmation
- **Leading-decimal thickness** — thickness inputs now accept `.2mm`, `.5µm`, etc.

### Fixed

- **Li-5 phantom activity (#40)** — matrix exponential residuals for ultra-short half-lives (t½ < 1 µs) are replaced with analytical Bateman solution, eliminating spurious MBq readings
- **Production depth plot zero-crossing (#41)** — isotopes that don't appear in a layer now produce a clean zero segment rather than a diagonal artifact
- **Material picker / enrichment context (#42)** — popup handlers now use internal item indices (not expanded flat layer indices), so clicking enrichment on a CaO layer no longer defaults to Al
- **GitHub Actions storage** — Rust build caches only saved on tag releases; Pages and dist artifacts have explicit retention limits; benchmark job scoped to `src/**` changes

## [0.5.0] — 2026-03-18

### Added

- **Desktop app** — offline-capable native app via Tauri v2 for air-gapped machines; bundles all nuclear data (~68 MB Parquet); builds for Windows (.msi/.exe), macOS (.dmg, ARM64 + x64), and Linux (.deb/.AppImage)
- **Tauri CI workflow** (`.github/workflows/tauri-build.yml`) — cross-platform build + GitHub Release upload on `v*` tags
- **Conditional base path** — frontend `vite.config.ts` uses `./` for Tauri, `/hyrr/` for web
- **Logo asset imports** — `HeaderBar` and `WelcomeScreen` use Vite `?url` imports for base-path-independent logo resolution

### Fixed

- **Projectile select styling** — added `appearance: none` with custom chevron for consistent rendering across Chrome and macOS WebKit (Tauri webview)

## [0.4.0] — 2026-03-16

### Added

- **`@hyrr/compute` shared package** — extracted physics engine into `packages/compute/` as an npm workspace, consumable by both frontend and Node.js tools
- **MCP server** (`mcp/`) — agent-driven irradiation analysis via Model Context Protocol; tools: `simulate`, `list_materials`, `get_cross_sections`, `get_decay_data`, `compare_results`; resources: `hyrr://libraries`, `hyrr://elements`
- **NodeDataStore** — filesystem-backed Parquet data store for CLI/MCP usage (`@hyrr/compute/node`)
- **Isotope popup: Theory/Real toggle** — depth plot switches between raw σ(E(x)) and actual production rate (atoms/s/cm) from simulation, fully layer/density/abundance-aware

### Changed

- **Production depth plot** moved directly below stopping profile for visual continuity
- **Frontend imports** rewired from local `compute/` to `@hyrr/compute` workspace package
- **Service worker cache versioning** — cache name now includes app version; version bump automatically purges old caches

## [0.3.3] — 2026-03-16

### Fixed

- **URL sharing** — shared config URLs now correctly override session restore instead of being silently overwritten (#29)
- **Preset loading** — clicking a preset (e.g. Tc-99m) now always triggers simulation, even if the same config was previously loaded (#30)
- **Material picker auto-focus** — search input is focused on open so users can type immediately; pressing Enter selects the first result with proper casing (#31)

### Changed

- **Mobile responsive styles** — added `@media` breakpoints at 640px/1024px across 9 components: full-width history drawer, grid beam config, larger touch targets, sticky table columns, hidden low-priority columns, full-screen modals on phone

## [0.3.1] — 2026-03-13

### Security

- **Worker origin enforcement** — disallowed origins rejected server-side (not just CORS headers)
- **Cloudflare Turnstile** (invisible CAPTCHA) required before anonymous issue creation via worker
- **Upload content-type validation** — allowlist `image/jpeg`, `image/png`, `image/webp` with magic byte verification; `Content-Disposition: inline` forced on served images
- **Input limits** — title ≤200 chars, body ≤10,000 chars, labels restricted to `["bug"]`
- **Config URL validation** — shape validation after decode (max 20 layers, finite numeric fields)
- **History import validation** — validates each entry before storing; skips malformed entries

### Changed

- **Bug report modal** — three-button flow: Cancel / Open on GitHub / Submit. Email required only for worker submit; GitHub route uses the user's own session
- **nucl-parquet** bumped to v0.3.4

## [0.2.0] — 2026-03-13

### Added

- **Bug report modal** with GitHub App integration — users can submit issues with screenshots directly from the app, no GitHub account required
- **Screenshot upload** with client-side downsampling (max 1280px, JPEG 80%) to Cloudflare R2
- **HYRR logo** in header bar and browser favicon (isometric wave visualization)
- **Session tabs** with Chrome-style tab bar for managing multiple configurations
- **Compare isotopes** in IsotopePopup — multi-channel XS overlay with superscript notation
- **Reaction notation column** in activity table
- **Custom materials** editor with cstm/enr badges on layer cards
- **RNP% calculation** in activity table (relative contribution per layer)
- **3D geometry module** — STEP import, tetrahedral meshing, ray casting (Python)
- **Energy straggling** and beam profile support (Python)
- **nucl-parquet** as external data source with configurable library selection

### Changed

- Repo moved from `MorePET/hyrr` to `exoma-ch/hyrr` — all links updated
- Bug report body now compact: reproducible config URL + one-line summary instead of full JSON dump
- Session tab heights reduced for cleaner header
- XS scaled by atomic abundance (stoichiometry x isotopic fraction)

### Fixed

- Numerically stable decay chain solver
- IsotopePopup XS plot rendering and deep config tracking
- Plotly reactivity and MaterialPopup crash
- Clamp unrealistic activity for long-lived isotopes
- Removed broken TENDL links (PSI server down)

### Removed

- Legacy SQLite build-db command
- Unused frontend components

## [0.1.0] — 2025-12-01

### Added

- Initial frontend: Svelte 5 + TypeScript + Vite
- Pure TypeScript physics compute (ported from Python)
- Lazy-loaded Parquet nuclear data via hyparquet
- URL hash config sharing
- IndexedDB history
- Layer stack builder, beam configuration, activity table
- IsotopePopup with cross-section and activity plots

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

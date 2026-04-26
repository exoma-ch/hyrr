# HYRR

**Hierarchical Yield and Radionuclide Rates**

A pure Python package for predicting radio-isotope production in stacked
target assemblies, using TENDL cross-section data and NIST stopping power
tables.

**[Web App](https://exoma-ch.github.io/hyrr/)** | **[Desktop App](https://github.com/exoma-ch/hyrr/releases)** | **[Documentation](https://exoma-ch.github.io/hyrr/docs/)**

Try it now — full simulation runs in the browser, no install, no data leaves your machine. Need offline access? Download the [desktop app](https://github.com/exoma-ch/hyrr/releases) for Windows, macOS, or Linux.

## What it does

- Stopping power via PSTAR/ASTAR table lookup (replaces Bethe-Bloch)
- Energy-integrated production rates for any projectile (p, d, t, ³He, α)
- Bateman equations for activity, yield, and decay chains
- Compound materials with natural or enriched isotopic composition
- Stacked layer geometries (windows, targets, degraders, backings)
- Depth-resolved heat and activity profiles

## Performance

HYRR is designed for interactive use — simulations are fast enough for real-time parameter sweeps:

| Operation | Time |
|---|---|
| Single isotope production rate | ~56 µs |
| Full layer simulation (all isotopes) | ~1.6 ms |

Compared to tools like Isotopia, HYRR is significantly faster and lighter: pure NumPy/SciPy with Parquet-backed nuclear data (no heavy ORM, no database server). The browser frontend achieves similar performance with a pure TypeScript compute engine.

## Installation

```bash
uv add git+https://github.com/exoma-ch/hyrr.git
```

## Quick start

```python
from hyrr import TargetStack, Layer, Beam

stack = TargetStack(
    beam=Beam(projectile="p", energy_MeV=30.0, current_mA=0.15),
    layers=[
        Layer(material=havar, thickness_cm=0.0025),
        Layer(material=enriched_mo100, energy_out_MeV=12.0),
        Layer(material=copper, thickness_cm=0.5),
    ],
)

result = stack.run(irradiation_time_s=86400, cooling_time_s=86400)
result.summary()
```

## Frontend

**[exoma-ch.github.io/hyrr](https://exoma-ch.github.io/hyrr/)** — hosted on GitHub Pages, zero backend.

The browser frontend (`frontend/`) is a standalone Svelte 5 + TypeScript app with a pure-TS physics engine (no Python/WASM). Nuclear data is lazy-loaded from Parquet files via hyparquet. All computation runs locally — no server, no data upload.

The physics engine is also published as `@hyrr/compute` (npm workspace under `packages/compute/`) for use in Node.js tools and the MCP server.

Key frontend features:
- **Repeating layer groups** — wrap layers into groups repeated N times or until beam energy drops below a threshold
- **Undo/redo** — Cmd+Z / Cmd+Shift+Z with 50-deep history
- **Simulation mode** — Auto (live) or Manual (run on demand) with status indicator
- **URL sharing + sessions** — full config (including groups) encoded in URL hash; session tabs persist across reloads
- **Isotope filter** — shared filter bar above activity plots and activity table

## Install channels

HYRR ships through one channel per surface. Pick the one that matches what you actually want:

| You want… | Use | Command |
|---|---|---|
| The desktop GUI | GitHub Releases | [Download installer](https://github.com/exoma-ch/hyrr/releases) (`.dmg` / `.msi` / `.deb` / `.AppImage`) |
| The MCP server, no GUI | uvx (PyPI) | `claude mcp add hyrr -- uvx hyrr-mcp` |
| The MCP server, already have desktop | desktop binary | `claude mcp add hyrr -- /Applications/HYRR.app/Contents/MacOS/hyrr --mcp` |
| The Python library | pip / uv | `pip install hyrr` |
| The browser app | static GitHub Pages | [hyrr.app](https://exoma-ch.github.io/hyrr/) |
| Build the MCP from source (devs) | cargo | `cargo install hyrr-mcp` |

See [docs/adr/0001-mcp-single-ssot-and-install-channels.md](docs/adr/0001-mcp-single-ssot-and-install-channels.md) for the rationale behind the split.

## MCP Server

Agent-driven irradiation analysis via the Model Context Protocol. All entry points share the same Rust codepath (`core/src/mcp/`) — adding a tool means editing one file and every surface picks it up on next build.

Tools: `simulate`, `list_materials`, `list_reaction_channels`, `get_decay_data`, `compare_simulations`, `get_stack_energy_budget`, `get_stopping_power`, `get_isotope_production_curve`. Every response footer carries `*Library: <id>*` so agents see which nuclear data fed the calculation.

## Desktop App

**[Download](https://github.com/exoma-ch/hyrr/releases)** — available for Windows, macOS (Apple Silicon & Intel), and Linux.

The desktop app (`desktop/`) wraps the same frontend in a native window using [Tauri v2](https://v2.tauri.app/). All nuclear data (~68 MB Parquet) is bundled, so it works fully offline on air-gapped machines. Built with the system webview — the installer is ~15 MB.

| Platform | Artifact |
|---|---|
| Windows 10+ | `.msi` installer + `.exe` (NSIS) |
| macOS 10.15+ | `.dmg` (Apple Silicon) / `.dmg` (Intel) |
| Ubuntu 22.04+ | `.deb` + `.AppImage` |

Releases are built automatically via GitHub Actions on version tags (`v*`).

## Development

```bash
git clone --recurse-submodules https://github.com/exoma-ch/hyrr.git
cd hyrr
uv sync --all-extras
uv run pytest
```

Frontend:

```bash
cd frontend
npm ci
npm run dev
npm test          # vitest
npm run check     # svelte-check (TypeScript)
```

Desktop (requires [Tauri CLI](https://v2.tauri.app/start/prerequisites/) and Rust):

```bash
npm install -g @tauri-apps/cli
cd desktop && npx tauri dev
```

## Contributing

1. Fork and create a feature branch
2. **Python:** `uv sync --all-extras`, then `uv run pytest` and `uv run ruff check src/`
3. **Frontend:** `cd frontend && npm ci`, then `npm test` and `npm run check`
4. Commit format: `type(scope): description`
5. Open a PR against `main`

## Dependencies

- numpy, scipy — numerics
- polars — data access (Parquet backend)
- matplotlib — plotting
- py-mat — material definitions
- nucl-parquet — evaluated nuclear data (TENDL, ENDF/B, JENDL, JEFF, EXFOR)

## About eXoma

**eXoma** — *Exotic Matter Applications* — is a research group at ETH Zürich focused on novel radioisotope production methods and targetry.

## License

MIT

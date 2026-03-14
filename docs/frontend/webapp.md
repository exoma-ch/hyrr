# Web App

**[Launch HYRR](https://exoma-ch.github.io/hyrr/)** — the full simulation runs in your browser.

## Overview

The HYRR web app is a standalone browser application for radioisotope production calculations. No install, no server, no data leaves your machine.

## Features

- **Beam configuration** — proton, deuteron, triton, helion, and alpha projectiles with configurable energy and current
- **Layer stack builder** — add/remove/reorder target layers with material picker, thickness, and enrichment controls
- **Real-time simulation** — results update instantly as you change parameters
- **Activity table** — isotope yields, activities (EOB/EOC), saturation activity, RNP%
- **Cross-section plots** — click any isotope to see XS curves with multi-channel overlay
- **Depth profiles** — spatially resolved production rates and energy deposition
- **URL sharing** — share your exact configuration via a compressed URL hash
- **History** — all runs saved locally in IndexedDB, exportable as JSON
- **Light/dark theme** — auto-detects system preference

## Supported nuclear data

Default library: **TENDL-2024**. Cross-sections, stopping powers, abundances, and decay data are loaded as Parquet files via hyparquet and cached in the browser.

## Bug reports

Use the built-in bug report button (bottom-right). Two options:

- **Open on GitHub** — for users with a GitHub account (no email needed)
- **Submit** — anonymous submission via Cloudflare Worker (requires email, protected by Turnstile)

Config, simulation state, and browser info are attached automatically.

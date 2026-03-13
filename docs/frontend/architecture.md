# Frontend Architecture

The frontend is a **Svelte 5 + TypeScript** single-page app built with Vite. It implements the same physics as the Python library in pure TypeScript — no Python, no WASM, no backend.

## Tech stack

| Layer | Technology |
|---|---|
| Framework | Svelte 5 (runes: `$state`, `$derived`, `$effect`) |
| Language | TypeScript (strict) |
| Build | Vite |
| Charts | Plotly.js |
| Data | hyparquet (Parquet in the browser) |
| Storage | IndexedDB (history), URL hash (sharing) |
| Hosting | GitHub Pages (static, zero backend) |

## Compute engine

```
frontend/src/lib/compute/
    production.ts    — ∫σ/dEdx numerical integration
    chains.ts        — Bateman decay chain solver (matrix exponential)
    stopping.ts      — PSTAR/ASTAR interpolation + Bragg additivity
    interpolation.ts — log-log spline for XS and dE/dx
    matrix-exp.ts    — Padé approximant for decay matrix
```

The TypeScript engine produces results identical to the Python library. A single simulation completes in ~1-2 ms.

## Data pipeline

1. **Build time** — Parquet files from nucl-parquet copied to `public/data/parquet/`
2. **Runtime** — hyparquet reads Parquet directly in the browser (no server)
3. **Caching** — Service worker + IndexedDB cache parsed data for instant reload

## Reactive architecture

```
User input → config store ($state)
                ↓
         scheduler (debounced)
                ↓
         compute engine (sync, ~1ms)
                ↓
         result store ($state)
                ↓
         UI components (auto-update via $derived)
```

## Development

```bash
cd frontend
npm ci
npm run dev       # dev server with HMR
npm test          # vitest
npm run check     # svelte-check (TypeScript)
npm run build     # production build
```

# Architecture

HYRR has two independent compute implementations that share the same physics and nuclear data:

1. **Python library** (`src/hyrr/`) — for scripting, notebooks, CLI
2. **TypeScript frontend** (`frontend/`) — for the browser app at [exoma-ch.github.io/hyrr](https://exoma-ch.github.io/hyrr/)

## Python module graph

```
models.py <- (all modules depend on models)
    |
    +-- db.py (DatabaseProtocol + DataStore, Parquet/Polars backend)
    |     |
    |     +-- stopping.py (PSTAR/ASTAR lookup + Bragg additivity)
    |     |     |
    |     |     +-- production.py (integral-sigma/dEdx integration + Bateman equations)
    |     |
    |     +-- materials.py (py-mat bridge + isotopic resolution)
    |
    +-- plotting.py (consumes result types only)
    |
    +-- output.py (consumes result types only)
    |
    +-- cli.py (wires everything together)
```

## Frontend architecture

```
frontend/src/lib/
    |
    +-- compute/          Pure TypeScript physics engine
    |     +-- production  ∫σ/dEdx integration
    |     +-- chains      Bateman decay equations
    |     +-- stopping    PSTAR/ASTAR interpolation
    |
    +-- stores/           Svelte 5 rune stores (config, results, history)
    +-- components/       UI components (layer table, beam config, charts)
    +-- scheduler/        Reactive simulation scheduler
```

- **No Python/WASM** — physics ported to TypeScript for instant startup
- **Parquet via hyparquet** — nuclear data lazy-loaded from static assets, cached in IndexedDB
- **URL hash sharing** — compressed config encoding (`#config=1:<deflate-base64>`)
- **Cloudflare Worker** — bug report proxy with Turnstile verification

## Key design decisions

| Decision | Choice | Rationale |
|---|---|---|
| Data storage | Parquet (Polars) | Columnar, fast indexed lookups, single code path for pip and browser |
| Materials | py-mat | Already provides density + composition |
| Stopping powers | PSTAR/ASTAR | NIST reference data, more accurate than Bethe-Bloch at cyclotron energies |
| Frontend framework | Svelte 5 | Runes, small bundle, no virtual DOM |
| Frontend data | hyparquet | Read Parquet directly in the browser, no server needed |
| Dependency injection | `DatabaseProtocol` | Enables testing with mocks |

## Data layout

The nucl-parquet data directory contains:

- `meta/abundances.parquet` — IUPAC isotopic abundances (~290 rows)
- `meta/decay.parquet` — ENDF-6 decay properties (~5.5K rows)
- `meta/elements.parquet` — Element symbols and Z
- `stopping/stopping.parquet` — NIST PSTAR/ASTAR tables (~19K rows)
- `xs/{projectile}_{element}.parquet` — TENDL/IAEA residual production cross-sections

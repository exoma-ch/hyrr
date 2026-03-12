# Architecture

## Module dependency graph

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

## Key design decisions

| Decision | Choice | Rationale |
|---|---|---|
| Data storage | Parquet (Polars) | Columnar, fast indexed lookups, single code path for pip and WASM |
| DataFrames | Polars | Faster, better API, no legacy baggage |
| Materials | py-mat | Already provides density + composition |
| Stopping powers | PSTAR/ASTAR | NIST reference data |
| Dependency injection | `DatabaseProtocol` | Enables testing with mocks |

## Data layout

The parquet data directory contains:

- `meta/abundances.parquet` — IUPAC isotopic abundances (~290 rows)
- `meta/decay.parquet` — ENDF-6 decay properties (~5.5K rows)
- `meta/elements.parquet` — Element symbols and Z
- `stopping/stopping.parquet` — NIST PSTAR/ASTAR tables (~19K rows)
- `xs/{projectile}_{element}.parquet` — TENDL/IAEA residual production cross-sections

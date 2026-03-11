# Architecture

## Module dependency graph

```
models.py ← (all modules depend on models)
    │
    ├── db.py (DatabaseProtocol + HyrrDatabase)
    │     │
    │     ├── stopping.py (PSTAR/ASTAR lookup + Bragg additivity)
    │     │     │
    │     │     └── production.py (∫σ/dEdx integration + Bateman equations)
    │     │
    │     └── materials.py (py-mat bridge + isotopic resolution)
    │
    ├── plotting.py (consumes result types only)
    │
    ├── output.py (consumes result types only)
    │
    └── cli.py (wires everything together)
```

## Key design decisions

| Decision | Choice | Rationale |
|---|---|---|
| Data storage | SQLite (stdlib) | Indexed lookups, zero deps, single file |
| DataFrames | Polars | Faster, better API, no legacy baggage |
| Materials | py-mat | Already provides density + composition |
| Stopping powers | PSTAR/ASTAR | NIST reference data |
| Dependency injection | `DatabaseProtocol` | Enables testing with mocks |

## Database schema

The SQLite database contains 4 main tables:

- `cross_sections` — TENDL/IAEA residual production cross-sections (~20M rows)
- `stopping_power` — NIST PSTAR/ASTAR tables (~19K rows)
- `natural_abundances` — IUPAC isotopic abundances (~290 rows)
- `decay_data` — ENDF-6 decay properties (~5.5K rows)

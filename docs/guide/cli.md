# CLI Reference

HYRR provides a command-line interface for data management and simulations.

## Commands

### `hyrr info`

Show data store statistics.

```bash
hyrr info --data-dir data/parquet
```

### `hyrr run`

Run a simulation from a TOML input file.

```bash
hyrr run simulation.toml --data-dir data/parquet
```

### `hyrr download-data`

Download pre-built parquet data files.

```bash
hyrr download-data --output-dir ~/.hyrr/
```

### `hyrr generate-xs`

Generate cross-section data using TALYS (requires TALYS installation).

```bash
hyrr generate-xs --projectile p --target Mo --energy-range 5-50
```

# CLI Reference

HYRR provides a command-line interface for database management and simulations.

## Commands

### `hyrr build-db`

Build the SQLite database from raw nuclear data sources.

```bash
hyrr build-db \
  --tendl-path ../curie/isotopia.libs/ \
  --abundance-path ../curie/isotopia/files/abundance/ \
  --decay-path ../curie/isotopia/files/decay/ \
  -o data/hyrr.sqlite -v
```

### `hyrr info`

Show database statistics.

```bash
hyrr info --db data/hyrr.sqlite
```

### `hyrr run`

Run a simulation from a TOML input file.

```bash
hyrr run simulation.toml --db data/hyrr.sqlite
```

### `hyrr download-data`

Download pre-built data files.

```bash
hyrr download-data --output-dir data/raw/
```

# Installation

## From PyPI (when released)

```bash
pip install hyrr
```

## From source

```bash
git clone https://github.com/MorePET/hyrr.git
cd hyrr
pip install -e ".[dev]"
```

## Database setup

HYRR requires a SQLite database with nuclear data. Build it from source data:

```bash
hyrr build-db \
  --tendl-path path/to/isotopia.libs/ \
  --abundance-path path/to/abundance/ \
  --decay-path path/to/decay/ \
  --output data/hyrr.sqlite
```

Or download a pre-built database:

```bash
hyrr download-data
```

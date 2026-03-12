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

## Data setup

HYRR requires parquet data files with nuclear data. Download pre-built data:

```bash
hyrr download-data
```

Or build from source data:

```bash
python data/build_parquet.py \
  --tendl-path path/to/isotopia.libs/ \
  --abundance-path path/to/abundance/ \
  --decay-path path/to/decay/ \
  --output-dir data/parquet
```

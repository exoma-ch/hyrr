# Contributing

## Development setup

```bash
git clone https://github.com/MorePET/hyrr.git
cd hyrr
pip install -e ".[all]"
```

## Running tests

```bash
# Unit tests
pytest tests/ -v --benchmark-disable

# With benchmarks
pytest tests/ -v --benchmark-only

# Integration tests (requires database)
pytest tests/integration/ -v
```

## Code quality

```bash
# Linting
ruff check src/ tests/ data/

# Type checking
pyright src/hyrr/

# Build docs locally
mkdocs serve
```

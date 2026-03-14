# Contributing

## Development setup

### Python

```bash
git clone --recurse-submodules https://github.com/exoma-ch/hyrr.git
cd hyrr
uv sync --all-extras
```

### Frontend

```bash
cd frontend
npm ci
npm run dev
```

## Running tests

### Python

```bash
# Unit tests
uv run pytest tests/ -v --benchmark-disable

# With benchmarks
uv run pytest tests/ -v --benchmark-only

# Integration tests
uv run pytest tests/integration/ -v
```

### Frontend

```bash
cd frontend
npm test             # vitest
npm run check        # svelte-check (TypeScript)
```

## Code quality

```bash
# Python linting
uv run ruff check src/ tests/

# Build docs locally
uv run mkdocs serve

# Frontend type checking
cd frontend && npm run check
```

## Commit format

```
type(scope): description
```

Examples: `feat(frontend):`, `fix(worker):`, `docs:`, `chore:`, `build(ci):`

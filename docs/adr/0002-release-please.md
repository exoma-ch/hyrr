# ADR 0002: Adopt release-please for automated versioning

**Status:** Accepted (2026-05-22)
**Implements:** #192
**PR:** #295

## Context

Between v0.7.0 (2026-03-27) and v0.8.0 (2026-05-13), ~80 PRs merged with
no automated CHANGELOG tracking. The v0.8.0 entry was hand-written
retroactively — accurate but unsustainable. Version strings live in 6 files
across 4 languages (Python, JSON, TOML, Rust) and must stay in sync.

## Decision

Adopt **release-please** (Option A from #192) with a single-package
monorepo config. All version files are synced from a single source of
truth in `.release-please-manifest.json`.

### Why release-please over alternatives

| Option | Verdict |
|---|---|
| **A. release-please** | Tracks conventional commits automatically; one config file; GitHub-native (action + API); the release PR is editable before merge |
| B. changesets | Per-PR `.changeset/*.md` — extra ceremony on every PR for a small team |
| C. git-cliff | Good for CHANGELOG generation, but doesn't handle version bumps or release creation |
| D. Status quo | Drift-prone, doesn't scale |

### How it works

```
feat: add α emission tab  →  release-please scans commits
fix: β spectrum dips       →  creates/updates "Release v0.9.0" PR
                           →  PR has CHANGELOG + version bumps in 6 files
                           →  merge PR → tag v0.9.0 created
                           →  tag triggers: release.yml (PyPI)
                                            tauri-build.yml (desktop)
                                            release-hyrr-mcp.yml (MCP wheel)
                           →  manual: promote-to-prod.yml (web)
```

### Version file sync

| File | Updater | Package |
|---|---|---|
| `pyproject.toml` | python (native) | root |
| `frontend/package.json` | json (extra-file) | root |
| `desktop/src-tauri/tauri.conf.json` | json (extra-file) | root |
| `core/Cargo.toml` | toml (extra-file) | root |
| `hyrr-mcp/Cargo.toml` | toml (extra-file) | root |
| `py-mcp/pyproject.toml` | toml (extra-file) | root |

### Tag scheme

- **Root package:** `v{VERSION}` (e.g. `v0.9.0`) — triggers `release.yml` + `tauri-build.yml`
- **MCP wheel:** `hyrr-mcp-v{VERSION}` — created by `release-please.yml` as a follow-up step, triggers `release-hyrr-mcp.yml`

Both tags point at the same commit and share the same version number.

### Branch strategy

GitHub Flow: feature branches → PR to `main` (protected) → release-please
manages releases from main. No `dev` branch.

## Consequences

- Every PR to main should use conventional commit format (`feat:`, `fix:`,
  `test:`, `chore:`, etc.) for accurate CHANGELOG generation
- The release PR is auto-generated but editable — maintainer can rewrite
  the body before merging for narrative-style release notes
- `release.yml` no longer creates the GitHub Release (release-please does);
  it only handles PyPI publishing
- `tauri-build.yml` uploads artifacts to the existing release (not draft)
- Manual version bumps are no longer needed — release-please owns all 6 files

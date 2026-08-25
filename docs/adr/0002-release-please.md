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

```text
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

#### How the second tag is created (amended 2026-08-25, #676)

`hyrr-mcp-v{VERSION}` is created by `scripts/tag-mcp-release.sh` through
`POST /repos/{owner}/{repo}/git/refs`, at the commit the `v{VERSION}` tag
names, immediately after the release step. It used to be `git tag && git push`
from a `fetch-depth: 0` checkout that ran *after* the release was published.

On 0.21.0 that push was rejected:

```text
! [remote rejected] hyrr-mcp-v0.21.0 -> hyrr-mcp-v0.21.0
  (refusing to allow a GitHub App to create or update workflow
   `.github/workflows/tauri-build.yml` without `workflows` permission)
```

The obvious reading — "the release tree touched `.github/workflows/`" — is
wrong. The run checked out `9fd188a`, the correct release commit, which
changes no workflow file. It was a **race**:

| time | event |
| --- | --- |
| 22:34:39 | release-please publishes the Release; `main` is at `9fd188a` |
| 22:34:42 | #672 merges — the one commit editing `tauri-build.yml` — `main` becomes `aedc4d3` |
| 22:35:11 | the tag step (after a 12 s checkout) asks for a ref at `9fd188a`, which is no longer the tip and whose workflow tree now differs from it. Refused. |

**The rule, measured against this installation rather than assumed** (#676
asked for exactly that): a GitHub App without `workflows` may create or move a
ref only where doing so does not change `.github/workflows/**` relative to the
default branch's tip. On a fresh non-tip commit that edits a workflow file,
with the `hyrr-release-bot` token, every route is refused:

| route | result |
| --- | --- |
| `git push` of the tag | remote rejected, message above |
| `POST /git/refs` | 403 Resource not accessible by integration |
| `POST /releases` (`tag_name` + `target_commitish`) | 403 |
| create at the tip, then force-`PATCH` onto the commit | 403 |

At the tip, all four succeed. So **#676's option 3 does not hold** — the REST
ref API is subject to the same check and merely reports it as a bare 403.
Separately confirmed, because it is the other half of the requirement: a tag
ref created over the API by this App token *does* fire `on: push: tags:`
workflows (three tags, three listener runs), so nothing here re-introduces the
GITHUB_TOKEN problem #571 rejected.

Since no credential-preserving route exists, the design attacks the race and
the silence instead:

1. **Shrink the window.** The checkout is hoisted above `release-please`, and
   the ref is created over the API (no working tree, no credentialed
   checkout), so the tag is asked for ~1 s after the Release rather than ~32 s.
   A smaller target, not an eliminated one.
2. **Anchor on the release.** The commit comes from the `v*` tag, never from
   `main`, so the MCP tag can never name a commit that was not released.
3. **Read back.** Both tags are re-read from the API after the create; a 201
   is a claim, the ref listing is the fact.
4. **Assert the trigger.** `release-hyrr-mcp.yml` must have a run for the tag.
5. **Detect drift.** Every push to `main` re-checks that the latest release has
   both tags on the same commit, so a half-tagged release cannot stay green —
   which is what let 0.21.0 hide.
6. **Recover in one click.** `workflow_dispatch` with `retag: v0.21.0`.
7. **Warn before the fact.** The release PR runs `preflight`, which builds an
   unreferenced commit touching `.github/workflows/` and tries to point a ref
   at it — the same shape that fails at release time — and warns if the
   credential is refused.

The permanent fix is #676's option 1: grant `hyrr-release-bot` the `workflows`
permission. It is the only thing that makes the refused case succeed. It is
also a real privilege increase — the release bot could then rewrite CI — so it
is left as a maintainer decision, and `preflight` keeps saying so until it is
made.

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

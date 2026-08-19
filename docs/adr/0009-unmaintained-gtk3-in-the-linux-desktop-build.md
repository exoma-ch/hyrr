# ADR 0009 — Unmaintained GTK3 ships in the Linux desktop build

- **Status**: accepted
- **Date**: 2026-08-19
- **Implements**: closes #640
- **Scope**: `deny.toml`, `desktop/src-tauri`, the nightly `cargo-deny` sweep
- **Applies to**: the Linux desktop target only — the browser, MCP, Python and
  macOS/Windows desktop surfaces are unaffected

## Context

The scheduled `cargo-deny` run failed four consecutive nights (16–19 Aug 2026)
on `desktop/src-tauri` with 16 advisories. Every other crate — `core`, `wasm`,
`py`, `py-mcp`, `hyrr-mcp` — was clean throughout.

All 16 are **`unmaintained`, not vulnerabilities**. There is no CVE, no known
exploit and no attacker-reachable defect. The advisory class means exactly one
thing: upstream archived the repository.

They fall into three families, all transitive through Tauri:

| Family | Count | Path | Runtime or build |
| --- | --- | --- | --- |
| gtk-rs GTK3 bindings | 10 | `gtk 0.18.2` ← `muda` ← `tauri 2.11.5` | **runtime** |
| `unic-*` | 5 | `urlpattern` ← `tauri-utils` ← `tauri-codegen` ← `tauri-macros` | build only |
| `proc-macro-error` | 1 | `glib-macros` ← `glib` ← `atk` ← `gtk` | build only |

`tauri-macros` and `glib-macros` are proc-macro crates, so those six run at
compile time and are never linked into a shipped binary. The ten GTK3 bindings
are different: they are **real runtime code in the Linux desktop artifact**.
That distinction is the substance of this decision, and blurring it would be
the easy mistake.

## Decision

**Accept the GTK3 advisories and keep the Linux desktop target.** Record them
as justified ignores in `deny.toml`, one entry per advisory, each naming the
crate, the dependency path and the advisory URL.

### Why there is nothing to upgrade to

Tauri v2's Linux backend **is** GTK3 plus webkit2gtk. gtk-rs 0.19 and later
target GTK4, which Tauri does not use. So this is not a stale pin we could bump:
no version of these crates exists that is both maintained and compatible with
the framework. It resolves when Tauri itself migrates, and not before.

### Why "just drop Linux desktop" was rejected

It is the only other lever that actually removes the code, and it costs more
than it buys. The Linux desktop build is how the app runs natively on the
workstations at accelerator and hospital sites — the same population that
depends on the air-gapped install path (ADR 0007). Removing a working platform
to clear an advisory that describes maintenance status rather than a defect
trades real capability for a cleaner report.

### Why the ignores are annotated rather than bare

`deny.toml` already said it: *"a bare id with no justification is how an ignore
list rots into a blindfold."* Each entry therefore carries the crate, the path
that pulls it in, and a link. When Tauri moves to GTK4, cargo-deny reports the
entries as unused and they should be deleted — the annotation is what makes
that a five-minute review rather than an archaeology exercise.

## Consequences

- The nightly sweep goes green, which is the point. A gate that is red every
  morning is one people stop reading, and four silent nights had already
  demonstrated that.
- **We are knowingly shipping unmaintained runtime code on one platform.** This
  is the cost. It is bounded to the Linux desktop artifact and does not reach
  the browser app, the MCP server, the Python bindings, or the macOS/Windows
  desktop builds.
- If a *vulnerability* (rather than an unmaintained notice) is ever published
  against the GTK3 tree, this decision does not cover it. The ignores are
  per-advisory-ID, so a new ID fails the sweep exactly as it should. That is a
  deliberate property of listing IDs rather than silencing the crates.
- The five Tauri-free crates now emit 16 `advisory-not-detected` warnings each,
  because one shared `deny.toml` covers six independent crates. Nothing fails,
  but "this ignore is unused" is no longer readable off a clean crate's log —
  check the `desktop/src-tauri` job, where these IDs are live.

### Rejected: a per-crate `deny.toml`

It would silence those warnings, but `cargo-deny` takes one `--config`, so it
means a second file duplicating the license and bans policy. A drifting
duplicate of the *licensing* policy is a worse failure than log noise.

## Re-check triggers

- **Any Tauri upgrade.** Re-run `cargo deny --manifest-path
  desktop/src-tauri/Cargo.toml check advisories` and delete whatever no longer
  matches.
- **A new RUSTSEC ID in this tree**, which will fail the sweep rather than being
  covered by the existing entries.
- **Tauri announcing GTK4 support**, which retires the ten runtime entries
  outright.

## References

- Issue #640 (nightly sweep), PR #665 (this triage)
- `deny.toml` — the annotated ignore list
- ADR 0007 — the air-gapped users this platform serves
- <https://rustsec.org/advisories/RUSTSEC-2024-0415> (representative of the
  GTK3 family)

---
title: "cargo-deny: new RustSec advisory affecting a hyrr crate"
labels: dependencies, rust
---

The nightly `cargo-deny` sweep failed on the `advisories` check.

**Run:** {{ env.RUN_URL }}

This issue is updated in place while the advisory is outstanding, so it will
not spam a new issue every night.

## What this means

Advisories are deliberately **non-blocking on pull requests** and blocking only
on this scheduled run — a new advisory can land with no code change on our side,
and stranding unrelated PRs behind a transitive vulnerability with no upstream
fix is how a gate becomes something people bypass. So nothing is broken right
now; this is a queue item, not an incident.

## Triage

Open the run above and read the finding. Then pick one:

1. **A fix exists upstream** — bump the dependency. Remember there is no cargo
   workspace: the affected crate has its own `Cargo.lock`, and if the bump
   touches `hyrr-core` it must be repinned in all six
   (`scripts/check-lockfiles.sh` walks the set).
2. **No fix exists, and it is not reachable from our code** — add an entry to
   `deny.toml`'s `[advisories].ignore` **with a `reason` that says why it is
   unreachable, and a link**. A bare advisory id with no justification is how an
   ignore list rots into a blindfold. Re-check when a fix ships.
3. **No fix exists and it IS reachable** — that is a real problem. Assess
   whether it affects the published wheel, the desktop installer, or only a dev
   path, and decide accordingly.

## Note on `unmaintained` advisories

RustSec flags unmaintained crates through the same channel as vulnerabilities.
Those are worth knowing about but are rarely urgent — do not treat an
`unmaintained` finding as though it were an exploitable bug.

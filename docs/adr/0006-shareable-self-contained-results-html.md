# ADR 0006 — Shareable self-contained results HTML

- **Status**: proposed (2026-08-13) — **Tier B is blocked on #421**; see
  "The licensing boundary" below
- **Date**: 2026-08-13
- **Relates to**: #421 (dual-use / export-control determination), #425 (epic —
  legal & licensing readiness), #565 (refuse to deploy a stale ETH access
  whitelist), #539 / ADR-0004 (config codec — what is *not* reusable here),
  #550 (`.hyrr.json` v3 self-contained round-trip — the payload precedent)

## Context

`hyrr.ethz.ch` is gated behind SWITCH AAI (Shibboleth) and restricted to an
explicit allowlist. The gate is not incidental infrastructure — it is the
enforcement mechanism for nuclear-data licensing:

> "Access gating (nuclear-data licensing) … gated behind SWITCH AAI
> (Shibboleth) and limited to a whitelist … The whitelist lives in
> `infra/eth-webhosting/whitelist.txt`"
> — `infra/README.md:44-58`

The landing page states the same to users, and is where the term this ADR
answers to comes from: *"you'll be added to the allow-list and notified by
email"* (`landing/index.html:133`). Enforcement is `.htaccess` with
`AuthType shibboleth` + `Require shib-attr`, written by `scripts/deploy-eth.sh`
from `WHITELIST_B64` (sops).

The consequence: **a collaborator who is not on the allowlist cannot load the
app at all**, so they cannot look at a result someone else produced — even
though the result is the output of *our* run, and even though the physics
engine itself is public.

Today's export surfaces do not close that gap:

| Artifact | Carries result? | Viewable by a non-user? |
|---|---|---|
| `#config=` URL (ADR-0004) | ❌ config only, 2 KB budget, 1 MiB decode cap | needs the gated app |
| `.hyrr.json` session (v3) | ✅ config + result | needs the gated app to open |
| CSV / Parquet per plot | ✅ one trace | needs their own tooling |
| MCP Parquet resources | ✅ | needs an MCP client + tooling |
| PNG (Plotly modebar) | ✅ one static image | yes — but inert |

So the only artifact an external can *use* today is a PNG. There is no HTML or
report generation anywhere in the repo: no templating engine in `core/`
(deliberate), no print stylesheet, no PDF path.

## Decision

Emit a **single self-contained HTML file** that embeds a result snapshot and a
read-only build of the existing results UI, so a recipient can open it from
disk — no server, no network, no install — and *explore* the result
(filter, sort, select, switch axes) but not re-run or re-tune it.

### Why this is cheap

The results components were already written prop-driven off `{ result }` and
trigger no recompute. Every interaction worth giving an external is **already
pure client-side state**: text/Z/A/layer filtering
(`stores/isotope-filter.svelte.ts`), isotope selection
(`stores/selection.svelte.ts`), sort and group-by-isotope
(`ActivityTableEnhanced.svelte:253-269`), log axes, EOB↔EOC switching, and RNP%
mode. None of it touches the engine.

Only three couplings stand between that and a static file:

| Coupling | Severity | Resolution |
|---|---|---|
| `getEmissions(Z,A,state)` — emission spectra | hard | snapshot rows at export (**Tier B**) |
| `getDoseConstant(...)` — dose column | soft; degrades to an 8-isotope hardcoded table (`utils/dose-constants.ts:21-24`) | embed the needed rows |
| `getDepthPreview()` — x-axis extent | trivial | pre-bake into the snapshot |

`IsotopePopup`'s cross-section drilldown genuinely needs the engine and is
**out of scope** — it is disabled in the viewer. Its "Real" depth plot
(`IsotopePopup.svelte:585-679`) reads `depth_production_rates` straight from
the result and may stay.

### The licensing boundary — the crux

The tiers below are not a technical dial. They sit on opposite sides of the
control the Shibboleth gate enforces.

- **Tier A — view-only, zero evaluated data.** Embeds the `SimulationResult`
  plus the depth-preview payload. Yields the activity table, activity-vs-time,
  depth profile, production-vs-depth, and all filtering/sorting/selection.
  Activities, yields, and curves are **outputs of our run**; nothing from
  TENDL / ENDF / ENSDF is embedded. Ships without a licensing determination.

- **Tier B — Tier A + emission spectra.** Adds γ / α / β± / CE / X-ray / Auger
  stick spectra by snapshotting ENSDF emission rows **for the produced
  nuclides only** (kilobytes). This is materially more useful — judging a
  result usually means judging its gamma lines. It also **redistributes
  evaluated decay data to recipients specifically outside the gate that exists
  to restrict it.**

- **Tier C — tunable.** Would require cross-sections, stopping tables, and
  WASM: effectively shipping the gated application to non-gated recipients.
  **Rejected.** It contradicts the licensing rationale for the allowlist
  outright, and no export-shaped framing changes that.

**Decision: implement Tier A now; Tier B is gated on #421.**

Tier B is the chosen target, but #421 (dual-use / export-control
determination) is open, so the determination it depends on does not yet exist.
The build must therefore keep the emission snapshot behind a **single explicit
switch** — one flag at export time and one embedded key — so Tier B can be
enabled by decision rather than by refactor, and so an artifact's tier is
auditable from the file itself.

This ADR does not assert that Tier B is permissible. It asserts that the
architecture must make the question answerable and the answer enforceable.

### Degrade, don't gate

Because the recipient is by construction outside the allowlist, the artifact
carries its own path back in:

- a `#config=` deep link (2 KB, config-only — already fits, ADR-0004) so an
  *allowlisted* colleague can reproduce the run;
- a link to the landing page's request-access flow, which already feeds the
  whitelist (`landing/index.html:133`);
- a link to the Releases page for the desktop app, which ships its own data.

## Architecture

A **second Vite entry** producing a single inlined `viewer.html` template, with
a stub `DataStore` served from the embedded snapshot rather than Parquet.

- **Template**: built once, all JS/CSS inlined, one substitution token for the
  payload. No `vite-plugin-singlefile` in the tree today — the inlining step is
  new.
- **Payload**: the existing `SessionFile` shape (`session-io.ts:28`, v3 —
  `{config, result, display, customMaterials}`) plus the depth preview, the
  dose-constant rows, and — under Tier B only — the emission rows. Reusing
  `buildSessionFile` avoids inventing a second result format.
- **Plotly**: the result plots use only `scatter` (22 sites) and `bar` (1), so
  the viewer builds against the **basic** distribution: 4.4 MB → ~2 MB.

Budget: **~2.5 MB raw / ~800 KB gzipped**, of which the result data is 10–30 KB
(a 1.3 MB result JSON gzips to ~30 KB; every isotope redundantly repeats an
identical 200-point `time_grid_s`, so deduping it per layer saves ~40% before
compression).

Emission across all three surfaces needs no new infrastructure:

- **Web** — reuse `triggerDownload` (`plotting/csv-export.ts:14`).
- **Tauri** — `dialog.save()` + `writeTextFile`; permissions already granted
  (`capabilities/default.json:11-13`), pattern proven at
  `BugReportModal.svelte:319-343`.
- **MCP** — `include_str!` the built template and substitute one token. This is
  exactly how `release_notes.rs:46` already ships `release-notes.json`, so
  `core/` gains **no templating engine** — the deliberate absence noted in
  Context is preserved.

### The MCP template must live in the leaf crate, not in `core`

The viewer template becomes a build input to a Rust crate. **Where** it is
`include_str!`d is not a detail — putting it in `core/` creates a build-graph
cycle:

```text
core  →  needs frontend/dist/viewer.html
      →  needs `npm run build`
      →  needs hyrr-wasm  →  wasm/  →  core
```

`core` is depended on by `wasm`, `hyrr-mcp`, `desktop/src-tauri`, `py`, and
`py-mcp` (there is no cargo workspace; each crate is standalone with path
deps). So `include_str!` in `core` would also embed ~2.5 MB into **every**
consumer — the WASM bundle, the PyO3 wheel, the desktop binary — none of which
need a viewer. Feature-gating behind `mcp` fixes the bloat but not the cycle.

**Decision: the template lives in `hyrr-mcp/`, a leaf crate.** Nothing depends
on `hyrr-mcp`, so the graph stays a DAG:

```text
core  →  wasm  →  frontend  →  hyrr-mcp
```

The hermetic build is the remaining cost, and the existing `include_str!`
precedent shows why. `flake.nix:133` builds core with
`craneLib.cleanCargoSource ./core`, which **strips every non-Rust file**;
`release-notes.json` survives only because `provisionSiblings`
(`flake.nix:118-123`) materialises it in the sandbox from a pure Nix input:

> "`core/src/release_notes.rs` also `include_str!`s `../../release-notes.json`
> (#572) — a COMPILE-time read, so its absence breaks the build outright, not
> just a test."

That trick works for a **committed, small, static** file. A vite-generated
2.5 MB template is none of those, so the MCP surface must pick one of:

1. **commit the built template** — a pure Nix input like `release-notes.json`,
   at the cost of a large generated artifact in git that must be regenerated
   in lockstep (a stale one silently ships a wrong-version viewer);
2. **generate it at release time** in CI and attach it to the build;
3. **give `hyrr-mcp` a `build.rs`** that emits a stub when the template is
   absent, so a plain `cargo build` never requires a frontend build.

(3) is the compatibility floor regardless of which of (1)/(2) is chosen —
without it, `cargo build` on a clean checkout needs node, vite, and a WASM
build. This is the main structural cost of including the MCP surface and
should be settled before that surface is implemented, not during.

## Consequences

- The results components gain a second consumer, which pins their prop-driven
  shape. Anything reaching for `getDataStore()` inside a results component now
  breaks two things, not one.
- Emission lines are **lazy-loaded post-compute**
  (`sim-scheduler.svelte.ts:172-181`) and never persisted — IndexedDB history
  does not capture them either. A snapshot step must be added; it does not
  exist today for any surface.
- The artifact is inert by construction: nothing in the results path fetches,
  and there is no analytics anywhere in the app. That property is now
  load-bearing for a file leaving the institution and should be asserted in a
  test, not assumed.
- Every shipped artifact is a redistribution decision. Tier must be recorded
  *in* the file, and the export UI must state plainly what is being shared.
- `frontend/src/lib/export.ts` is dead code (no import sites) and should be
  removed rather than mistaken for a starting point.

## Alternatives considered

- **Extend the `#config=` codec to carry results.** Impossible without
  redesign: 8 KB compressed cap, 1 MiB decompressed guard, 2000-byte URL budget
  (`config_codec.rs:42-63`) against a typical 200–400 KB result.
- **Server-side rendered report.** Contradicts the local-first, serverless
  architecture, and would put results on a host — the opposite of the current
  privacy property.
- **PDF export.** Static, so it sidesteps the licensing question, but drops
  exactly the filtering and exploration that motivates this. Worth revisiting
  as a complement, not a replacement.
- **Static HTML + images only (no embedded data).** Lowest exposure and
  cheapest, but the recipient cannot explore. Retained as the fallback if #421
  lands restrictively.
- **Ship the whole app as one file (Tier C).** See above — rejected on
  licensing, not on size.

## References

- `infra/README.md:44-58` — allowlist rationale and location
- `landing/index.html:113,133` — user-facing access-gate copy
- `scripts/deploy-eth.sh:113,238,284-290` — Shibboleth `.htaccess` enforcement
- `packages/compute/src/config-bridge.ts:95-138` — `SimulationResult` shape
- `frontend/src/lib/session-io.ts:28-84` — `SessionFile` v3 payload precedent
- `frontend/src/lib/scheduler/sim-scheduler.svelte.ts:172-181` — emissions
  lazy-load, the snapshot seam
- `core/src/release_notes.rs:46` — `include_str!` precedent for the MCP path
- `flake.nix:105-135` — `cleanCargoSource` strips non-Rust files;
  `provisionSiblings` is the workaround that makes the precedent work
- `core/src/config_codec.rs:42-63` — codec caps that rule out the URL route

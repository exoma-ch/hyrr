# ADR 0008 — Shareable self-contained results HTML

- **Status**: accepted (2026-08-13) — implemented on all three surfaces.
  Tier A + Tier B shipped; Tier C rejected. See "The licensing boundary" for
  why Tier B does not wait on #421.
- **Date**: 2026-08-13
- **Relates to**: #421 (dual-use / export-control determination), #425 (epic —
  legal & licensing readiness), #565 (refuse to deploy a stale ETH access
  whitelist), #539 / ADR-0004 (config codec — what is *not* reusable here),
  #550 (`.hyrr.json` v3 self-contained round-trip — the payload precedent),
  and #681 (σ(E) over MCP raised against this tier line — rejected; see
  "Alternatives considered")

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

**Decision: implement Tier A and Tier B. Tier B does not wait on #421.**

The reasoning (project owner's determination, recorded here so it is reviewable
rather than implicit):

- **The artifact conveys neither the tool nor the reactions.** It cannot
  re-run, re-tune, or evaluate a different target — no engine, no
  cross-sections, no stopping tables. What crosses is one run's derived output
  plus, at Tier B, decay-emission lines for the 113 nuclides that run happened
  to produce. That is a fixed, run-specific slice, not a capability. The
  restriction the gate enforces is about *cross-sections for arbitrary
  reactions*; emission lines for named nuclides are published reference data of
  a different character.
- **Sharing is largely *how* #421 gets decided.** #421 requires a written ETH
  export-control determination "for the full range". Regulators and reviewers
  cannot assess a tool they cannot open, and they are by definition not on the
  allowlist. A view-only artifact is the mechanism for showing them what the
  tool does — an *input* to the determination, not something blocked by it.

**#421 remains open and still governs the full range.** This ADR does not
settle it; it scopes one artifact narrowly enough to sit outside it. If #421
lands restrictively on emission data, Tier B is the thing that changes.

The tier switch stays regardless — one flag at export, one key in the payload —
because it is what makes an artifact's contents **auditable after it has
left**, which matters whichever way #421 lands.

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
  the viewer builds against the **basic** distribution: 4.84 MB → 1.12 MB.

### Measured budget

Built and measured against a real run (16 MeV p → havar + Mo-100 + Cu;
3 layers, 198 isotopes, 200-point grids) — not estimated:

| | raw | gzipped |
|---|---|---|
| viewer template (all JS+CSS inlined) | 1.30 MB | 430 KB |
| Tier A artifact (template + result) | 2.18 MB | **593 KB** |
| Tier B artifact (+ emissions + dose) | 2.87 MB | **652 KB** |

**Tier B costs ~60 KB gzipped over Tier A** — 7,611 emission lines across 113
nuclides plus 119 dose constants. That is the entire licensing-relevant
increment.

Two corrections to earlier estimates, both material:

- **Results do not gzip 40×.** The sizing probe in `core/src/mcp/cache.rs`
  suggested ~30 KB for a large result; a real 2.11 MB result gzips to **707 KB**
  (3×). Synthetic probe data is far more repetitive than real float arrays.
- **Precision, not deduplication, is the lever.** Hoisting the 198 identical
  `time_grid_s` copies saves 475 KB raw but only 10 KB gzipped — gzip already
  finds them. Rounding to **4 significant figures** (the UI displays ~4) takes
  the same result from 707 KB to **187 KB** gzipped. Doing both gives 181 KB.

Where the raw bytes actually live: `depth_production_rates` 46%,
`activity_vs_time_Bq` 23%, `time_grid_s` 22%, `depth_profile` 5%.

Emission across all three surfaces needs no new infrastructure, and the
template needs no per-surface delivery path at all:

- **Web** — `Header ▸ Save ▸ "Share result…"` → blob download.
- **Tauri** — the same menu entry → `dialog.save()` + `writeTextFile`;
  permissions already granted (`capabilities/default.json`), pattern proven at
  `BugReportModal.svelte:319-343`.
- **MCP** — `export_result_html` returns the HTML as a `text/html`
  `ToolResource`. The transport is mime-agnostic, so this reuses the same code
  path the Parquet resources already take.

**Template delivery is one mechanism, not three.** `npm run build` runs
`build:viewer` first, which stages the built template into `public/`. From
there it is an ordinary static asset: served on the web, and bundled into the
desktop app through `frontendDist`. A single
`fetch(BASE_URL + "viewer-template.html")` works on both, and the staged copy
is gitignored so a stale template cannot ship. Only MCP reads it from disk,
because it has no origin to fetch from.

### The MCP template must live in the leaf crate, not in `core`

**Resolved differently than first proposed, and better: nothing `include_str!`s
the template at all.** It is a `&str` argument to
`hyrr_core::viewer::render_html`, read at call time. That removes the question
of *which crate* embeds it, keeps ~1.3 MB out of every consumer of core, and
lets the template change without recompiling anything.

The original analysis stands as the reason not to embed it. Putting it in
`core/` would create a build-graph cycle:

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

None of those three were needed. As a runtime argument the template is not a
build input at all: `cargo build` on a clean checkout needs no node, no vite
and no WASM, and the hermetic Rust checks are untouched. The MCP tool resolves
it from its `template_path` argument or `HYRR_VIEWER_TEMPLATE`, and reports how
to build one rather than emitting a half-artifact when neither is set.

## Spike findings

A working spike exists (`frontend/vite.viewer.config.ts`, `src/Viewer.svelte`,
`src/lib/viewer/`, `scripts/make-viewer.mjs`). It renders the real components
from an embedded snapshot with **zero network requests**, verified by loading
the artifact over `file://` and asserting
`performance.getEntriesByType("resource")` is empty. Filtering, sorting and
selection all work; 3 plots and 36 traces render.

Four things the build surfaced that are not obvious from reading the code:

1. **Three silent single-file traps, each of which produced a broken artifact
   with no error at all.**
   - `String.replace(re, str)` interprets `$&`, `` $` `` and `$'` in the
     *replacement*. Minified plotly is full of them, so inlining with a string
     replacement corrupts the bundle. Use a function replacement — in the
     inliner **and** in the payload substitution.
   - The inliner must not delete an emitted chunk it failed to inline. Plotly
     arrives via `await import()`, so its chunk is referenced by no
     `<script src>`; dropping it yields an artifact whose plots never render.
     The plugin now errors instead.
   - Vite leaves a `__VITE_PRELOAD__` marker inside its dynamic-import helper
     for a later pass to substitute. That pass does not run here, and the
     surviving identifier makes the helper's promise **never settle** — no
     rejection, no console error, just permanently absent plots. A single-file
     bundle has nothing to preload, so the marker is replaced with `void 0`.

2. **The evaluated data contains non-finite values.** 516 emission rows carry
   NaN energy/intensity. `NaN <= threshold` is `false`, so a naive intensity
   filter passes them through, and Python's `json.dumps` then emits bare `NaN`
   — which is not valid JSON and fails at parse time in the browser. The
   generator must filter on finiteness explicitly.

3. **Tier A's dose column is wrong, not merely absent.** Without embedded dose
   constants `getDoseConstant` silently falls back to the hardcoded 8-isotope
   table (`utils/dose-constants.ts:21-24`), which disagrees materially — for
   ⁹⁹Mo in the reference run, **142 µSv/h against the correct 75.6 µSv/h**.
   Tier A must therefore either embed the dose rows (a 119-entry table, and a
   separate question from ENSDF emission lines) or suppress the column
   outright. Shipping a plausible-but-wrong dose to an external reviewer is the
   worst of the three options.

4. **`index.html` carries a Cloudflare analytics beacon.** The viewer entry is
   written from scratch rather than copied, precisely so the artifact inherits
   no beacon, no preconnect, and no external script.

Also confirmed: the viewer's depth axis spans the **configured** stack
thickness, not the traversed depth — matching the app, which does this
deliberately so the two depth plots align (#435). In the reference run the beam
stops 0.33 mm into a 5 mm Cu backing, so the two differ by 10×.

## As built

All three surfaces ship the export, each routed through the single
Rust-owned builder in `core/src/viewer.rs`.

| | entry point | save |
|---|---|---|
| Web | Header ▸ Save ▸ "Share result…" | blob download |
| Desktop | same menu entry | `dialog.save()` + `writeTextFile` |
| MCP | `export_result_html` | `text/html` `ToolResource` |

`core/src/viewer.rs` owns the frontend wire types, `convert_stack_result`,
snapshot construction and `render_html`. `build_snapshot` takes the **wire**
result rather than a `StackResult`, because that is the form the browser
already holds; callers holding a `StackResult` convert first. One entry point,
so the tier gate cannot diverge per surface.

Closing the duplication fixed a live bug: the desktop copy of
`convert_stack_result` silently dropped `pruned_negligible_count`, so desktop
users never saw that the negligible-inventory prune (#533) had filtered
isotopes.

Two guards exist because the failure they prevent is silent rather than loud:

- **Emissions and dose constants are arguments, not lookups.** Only
  `ParquetDataStore` implements `DatabaseProtocol::get_emissions`;
  `EmbeddedDataStore` and `InMemoryDataStore` inherit the empty default. A
  builder that fetched for itself would emit Tier A on two of three surfaces
  while labelling the artifact Tier B.
- **A Tier B request with no emission data is refused, not downgraded.** An
  artifact whose payload claims spectra it does not contain is worse than no
  artifact.

Verified end to end rather than compiled: a real 16 MeV p → havar+Mo-100+Cu
run exported from the built web app produced a 2.97 MB artifact that opens
from `file://` with 3 plots, 36 traces, 58 rows, **zero network requests** and
the correct ENSDF doses (⁹⁹ᵐTc 211, ⁹⁹Mo 75.6 µSv/h). The MCP tool produced
the same for the same run. The desktop branch is covered by `export.test.ts`
rather than asserted, since a Tauri binary cannot be driven here.

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

### σ(E) over MCP, bounded by the run (raised 2026-08-26 in #681 — rejected)

Proposed as the better fix for #681: rather than annotate the outward referral
to `nucl-parquet-mcp`, remove the need for it by adding a run-bounded
`get_cross_section_curve` to hyrr-mcp — σ(E) for the channels a run produced,
bounded the way `produced_nuclides()` bounds Tier B, on the argument that "why
is this yield shaped like that" is *result interpretation* rather than data
exploration.

The reasoning is sound about **the question** and wrong about **the boundary**.
Recorded here rather than left implicit, because the next person to have the
idea should find the answer instead of re-deriving it — or worse, drifting into
it one tool at a time.

- **It is the category this ADR already rejected.** Tier C was refused because
  it "would require cross-sections", and Tier B's justification rests on the
  contrast: *"the restriction the gate enforces is about cross-sections for
  arbitrary reactions; emission lines for named nuclides are published
  reference data of a different character."* σ(E) is not a wider surface than
  the tiers considered — it is the one they named as restricted. #421 is still
  open and still governs.
- **"Bounded by the run" is not a bound when the caller chooses the run.**
  Tier B's bound holds because the *sender* fixes the run and the *recipient*
  is a third party outside the gate: one artifact is one fixed slice. Over MCP
  the caller both chooses the run and receives the data, so the bound is
  self-selected. Worse, `produced_nuclides()` would only filter *which residual
  channels* are returned — `DatabaseProtocol::get_cross_sections` returns each
  channel's full library energy grid, and `list_reaction_channels` needs no
  simulation at all. The run restricts almost nothing about the data that comes
  back, so the Tier B gate does not transfer even though it can be called.
- **The tool surface already sits on this line, deliberately.** ADR 0001 renamed
  hyrr's tool away from `get_cross_sections` "to avoid collision with
  `nucl-parquet-mcp`", and `core/src/mcp/tools.rs` calls
  `db.get_cross_sections` exactly once, internally: only peak σ and energy range
  escape. Raw evaluated *decay* data does leave via `get_nuclide_data` — which
  is precisely Tier B's category. The surface is coherent with the tier line
  today, and this would be the first crossing.
- **It would not remove the hand-off anyway.** EXFOR, coincidences, β-spectra,
  abundances, and σ(E) for any target *not* in the run all still refer outward.
  Annotating the referral is required either way, which makes this purely
  additive scope — and additive scope carrying an unresolved licensing question
  is exactly what should not ride along on a fix.

**What was implemented instead (#681):** every outward referral names the live
library and data release, read from the same pin `get_version_info` prints, and
states the consequence of a mismatch. #682 keeps the narrower idea worth
keeping — σ(E) *restricted to the energy window the beam actually traversed in
a layer* is a derived, genuinely run-bounded quantity (the window comes from
geometry and beam energy, not from a caller-supplied `(Z, A)`), and does not
raise this question.

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

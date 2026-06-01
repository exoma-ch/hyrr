# Contributing to the HYRR frontend

This document covers the conventions a contributor needs to land a PR
against `frontend/`. The repository-wide rules (commit format, issue
linking, branch hygiene) live in the top-level `CLAUDE.md` and ADRs; the
notes here are frontend-specific.

## Quick reference

| Task                  | Command                                  |
| --------------------- | ---------------------------------------- |
| Install               | `npm install --workspaces`               |
| Type / template check | `npm run check --workspace=hyrr-frontend` |
| Unit + render tests   | `npm test --workspace=hyrr-frontend`     |
| Dev server            | `npm run dev --workspace=hyrr-frontend`  |
| E2E (Playwright)      | See `frontend/e2e/README.md`             |

`npm run check` must report `0 ERRORS 0 WARNINGS` before a PR is ready
to merge. The full test suite runs in ~15 s and is expected to pass on
every PR.

## Testing Svelte components

### The vitest split

`vite.config.ts` configures vitest with **two projects**:

- **`node`** — runs `src/**/*.test.ts`. Plain Node, no DOM. Fast.
  - Use this for pure functions, helpers, store logic that doesn't
    render anything.
  - Files: `format.test.ts`, `bug-report-body.test.ts`,
    `results.svelte.test.ts`, etc.
- **`jsdom`** — runs `src/lib/components/**/*.svelte.test.ts`. jsdom +
  `@testing-library/svelte`. Slower (svelte rune proxies + DOM globals).
  - Use this for component render tests.
  - Files: `FetchErrorCard.svelte.test.ts`,
    `DataFetchSplash.svelte.test.ts`,
    `BugReportModal.svelte.test.ts`.

The split exists because Svelte 5 rune proxies behave differently under
jsdom — `expect(x).toBe(x)` can fail because the rune wraps `$state` in
a Proxy that's only identity-stable inside the same project. Keeping
pure code on the node runner avoids that whole class of failure and
keeps the fast path fast.

### Naming convention

| Goal                        | Filename                            | Location                       |
| --------------------------- | ----------------------------------- | ------------------------------ |
| Pure-function unit test     | `<thing>.test.ts`                   | next to the source             |
| Svelte component render test | `<ComponentName>.svelte.test.ts`   | next to the `.svelte` file     |

A pure helper extracted **from** a component (like
`bug-report-body.ts`) stays in the node project — its test file is
`bug-report-body.test.ts` (no `.svelte.` infix), not
`BugReportBody.svelte.test.ts`. Reserve the `.svelte.test.ts` suffix
for files that actually call `render()`.

### A worked example: render the contract, not the implementation

The minimal shape of a render test:

```ts
import { describe, it, expect, vi, afterEach } from "vitest";
import { render, fireEvent, cleanup } from "@testing-library/svelte";

// vi.mock is hoisted above imports — keep factories self-contained.
vi.mock("../utils/platform", () => ({
  isTauri: vi.fn(() => false),
  detectOS: () => "macos",
}));
vi.mock("../utils/open-url", () => ({
  openExternalUrl: vi.fn(async () => {}),
}));

import MyComponent from "./MyComponent.svelte";

afterEach(() => cleanup());

describe("MyComponent", () => {
  it("calls onsubmit when the user clicks Submit", async () => {
    const onsubmit = vi.fn();
    const { getByRole } = render(MyComponent, { onsubmit });
    await fireEvent.click(getByRole("button", { name: /submit/i }));
    expect(onsubmit).toHaveBeenCalledTimes(1);
  });
});
```

Three rules of thumb:

1. **Couple to the rendered contract** — button text, accessible roles
   (`getByRole`), `aria-label`, `data-testid` if you must. Do NOT
   couple to CSS classes, internal `$state` variable names, or DOM
   structure that's incidental.
2. **One assertion per concept** — if you're checking "the button is
   disabled AND the tooltip explains why", that's two `expect()`s in
   one test, not two tests.
3. **Reset mocks in `beforeEach`** — the jsdom project does not reset
   between cases by default. Use `vi.fn().mockClear()` or
   `vi.mocked(fn).mockReset()` to keep cases independent.

### The `flush()` pattern for `onMount`-async components

Several components in this codebase do async work inside `onMount`:

- `DataFetchSplash` dynamically imports `@tauri-apps/api/event` and
  awaits `listen()`.
- `FetchErrorCard` awaits the data-fetch-meta SSoT getters.

After `render()`, the rendered DOM reflects the **pre-onMount** state.
You need to wait two microtask ticks for the async work to settle:

```ts
async function flush() {
  // tick 1 — onMount async kicks the dynamic import
  // tick 2 — listen()/getters resolve, $state assignment, $derived recompute
  await new Promise((r) => setTimeout(r, 0));
  await new Promise((r) => setTimeout(r, 0));
}
```

Call `await flush()` after `render()` and after each event you fire
that touches a `$derived` block. See
`DataFetchSplash.svelte.test.ts` for the canonical example.

### Mocking the surfaces a component reaches into

The mock recipes below match the patterns the existing render tests
already use — crib them rather than reinventing.

**Platform / Tauri detection:**

```ts
vi.mock("../utils/platform", () => ({
  isTauri: vi.fn(() => false), // flip per-test for desktop branches
  detectOS: () => "macos",
}));
```

**External URL open (Tauri opener):**

```ts
vi.mock("../utils/open-url", () => ({
  openExternalUrl: vi.fn(async () => {}),
}));
```

**`@tauri-apps/api/event` listener (capture-the-callback pattern):**

```ts
const listenCalls: Array<{ event: string; cb: (e: { payload: unknown }) => void }> = [];
const unsubscribeMock = vi.fn(() => {});
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event, cb) => {
    listenCalls.push({ event, cb });
    return unsubscribeMock;
  }),
}));
// In the test: drive the component by calling listenCalls[0].cb({ payload }).
```

**SSoT getters from `data-fetch-meta`** (mock so jsdom doesn't try to
talk to the Tauri invoke surface):

```ts
vi.mock("../compute/data-fetch-meta", () => ({
  getReleaseUrl: vi.fn(async () => "https://example.test/data.tar.zst"),
  getCacheRootPattern: vi.fn(async () => "~/.hyrr/nucl-parquet/<version>"),
  getDataVersion: vi.fn(async () => "v0.10.0"),
  DEFAULT_LIBRARY: "tendl-2023-iso",
}));
```

**Rune stores (e.g. `bugreport.svelte.ts`, `results.svelte.ts`):**
mock the whole module so the test controls `getX()` return values
directly. Rune state can't easily be reset between cases otherwise.

```ts
vi.mock("../stores/bugreport.svelte", () => ({
  getBugReportOpen: vi.fn(() => true),
  closeBugReport: vi.fn(),
}));
```

### When NOT to write a component test

If the behaviour you want to assert is pure (a string transform, a
config validator, a body-builder), extract it to a helper module and
test it on the node project. Component tests are 5-10× slower than
node tests and exercise the rendered contract — they're the wrong
tool for assertion on what a function returns.

Concrete example: `bug-report-body.ts` was extracted from
`BugReportModal.svelte` specifically so its branching logic could be
covered without spinning up a render. The modal's test then asserts
the wiring (does the click actually reach the helper?), not the
helper's branches.

### Worked examples

Read these in order if you're writing your first render test:

- `frontend/src/lib/components/FetchErrorCard.svelte.test.ts` —
  render-matrix pattern (variant × platform × props), `getByRole`
  for buttons, callback-fired-once assertions.
- `frontend/src/lib/components/DataFetchSplash.svelte.test.ts` —
  capture-the-callback for the `listen()` mock, two-tick `flush()`,
  unmount-cleanup assertion.
- `frontend/src/lib/components/BugReportModal.svelte.test.ts` —
  mocking rune stores, end-to-end URL-construction assertion,
  exercising the form-state contract without coupling to internal
  `$state` names.

## Commits

Use the repo-wide format:

```text
type(scope): short imperative description

Optional longer body explaining why.

Refs: #123
```

Every commit must have a `Refs:` line pointing at the GitHub issue.
PRs that touch the frontend must keep `npm run check` and
`npm test` green.

/**
 * Render-level coverage for `BugReportModal` (#178).
 *
 * The pure body-builder is covered by `bug-report-body.test.ts`; this suite
 * asserts the form-state contract that wraps it:
 *
 *   - the modal is only visible when `getBugReportOpen() === true`
 *   - "Open on GitHub" is gated on description / screenshot (disable-with-hover)
 *   - typing a description enables the button
 *   - clicking the button hands a well-formed GitHub `issues/new` URL to
 *     `openExternalUrl()` with the title + body encoded via the shared
 *     `bug-report-body` / `bug-report-title` helpers
 *   - a non-null `computeError` from the results store flows into the body
 *
 * We exercise the rendered contract (button text, accessible roles, callback
 * wiring) — NOT the component's internal `$state` names or CSS classes.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, fireEvent, cleanup } from "@testing-library/svelte";

/** Grab a field by its DOM `id`. The modal's labels overlap textually
 *  (the Title label mentions "description" in its hint), so by-label
 *  matching is ambiguous — id is the stable handle. */
function fieldById<T extends HTMLElement>(id: string): T {
  const el = document.getElementById(id);
  if (!el) throw new Error(`expected #${id} in the rendered modal`);
  return el as T;
}

// ── Module mocks (hoisted by Vitest above all imports) ────────────────────

// Platform: default to browser-mode so the non-desktop button matrix is shown
// (an "Open on GitHub" button is present in both modes — they just sit in a
// different action block).
vi.mock("../utils/platform", () => ({
  isTauri: vi.fn(() => false),
  detectOS: () => "macos",
}));

// External-URL opener — the load-bearing observable for the openOnGitHub path.
vi.mock("../utils/open-url", () => ({
  openExternalUrl: vi.fn(async () => {}),
}));

// Bug-report open/close store — return open=true so the modal actually mounts.
vi.mock("../stores/bugreport.svelte", () => ({
  getBugReportOpen: vi.fn(() => true),
  openBugReport: vi.fn(),
  closeBugReport: vi.fn(),
}));

// Config store — supply a minimal valid SimulationConfig + SerializableConfig.
vi.mock("../stores/config.svelte", () => ({
  getConfig: vi.fn(() => ({
    beam: { projectile: "p", energy_MeV: 16, current_mA: 0.15 },
    layers: [{ material: "Cu", thickness_cm: 0.5 }],
    irradiation_s: 86400,
    cooling_s: 0,
  })),
  getSerializableConfig: vi.fn(() => ({
    beam: { projectile: "p", energy_MeV: 16, current_mA: 0.15 },
    items: [{ material: "Cu", thickness_cm: 0.5 }],
    irradiation_s: 86400,
    cooling_s: 0,
  })),
}));

// Results store — getter trio used by the modal. `getResultError` is the
// channel that feeds the "Compute error:" line of the body.
const resultErrorMock = vi.fn(() => null as unknown);
vi.mock("../stores/results.svelte", () => ({
  getResult: vi.fn(() => null),
  getResultError: () => resultErrorMock(),
  // #159: the modal reads the active trace id to build the (default-off) trace
  // preview. No active run in these tests → null → no preview rendered.
  getActiveTraceId: vi.fn(() => null),
}));

// Shareable-URL builder — return a stable fake. The body builder embeds this
// verbatim, so we just need a known string to grep for if needed.
vi.mock("../config-url", () => ({
  getShareableUrl: vi.fn(() => "https://example.test/#config=stub"),
}));

// ── Imports (must come after vi.mock) ──────────────────────────────────────

import BugReportModal from "./BugReportModal.svelte";
import { openExternalUrl } from "../utils/open-url";
import { closeBugReport } from "../stores/bugreport.svelte";

const openExternalUrlMock = vi.mocked(openExternalUrl);
const closeBugReportMock = vi.mocked(closeBugReport);

/** Allow onMount + $derived to settle. Two ticks for parity with the
 *  DataFetchSplash pattern (one for the rune proxy reaction, one for the
 *  DOM commit). One is usually enough here, but two is harmless. */
async function flush() {
  await new Promise((r) => setTimeout(r, 0));
  await new Promise((r) => setTimeout(r, 0));
}

beforeEach(() => {
  openExternalUrlMock.mockClear();
  closeBugReportMock.mockClear();
  resultErrorMock.mockReturnValue(null);
});

afterEach(() => {
  cleanup();
});

describe("BugReportModal — initial render", () => {
  it("renders the dialog when the bug-report store is open", async () => {
    const { getByRole } = render(BugReportModal);
    await flush();
    expect(getByRole("dialog", { name: /report a bug/i })).toBeInTheDocument();
  });

  it("shows both report-type toggles (Bug / Feature)", async () => {
    const { getByRole } = render(BugReportModal);
    await flush();
    expect(getByRole("button", { name: /^bug$/i })).toBeInTheDocument();
    expect(getByRole("button", { name: /^feature$/i })).toBeInTheDocument();
  });

  it("renders an Open-on-GitHub button", async () => {
    const { getByRole } = render(BugReportModal);
    await flush();
    expect(getByRole("button", { name: /open on github/i })).toBeInTheDocument();
  });
});

describe("BugReportModal — Open-on-GitHub disable gate (description required)", () => {
  it("is disabled when description is empty AND no screenshot is attached", async () => {
    const { getByRole } = render(BugReportModal);
    await flush();
    const btn = getByRole("button", { name: /open on github/i });
    expect(btn).toBeDisabled();
  });

  it("surfaces the disabledReason via the title attribute", async () => {
    const { getByRole } = render(BugReportModal);
    await flush();
    const btn = getByRole("button", { name: /open on github/i });
    // Hover-tooltip explains *why* the click is dead (see #160 / pre-PR-#160
    // disable-with-hover behaviour). We only assert it mentions "description"
    // — exact wording can drift.
    expect(btn).toHaveAttribute("title", expect.stringMatching(/description/i));
  });

  it("becomes enabled once the description has content", async () => {
    const { getByRole } = render(BugReportModal);
    await flush();

    const desc = fieldById<HTMLTextAreaElement>("bug-desc");
    await fireEvent.input(desc, { target: { value: "it broke when I clicked X" } });
    await flush();

    expect(getByRole("button", { name: /open on github/i })).not.toBeDisabled();
  });
});

describe("BugReportModal — title field", () => {
  it("has a static placeholder hinting at the format", async () => {
    render(BugReportModal);
    await flush();
    const title = fieldById<HTMLInputElement>("bug-title");
    // The placeholder is a static example; the actual auto-fill from
    // description happens inside buildIssueTitle() at click time, which is
    // covered separately in bug-report-title's own test.
    expect(title).toHaveAttribute("placeholder", expect.stringMatching(/short summary/i));
  });

  it("accepts user input up to the maxlength cap", async () => {
    render(BugReportModal);
    await flush();
    const title = fieldById<HTMLInputElement>("bug-title");
    expect(title.maxLength).toBe(70);
    await fireEvent.input(title, { target: { value: "Heavy ions crash compute" } });
    expect(title.value).toBe("Heavy ions crash compute");
  });
});

describe("BugReportModal — Open-on-GitHub click wiring", () => {
  it("calls openExternalUrl with a GitHub issues/new URL containing the title + body", async () => {
    const { getByRole } = render(BugReportModal);
    await flush();

    await fireEvent.input(fieldById<HTMLInputElement>("bug-title"), {
      target: { value: "Heavy ions crash compute" },
    });
    await fireEvent.input(fieldById<HTMLTextAreaElement>("bug-desc"), {
      target: { value: "it broke when I clicked X" },
    });
    await flush();

    await fireEvent.click(getByRole("button", { name: /open on github/i }));

    expect(openExternalUrlMock).toHaveBeenCalledTimes(1);
    const url = openExternalUrlMock.mock.calls[0][0] as string;

    // Couple to the contract: it's a GitHub issues/new URL on the right repo.
    expect(url).toMatch(/^https:\/\/github\.com\/exoma-ch\/hyrr\/issues\/new\?/);

    // Title is encoded into the `title=` param via buildIssueTitle, which
    // prefixes "[Bug] " for the default "bug" report type.
    const params = new URLSearchParams(url.split("?")[1]);
    expect(params.get("title")).toBe("[Bug] Heavy ions crash compute");

    // Body carries the description plus the shared bug-report-body sections.
    const body = params.get("body") ?? "";
    expect(body).toContain("## Description");
    expect(body).toContain("it broke when I clicked X");
    // Beam line from buildBugReportBody — proves the config helper actually ran.
    expect(body).toContain("**Beam:** p @ 16 MeV");

    // Labels param follows the same rule (bug/enhancement).
    expect(params.get("labels")).toBe("bug");
  });

  it("closes the modal after firing the open call", async () => {
    const { getByRole } = render(BugReportModal);
    await flush();
    await fireEvent.input(fieldById<HTMLTextAreaElement>("bug-desc"), {
      target: { value: "anything goes here" },
    });
    await flush();
    await fireEvent.click(getByRole("button", { name: /open on github/i }));
    expect(closeBugReportMock).toHaveBeenCalledTimes(1);
  });

  it("uses [Feature] in the title when the Feature toggle is active", async () => {
    const { getByRole } = render(BugReportModal);
    await flush();

    await fireEvent.click(getByRole("button", { name: /^feature$/i }));
    await fireEvent.input(fieldById<HTMLInputElement>("bug-title"), {
      target: { value: "Add streaming results" },
    });
    await fireEvent.input(fieldById<HTMLTextAreaElement>("bug-desc"), {
      target: { value: "would be nice" },
    });
    await flush();

    await fireEvent.click(getByRole("button", { name: /open on github/i }));
    const url = openExternalUrlMock.mock.calls[0][0] as string;
    const params = new URLSearchParams(url.split("?")[1]);
    expect(params.get("title")).toBe("[Feature] Add streaming results");
    expect(params.get("labels")).toBe("enhancement");
  });
});

describe("BugReportModal — computeError flows into the body", () => {
  it("includes the compute-error text in the URL body when getResultError() returns non-null", async () => {
    resultErrorMock.mockReturnValue(new Error("StoppingError: O-16 unsupported"));

    const { getByRole } = render(BugReportModal);
    await flush();
    await fireEvent.input(fieldById<HTMLTextAreaElement>("bug-desc"), {
      target: { value: "compute died" },
    });
    await flush();
    await fireEvent.click(getByRole("button", { name: /open on github/i }));

    const url = openExternalUrlMock.mock.calls[0][0] as string;
    const body = new URLSearchParams(url.split("?")[1]).get("body") ?? "";
    expect(body).toContain("**Compute error:**");
    expect(body).toContain("StoppingError: O-16 unsupported");
  });
});

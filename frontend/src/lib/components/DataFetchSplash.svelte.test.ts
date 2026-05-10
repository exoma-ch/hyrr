/**
 * Render coverage for `DataFetchSplash` (#169).
 *
 * The component subscribes to `hyrr://data-fetch-progress` over Tauri
 * IPC and translates each `RustProgress` payload into a stage label +
 * progress-bar state. This test mocks `@tauri-apps/api/event::listen`
 * so we can grab the registered callback, fire fake progress events,
 * and assert the rendered transitions.
 *
 * Why this test exists separate from `parse-fetch-error.test.ts`:
 * the splash's "Connecting → Downloading → Extracting" wire-up
 * lives in the Svelte `$derived` block, not in any pure helper —
 * the only way to assert it is via render.
 */
import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { render, cleanup } from "@testing-library/svelte";

// Force desktop mode so the component subscribes (browser mode no-ops).
vi.mock("../utils/platform", () => ({
  isTauri: vi.fn(() => true),
  detectOS: () => "macos",
}));

// SSoT mocks for the FetchErrorCard subtree — DataFetchSplash mounts it
// when `loadingError` is non-null. We never set that in these tests, so
// the mocks act as safety net rather than load-bearing.
vi.mock("../compute/data-fetch-meta", () => ({
  getReleaseUrl: vi.fn(async () => null),
  getCacheRootPattern: vi.fn(async () => null),
  getTarballFilename: vi.fn(async () => null),
  getReleaseBaseUrl: vi.fn(async () => null),
  getDataVersion: vi.fn(async () => null),
  DEFAULT_LIBRARY: "tendl-2025",
}));

// Capture every `listen()` call so each test can grab the latest handler.
type EventCallback = (event: { payload: unknown }) => void;
const listenCalls: Array<{ event: string; cb: EventCallback }> = [];
const unsubscribeMock = vi.fn(() => {});

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, cb: EventCallback) => {
    listenCalls.push({ event, cb });
    return unsubscribeMock;
  }),
}));

import DataFetchSplash from "./DataFetchSplash.svelte";

/** Wait one microtask for onMount + dynamic import + $derived to settle. */
async function flush() {
  // The component awaits a dynamic import of @tauri-apps/api/event inside
  // onMount, so we need a few ticks. Two `setTimeout` settles handle:
  //   tick 1 — onMount async kicks the dynamic import
  //   tick 2 — listen resolves, $state assignment, $derived recompute
  await new Promise((r) => setTimeout(r, 0));
  await new Promise((r) => setTimeout(r, 0));
}

beforeEach(() => {
  listenCalls.length = 0;
  unsubscribeMock.mockClear();
});

afterEach(() => {
  cleanup();
});

describe("DataFetchSplash — initial render falls back to loadingState", () => {
  it("shows the parent's loading text when no rust event has arrived", async () => {
    const { getByTestId } = render(DataFetchSplash, {
      loadingState: "Loading nuclear data…",
      fallbackFraction: 0,
      loadingError: null,
      onretry: vi.fn(),
    });
    await flush();
    expect(getByTestId("splash-stage")).toHaveTextContent("Loading nuclear data…");
  });

  it("subscribes to the hyrr://data-fetch-progress event on mount", async () => {
    render(DataFetchSplash, {
      loadingState: "init",
      fallbackFraction: 0,
      loadingError: null,
      onretry: vi.fn(),
    });
    await flush();
    expect(listenCalls.length).toBe(1);
    expect(listenCalls[0].event).toBe("hyrr://data-fetch-progress");
  });
});

describe("DataFetchSplash — progress events update stage label", () => {
  it("Connecting event → 'Connecting…'", async () => {
    const { getByTestId } = render(DataFetchSplash, {
      loadingState: "init",
      fallbackFraction: 0,
      loadingError: null,
      onretry: vi.fn(),
    });
    await flush();

    listenCalls[0].cb({
      payload: { stage: "connecting", bytes_done: 0, bytes_total: null },
    });
    await flush();

    expect(getByTestId("splash-stage")).toHaveTextContent("Connecting…");
  });

  it("Downloading event with bytes → 'Downloading nuclear data…' + size hint", async () => {
    const { getByTestId } = render(DataFetchSplash, {
      loadingState: "init",
      fallbackFraction: 0,
      loadingError: null,
      onretry: vi.fn(),
    });
    await flush();

    listenCalls[0].cb({
      payload: {
        stage: "downloading",
        bytes_done: 10 * 1024 * 1024,
        bytes_total: 100 * 1024 * 1024,
      },
    });
    await flush();

    expect(getByTestId("splash-stage")).toHaveTextContent(
      "Downloading nuclear data…",
    );
    // Size hint surfaces MiB-formatted progress when total is known.
    expect(getByTestId("splash-size")).toHaveTextContent("10.0 / 100.0 MiB");
  });

  it("Extracting event → 'Extracting…'", async () => {
    const { getByTestId } = render(DataFetchSplash, {
      loadingState: "init",
      fallbackFraction: 0,
      loadingError: null,
      onretry: vi.fn(),
    });
    await flush();

    listenCalls[0].cb({
      payload: { stage: "extracting", bytes_done: 0, bytes_total: null },
    });
    await flush();

    expect(getByTestId("splash-stage")).toHaveTextContent("Extracting…");
  });

  it("Verifying event → 'Finalising…' (no size hint without total)", async () => {
    const { getByTestId, queryByTestId } = render(DataFetchSplash, {
      loadingState: "init",
      fallbackFraction: 0,
      loadingError: null,
      onretry: vi.fn(),
    });
    await flush();

    listenCalls[0].cb({
      payload: { stage: "verifying", bytes_done: 0, bytes_total: null },
    });
    await flush();

    expect(getByTestId("splash-stage")).toHaveTextContent("Finalising…");
    expect(queryByTestId("splash-size")).toBeNull();
  });

  it("transitions Connecting → Downloading → Extracting as events fire", async () => {
    const { getByTestId } = render(DataFetchSplash, {
      loadingState: "init",
      fallbackFraction: 0,
      loadingError: null,
      onretry: vi.fn(),
    });
    await flush();

    const { cb } = listenCalls[0];

    cb({ payload: { stage: "connecting", bytes_done: 0, bytes_total: null } });
    await flush();
    expect(getByTestId("splash-stage")).toHaveTextContent("Connecting…");

    cb({
      payload: {
        stage: "downloading",
        bytes_done: 1024,
        bytes_total: 2048,
      },
    });
    await flush();
    expect(getByTestId("splash-stage")).toHaveTextContent(
      "Downloading nuclear data…",
    );

    cb({ payload: { stage: "extracting", bytes_done: 0, bytes_total: null } });
    await flush();
    expect(getByTestId("splash-stage")).toHaveTextContent("Extracting…");
  });
});

describe("DataFetchSplash — unsubscribes the listener on unmount", () => {
  it("invokes the listen() return value when destroyed", async () => {
    const { unmount } = render(DataFetchSplash, {
      loadingState: "init",
      fallbackFraction: 0,
      loadingError: null,
      onretry: vi.fn(),
    });
    await flush();
    expect(unsubscribeMock).not.toHaveBeenCalled();

    unmount();
    expect(unsubscribeMock).toHaveBeenCalledTimes(1);
  });
});

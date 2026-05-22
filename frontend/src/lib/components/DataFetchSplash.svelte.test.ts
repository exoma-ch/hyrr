/**
 * Render coverage for `DataFetchSplash`.
 *
 * With bundled data the splash is shown briefly during init (disk I/O
 * only, no download). Tests verify the loading state text and error
 * card mounting.
 */
import { describe, it, expect, vi, afterEach } from "vitest";
import { render, cleanup } from "@testing-library/svelte";

vi.mock("../utils/platform", () => ({
  isTauri: vi.fn(() => false),
  detectOS: () => "macos",
}));

vi.mock("../compute/data-fetch-meta", () => ({
  getReleaseUrl: vi.fn(async () => null),
  getCacheRootPattern: vi.fn(async () => null),
  getReleaseBaseUrl: vi.fn(async () => null),
  getDataVersion: vi.fn(async () => null),
  DEFAULT_LIBRARY: "tendl-2023-iso",
}));

import DataFetchSplash from "./DataFetchSplash.svelte";

async function flush() {
  await new Promise((r) => setTimeout(r, 0));
}

afterEach(() => {
  cleanup();
});

describe("DataFetchSplash — loading state", () => {
  it("shows the loading text", async () => {
    const { getByTestId } = render(DataFetchSplash, {
      loadingState: "Loading nuclear data…",
      fallbackFraction: 0,
      loadingError: null,
      onretry: vi.fn(),
    });
    await flush();
    expect(getByTestId("splash-stage")).toHaveTextContent("Loading nuclear data…");
  });

  it("shows indeterminate progress bar when fraction is 0", async () => {
    const { container } = render(DataFetchSplash, {
      loadingState: "init",
      fallbackFraction: 0,
      loadingError: null,
      onretry: vi.fn(),
    });
    await flush();
    const bar = container.querySelector(".progress-bar");
    expect(bar?.classList.contains("indeterminate")).toBe(true);
  });

  it("shows determinate progress bar when fraction > 0", async () => {
    const { container } = render(DataFetchSplash, {
      loadingState: "init",
      fallbackFraction: 0.5,
      loadingError: null,
      onretry: vi.fn(),
    });
    await flush();
    const bar = container.querySelector(".progress-bar");
    expect(bar?.classList.contains("indeterminate")).toBe(false);
  });
});

describe("DataFetchSplash — error state", () => {
  it("mounts FetchErrorCard when loadingError is set", async () => {
    const { container } = render(DataFetchSplash, {
      loadingState: "init",
      fallbackFraction: 0,
      loadingError: "something went wrong",
      onretry: vi.fn(),
    });
    await flush();
    // FetchErrorCard renders an .error-card div.
    expect(container.querySelector(".error-card")).toBeInTheDocument();
  });

  it("does not show progress bar when error is present", async () => {
    const { container } = render(DataFetchSplash, {
      loadingState: "init",
      fallbackFraction: 0,
      loadingError: "fail",
      onretry: vi.fn(),
    });
    await flush();
    expect(container.querySelector(".progress-bar")).toBeNull();
  });
});

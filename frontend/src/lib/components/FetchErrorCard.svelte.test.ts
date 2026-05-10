/**
 * Render-matrix coverage for `FetchErrorCard` (#169).
 *
 * The pure-function path is covered by `parse-fetch-error.test.ts`; this
 * suite asserts the rendered contract — title text, the four-button
 * visibility matrix (varies by variant × `isTauri()` × `onuselimited`
 * presence), and the callback wiring.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, fireEvent, cleanup } from "@testing-library/svelte";

// Mock the platform helper. The mock factory must be self-contained because
// vi.mock is hoisted above imports.
vi.mock("../utils/platform", () => ({
  isTauri: vi.fn(() => false),
  detectOS: () => "macos",
}));

// Mock the data-fetch-meta SSoT — we don't want the test to hit the
// (non-existent in jsdom) Tauri invoke surface.
vi.mock("../compute/data-fetch-meta", () => ({
  getReleaseUrl: vi.fn(async () =>
    "https://github.com/exoma-ch/nucl-parquet/releases/download/v0.10.0/nucl-parquet-data-v0.10.0.tar.zst",
  ),
  getCacheRootPattern: vi.fn(async () => "~/.hyrr/nucl-parquet/<version>"),
  getTarballFilename: vi.fn(async () => "nucl-parquet-data-v0.10.0.tar.zst"),
  getReleaseBaseUrl: vi.fn(async () => "https://github.com/exoma-ch/nucl-parquet"),
  getDataVersion: vi.fn(async () => "v0.10.0"),
  DEFAULT_LIBRARY: "tendl-2025",
}));

// Mock open-url so we don't try to escape jsdom.
vi.mock("../utils/open-url", () => ({
  openExternalUrl: vi.fn(async () => {}),
}));

import FetchErrorCard from "./FetchErrorCard.svelte";
import {
  fetchErrorTitle,
  type ParsedFetchError,
} from "../utils/parse-fetch-error";
import { isTauri } from "../utils/platform";

const mockedIsTauri = vi.mocked(isTauri);

const CACHE = "/Users/x/.hyrr/nucl-parquet/v0.10.0";
const URL_ = "https://github.com/exoma-ch/nucl-parquet/releases/download/v0.10.0/nucl-parquet-data-v0.10.0.tar.zst";

/** One representative payload per parsed-error variant. Seven entries. */
const variants: Array<{ name: string; payload: ParsedFetchError; carriesUrl: boolean }> = [
  {
    name: "HttpStatus",
    carriesUrl: true,
    payload: {
      kind: "FetchError",
      variant: "HttpStatus",
      status: 404,
      url: URL_,
      cache_dir: CACHE,
      message: "HTTP 404",
    },
  },
  {
    name: "Network",
    carriesUrl: true,
    payload: {
      kind: "FetchError",
      variant: "Network",
      detail: "dns lookup failed",
      url: URL_,
      cache_dir: CACHE,
      message: "network error: dns lookup failed",
    },
  },
  {
    name: "Decompress",
    // Variants without a `url` field fall back to the SSoT `releaseUrl`
    // mock above, so the "Open URL" button still renders after onMount.
    carriesUrl: true,
    payload: {
      kind: "FetchError",
      variant: "Decompress",
      cache_dir: CACHE,
      message: "zstd: bad magic",
    },
  },
  {
    name: "Extract",
    carriesUrl: true,
    payload: {
      kind: "FetchError",
      variant: "Extract",
      cache_dir: CACHE,
      message: "unexpected eof",
    },
  },
  {
    name: "UnsafeTarballEntry",
    carriesUrl: true,
    payload: {
      kind: "FetchError",
      variant: "UnsafeTarballEntry",
      entry_kind: "Symlink",
      entry_path: "data/meta/evil",
      cache_dir: CACHE,
      message: "unsafe tarball entry Symlink at data/meta/evil",
    },
  },
  {
    name: "Io",
    carriesUrl: true,
    payload: {
      kind: "FetchError",
      variant: "Io",
      cache_dir: CACHE,
      message: "insufficient disk space",
    },
  },
  {
    name: "NoHome",
    carriesUrl: true,
    payload: {
      kind: "FetchError",
      variant: "NoHome",
      message: "HOME environment variable not set",
    },
  },
];

/** Wait one microtask + a tick for onMount + $derived to settle. */
async function flush() {
  await new Promise((r) => setTimeout(r, 0));
}

afterEach(() => {
  cleanup();
  mockedIsTauri.mockReset();
  mockedIsTauri.mockImplementation(() => false);
});

describe("FetchErrorCard — title text matches fetchErrorTitle()", () => {
  for (const { name, payload } of variants) {
    it(`renders the ${name} title`, async () => {
      mockedIsTauri.mockReturnValue(false);
      const { getByText } = render(FetchErrorCard, {
        error: payload,
        onretry: vi.fn(),
      });
      await flush();
      // Title text is the SSoT — couples the rendered header to the same
      // string the support triage uses.
      expect(getByText(fetchErrorTitle(payload))).toBeInTheDocument();
    });
  }
});

describe("FetchErrorCard — Retry button is always rendered", () => {
  for (const { name, payload } of variants) {
    for (const desktop of [false, true]) {
      it(`shows Retry for ${name} (isTauri=${desktop})`, async () => {
        mockedIsTauri.mockReturnValue(desktop);
        const { getByRole } = render(FetchErrorCard, {
          error: payload,
          onretry: vi.fn(),
        });
        await flush();
        const retry = getByRole("button", { name: /retry/i });
        expect(retry).toBeInTheDocument();
        expect(retry).not.toBeDisabled();
      });
    }
  }
});

describe("FetchErrorCard — Open URL button visibility tracks triedUrl", () => {
  it("renders Open URL when the variant carries a URL (HttpStatus)", async () => {
    mockedIsTauri.mockReturnValue(false);
    const { queryByRole } = render(FetchErrorCard, {
      error: variants[0].payload,
      onretry: vi.fn(),
    });
    await flush();
    expect(queryByRole("button", { name: /open url/i })).toBeInTheDocument();
  });

  it("renders Open URL for variants without a URL via the SSoT fallback", async () => {
    // Decompress carries no url field; the component falls back to
    // `getReleaseUrl()` (mocked above to return a non-empty string).
    mockedIsTauri.mockReturnValue(false);
    const { queryByRole } = render(FetchErrorCard, {
      error: variants[2].payload,
      onretry: vi.fn(),
    });
    await flush();
    expect(queryByRole("button", { name: /open url/i })).toBeInTheDocument();
  });
});

describe("FetchErrorCard — desktop-only buttons gated by isTauri()", () => {
  it("hides Install/Use-bundled in browser mode (isTauri=false)", async () => {
    mockedIsTauri.mockReturnValue(false);
    const { queryByRole } = render(FetchErrorCard, {
      error: variants[0].payload,
      onretry: vi.fn(),
      onuselimited: vi.fn(),
    });
    await flush();
    expect(
      queryByRole("button", { name: /install from local tarball/i }),
    ).toBeNull();
    expect(queryByRole("button", { name: /use bundled data only/i })).toBeNull();
  });

  it("shows Install in desktop mode (isTauri=true)", async () => {
    mockedIsTauri.mockReturnValue(true);
    const { queryByRole } = render(FetchErrorCard, {
      error: variants[0].payload,
      onretry: vi.fn(),
    });
    await flush();
    expect(
      queryByRole("button", { name: /install from local tarball/i }),
    ).toBeInTheDocument();
    // Without onuselimited, the bundled button stays hidden even on desktop.
    expect(queryByRole("button", { name: /use bundled data only/i })).toBeNull();
  });

  it("shows Use-bundled only when isTauri=true AND onuselimited provided", async () => {
    mockedIsTauri.mockReturnValue(true);
    const { queryByRole } = render(FetchErrorCard, {
      error: variants[0].payload,
      onretry: vi.fn(),
      onuselimited: vi.fn(),
    });
    await flush();
    expect(
      queryByRole("button", { name: /use bundled data only/i }),
    ).toBeInTheDocument();
  });
});

describe("FetchErrorCard — callbacks fire on button click", () => {
  it("Retry → onretry()", async () => {
    mockedIsTauri.mockReturnValue(false);
    const onretry = vi.fn();
    const { getByRole } = render(FetchErrorCard, {
      error: variants[0].payload,
      onretry,
    });
    await flush();
    await fireEvent.click(getByRole("button", { name: /retry/i }));
    expect(onretry).toHaveBeenCalledTimes(1);
  });

  it("Use bundled data only → onuselimited()", async () => {
    mockedIsTauri.mockReturnValue(true);
    const onretry = vi.fn();
    const onuselimited = vi.fn();
    const { getByRole } = render(FetchErrorCard, {
      error: variants[0].payload,
      onretry,
      onuselimited,
    });
    await flush();
    await fireEvent.click(
      getByRole("button", { name: /use bundled data only/i }),
    );
    expect(onuselimited).toHaveBeenCalledTimes(1);
    // The use-bundled handler must not also kick a retry.
    expect(onretry).not.toHaveBeenCalled();
  });

  it("Open URL → openExternalUrl(triedUrl)", async () => {
    mockedIsTauri.mockReturnValue(false);
    const { openExternalUrl } = await import("../utils/open-url");
    const mocked = vi.mocked(openExternalUrl);
    mocked.mockClear();
    const { getByRole } = render(FetchErrorCard, {
      error: variants[0].payload, // HttpStatus carries url=URL_
      onretry: vi.fn(),
    });
    await flush();
    await fireEvent.click(getByRole("button", { name: /open url/i }));
    expect(mocked).toHaveBeenCalledTimes(1);
    expect(mocked).toHaveBeenCalledWith(URL_);
  });
});

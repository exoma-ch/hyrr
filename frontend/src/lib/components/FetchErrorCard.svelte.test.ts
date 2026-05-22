/**
 * Render-matrix coverage for `FetchErrorCard` (#169).
 *
 * The pure-function path is covered by `parse-fetch-error.test.ts`; this
 * suite asserts the rendered contract — title text, button visibility,
 * and callback wiring.
 */
import { describe, it, expect, vi, afterEach } from "vitest";
import { render, fireEvent, cleanup } from "@testing-library/svelte";

// Mock the platform helper.
vi.mock("../utils/platform", () => ({
  isTauri: vi.fn(() => false),
  detectOS: () => "macos",
}));

// Mock the data-fetch-meta SSoT.
vi.mock("../compute/data-fetch-meta", () => ({
  getReleaseUrl: vi.fn(async () =>
    "https://github.com/exoma-ch/nucl-parquet/releases/download/data-2026.5.0/nucl-parquet-data-2026.5.0.tar.zst",
  ),
  getCacheRootPattern: vi.fn(async () => "~/.hyrr/nucl-parquet/<version>"),
  getReleaseBaseUrl: vi.fn(async () => "https://github.com/exoma-ch/nucl-parquet"),
  getDataVersion: vi.fn(async () => "2026.5.0"),
  DEFAULT_LIBRARY: "tendl-2023-iso",
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

const CACHE = "/Users/x/.hyrr/nucl-parquet/v2026.5.0";
const URL_ = "https://github.com/exoma-ch/nucl-parquet/releases/download/data-2026.5.0/nucl-parquet-data-2026.5.0.tar.zst";

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
      expect(getByText(fetchErrorTitle(payload))).toBeInTheDocument();
    });
  }
});

describe("FetchErrorCard — Retry button is always rendered", () => {
  for (const { name, payload } of variants) {
    it(`shows Retry for ${name}`, async () => {
      mockedIsTauri.mockReturnValue(false);
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
    mockedIsTauri.mockReturnValue(false);
    const { queryByRole } = render(FetchErrorCard, {
      error: variants[2].payload,
      onretry: vi.fn(),
    });
    await flush();
    expect(queryByRole("button", { name: /open url/i })).toBeInTheDocument();
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

  it("Open URL → openExternalUrl(triedUrl)", async () => {
    mockedIsTauri.mockReturnValue(false);
    const { openExternalUrl } = await import("../utils/open-url");
    const mocked = vi.mocked(openExternalUrl);
    mocked.mockClear();
    const { getByRole } = render(FetchErrorCard, {
      error: variants[0].payload,
      onretry: vi.fn(),
    });
    await flush();
    await fireEvent.click(getByRole("button", { name: /open url/i }));
    expect(mocked).toHaveBeenCalledTimes(1);
    expect(mocked).toHaveBeenCalledWith(URL_);
  });
});

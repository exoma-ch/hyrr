import { describe, it, expect } from "vitest";
import {
  parseFetchError,
  fetchErrorTitle,
  type FetchErrorPayload,
} from "./parse-fetch-error";

describe("parseFetchError — every Rust variant round-trips", () => {
  it("classifies HttpStatus from a structured payload", () => {
    const payload = {
      kind: "FetchError",
      variant: "HttpStatus",
      status: 404,
      url: "https://github.com/exoma-ch/nucl-parquet/releases/download/v0.10.0/nucl-parquet-data-v0.10.0.tar.zst",
      cache_dir: "/Users/x/.hyrr/nucl-parquet/v0.10.0",
      message: "HTTP 404",
    };
    const result = parseFetchError(payload);
    expect(result.kind).toBe("FetchError");
    if (result.kind !== "FetchError" || result.variant !== "HttpStatus") {
      throw new Error("expected HttpStatus variant");
    }
    expect(result.status).toBe(404);
    expect(result.url).toContain("nucl-parquet-data-v0.10.0.tar.zst");
    expect(result.cache_dir).toContain(".hyrr");
  });

  it("classifies Network", () => {
    const payload = {
      kind: "FetchError",
      variant: "Network",
      detail: "dns lookup failed",
      url: "https://example.com/x.tar.zst",
      cache_dir: "/home/x/.hyrr/nucl-parquet/v0.10.0",
      message: "network error: dns lookup failed",
    };
    const result = parseFetchError(payload);
    if (result.kind !== "FetchError" || result.variant !== "Network") {
      throw new Error("expected Network variant");
    }
    expect(result.detail).toBe("dns lookup failed");
    expect(result.url).toContain("https://");
  });

  it("classifies Decompress", () => {
    const result = parseFetchError({
      kind: "FetchError",
      variant: "Decompress",
      cache_dir: "/h/.hyrr/nucl-parquet/v0.10.0",
      message: "zstd: bad magic",
    });
    if (result.kind !== "FetchError" || result.variant !== "Decompress") {
      throw new Error("expected Decompress variant");
    }
    expect(result.message).toContain("zstd");
  });

  it("classifies Extract", () => {
    const result = parseFetchError({
      kind: "FetchError",
      variant: "Extract",
      cache_dir: "/h/.hyrr/nucl-parquet/v0.10.0",
      message: "unexpected eof",
    });
    if (result.kind !== "FetchError" || result.variant !== "Extract") {
      throw new Error("expected Extract variant");
    }
    expect(result.cache_dir).toContain(".hyrr");
  });

  it("classifies UnsafeTarballEntry", () => {
    const result = parseFetchError({
      kind: "FetchError",
      variant: "UnsafeTarballEntry",
      entry_kind: "Symlink",
      entry_path: "data/meta/evil",
      cache_dir: "/h/.hyrr/nucl-parquet/v0.10.0",
      message: "unsafe tarball entry Symlink at data/meta/evil",
    });
    if (
      result.kind !== "FetchError" ||
      result.variant !== "UnsafeTarballEntry"
    ) {
      throw new Error("expected UnsafeTarballEntry variant");
    }
    expect(result.entry_kind).toBe("Symlink");
    expect(result.entry_path).toBe("data/meta/evil");
  });

  it("classifies Io", () => {
    const result = parseFetchError({
      kind: "FetchError",
      variant: "Io",
      cache_dir: "/h/.hyrr/nucl-parquet/v0.10.0",
      message: "insufficient disk space: have 2 MiB free, need at least 1024 MiB",
    });
    if (result.kind !== "FetchError" || result.variant !== "Io") {
      throw new Error("expected Io variant");
    }
    expect(result.message).toContain("disk space");
  });

  it("classifies NoHome (no url / cache_dir)", () => {
    const result = parseFetchError({
      kind: "FetchError",
      variant: "NoHome",
      message: "HOME environment variable not set",
    });
    if (result.kind !== "FetchError" || result.variant !== "NoHome") {
      throw new Error("expected NoHome variant");
    }
    expect(result.message).toContain("HOME");
  });
});

describe("parseFetchError — fallbacks", () => {
  it("parses a JSON-string payload (Tauri Result<_, String> shape)", () => {
    const json = JSON.stringify({
      kind: "FetchError",
      variant: "HttpStatus",
      status: 503,
      url: "https://e/a.tar.zst",
      cache_dir: "/h/.hyrr/nucl-parquet/v0.10.0",
      message: "HTTP 503",
    });
    const result = parseFetchError(json);
    if (result.kind !== "FetchError" || result.variant !== "HttpStatus") {
      throw new Error("expected HttpStatus from JSON string");
    }
    expect(result.status).toBe(503);
  });

  it("falls back to unknown for non-JSON strings", () => {
    const result = parseFetchError("ensure_meta_stopping: io error: foo");
    expect(result.kind).toBe("unknown");
    expect(result.message).toContain("ensure_meta_stopping");
  });

  it("falls back to unknown for plain Error instances", () => {
    const result = parseFetchError(new Error("Failed to load nuclear data"));
    expect(result.kind).toBe("unknown");
    expect(result.message).toContain("Failed to load nuclear data");
  });

  it("falls back to unknown for an unrecognised variant", () => {
    const result = parseFetchError({
      kind: "FetchError",
      variant: "TotallyMadeUpVariant",
      message: "x",
    });
    expect(result.kind).toBe("unknown");
  });

  it("falls back to unknown for null / undefined", () => {
    expect(parseFetchError(null).kind).toBe("unknown");
    expect(parseFetchError(undefined).kind).toBe("unknown");
  });

  it("falls back to unknown for a non-FetchError object", () => {
    const result = parseFetchError({ kind: "StoppingError", message: "x" });
    expect(result.kind).toBe("unknown");
  });
});

describe("fetchErrorTitle — variant-aware copy for support triage", () => {
  type Case = [FetchErrorPayload, string];
  const cases: Case[] = [
    [
      {
        kind: "FetchError",
        variant: "HttpStatus",
        status: 404,
        url: "x",
        cache_dir: "x",
        message: "",
      },
      "HTTP 404",
    ],
    [
      {
        kind: "FetchError",
        variant: "Network",
        detail: "x",
        url: "x",
        cache_dir: "x",
        message: "",
      },
      "reach",
    ],
    [
      {
        kind: "FetchError",
        variant: "Decompress",
        cache_dir: "x",
        message: "",
      },
      "decompress",
    ],
    [
      { kind: "FetchError", variant: "Extract", cache_dir: "x", message: "" },
      "extract",
    ],
    [
      { kind: "FetchError", variant: "Io", cache_dir: "x", message: "" },
      "write",
    ],
    [{ kind: "FetchError", variant: "NoHome", message: "" }, "home"],
  ];

  for (const [payload, fragment] of cases) {
    it(`includes "${fragment}" for ${payload.variant}`, () => {
      const title = fetchErrorTitle(payload);
      expect(title.toLowerCase()).toContain(fragment.toLowerCase());
    });
  }

  it("provides a generic title for unknown errors", () => {
    expect(fetchErrorTitle({ kind: "unknown", message: "x" })).toContain(
      "Couldn't download",
    );
  });
});

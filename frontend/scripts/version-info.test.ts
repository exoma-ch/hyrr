import { describe, expect, it } from "vitest";
import {
  buildVersionInfo,
  gitHeadFromWorkTree,
  resolveBuiltAt,
  resolveCommit,
} from "./version-info";

const never = () => {
  throw new Error("git lookup should not have been attempted");
};

describe("resolveCommit", () => {
  it("prefers an explicit VITE_BUILD_COMMIT over everything else", () => {
    expect(resolveCommit({ VITE_BUILD_COMMIT: "abc123", GITHUB_SHA: "def456" }, never)).toBe(
      "abc123",
    );
  });

  it("prefers GITHUB_SHA over the local work tree", () => {
    // On a pull_request event HEAD is a synthetic merge commit that exists in
    // no repository; GITHUB_SHA is the one that can actually be resolved.
    expect(resolveCommit({ GITHUB_SHA: "def456" }, () => "synthetic-merge")).toBe("def456");
  });

  it("falls back to the work tree when no env var is set", () => {
    expect(resolveCommit({}, () => "0cb27e1e")).toBe("0cb27e1e");
  });

  it("reports 'unknown' rather than throwing when git is unavailable", () => {
    // A tarball or nix-sandbox build has no repository. Provenance is a
    // nice-to-have; it must never be why a build fails.
    expect(resolveCommit({}, () => null)).toBe("unknown");
  });

  it("treats blank and whitespace-only values as absent", () => {
    expect(resolveCommit({ VITE_BUILD_COMMIT: "  ", GITHUB_SHA: "" }, () => "  ")).toBe("unknown");
  });

  it("trims surrounding whitespace off a provided value", () => {
    expect(resolveCommit({ GITHUB_SHA: " def456\n" }, never)).toBe("def456");
  });
});

describe("resolveBuiltAt", () => {
  const now = () => new Date("2026-08-06T12:00:00.000Z");

  it("uses wall-clock time by default", () => {
    expect(resolveBuiltAt({}, now)).toBe("2026-08-06T12:00:00.000Z");
  });

  it("IGNORES SOURCE_DATE_EPOCH", () => {
    // Regression guard, not a preference. Nix exports SOURCE_DATE_EPOCH
    // globally as 315532800 (1980-01-01), so honouring it would stamp every
    // build from a nix shell — i.e. every build on the dev host — as 1980 and
    // silently destroy the field's only purpose. Caught by running a real
    // `vite build` rather than trusting the unit test.
    expect(resolveBuiltAt({ SOURCE_DATE_EPOCH: "315532800" }, now)).toBe("2026-08-06T12:00:00.000Z");
  });

  it("honours an explicit VITE_BUILD_TIMESTAMP pin", () => {
    expect(resolveBuiltAt({ VITE_BUILD_TIMESTAMP: "2020-01-02T03:04:05Z" }, now)).toBe(
      "2020-01-02T03:04:05.000Z",
    );
  });

  it("falls back to wall-clock on an unparseable pin instead of emitting Invalid Date", () => {
    expect(resolveBuiltAt({ VITE_BUILD_TIMESTAMP: "yesterday" }, now)).toBe(
      "2026-08-06T12:00:00.000Z",
    );
  });
});

describe("buildVersionInfo", () => {
  it("emits exactly the three documented fields", () => {
    const info = buildVersionInfo("0.19.0", {
      env: { GITHUB_SHA: "0cb27e1e" },
      now: () => new Date("2026-08-06T12:00:00.000Z"),
    });
    expect(info).toEqual({
      version: "0.19.0",
      commit: "0cb27e1e",
      built_at: "2026-08-06T12:00:00.000Z",
    });
  });

  it("serialises to JSON that a deploy check can parse", () => {
    const info = buildVersionInfo("0.19.0", { env: {}, gitHead: () => null, now: () => new Date(0) });
    expect(JSON.parse(JSON.stringify(info)).version).toBe("0.19.0");
  });
});

describe("gitHeadFromWorkTree", () => {
  it("returns a full SHA or null, never throws", () => {
    // Runs against whatever the checkout actually is — the contract under test
    // is "never throws", since the build depends on that.
    const head = gitHeadFromWorkTree();
    expect(head === null || /^[0-9a-f]{40}$/.test(head)).toBe(true);
  });
});

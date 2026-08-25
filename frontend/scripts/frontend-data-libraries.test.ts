import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import { DEFAULT_SUBDIR, parseLibraryList } from "./frontend-data-libraries";

const repoRoot = fileURLToPath(new URL("../..", import.meta.url));
const listPath = join(repoRoot, "scripts", "frontend-data-libraries.txt");
const realList = readFileSync(listPath, "utf8");

describe("parseLibraryList", () => {
  it("parses bare names into the default subdir", () => {
    expect(parseLibraryList("tendl-2023-iso\n")).toEqual([
      { spec: "tendl-2023-iso", library: "tendl-2023-iso", subdir: DEFAULT_SUBDIR, line: 1 },
    ]);
  });

  it("parses name:subdir", () => {
    const [spec] = parseLibraryList("endfb-8.0:neutron-xs\n");
    expect(spec.library).toBe("endfb-8.0");
    expect(spec.subdir).toBe("neutron-xs");
  });

  it("ignores comments, inline comments, and blank lines", () => {
    const specs = parseLibraryList(
      ["# a header", "", "tendl-2023-iso  # the default", "   ", "# trailing note"].join("\n"),
    );
    expect(specs.map((s) => s.spec)).toEqual(["tendl-2023-iso"]);
  });

  // The #677 regression. On a CRLF checkout the old inline
  // `line.replace(/#.*$/, "")` was a no-op — `.` does not match `\r`, so `$`
  // could never be reached — and every comment line survived as a "library".
  // Windows CI then failed demanding subdirectories named after prose, while
  // the data bundle it was rejecting was complete.
  it.each([
    ["LF", (s: string) => s],
    ["CRLF", (s: string) => s.replace(/\n/g, "\r\n")],
    ["CR", (s: string) => s.replace(/\n/g, "\r")],
  ])("gives identical results for %s line endings", (_label, rewrite) => {
    const specs = parseLibraryList(rewrite(realList));
    expect(specs.map((s) => `${s.library}:${s.subdir}`)).toEqual([
      "tendl-2023-iso:xs",
      "hi-xs-prod:hi-xs-prod",
      "endfb-8.0:neutron-xs",
    ]);
  });

  it("rejects prose instead of silently treating it as a library", () => {
    expect(() => parseLibraryList("tendl-2023-iso\nFormat: one entry per line\n")).toThrow(
      /:2: malformed entry/,
    );
  });

  it("rejects path traversal in a subdir", () => {
    expect(() => parseLibraryList("endfb-8.0:../../etc\n")).toThrow(/malformed entry/);
  });

  it("rejects an empty list rather than shipping a bundle with no cross-sections", () => {
    expect(() => parseLibraryList("# only comments\n")).toThrow(/no libraries listed/);
  });

  it("keeps the real SSoT list parseable and non-empty", () => {
    const specs = parseLibraryList(realList, listPath);
    expect(specs.length).toBeGreaterThan(0);
    // Every neutron preset reads from `neutron-xs/` (#651) — losing it is the
    // silent-empty-table bug the whole guard exists to prevent.
    expect(specs.map((s) => s.subdir)).toContain("neutron-xs");
  });
});

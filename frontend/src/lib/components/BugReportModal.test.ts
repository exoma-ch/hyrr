import { describe, it, expect } from "vitest";
import { buildIssueTitle, bodyTitleHeader, TITLE_MAX_LEN } from "./bug-report-title";

describe("buildIssueTitle", () => {
  it("uses the user-supplied title when set", () => {
    const got = buildIssueTitle("bug", "Heavy ions crash compute", "long description text");
    expect(got).toBe("[Bug] Heavy ions crash compute");
  });

  it("falls back to truncated description when title is blank", () => {
    const desc = "x".repeat(120);
    const got = buildIssueTitle("bug", "", desc);
    expect(got).toBe(`[Bug] ${"x".repeat(70)}`);
  });

  it("falls back to truncated description when title is whitespace-only", () => {
    const desc = "no depth profile, stopping profile, power, no cross sections viewable in foo bar";
    const got = buildIssueTitle("bug", "   ", desc);
    expect(got).toBe(`[Bug] ${desc.slice(0, 70)}`);
  });

  it("slices a too-long user title to the cap", () => {
    const longTitle = "y".repeat(120);
    const got = buildIssueTitle("bug", longTitle, "anything");
    expect(got).toBe(`[Bug] ${"y".repeat(TITLE_MAX_LEN)}`);
  });

  it("uses [Feature] prefix for feature requests", () => {
    const got = buildIssueTitle("feature", "Add streaming", "");
    expect(got).toBe("[Feature] Add streaming");
  });
});

describe("bodyTitleHeader", () => {
  it("renders the user title as a section header", () => {
    expect(bodyTitleHeader("My title", "ignored desc")).toBe("## My title");
  });

  it("renders the truncated description when title is blank", () => {
    const desc = "z".repeat(100);
    expect(bodyTitleHeader("", desc)).toBe(`## ${"z".repeat(70)}`);
  });
});

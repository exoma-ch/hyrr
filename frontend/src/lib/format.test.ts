import { describe, it, expect } from "vitest";
import { formatDecayMode, formatProjectile, formatReaction, reactionFilterMatches } from "./format";

describe("formatDecayMode", () => {
  it("maps beta_plus / beta+ / positron to β⁺", () => {
    expect(formatDecayMode("beta_plus")).toBe("β⁺");
    expect(formatDecayMode("beta+")).toBe("β⁺");
    expect(formatDecayMode("positron")).toBe("β⁺");
    expect(formatDecayMode("B+")).toBe("β⁺");
  });

  it("maps beta_minus / beta- / electron to β⁻", () => {
    expect(formatDecayMode("beta_minus")).toBe("β⁻");
    expect(formatDecayMode("beta-")).toBe("β⁻");
    expect(formatDecayMode("electron")).toBe("β⁻");
  });

  it("maps alpha → α and gamma → γ", () => {
    expect(formatDecayMode("alpha")).toBe("α");
    expect(formatDecayMode("gamma")).toBe("γ");
  });

  it("passes IT and EC through unchanged", () => {
    expect(formatDecayMode("IT")).toBe("IT");
    expect(formatDecayMode("EC")).toBe("EC");
  });

  it("passes unknown / empty modes through", () => {
    expect(formatDecayMode("")).toBe("");
    expect(formatDecayMode("unknown_mode")).toBe("unknown_mode");
  });
});

describe("formatProjectile", () => {
  it("maps light hadrons unchanged", () => {
    expect(formatProjectile("p")).toBe("p");
    expect(formatProjectile("n")).toBe("n");
    expect(formatProjectile("d")).toBe("d");
    expect(formatProjectile("t")).toBe("t");
  });

  it("maps alpha → α and gamma → γ", () => {
    expect(formatProjectile("alpha")).toBe("α");
    expect(formatProjectile("gamma")).toBe("γ");
  });

  it("maps he3 variants to ³He", () => {
    expect(formatProjectile("he3")).toBe("³He");
    expect(formatProjectile("he-3")).toBe("³He");
    expect(formatProjectile("3he")).toBe("³He");
  });

  it("passes unknown through", () => {
    expect(formatProjectile("xx")).toBe("xx");
    expect(formatProjectile("")).toBe("");
  });
});

describe("formatReaction", () => {
  it("maps (p,2n) unchanged (no Greek particles)", () => {
    expect(formatReaction("(p,2n)")).toBe("(p,2n)");
  });

  it("maps (alpha,gamma) to (α,γ)", () => {
    expect(formatReaction("(alpha,gamma)")).toBe("(α,γ)");
  });

  it("maps (d,p) and (alpha,n)", () => {
    expect(formatReaction("(d,p)")).toBe("(d,p)");
    expect(formatReaction("(alpha,n)")).toBe("(α,n)");
  });

  it("preserves multiplier prefixes like 2n, 3alpha", () => {
    expect(formatReaction("(p,2alpha)")).toBe("(p,2α)");
  });

  it("is idempotent on already-Greek input", () => {
    expect(formatReaction("(α,γ)")).toBe("(α,γ)");
    expect(formatReaction("²⁷Al(p,αn)²²Na")).toBe("²⁷Al(p,αn)²²Na");
  });

  it("maps decay-arrow notation", () => {
    expect(formatReaction("²²Mg →beta+→ ²²Na")).toBe("²²Mg →β⁺→ ²²Na");
    expect(formatReaction("⁹⁹Mo →β-→ ⁹⁹Tc")).toBe("⁹⁹Mo →β⁻→ ⁹⁹Tc");
  });

  it("returns empty for empty input", () => {
    expect(formatReaction("")).toBe("");
  });
});

describe("reactionFilterMatches (round-trip aliases)", () => {
  it("ASCII filter input 'beta+' matches a row whose text is 'β⁺'", () => {
    expect(reactionFilterMatches("²²Na (β⁺)", "beta+")).toBe(true);
  });

  it("Greek filter input 'β⁺' matches a row whose text contains 'beta+'", () => {
    expect(reactionFilterMatches("Na-22 (beta+)", "β⁺")).toBe(true);
  });

  it("plain substring match still works", () => {
    expect(reactionFilterMatches("(p,2n)", "p,2n")).toBe(true);
    expect(reactionFilterMatches("²²Na (β⁺)", "β⁺")).toBe(true);
  });

  it("alpha alias works both directions", () => {
    expect(reactionFilterMatches("²⁷Al(p,α)²⁴Na", "alpha")).toBe(true);
    expect(reactionFilterMatches("(p,alpha)", "α")).toBe(true);
  });

  it("non-matching needle returns false", () => {
    expect(reactionFilterMatches("(p,2n)", "alpha")).toBe(false);
  });

  it("empty needle matches anything", () => {
    expect(reactionFilterMatches("anything", "")).toBe(true);
  });
});

import { describe, it, expect } from "vitest";
import {
  aggregateDecayModes,
  dedupeDecayChain,
  formatDecayChainTooltip,
  formatDecayMode,
  formatProjectile,
  formatReaction,
  isShellEC,
  reactionFilterMatches,
} from "./format";

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

describe("isShellEC", () => {
  it("matches Kshell/Lshell/Mshell EC in any case", () => {
    expect(isShellEC("KshellEC")).toBe(true);
    expect(isShellEC("LshellEC")).toBe(true);
    expect(isShellEC("MshellEC")).toBe(true);
    expect(isShellEC("kshellec")).toBe(true);
    expect(isShellEC("L_shell_EC")).toBe(true);
  });

  it("does not match plain EC, beta+, or other modes", () => {
    expect(isShellEC("EC")).toBe(false);
    expect(isShellEC("ec")).toBe(false);
    expect(isShellEC("beta+")).toBe(false);
    expect(isShellEC("alpha")).toBe(false);
    expect(isShellEC("")).toBe(false);
  });
});

describe("aggregateDecayModes", () => {
  it("collapses K/L/M shell EC into a single ec bucket with summed branching", () => {
    const out = aggregateDecayModes([
      { mode: "KshellEC", branching: 0.892 },
      { mode: "LshellEC", branching: 0.093 },
      { mode: "MshellEC", branching: 0.015 },
    ]);
    expect(out).toHaveLength(1);
    expect(out[0].mode).toBe("ec");
    expect(out[0].branching).toBeCloseTo(1.0, 6);
    expect(out[0].sources).toEqual(["KshellEC", "LshellEC", "MshellEC"]);
  });

  it("renders the bucket label via formatDecayMode as EC", () => {
    const out = aggregateDecayModes([
      { mode: "KshellEC", branching: 0.5 },
      { mode: "LshellEC", branching: 0.5 },
    ]);
    expect(formatDecayMode(out[0].mode)).toBe("EC");
  });

  it("aggregates EC only — keeps β⁺ and other modes as their own buckets", () => {
    const out = aggregateDecayModes([
      { mode: "KshellEC", branching: 0.025 },
      { mode: "LshellEC", branching: 0.003 },
      { mode: "MshellEC", branching: 0.0005 },
      { mode: "beta+", branching: 0.9715 },
    ]);
    expect(out).toHaveLength(2);
    expect(out[0].mode).toBe("ec");
    expect(out[0].branching).toBeCloseTo(0.0285, 6);
    expect(out[1].mode).toBe("beta+");
    expect(out[1].branching).toBeCloseTo(0.9715, 6);
  });

  it("is a passthrough when no shell-EC entries are present", () => {
    const out = aggregateDecayModes([
      { mode: "beta-", branching: 1.0 },
    ]);
    expect(out).toHaveLength(1);
    expect(out[0].mode).toBe("beta-");
    expect(out[0].branching).toBe(1.0);
    expect(out[0].sources).toEqual(["beta-"]);
  });

  it("preserves first-occurrence order of distinct modes", () => {
    const out = aggregateDecayModes([
      { mode: "beta+", branching: 0.5 },
      { mode: "KshellEC", branching: 0.4 },
      { mode: "alpha", branching: 0.1 },
    ]);
    expect(out.map((m) => m.mode)).toEqual(["beta+", "ec", "alpha"]);
  });

  it("returns empty array for empty input", () => {
    expect(aggregateDecayModes([])).toEqual([]);
  });
});

describe("dedupeDecayChain", () => {
  it("collapses Cr-51 K/L/M EC chain into a single V-51 daughter entry", () => {
    const daughters = [
      { mode: "KshellEC", branching: 0.89182, daughterZ: 23, daughterA: 51, daughterState: "" },
      { mode: "LshellEC", branching: 0.092759, daughterZ: 23, daughterA: 51, daughterState: "" },
      { mode: "MshellEC", branching: 0.015425, daughterZ: 23, daughterA: 51, daughterState: "" },
    ];
    const out = dedupeDecayChain(
      daughters,
      (d) => `${d.daughterZ}|${d.daughterA}|${d.daughterState}`,
    );
    expect(out).toHaveLength(1);
    expect(out[0].mode).toBe("ec");
    expect(out[0].branching).toBeCloseTo(1.0, 4);
    expect(out[0].sources).toHaveLength(3);
    expect(out[0].daughterZ).toBe(23);
    expect(out[0].daughterA).toBe(51);
  });

  it("collapses 51Mn parent K/L/M EC + β⁺ into a single Mn-51 parent entry", () => {
    const parents = [
      { name: "Mn-51", Z: 25, A: 51, state: "", mode: "KshellEC", branching: 0.025955 },
      { name: "Mn-51", Z: 25, A: 51, state: "", mode: "LshellEC", branching: 0.0027038 },
      { name: "Mn-51", Z: 25, A: 51, state: "", mode: "MshellEC", branching: 0.00046853 },
      { name: "Mn-51", Z: 25, A: 51, state: "", mode: "beta+", branching: 0.97087 },
    ];
    const out = dedupeDecayChain(parents, (p) => `${p.Z}|${p.A}|${p.state}`);
    expect(out).toHaveLength(1);
    expect(out[0].name).toBe("Mn-51");
    // Dominant mode is β⁺ at 97.1%, not EC at 2.9%
    expect(out[0].mode).toBe("beta+");
    expect(out[0].branching).toBeCloseTo(1.0, 4);
    expect(out[0].sources).toHaveLength(4);
  });

  it("keeps distinct daughter nuclides separate", () => {
    const daughters = [
      { mode: "beta-", branching: 0.6, daughterZ: 24, daughterA: 51, daughterState: "" },
      { mode: "alpha", branching: 0.4, daughterZ: 22, daughterA: 47, daughterState: "" },
    ];
    const out = dedupeDecayChain(
      daughters,
      (d) => `${d.daughterZ}|${d.daughterA}|${d.daughterState}`,
    );
    expect(out).toHaveLength(2);
  });

  it("keeps distinct daughter states separate (ground vs metastable)", () => {
    const daughters = [
      { mode: "KshellEC", branching: 0.7, daughterZ: 23, daughterA: 51, daughterState: "" },
      { mode: "KshellEC", branching: 0.3, daughterZ: 23, daughterA: 51, daughterState: "m" },
    ];
    const out = dedupeDecayChain(
      daughters,
      (d) => `${d.daughterZ}|${d.daughterA}|${d.daughterState}`,
    );
    expect(out).toHaveLength(2);
    expect(out[0].daughterState).toBe("");
    expect(out[1].daughterState).toBe("m");
  });

  it("returns empty array for empty input", () => {
    expect(dedupeDecayChain([], () => "")).toEqual([]);
  });
});

describe("formatDecayChainTooltip", () => {
  it("annotates aggregated EC with K/L/M breakdown", () => {
    const tip = formatDecayChainTooltip([
      { mode: "KshellEC", branching: 0.892 },
      { mode: "LshellEC", branching: 0.093 },
      { mode: "MshellEC", branching: 0.015 },
    ]);
    expect(tip).toBe("EC (100.0%) [KshellEC 89.2%, LshellEC 9.3%, MshellEC 1.5%]");
  });

  it("does not annotate single-source modes", () => {
    const tip = formatDecayChainTooltip([{ mode: "beta+", branching: 1.0 }]);
    expect(tip).toBe("β⁺ (100.0%)");
  });

  it("lists multiple aggregated buckets comma-separated", () => {
    const tip = formatDecayChainTooltip([
      { mode: "beta+", branching: 0.97 },
      { mode: "KshellEC", branching: 0.025 },
      { mode: "LshellEC", branching: 0.005 },
    ]);
    expect(tip).toBe("β⁺ (97.0%), EC (3.0%) [KshellEC 2.5%, LshellEC 0.5%]");
  });

  it("returns empty string for empty input", () => {
    expect(formatDecayChainTooltip([])).toBe("");
  });
});

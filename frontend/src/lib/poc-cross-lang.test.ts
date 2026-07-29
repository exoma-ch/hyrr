/**
 * Cross-language round-trip proof for issue #539 (codec-only B′).
 *
 * The money shot: a `#config=` hash produced by the **Rust** codec
 * (`core/src/config_codec.rs`, committed to `__fixtures__/poc-rust-encoded.txt`
 * by the Rust test `write_cross_lang_fixture`) is decoded by the **real,
 * unmodified** TS decoder `decodeConfigV2Ser`. We assert the embedded custom
 * alloy — density + mass fractions + formula — survives Rust → TS with ZERO
 * changes to the TS decoder.
 *
 * This is exactly the #531 failure mode inverted: today the Rust encoder never
 * emits the `x` InlineComposition, so MCP links silently drop the alloy. Extend
 * the Rust encoder to emit `x`, and the decoder that was ALREADY there round-
 * trips it — validating that B′ fixes #531 cross-language.
 */
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import {
  decodeConfigV2Ser,
  getSharedCustomMaterial,
} from "./config-url-v2";
import { initCustomMaterialRegistry } from "./compute/custom-material-registry";

// The recipient has NO local library — the alloy must arrive entirely via the
// link (the cross-machine break #531 describes).
initCustomMaterialRegistry(() => []);

const FIXTURE = fileURLToPath(
  new URL("./__fixtures__/poc-rust-encoded.txt", import.meta.url),
);

describe("cross-language codec round-trip (#539)", () => {
  it("decodes a Rust-encoded hash and recovers the embedded custom alloy", () => {
    const hash = readFileSync(FIXTURE, "utf8").trim();
    expect(hash.startsWith("#config=1:")).toBe(true);

    const decoded = decodeConfigV2Ser(hash.replace("#config=1:", ""));
    expect(decoded).not.toBeNull();

    // The Rust encoder set `x` on the alloy layer → the TS decoder set the
    // layer density from it (so the Rust backend gets the custom density too).
    const alloyLayer = decoded!.items[0] as { material: string; density_g_cm3?: number };
    expect(alloyLayer.material).toBe("poc-inconel");
    expect(alloyLayer.density_g_cm3).toBeCloseTo(8.44);

    // THE PROOF: the full custom-material definition — density, formula, and the
    // per-element mass fractions that alter stopping power — survived Rust → TS.
    const shared = getSharedCustomMaterial("poc-inconel");
    expect(shared).not.toBeNull();
    expect(shared!.density).toBeCloseTo(8.44);
    expect(shared!.formula).toBe("Ni58Cr22Fe5Mo9Nb3.6Ti0.4");
    expect(shared!.massFractions).toEqual({
      Ni: 0.58,
      Cr: 0.22,
      Fe: 0.05,
      Mo: 0.09,
      Nb: 0.036,
      Ti: 0.004,
    });
  });
});

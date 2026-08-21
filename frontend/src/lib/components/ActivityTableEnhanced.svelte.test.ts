/**
 * Empty-state rendering for `ActivityTableEnhanced` (#650, epic #649).
 *
 * The bug users report is a simulation that completes successfully and shows an
 * empty table. `ComputeErrorCard` cannot help — it is gated on
 * `computeError && !result`, and these runs succeed — so the table itself has to
 * explain the emptiness.
 *
 * This suite is the "mandatory render" half of #650: the engine emitting a
 * `Diagnostic` is worthless if nothing displays it, which is precisely how
 * `pruned_negligible_count` ended up on the wire with zero consumers. Every
 * `DiagnosticKind` variant should gain a case here.
 */
import { describe, it, expect, afterEach } from "vitest";
import { render, cleanup, screen } from "@testing-library/svelte";

import ActivityTableEnhanced from "./ActivityTableEnhanced.svelte";
import type { SimulationResult } from "../types";

afterEach(() => cleanup());

/** A structurally valid result that produced nothing. */
function emptyResult(
  diagnostics: SimulationResult["diagnostics"] = undefined,
): SimulationResult {
  return {
    config: {
      beam: { projectile: "p", energy_MeV: 10, current_mA: 0.04 },
      layers: [{ material: "Li", thickness_cm: 0.01 }],
      irradiation_s: 3600,
      cooling_s: 0,
    },
    layers: [
      {
        layer_index: 0,
        energy_in: 10,
        energy_out: 9.5,
        delta_E_MeV: 0.5,
        heat_kW: 0,
        isotopes: [],
        depth_profile: [],
      },
    ],
    timestamp: 0,
    diagnostics,
  } as unknown as SimulationResult;
}

describe("ActivityTableEnhanced — empty state", () => {
  it("explains a data gap rather than rendering a blank table", () => {
    render(ActivityTableEnhanced, {
      props: {
        result: emptyResult([
          {
            kind: "no_cross_section_data",
            severity: "error",
            layer_index: 0,
            message:
              "No cross-section data for p + Li-7 in this library — that target isotope produced nothing.",
            projectile: "p",
            target_z: 3,
            target_symbol: "Li",
            target_a: 7,
          },
        ]),
      },
    });

    expect(screen.getByTestId("diag-empty-state")).toBeTruthy();
    expect(screen.getByTestId("diag-no_cross_section_data")).toBeTruthy();
    expect(screen.getByText(/No cross-section data for p \+ Li-7/)).toBeTruthy();
  });

  it("renders the empty-isotope-composition reason", () => {
    render(ActivityTableEnhanced, {
      props: {
        result: emptyResult([
          {
            kind: "empty_isotope_composition",
            severity: "error",
            layer_index: 0,
            message:
              "Ra has no naturally-occurring isotopes, so it contributes no target mass. Specify an enrichment to use it as a target.",
            symbol: "Ra",
            z: 88,
          },
        ]),
      },
    });

    expect(screen.getByTestId("diag-empty_isotope_composition")).toBeTruthy();
    expect(screen.getByText(/no naturally-occurring isotopes/)).toBeTruthy();
  });

  it("renders the out-of-energy-range reason", () => {
    // Found by the #654 sweep: jendl-5's d + Cu channels are tabulated
    // 130-200 MeV, so a 20 MeV run produced nothing and explained nothing.
    render(ActivityTableEnhanced, {
      props: {
        result: emptyResult([
          {
            kind: "reaction_outside_energy_range",
            severity: "error",
            layer_index: 0,
            message:
              "Cross-sections for Cu are tabulated from 130.000 to 200.000 MeV, but the beam only spans 19.998-20.000 MeV in this layer — no channel overlaps, so nothing is produced. Try a different beam energy or library.",
            symbol: "Cu",
            data_min_mev: 130,
            data_max_mev: 200,
            beam_min_mev: 19.998,
            beam_max_mev: 20,
          },
        ]),
      },
    });

    expect(screen.getByTestId("diag-reaction_outside_energy_range")).toBeTruthy();
    expect(screen.getByText(/tabulated from 130/)).toBeTruthy();
  });

  it("distinguishes a genuine zero yield from a data gap", () => {
    // No diagnostics: the data loaded and the reaction really produces nothing.
    // This must NOT claim a data problem — that would be a different lie.
    render(ActivityTableEnhanced, { props: { result: emptyResult([]) } });

    const state = screen.getByTestId("diag-empty-state");
    expect(state.textContent).toMatch(/genuinely yields nothing/);
    expect(state.textContent).not.toMatch(/here's why/);
  });

  it("tolerates a result with no diagnostics field (pre-#650 payloads)", () => {
    render(ActivityTableEnhanced, { props: { result: emptyResult(undefined) } });
    expect(screen.getByTestId("diag-empty-state")).toBeTruthy();
  });
});

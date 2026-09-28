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

/** A result with a non-empty isotope table plus an error-severity diagnostic —
 *  the #668 shape. `ActivityTableEnhanced` must show the diagnostic above the
 *  table so the user learns why a downstream yield is missing even when
 *  charged direct products fill the rows. */
function nonEmptyResultWithDiag(
  diagnostics: SimulationResult["diagnostics"] = undefined,
): SimulationResult {
  return {
    config: {
      beam: { projectile: "p", energy_MeV: 17.8, current_mA: 0.02 },
      layers: [
        { material: "Be", thickness_cm: 0.2 },
        { material: "Al", thickness_cm: 0.2 },
      ],
      irradiation_s: 3600,
      cooling_s: 0,
    },
    layers: [
      {
        layer_index: 0,
        energy_in: 17.8,
        energy_out: 5.33,
        delta_E_MeV: 12.47,
        heat_kW: 0,
        // Al direct products present — non-empty table, so the empty-tbody
        // branch of the component does NOT fire. Field names match
        // `IsotopeResultData` (mixed camelCase — TS side, not the on-wire
        // serde shape).
        isotopes: [
          {
            name: "Al-27",
            Z: 13,
            A: 27,
            state: "",
            half_life_s: null,
            production_rate: 1.54e10,
            saturation_yield_Bq_uA: 0,
            activity_Bq: 1e5,
            activity_direct_Bq: 1e5,
            activity_ingrowth_Bq: 0,
            time_grid_s: [0, 3600],
            activity_vs_time_Bq: [0, 1e5],
            source: "direct",
            reactions: ["²⁷Al(p,p)"],
            decay_notations: [],
          },
        ],
        depth_profile: [],
      },
    ],
    timestamp: 0,
    diagnostics,
  } as unknown as SimulationResult;
}

describe("ActivityTableEnhanced — above-table notice (#668)", () => {
  it("renders a collapsible notice above a non-empty table for error-severity diagnostics", () => {
    render(ActivityTableEnhanced, {
      props: {
        result: nonEmptyResultWithDiag([
          {
            kind: "secondary_neutrons_no_source",
            severity: "error",
            layer_index: null,
            message:
              "`secondary_neutron: true` was requested, but the charged pass emitted zero (x,n) free neutrons — the downstream neutron-activation pass was skipped. No cross-section data for p + Be-9 in layer 1 in this library — that upstream converter produced no free neutrons. Pick a library that carries Be-9 to restore the source.",
            missing_converter_data: [
              { layer_index: 0, projectile: "p", target_symbol: "Be", target_a: 9 },
            ],
          },
        ]),
      },
    });

    const notice = screen.getByTestId("diag-notice");
    expect(notice).toBeTruthy();
    // Non-empty tbody: the empty-state must NOT be rendered — this is the
    // above-the-table path, not the empty-tbody one.
    expect(screen.queryByTestId("diag-empty-state")).toBeNull();
    // The message must be there in full — pre-rendered by the engine.
    expect(notice.textContent).toMatch(/secondary_neutron/);
    expect(notice.textContent).toMatch(/p \+ Be-9/);
    // Notice defaults to open so the user sees the reason immediately.
    expect((notice as HTMLDetailsElement).open).toBe(true);
    // Copy must not contradict the open-by-default: the previous "Click to
    // expand" line was a bug flagged by the coordinator.
    expect(notice.textContent).not.toMatch(/click to expand/i);
  });

  it("hides the notice when only Warning-severity diagnostics are present", () => {
    render(ActivityTableEnhanced, {
      props: {
        result: nonEmptyResultWithDiag([
          {
            kind: "secondary_neutrons_no_source",
            severity: "warning",
            layer_index: null,
            message:
              "`secondary_neutron: true` was requested, but the charged pass emitted zero (x,n) free neutrons — the downstream neutron-activation pass was skipped. The library covered every upstream target, so this looks like a physically legitimate zero — no (x,n) channel is open at these energies. Raise the beam energy or drop the flag.",
            missing_converter_data: [],
          },
        ]),
      },
    });

    // Warning-severity zeros are a legitimate physical result — no banner.
    expect(screen.queryByTestId("diag-notice")).toBeNull();
  });

  it("hides the notice on a healthy run with no diagnostics", () => {
    render(ActivityTableEnhanced, {
      props: { result: nonEmptyResultWithDiag([]) },
    });
    expect(screen.queryByTestId("diag-notice")).toBeNull();
  });
});

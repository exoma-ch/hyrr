/**
 * Render-matrix coverage for `ComputeErrorCard` (#213).
 *
 * Sister to `parse-error.test.ts` which covers the typed-error
 * deserialization. This suite asserts the rendered contract — that the
 * headline + data-points + consequence text actually reflect the variant,
 * and that Unknown errors don't masquerade as missing-stopping-data.
 */
import { describe, it, expect, afterEach } from "vitest";
import { render, cleanup } from "@testing-library/svelte";

import ComputeErrorCard from "./ComputeErrorCard.svelte";
import type { ComputeError } from "../types";

afterEach(() => cleanup());

const ENERGY_OUT_OF_RANGE: ComputeError = {
  kind: "StoppingError",
  variant: "EnergyOutOfRange",
  source: "PSTAR",
  target_symbol: "O",
  target_z: 8,
  energy_mev: 0.0,
  min_mev: 0.001,
  max_mev: 10000,
  message: "Energy 0.000 MeV out of range [0.001, 10000.000]",
  layer_index: 1,
  layer_material: "H2O-18",
};

const NO_SOURCE_TABLE: ComputeError = {
  kind: "StoppingError",
  variant: "NoSourceTable",
  source: "catima_Cl-35",
  projectile: "Cl-35",
  available: ["catima_C-12", "catima_O-16"],
  available_pretty: "C-12, O-16",
  message: "No catima_Cl-35 stopping table — projectile Cl-35 not in bundled set",
};

const UNKNOWN_WASM_PANIC: ComputeError = {
  kind: "Unknown",
  message:
    "recursive use of an object detected which would lead to unsafe aliasing in rust",
};

describe("ComputeErrorCard", () => {
  it("renders a variant-specific headline for each StoppingError variant", () => {
    const { getByText, unmount } = render(ComputeErrorCard, {
      props: { error: ENERGY_OUT_OF_RANGE },
    });
    expect(getByText("Energy outside tabulated range")).toBeTruthy();
    unmount();

    const r2 = render(ComputeErrorCard, { props: { error: NO_SOURCE_TABLE } });
    expect(r2.getByText("No stopping data for this projectile")).toBeTruthy();
  });

  it("shows the layer label when StoppingError carries layer context (#213)", () => {
    const { getByText } = render(ComputeErrorCard, {
      props: { error: ENERGY_OUT_OF_RANGE },
    });
    expect(getByText("Layer")).toBeTruthy();
    expect(getByText("L2 (H2O-18)")).toBeTruthy();
  });

  it("omits the Layer row when no layer context is attached", () => {
    const { queryByText } = render(ComputeErrorCard, {
      props: { error: NO_SOURCE_TABLE },
    });
    expect(queryByText("Layer")).toBeNull();
  });

  it("does NOT show fake 'Target —' placeholders for Unknown errors (#213)", () => {
    const { queryByText, getByText, container } = render(ComputeErrorCard, {
      props: { error: UNKNOWN_WASM_PANIC, projectile: "p", energyMev: 18 },
    });
    expect(getByText("Compute backend error")).toBeTruthy();
    // No empty Target / Source placeholders — only fields we actually know.
    expect(queryByText("Target")).toBeNull();
    expect(queryByText("Source")).toBeNull();
    // The actual error message must be front-and-centre.
    expect(container.textContent).toContain("recursive use of an object");
  });

  it("shows projectile + energy from props on Unknown errors when known", () => {
    const { getByText } = render(ComputeErrorCard, {
      props: { error: UNKNOWN_WASM_PANIC, projectile: "p", energyMev: 18 },
    });
    expect(getByText("Projectile")).toBeTruthy();
    expect(getByText("p")).toBeTruthy();
    expect(getByText("Energy")).toBeTruthy();
    expect(getByText("18.000 MeV")).toBeTruthy();
  });

  it("swaps the consequence line for non-stopping errors", () => {
    const stop = render(ComputeErrorCard, { props: { error: NO_SOURCE_TABLE } });
    expect(stop.container.textContent).toContain("depth profile");
    stop.unmount();

    const unknown = render(ComputeErrorCard, { props: { error: UNKNOWN_WASM_PANIC } });
    expect(unknown.container.textContent).not.toContain("depth profile");
    expect(unknown.container.textContent).toContain("WASM module state may be poisoned");
  });
});

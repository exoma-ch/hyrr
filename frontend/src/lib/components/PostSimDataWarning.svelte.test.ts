/**
 * Runtime-render coverage for `PostSimDataWarning` (#689 PR #715 review).
 *
 * The bug this test pins: a `DataFetchError` thrown from
 * `dataStore.ensureEmissionsByZ` used to reach `sim-scheduler`'s outer
 * catch and get run through `parseComputeError`, which only knows
 * `kind:"StoppingError"` — so the wire payload was classified as
 * `kind:"Unknown"` and `ComputeErrorCard` rendered raw JSON (the
 * FetchErrorPayload JSON we stashed in `.message`) instead of the actual
 * message. Worse, it threw the whole (correct) simulation result away
 * for what is actually a dose-only degradation.
 *
 * The fix: emissions failures degrade — `PostSimDataWarning` renders the
 * message text via `fetchErrorTitle` + `.message`, keeps the result
 * visible, and offers Retry.
 */
import { describe, it, expect, vi, afterEach } from "vitest";
import { render, fireEvent, cleanup } from "@testing-library/svelte";
import PostSimDataWarning from "./PostSimDataWarning.svelte";
import { parseFetchError } from "../utils/parse-fetch-error";
import { DataFetchError } from "@hyrr/compute";

afterEach(cleanup);

describe("PostSimDataWarning — never renders raw JSON (#689)", () => {
  it("renders the parsed message text for an emissions HTTP 500", () => {
    // The exact runtime shape sim-scheduler now hands us: a DataFetchError
    // from ensureEmissionsByZ, routed through parseFetchError (not
    // parseComputeError), landing here.
    const err = DataFetchError.http({
      url: "https://hyrr.example.com/data/parquet/meta/ensdf/emissions/Cu.parquet",
      status: 500,
      source: "meta/ensdf/emissions/Cu",
      humanMessage:
        "Failed to load meta/ensdf/emissions/Cu (HTTP 500 from https://hyrr.example.com/data/parquet/meta/ensdf/emissions/Cu.parquet)",
    });
    const parsed = parseFetchError(err);

    const { container } = render(PostSimDataWarning, {
      props: { warning: parsed, onretry: vi.fn(), ondismiss: vi.fn() },
    });

    // The rendered message text must NOT contain the raw JSON envelope —
    // the very failure mode the reviewer caught. `kind:"FetchError"` in
    // the DOM would mean parseFetchError was bypassed or PostSimDataWarning
    // is stringifying the wrong field.
    const text = container.textContent ?? "";
    expect(text).not.toContain('"kind":"FetchError"');
    expect(text).not.toContain('"variant":"HttpStatus"');

    // Positive: the human-readable message from the payload is present.
    // "HTTP 500" appears in both the title (from `fetchErrorTitle`) and
    // the payload message body — assert on the container's textContent
    // rather than getByText, which throws on multiple matches.
    expect(text).toContain("HTTP 500");
    expect(text).toContain("Cu.parquet");
  });

  it("also handles the UnexpectedContent (auth-gate) shape from the ETH deploy", () => {
    // The ETH WAYF gate returns 200 with text/html at a parquet URL —
    // #684's real signature, now typed. FetchErrorCard's title is
    // variant-aware; PostSimDataWarning re-uses fetchErrorTitle so the
    // same wording lands here too.
    const err = DataFetchError.unexpectedContent({
      url: "https://hyrr.ethz.ch/data/parquet/meta/ensdf/emissions/Cu.parquet",
      source: "meta/ensdf/emissions/Cu",
      contentType: "text/html; charset=utf-8",
      humanMessage: "Expected parquet, got HTML at hyrr.ethz.ch (auth gate).",
    });
    const parsed = parseFetchError(err);

    const { container } = render(PostSimDataWarning, {
      props: { warning: parsed, onretry: vi.fn(), ondismiss: vi.fn() },
    });

    const text = container.textContent ?? "";
    expect(text).toContain("wrong kind"); // fetchErrorTitle for UnexpectedContent
    expect(text).toContain("auth gate");
    expect(text).not.toContain('"kind":"FetchError"');
  });

  it("scope note reassures the user that yields/activities are unaffected", () => {
    // The reviewer's core point: emissions-only failures MUST NOT read as
    // "your simulation is wrong". The banner must say the scope of the
    // damage out loud.
    const err = DataFetchError.http({
      url: "x",
      status: 502,
      source: "meta/ensdf/emissions/Cu",
      humanMessage: "x",
    });
    const { getByText } = render(PostSimDataWarning, {
      props: {
        warning: parseFetchError(err),
        onretry: vi.fn(),
        ondismiss: vi.fn(),
      },
    });
    expect(getByText(/Activities.*unaffected/i)).toBeTruthy();
    expect(getByText(/Dose.*missing|Dose.*incomplete/i)).toBeTruthy();
  });

  it("Retry and Dismiss wire through to the passed callbacks", () => {
    const onretry = vi.fn();
    const ondismiss = vi.fn();
    const err = DataFetchError.http({
      url: "x",
      status: 500,
      source: "meta/ensdf/emissions/Cu",
      humanMessage: "x",
    });
    const { getByText } = render(PostSimDataWarning, {
      props: { warning: parseFetchError(err), onretry, ondismiss },
    });
    fireEvent.click(getByText("Retry"));
    fireEvent.click(getByText("Dismiss"));
    expect(onretry).toHaveBeenCalledTimes(1);
    expect(ondismiss).toHaveBeenCalledTimes(1);
  });
});

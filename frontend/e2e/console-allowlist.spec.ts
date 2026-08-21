/**
 * The allow-list must not rot (#656, epic #649).
 *
 * A suppression list nobody re-reviews is how a strict console check decays
 * into a no-op: entries get added "temporarily", stop matching anything, and
 * widen until the check means nothing. Every entry therefore carries an expiry,
 * and this fails once one is past it — forcing a decision rather than silent
 * carry-forward.
 *
 * Deliberately browser-free, so it is instant and always runs.
 */
import { test, expect } from "@playwright/test";
import { CONSOLE_ALLOWLIST } from "./fixtures";

test.describe("console allow-list hygiene", { tag: "@smoke" }, () => {
  test("no entry is past its expiry", () => {
    const today = new Date().toISOString().slice(0, 10);
    const expired = CONSOLE_ALLOWLIST.filter((e) => e.expires < today);
    expect(
      expired.map((e) => `${e.pattern} (ticket ${e.ticket}, expired ${e.expires})`),
      "these console suppressions are past their expiry — re-review them, " +
        "then either fix the underlying warning or extend the entry deliberately",
    ).toEqual([]);
  });

  test("every entry is fully documented", () => {
    for (const e of CONSOLE_ALLOWLIST) {
      expect(e.ticket, `allow-list entry ${e.pattern} has no ticket`).toMatch(/#\d+|[A-Z]+-\d+/);
      expect(e.reason.length, `allow-list entry ${e.pattern} has no reason`).toBeGreaterThan(20);
      expect(e.expires, `allow-list entry ${e.pattern} has a malformed expiry`).toMatch(
        /^\d{4}-\d{2}-\d{2}$/,
      );
    }
  });
});

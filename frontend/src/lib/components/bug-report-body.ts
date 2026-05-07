/**
 * Pure helper extracted from BugReportModal.svelte so the body-assembly
 * logic is unit-testable without spinning up a Svelte component.
 *
 * #143 added the compute-error section: bug reports were silently embedding
 * the previous successful run's stats when the latest compute had failed
 * (root cause of #137).
 */

import type { SimulationConfig, SimulationResult } from "@hyrr/compute";

export interface BugReportBodyInput {
  reportType: "bug" | "feature";
  /** Optional one-line summary from the form's Title input (#144).
   *  When absent, the helper falls back to a description slice. */
  title?: string;
  name: string;
  email: string;
  description: string;
  screenshotUrl?: string;
  config: SimulationConfig;
  configUrl: string;
  result: SimulationResult | null;
  computeError: unknown | null;
  appVersion: string;
  userAgent: string;
  /** Defaults to `new Date()` for testability. */
  now?: Date;
}

export function buildBugReportBody(input: BugReportBodyInput): string {
  const sections: string[] = [];

  sections.push(input.reportType === "bug" ? `## Bug Report` : `## Feature Request`);
  const titleLine = (input.title ?? "").trim() || input.description.slice(0, 70);
  if (titleLine) sections.push(`## ${titleLine}`);
  sections.push(
    `**Reporter:** ${input.name || "Anonymous"}${input.email ? ` (${input.email})` : ""}`,
  );
  sections.push(`## Description\n\n${input.description}`);

  if (input.screenshotUrl) {
    sections.push(`## Screenshot\n\n![screenshot](${input.screenshotUrl})`);
  }

  sections.push(`## Debug Context`);
  sections.push(`**[Reproduce this config](${input.configUrl})**`);
  sections.push(
    `**Beam:** ${input.config.beam?.projectile ?? "?"} @ ${input.config.beam?.energy_MeV ?? "?"} MeV, ${input.config.beam?.current_mA ?? "?"} mA`,
  );
  const layerNames = (input.config.layers ?? []).map((l: any) => l.material).join(" → ");
  sections.push(`**Stack:** ${layerNames || "empty"} (${input.config.layers?.length ?? 0} layers)`);

  if (input.result) {
    const nIso = input.result.layers.reduce(
      (n: number, l: any) => n + (l.isotopes?.length ?? 0),
      0,
    );
    sections.push(`**Result:** ${nIso} isotopes produced`);
  } else {
    sections.push(`**Result:** No simulation result available`);
  }

  if (input.computeError != null) {
    sections.push(`**Compute error:** ${String(input.computeError)}`);
  }

  sections.push(`**Version:** ${input.appVersion}`);
  sections.push(`**Browser:** ${input.userAgent}`);
  sections.push(`**Timestamp:** ${(input.now ?? new Date()).toISOString()}`);

  return sections.join("\n\n");
}

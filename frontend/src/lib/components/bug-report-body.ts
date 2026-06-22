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
  /** Opt-in diagnostic trace (#159) — the literal merged JSON the user previewed.
   *  Omitted when the attach checkbox is off. */
  trace?: string;
}

/**
 * Stringify a raw compute error for the bug-report body. The WASM backend
 * historically returned `serde_wasm_bindgen` Map values (pre-#211) and Tauri
 * returns JSON strings — both should render as legible JSON, never the bare
 * `[object Map]` / `[object Object]` placeholders that #211 was filed about.
 */
function formatComputeError(err: unknown): string {
  if (err == null) return "";
  if (typeof err === "string") return err;
  if (err instanceof Error) return String(err);
  if (err instanceof Map) {
    return JSON.stringify(Object.fromEntries(err as Map<unknown, unknown>));
  }
  if (typeof err === "object") {
    const message = (err as { message?: unknown }).message;
    if (typeof message === "string" && message) return message;
    try {
      return JSON.stringify(err);
    } catch {
      return Object.prototype.toString.call(err);
    }
  }
  return String(err);
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
    sections.push(`**Compute error:** ${formatComputeError(input.computeError)}`);
  }

  sections.push(`**Version:** ${input.appVersion}`);
  sections.push(`**Browser:** ${input.userAgent}`);
  sections.push(`**Timestamp:** ${(input.now ?? new Date()).toISOString()}`);

  // Diagnostic trace last, fenced as ```json so GitHub renders it literally and
  // the human-readable context stays on top (#159). Only present when attached.
  if (input.trace && input.trace.trim()) {
    sections.push(`## Diagnostic trace\n\n\`\`\`json\n${input.trace}\n\`\`\``);
  }

  return sections.join("\n\n");
}

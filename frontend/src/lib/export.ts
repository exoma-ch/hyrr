/**
 * Client-side export utilities.
 *
 * Generate and download JSON/CSV files from simulation results
 * without any server interaction.
 */

import type { SimulationConfig, SimulationResult } from "./types";

/** Trigger a file download in the browser. */
function downloadFile(filename: string, content: string, mimeType: string): void {
  const blob = new Blob([content], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}

/** Export simulation config as JSON. */
export function exportConfigJson(config: SimulationConfig): void {
  const json = JSON.stringify(config, null, 2);
  downloadFile("hyrr-config.json", json, "application/json");
}

/** Export full result (config + results) as JSON. */
export function exportResultJson(result: SimulationResult): void {
  const json = JSON.stringify(result, null, 2);
  const timestamp = new Date(result.timestamp).toISOString().slice(0, 10);
  downloadFile(`hyrr-result-${timestamp}.json`, json, "application/json");
}

/** Export layer results as CSV. */
export function exportResultCsv(result: SimulationResult): void {
  const header = [
    "layer",
    "isotope",
    "Z",
    "A",
    "state",
    "half_life_s",
    "production_rate",
    "saturation_yield_Bq_uA",
    "activity_Bq",
  ].join(",");

  const rows: string[] = [header];
  for (const layer of result.layers) {
    for (const iso of layer.isotopes) {
      rows.push(
        [
          layer.layer_index,
          iso.name,
          iso.Z,
          iso.A,
          iso.state,
          iso.half_life_s ?? "",
          iso.production_rate,
          iso.saturation_yield_Bq_uA,
          iso.activity_Bq,
        ].join(","),
      );
    }
  }

  const timestamp = new Date(result.timestamp).toISOString().slice(0, 10);
  downloadFile(`hyrr-result-${timestamp}.csv`, rows.join("\n"), "text/csv");
}

/** Import a config from a JSON file (via file input). */
export function importConfigFromFile(file: File): Promise<SimulationConfig> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      try {
        const config = JSON.parse(reader.result as string) as SimulationConfig;
        resolve(config);
      } catch (e) {
        reject(new Error(`Failed to parse config file: ${e}`));
      }
    };
    reader.onerror = () => reject(reader.error);
    reader.readAsText(file);
  });
}

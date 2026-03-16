/**
 * MCP tool: simulate — run an irradiation simulation.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import {
  type NodeDataStore,
  type SimulationConfig,
  computeStack,
  buildTargetStack,
  convertResult,
  getRequiredElements,
  setCustomDensityLookup,
  setCustomCompositionLookup,
  fmtActivity,
  fmtYield,
  nucLabel,
} from "@hyrr/compute";

const LayerSchema = z.object({
  material: z.string().describe("Material identifier (element symbol, formula like MoO3, or alloy name like Havar)"),
  thickness_cm: z.number().optional().describe("Layer thickness in cm"),
  areal_density_g_cm2: z.number().optional().describe("Areal density in g/cm²"),
  energy_out_MeV: z.number().optional().describe("Exit energy in MeV"),
  is_monitor: z.boolean().optional().describe("Whether this is a monitor foil"),
  enrichment: z.record(z.string(), z.record(z.string(), z.number())).optional()
    .describe("Isotopic enrichment overrides: { element: { mass_number: fraction } }"),
});

const CustomMaterialSchema = z.object({
  name: z.string().describe("Material name/identifier"),
  density: z.number().describe("Density in g/cm³"),
  massFractions: z.record(z.string(), z.number()).describe("Element mass fractions (must sum to ~1.0)"),
});

const SimulateInputSchema = z.object({
  projectile: z.enum(["p", "d", "t", "h", "a"]).describe("Beam projectile: p=proton, d=deuteron, t=triton, h=helion-3, a=alpha"),
  energy_MeV: z.number().positive().describe("Beam energy in MeV"),
  current_mA: z.number().positive().describe("Beam current in mA"),
  layers: z.array(LayerSchema).min(1).describe("Target layer stack (beam enters first layer)"),
  irradiation_s: z.number().positive().describe("Irradiation time in seconds"),
  cooling_s: z.number().nonnegative().describe("Cooling time in seconds"),
  library: z.string().optional().describe("Nuclear data library (default: tendl-2024)"),
  custom_materials: z.array(CustomMaterialSchema).optional()
    .describe("Custom material definitions for materials not in the built-in catalog"),
});

function formatMarkdownTable(result: ReturnType<typeof convertResult>): string {
  const lines: string[] = [];
  lines.push("## Simulation Results\n");

  for (const layer of result.layers) {
    lines.push(`### Layer ${layer.layer_index + 1}`);
    lines.push(`- Energy: ${layer.energy_in.toFixed(2)} → ${layer.energy_out.toFixed(2)} MeV (ΔE = ${layer.delta_E_MeV.toFixed(2)} MeV)`);
    lines.push(`- Heat deposition: ${(layer.heat_kW * 1000).toFixed(1)} W\n`);

    if (layer.isotopes.length > 0) {
      lines.push("| Isotope | Activity | Sat. Yield | Source | Reactions |");
      lines.push("|---------|----------|------------|--------|-----------|");

      // Show top 20 isotopes
      const shown = layer.isotopes.slice(0, 20);
      for (const iso of shown) {
        const name = nucLabel(iso.name);
        const activity = fmtActivity(iso.activity_Bq);
        const satYield = fmtYield(iso.saturation_yield_Bq_uA);
        const source = iso.source ?? "direct";
        const reactions = iso.reactions?.join(", ") ?? "";
        lines.push(`| ${name} | ${activity} | ${satYield} | ${source} | ${reactions} |`);
      }

      if (layer.isotopes.length > 20) {
        lines.push(`\n*... and ${layer.isotopes.length - 20} more isotopes*`);
      }
    } else {
      lines.push("*No significant isotope production*");
    }
    lines.push("");
  }

  return lines.join("\n");
}

export function registerSimulateTool(
  server: McpServer,
  getDataStore: (library?: string) => Promise<NodeDataStore>,
): void {
  server.tool(
    "simulate",
    "Run an irradiation simulation to predict radioisotope production in a target stack. " +
    "Returns isotope activities, yields, depth profiles, and energy loss for each layer.",
    SimulateInputSchema.shape,
    async (input) => {
      try {
        const params = SimulateInputSchema.parse(input);

        // Register custom materials if provided
        if (params.custom_materials && params.custom_materials.length > 0) {
          const customMap = new Map<string, { density: number; massFractions: Record<string, number> }>();
          for (const mat of params.custom_materials) {
            customMap.set(mat.name, { density: mat.density, massFractions: mat.massFractions });
          }
          setCustomDensityLookup((name) => customMap.get(name)?.density ?? null);
          setCustomCompositionLookup((name) => customMap.get(name)?.massFractions ?? null);
        }

        const db = await getDataStore(params.library);

        const config: SimulationConfig = {
          beam: {
            projectile: params.projectile,
            energy_MeV: params.energy_MeV,
            current_mA: params.current_mA,
          },
          layers: params.layers,
          irradiation_s: params.irradiation_s,
          cooling_s: params.cooling_s,
        };

        // Ensure cross-sections are loaded
        const elements = getRequiredElements(config);
        await db.ensureMultipleCrossSections(config.beam.projectile, elements);

        // Run simulation
        const stack = buildTargetStack(config, db);
        const stackResult = computeStack(db, stack);
        const result = convertResult(config, stackResult);

        // Clean up custom materials
        if (params.custom_materials) {
          setCustomDensityLookup(() => null);
          setCustomCompositionLookup(() => null);
        }

        const markdown = formatMarkdownTable(result);

        return {
          content: [
            { type: "text" as const, text: markdown },
            { type: "text" as const, text: "\n---\n### Raw JSON\n```json\n" + JSON.stringify(result, null, 2) + "\n```" },
          ],
        };
      } catch (error) {
        const msg = error instanceof Error ? error.message : String(error);
        return {
          content: [{ type: "text" as const, text: `Simulation error: ${msg}` }],
          isError: true,
        };
      }
    },
  );
}

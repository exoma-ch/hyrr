/**
 * MCP lookup tools: materials, cross-sections, decay data, compare.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import {
  type NodeDataStore,
  MATERIAL_CATALOG,
  ELEMENT_DENSITIES,
  COMPOUND_DENSITIES,
  SYMBOL_TO_Z,
  nucLabel,
  fmtActivity,
  fmtYield,
  computeStack,
  buildTargetStack,
  convertResult,
  getRequiredElements,
  type SimulationConfig,
} from "@hyrr/compute";

export function registerLookupTools(
  server: McpServer,
  getDataStore: (library?: string) => Promise<NodeDataStore>,
): void {
  // --- list_materials ---
  server.tool(
    "list_materials",
    "List available materials: catalog alloys, element densities, and compound densities.",
    {},
    async () => {
      const catalog = Object.entries(MATERIAL_CATALOG).map(([name, entry]) => ({
        name,
        density: entry.density,
        composition: entry.massFractions,
      }));

      const elements = Object.entries(ELEMENT_DENSITIES).map(([sym, d]) => ({
        symbol: sym,
        Z: SYMBOL_TO_Z[sym],
        density: d,
      }));

      const compounds = Object.entries(COMPOUND_DENSITIES).map(([formula, d]) => ({
        formula,
        density: d,
      }));

      return {
        content: [{
          type: "text" as const,
          text: JSON.stringify({ catalog, elements, compounds }, null, 2),
        }],
      };
    },
  );

  // --- get_cross_sections ---
  server.tool(
    "get_cross_sections",
    "Get available cross-section channels for a target isotope. " +
    "Returns all residual nuclei (reaction products) with their energy-dependent cross-sections.",
    {
      projectile: z.enum(["p", "d", "t", "h", "a"]).describe("Beam projectile"),
      target_element: z.string().describe("Target element symbol (e.g. Mo, Cu)"),
      target_A: z.number().int().positive().describe("Target mass number"),
      library: z.string().optional().describe("Nuclear data library (default: tendl-2024)"),
    },
    async (input) => {
      try {
        const db = await getDataStore(input.library);
        const Z = SYMBOL_TO_Z[input.target_element];
        if (Z === undefined) {
          return { content: [{ type: "text" as const, text: `Unknown element: ${input.target_element}` }], isError: true };
        }

        await db.ensureMultipleCrossSections(input.projectile, [input.target_element]);
        const xsList = db.getCrossSections(input.projectile, Z, input.target_A);

        const channels = xsList.map((xs) => ({
          residual: `${db.getElementSymbol(xs.residualZ)}-${xs.residualA}${xs.state || ""}`,
          residualZ: xs.residualZ,
          residualA: xs.residualA,
          state: xs.state,
          energyRange: [xs.energiesMeV[0], xs.energiesMeV[xs.energiesMeV.length - 1]],
          peakXs_mb: Math.max(...Array.from(xs.xsMb)),
          nPoints: xs.energiesMeV.length,
        }));

        return {
          content: [{
            type: "text" as const,
            text: JSON.stringify({
              target: `${input.target_element}-${input.target_A}`,
              projectile: input.projectile,
              channels,
            }, null, 2),
          }],
        };
      } catch (error) {
        return { content: [{ type: "text" as const, text: `Error: ${error instanceof Error ? error.message : error}` }], isError: true };
      }
    },
  );

  // --- get_decay_data ---
  server.tool(
    "get_decay_data",
    "Get decay data for an isotope: half-life, decay modes, and daughter nuclei.",
    {
      element: z.string().describe("Element symbol"),
      A: z.number().int().positive().describe("Mass number"),
      state: z.string().optional().describe("Nuclear state (e.g. 'm' for metastable)"),
      library: z.string().optional(),
    },
    async (input) => {
      try {
        const db = await getDataStore(input.library);
        const Z = SYMBOL_TO_Z[input.element];
        if (Z === undefined) {
          return { content: [{ type: "text" as const, text: `Unknown element: ${input.element}` }], isError: true };
        }

        const decay = db.getDecayData(Z, input.A, input.state ?? "");
        if (!decay) {
          return { content: [{ type: "text" as const, text: `No decay data for ${input.element}-${input.A}${input.state ?? ""} (stable or unknown)` }] };
        }

        const modes = decay.decayModes.map((m) => ({
          mode: m.mode,
          branching: m.branching,
          daughter: m.daughterZ !== null
            ? `${db.getElementSymbol(m.daughterZ!)}-${m.daughterA}${m.daughterState || ""}`
            : null,
        }));

        const result = {
          isotope: `${input.element}-${input.A}${input.state ?? ""}`,
          halfLifeS: decay.halfLifeS,
          halfLifeHuman: decay.halfLifeS !== null ? formatHalfLife(decay.halfLifeS) : "stable",
          decayModes: modes,
        };

        return { content: [{ type: "text" as const, text: JSON.stringify(result, null, 2) }] };
      } catch (error) {
        return { content: [{ type: "text" as const, text: `Error: ${error instanceof Error ? error.message : error}` }], isError: true };
      }
    },
  );

  // --- compare_results ---
  server.tool(
    "compare_results",
    "Run two simulations and compare isotope production. Useful for comparing different beam energies, targets, or irradiation times.",
    {
      config_a: z.object({
        projectile: z.enum(["p", "d", "t", "h", "a"]),
        energy_MeV: z.number().positive(),
        current_mA: z.number().positive(),
        layers: z.array(z.object({
          material: z.string(),
          thickness_cm: z.number().optional(),
          areal_density_g_cm2: z.number().optional(),
          energy_out_MeV: z.number().optional(),
          enrichment: z.record(z.string(), z.record(z.string(), z.number())).optional(),
        })).min(1),
        irradiation_s: z.number().positive(),
        cooling_s: z.number().nonnegative(),
        label: z.string().optional(),
      }).describe("First simulation configuration"),
      config_b: z.object({
        projectile: z.enum(["p", "d", "t", "h", "a"]),
        energy_MeV: z.number().positive(),
        current_mA: z.number().positive(),
        layers: z.array(z.object({
          material: z.string(),
          thickness_cm: z.number().optional(),
          areal_density_g_cm2: z.number().optional(),
          energy_out_MeV: z.number().optional(),
          enrichment: z.record(z.string(), z.record(z.string(), z.number())).optional(),
        })).min(1),
        irradiation_s: z.number().positive(),
        cooling_s: z.number().nonnegative(),
        label: z.string().optional(),
      }).describe("Second simulation configuration"),
      library: z.string().optional(),
    },
    async (input) => {
      try {
        const db = await getDataStore(input.library);

        async function runSim(cfg: typeof input.config_a) {
          const config: SimulationConfig = {
            beam: { projectile: cfg.projectile, energy_MeV: cfg.energy_MeV, current_mA: cfg.current_mA },
            layers: cfg.layers,
            irradiation_s: cfg.irradiation_s,
            cooling_s: cfg.cooling_s,
          };
          const elements = getRequiredElements(config);
          await db.ensureMultipleCrossSections(config.beam.projectile, elements);
          const stack = buildTargetStack(config, db);
          const stackResult = computeStack(db, stack);
          return convertResult(config, stackResult);
        }

        const [resultA, resultB] = await Promise.all([
          runSim(input.config_a),
          runSim(input.config_b),
        ]);

        const labelA = input.config_a.label ?? "Config A";
        const labelB = input.config_b.label ?? "Config B";

        // Build comparison table for first layer
        const isoA = new Map(resultA.layers[0]?.isotopes.map((i) => [i.name, i]) ?? []);
        const isoB = new Map(resultB.layers[0]?.isotopes.map((i) => [i.name, i]) ?? []);
        const allNames = new Set([...isoA.keys(), ...isoB.keys()]);

        const lines: string[] = [];
        lines.push(`## Comparison: ${labelA} vs ${labelB}\n`);
        lines.push("| Isotope | Activity (A) | Activity (B) | Ratio B/A |");
        lines.push("|---------|-------------|-------------|-----------|");

        const sorted = [...allNames].sort((a, b) => {
          const actA = isoA.get(a)?.activity_Bq ?? 0;
          const actB = isoB.get(b)?.activity_Bq ?? 0;
          return Math.max(actB, isoB.get(a)?.activity_Bq ?? 0) - Math.max(actA, isoA.get(b)?.activity_Bq ?? 0);
        });

        for (const name of sorted.slice(0, 30)) {
          const a = isoA.get(name);
          const b = isoB.get(name);
          const actA = a?.activity_Bq ?? 0;
          const actB = b?.activity_Bq ?? 0;
          const ratio = actA > 0 ? (actB / actA).toFixed(2) : actB > 0 ? "∞" : "—";
          lines.push(`| ${nucLabel(name)} | ${fmtActivity(actA)} | ${fmtActivity(actB)} | ${ratio} |`);
        }

        return { content: [{ type: "text" as const, text: lines.join("\n") }] };
      } catch (error) {
        return { content: [{ type: "text" as const, text: `Error: ${error instanceof Error ? error.message : error}` }], isError: true };
      }
    },
  );
}

function formatHalfLife(s: number): string {
  if (s <= 0) return "stable";
  if (s < 60) return `${s.toPrecision(3)} s`;
  if (s < 3600) return `${(s / 60).toPrecision(3)} min`;
  if (s < 86400) return `${(s / 3600).toPrecision(3)} h`;
  if (s < 86400 * 365.25) return `${(s / 86400).toPrecision(3)} d`;
  return `${(s / (86400 * 365.25)).toPrecision(3)} y`;
}

#!/usr/bin/env node
/**
 * HYRR MCP Server — agent-driven irradiation analysis.
 *
 * Exposes HYRR's physics engine as MCP tools for Claude Code and other agents.
 * Uses @hyrr/compute for all physics calculations with NodeDataStore for
 * filesystem-based Parquet data access.
 */

import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { NodeDataStore, resolveDataDir } from "@hyrr/compute/node";
import { registerSimulateTool } from "./tools/simulate.js";
import { registerLookupTools } from "./tools/lookup.js";

const server = new McpServer({
  name: "hyrr",
  version: "0.1.0",
});

// Cache data stores per library
const dataStores = new Map<string, NodeDataStore>();

async function getDataStore(library?: string): Promise<NodeDataStore> {
  const lib = library ?? process.env.HYRR_LIBRARY ?? "tendl-2024";
  const existing = dataStores.get(lib);
  if (existing?.isInitialized) return existing;

  const dataDir = resolveDataDir(undefined, lib);
  const store = new NodeDataStore(dataDir);
  await store.init((msg) => process.stderr.write(`[hyrr] ${msg}\n`));
  dataStores.set(lib, store);
  return store;
}

// Register tools
registerSimulateTool(server, getDataStore);
registerLookupTools(server, getDataStore);

// Register resources
server.resource("libraries", "hyrr://libraries", async (uri) => {
  const { readdirSync, existsSync } = await import("node:fs");
  const { resolve } = await import("node:path");

  const candidates = [
    process.env.HYRR_DATA,
    resolve(process.cwd(), "../nucl-parquet"),
    resolve((await import("node:os")).homedir(), ".hyrr", "nucl-parquet"),
  ].filter((p): p is string => !!p && existsSync(p));

  const libraries: string[] = [];
  for (const dir of candidates) {
    try {
      const entries = readdirSync(dir, { withFileTypes: true });
      for (const entry of entries) {
        if (entry.isDirectory() && !entry.name.startsWith(".")) {
          libraries.push(entry.name);
        }
      }
    } catch { /* skip inaccessible dirs */ }
  }

  return {
    contents: [{
      uri: uri.href,
      mimeType: "application/json",
      text: JSON.stringify({ libraries: [...new Set(libraries)] }, null, 2),
    }],
  };
});

server.resource("elements", "hyrr://elements", async (uri) => {
  const { SYMBOL_TO_Z, ELEMENT_DENSITIES } = await import("@hyrr/compute");
  const elements = Object.entries(SYMBOL_TO_Z).map(([symbol, Z]) => ({
    symbol,
    Z,
    density: ELEMENT_DENSITIES[symbol] ?? null,
  }));

  return {
    contents: [{
      uri: uri.href,
      mimeType: "application/json",
      text: JSON.stringify({ elements }, null, 2),
    }],
  };
});

// Start server
const transport = new StdioServerTransport();
await server.connect(transport);

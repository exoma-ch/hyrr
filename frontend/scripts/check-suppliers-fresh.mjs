#!/usr/bin/env node
/**
 * Warn (don't fail) if the supplier catalog hasn't been reviewed in > 90 days.
 *
 * Per #66's review cadence: enriched-isotope supplier inventories drift
 * (URL changes, new sanctions regimes, new entrants), so we want a
 * visible nudge in CI without blocking merges.
 */
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const STALE_DAYS = 90;
const here = dirname(fileURLToPath(import.meta.url));
const catalogPath = resolve(here, "../src/lib/data/suppliers.json");
const catalog = JSON.parse(readFileSync(catalogPath, "utf8"));

const reviewed = new Date(catalog.last_reviewed + "T00:00:00Z");
if (Number.isNaN(reviewed.getTime())) {
  console.error(`::error::suppliers.json last_reviewed "${catalog.last_reviewed}" is not a valid date`);
  process.exit(1);
}

const ageDays = Math.floor((Date.now() - reviewed.getTime()) / (1000 * 60 * 60 * 24));

if (ageDays > STALE_DAYS) {
  // GitHub Actions warning syntax — surfaces in the PR check summary.
  console.log(
    `::warning file=frontend/src/lib/data/suppliers.json::Supplier catalog last_reviewed=${catalog.last_reviewed} is ${ageDays} days old (> ${STALE_DAYS}). Re-verify supplier URLs, isotope offerings, and sanctions flags, then bump last_reviewed.`,
  );
} else {
  console.log(`Supplier catalog reviewed ${ageDays} days ago — fresh (≤ ${STALE_DAYS}).`);
}

process.exit(0);

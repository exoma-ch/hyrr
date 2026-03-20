/**
 * Cross-engine benchmark: TS compute runner.
 * Usage: node --import tsx tests/integration/ts_runner.mts '<config_json>' <data_dir>
 */

import { NodeDataStore, resolveDataDir } from "@hyrr/compute/node";
import {
  computeStack,
  buildTargetStack,
  convertResult,
  getRequiredElements,
} from "@hyrr/compute";

const config = JSON.parse(process.argv[2]);
const rawDataDir = process.argv[3];

async function main() {
  // resolveDataDir appends the library subdir (e.g., tendl-2024)
  const dataDir = resolveDataDir(rawDataDir);
  const db = new NodeDataStore(dataDir);
  await db.init();

  const elements = getRequiredElements(config);
  await db.ensureMultipleCrossSections(config.beam.projectile, elements);

  const stack = buildTargetStack(config, db);
  const stackResult = computeStack(db, stack);
  const simResult = convertResult(config, stackResult);

  process.stdout.write(JSON.stringify(simResult));
}

main().catch((e) => {
  process.stderr.write(e.message + "\n" + (e.stack || "") + "\n");
  process.exit(1);
});

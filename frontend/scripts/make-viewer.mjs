/**
 * Stamp a result snapshot into the built viewer template (ADR 0006 spike).
 *
 *   node scripts/make-viewer.mjs <result.json> <out.html> [emissions.json]
 *
 * Tier is implied by the arguments and recorded in the payload: with an
 * emissions file the artifact is Tier B, without it Tier A. Keeping that in the
 * file (rather than only in the UI) is what makes an artifact's contents
 * auditable after it has left.
 */
import { readFileSync, writeFileSync, statSync } from "node:fs";
import { gzipSync } from "node:zlib";
import { fileURLToPath } from "node:url";

const [, , resultPath, outPath, emissionsPath] = process.argv;
if (!resultPath || !outPath) {
  console.error("usage: make-viewer.mjs <result.json> <out.html> [emissions.json]");
  process.exit(2);
}

const TEMPLATE = fileURLToPath(new URL("../dist-viewer/viewer.html", import.meta.url));
const PLACEHOLDER = "__HYRR_SNAPSHOT__";

const template = readFileSync(TEMPLATE, "utf8");
if (!template.includes(PLACEHOLDER)) {
  console.error(`template has no ${PLACEHOLDER} — rebuild it with vite.viewer.config.ts`);
  process.exit(1);
}

const pkg = JSON.parse(readFileSync(fileURLToPath(new URL("../package.json", import.meta.url)), "utf8"));
const result = JSON.parse(readFileSync(resultPath, "utf8"));
const extra = emissionsPath ? JSON.parse(readFileSync(emissionsPath, "utf8")) : null;

const snapshot = {
  schema: "hyrr-viewer",
  schema_version: 1,
  tier: extra ? "B" : "A",
  hyrr_version: pkg.version,
  generated_at: new Date().toISOString(),
  result,
  ...(extra ? { emissions: extra.emissions, doseConstants: extra.doseConstants } : {}),
};

// Escaping inside <script type="application/json">: `</script>` would close the
// element early, and `<!--` opens a comment. Escaping every `<` covers both and
// stays valid JSON.
const json = JSON.stringify(snapshot).replace(/</g, "\\u003c");

// Function replacement, not a string: `$&` / `` $` `` / `$'` inside the payload
// would otherwise be treated as replacement patterns and corrupt the JSON.
writeFileSync(outPath, template.replace(PLACEHOLDER, () => json));

const bytes = statSync(outPath).size;
const gz = gzipSync(readFileSync(outPath)).length;
const kb = (n) => (n / 1024).toFixed(0).padStart(6) + " KB";
console.log(`tier          ${snapshot.tier}`);
console.log(`payload       ${kb(json.length)}  (${kb(gzipSync(Buffer.from(json)).length)} gzipped)`);
console.log(`artifact      ${kb(bytes)}  (${kb(gz)} gzipped)  → ${outPath}`);

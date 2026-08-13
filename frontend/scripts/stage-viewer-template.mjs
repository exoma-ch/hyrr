/**
 * Stage the built viewer template into `public/` (ADR 0008).
 *
 * Putting it there makes it an ordinary static asset: served in dev, copied
 * into `dist/` by the app build, and bundled into the desktop app — so the one
 * `fetch(BASE_URL + "viewer-template.html")` in `lib/viewer/export.ts` works on
 * web and Tauri alike, with no per-surface delivery path.
 *
 * It is generated, not committed (gitignored) — the app build regenerates it,
 * so a stale template can never ship.
 */
import { copyFileSync, existsSync, mkdirSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";

const src = fileURLToPath(new URL("../dist-viewer/viewer.html", import.meta.url));
const destDir = fileURLToPath(new URL("../public", import.meta.url));
const dest = `${destDir}/viewer-template.html`;

if (!existsSync(src)) {
  console.error(
    `stage-viewer-template: ${src} is missing.\n` +
      "Build it first: npx vite build --config vite.viewer.config.ts",
  );
  process.exit(1);
}

mkdirSync(destDir, { recursive: true });
copyFileSync(src, dest);
console.log(`stage-viewer-template: ${(statSync(dest).size / 1024).toFixed(0)} KB → ${dest}`);

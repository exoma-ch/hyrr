/**
 * Build config for the standalone results viewer (ADR 0008 spike).
 *
 * Produces ONE self-contained `viewer.html` — every script and stylesheet
 * inlined, no external request of any kind — which the generator then stamps
 * with a result snapshot.
 *
 * Three substitutions make it work without touching any shared component:
 *
 *   sim-scheduler  → viewer shim   (the only scheduler export the results path
 *                                   uses is `getDataStore`; the real module
 *                                   pulls in the compute backend, and with it
 *                                   the WASM engine the artifact must not have)
 *   depth-preview  → viewer shim   (derived from the snapshot; no backend call)
 *   plotly full    → plotly basic  (the result plots use only scatter + bar)
 *
 * The WASM alias is deliberately absent: nothing the viewer imports should
 * reach `hyrr-wasm`, so leaving it unaliased turns an accidental import into a
 * build error rather than a silently shipped 2.1 MB engine.
 */
import { defineConfig, type Plugin } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { fileURLToPath } from "node:url";
import pkg from "./package.json" with { type: "json" };
import hyrrConfig from "../hyrr.json" with { type: "json" };

const abs = (p: string) => fileURLToPath(new URL(p, import.meta.url));

/**
 * Inline every emitted asset into the HTML and drop the now-dangling tags.
 * A dedicated plugin rather than `vite-plugin-singlefile` — the project has no
 * such dependency today and the whole job is a few string operations.
 */
function singleFilePlugin(): Plugin {
  return {
    name: "hyrr-viewer-singlefile",
    enforce: "post",
    generateBundle(_options, bundle) {
      const htmlKey = Object.keys(bundle).find((k) => k.endsWith(".html"));
      if (!htmlKey) return;
      const html = bundle[htmlKey];
      if (html.type !== "asset" || typeof html.source !== "string") return;

      let out = html.source;
      const orphans: string[] = [];
      for (const [key, chunk] of Object.entries(bundle)) {
        if (key === htmlKey) continue;
        const before = out;

        // NOTE: the replacement is always a *function*. Passing a string would
        // let `$&`, `` $` `` and `$'` inside minified code be interpreted as
        // replacement patterns and silently corrupt the bundle.
        if (chunk.type === "chunk") {
          const code = chunk.code
            // Vite wraps `import()` in its preload helper and leaves a
            // `__VITE_PRELOAD__` marker for a later pass to fill with the
            // chunk's dependency list. That pass does not run for this build,
            // and the surviving identifier makes the helper's promise never
            // settle — plots then silently never render, with no error at all.
            // A single-file bundle has no dependencies to preload, so `void 0`
            // is exactly what vite itself substitutes in that case.
            .replace(/__VITE_PRELOAD__/g, "void 0")
            // `</script>` inside a JS string literal would close the tag early.
            .replace(/<\/script>/gi, "<\\/script>");
          out = out.replace(
            new RegExp(`<script[^>]*src="[^"]*${escapeRe(chunk.fileName)}"[^>]*></script>`),
            () => `<script type="module">\n${code}\n</script>`,
          );
        } else if (chunk.fileName.endsWith(".css")) {
          const css = String(chunk.source);
          out = out.replace(
            new RegExp(`<link[^>]*href="[^"]*${escapeRe(chunk.fileName)}"[^>]*>`),
            () => `<style>\n${css}\n</style>`,
          );
        }
        // A chunk we did not manage to inline is not a chunk we may drop: the
        // async plotly chunk is referenced by `import()`, not by a <script src>,
        // so deleting it silently produces an artifact whose plots never
        // render — with no error anywhere. Fail loudly instead.
        if (out === before) orphans.push(chunk.fileName);
        delete bundle[key];
      }
      if (orphans.length) {
        this.error(
          `viewer: ${orphans.length} emitted asset(s) could not be inlined and would be lost: ` +
            `${orphans.join(", ")}. Ensure the build emits a single chunk ` +
            `(build.rollupOptions.output.codeSplitting = false).`,
        );
      }
      html.source = out;
    },
  };
}

const escapeRe = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

export default defineConfig({
  plugins: [svelte(), singleFilePlugin()],
  // Relative: the artifact is opened from disk (file://), not served.
  base: "./",
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
    __DEFAULT_LIBRARY__: JSON.stringify(hyrrConfig.default_library),
  },
  resolve: {
    alias: [
      { find: /^\.\.\/scheduler\/sim-scheduler\.svelte$/, replacement: abs("./src/lib/viewer/scheduler-shim.ts") },
      { find: /^\.\.\/stores\/depth-preview\.svelte$/, replacement: abs("./src/lib/viewer/depth-preview-shim.ts") },
      { find: /^plotly\.js-dist-min$/, replacement: abs("./src/lib/viewer/plotly-shim.ts") },
    ],
  },
  build: {
    outDir: "dist-viewer",
    target: "esnext",
    emptyOutDir: true,
    // Without this, vite wraps each `import()` in its preload helper and leaves
    // a `__VITE_PRELOAD__` marker for a later pass to substitute. In a
    // single-file build that pass has nothing to do, the marker survives into
    // the artifact, and the helper's promise never settles — so plots silently
    // never render, with no error. There is nothing to preload here anyway:
    // every module is already inline.
    modulePreload: false,
    // One file in, one file out — no code-splitting, no asset URLs.
    assetsInlineLimit: Number.MAX_SAFE_INTEGER,
    cssCodeSplit: false,
    rollupOptions: {
      input: abs("./viewer.html"),
      // Everything in one chunk. Plotly is reached through `await import(...)`,
      // so with code-splitting on it lands in a separate chunk that no <script
      // src> references — which the inliner cannot fold in.
      output: { codeSplitting: false },
    },
  },
});

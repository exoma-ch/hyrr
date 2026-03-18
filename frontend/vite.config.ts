import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import pkg from "./package.json" with { type: "json" };

export default defineConfig({
  plugins: [svelte()],
  base: process.env.TAURI_ENV_PLATFORM ? "./" : "/hyrr/",
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
  },
  server: {
    fs: {
      allow: [".", "../../nucl-parquet"],
    },
    watch: {
      ignored: ["**/public/data/**"],
    },
  },
  build: {
    outDir: "dist",
    target: "esnext",
  },
});

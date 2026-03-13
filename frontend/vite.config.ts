import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import pkg from "./package.json" with { type: "json" };

export default defineConfig({
  plugins: [svelte()],
  base: "/hyrr/",
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
  },
  build: {
    outDir: "dist",
    target: "esnext",
  },
});

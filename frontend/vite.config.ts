import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  plugins: [svelte()],
  base: "/hyrr/",
  build: {
    outDir: "dist",
    target: "esnext",
  },
});

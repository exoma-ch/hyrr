import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: [
      "packages/**/*.test.{ts,js}",
      "frontend/**/*.test.{ts,js}",
    ],
    // Exclude submodule tests — nucl-parquet has its own test infra
    exclude: ["nucl-parquet/**", "node_modules/**"],
  },
});

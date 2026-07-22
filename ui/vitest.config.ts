import { defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Runes classes (`*.svelte.ts`) are compiled by the svelte plugin, so the
// controller's `$state`/`$derived` work when imported into a plain test.
export default defineConfig({
  plugins: [svelte({ hot: !process.env.VITEST })],
  resolve: {
    conditions: ["browser"],
  },
  test: {
    environment: "happy-dom",
    globals: true,
    include: ["src/**/*.test.ts"],
  },
});

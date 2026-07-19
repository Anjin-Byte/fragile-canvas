// Dev-server config for the component playground (`npm run dev`).
// Tests use vitest.config.ts, which takes precedence for vitest.
import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  root: "dev",
  plugins: [svelte()],
});

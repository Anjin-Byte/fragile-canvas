import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'

export default defineConfig({
  plugins: [svelte()],
  server: {
    open: false,
  },
  optimizeDeps: {
    exclude: ['@fragile-canvas/ui'],
  },
  ssr: {
    noExternal: ['@fragile-canvas/ui'],
  },
})

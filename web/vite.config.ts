import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import wasm from 'vite-plugin-wasm'

export default defineConfig({
  plugins: [svelte(), wasm()],
  base: process.env.GITHUB_ACTIONS ? '/fragile-canvas/' : '/',
  server: {
    fs: {
      allow: [
        '.',
        '../wasm/pkg',
        '../ui',
      ],
    },
  },
  // Treat the linked ui package as source (not pre-bundled)
  // so the Svelte plugin processes .svelte files from ../ui/src
  optimizeDeps: {
    exclude: ['@fragile-canvas/ui'],
  },
  ssr: {
    noExternal: ['@fragile-canvas/ui'],
  },
})

/// <reference types="svelte" />

// Allow importing .svelte files as modules in TypeScript
declare module "*.svelte" {
  import type { SvelteComponent } from "svelte";
  const component: typeof SvelteComponent;
  export default component;
}

declare module "*.jpg" {
  const src: string;
  export default src;
}

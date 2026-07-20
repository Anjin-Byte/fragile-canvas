<script lang="ts">
  /**
   * The game screen panel — the permanent center anchor of the workbench.
   * Wraps the WebGL2 Screen (DMG LCD shader); the dock's keep-alive panel
   * layer keeps the GL context mounted across panel moves and floats.
   * Shows the hero / ROM-load state until a ROM is running.
   */
  import Screen from "../../components/Screen.svelte";
  import { FolderOpen, Gamepad2 } from "lucide-svelte";
  import type { EmuController } from "../emu.svelte.js";
  import type { Settings } from "../settings.svelte.js";

  let { emu, settings }: { emu: EmuController; settings: Settings } = $props();

  let screen = $state<Screen>();
  let stageEl = $state<HTMLDivElement>();
  let fileInput = $state<HTMLInputElement>();
  let wellW = $state(320);
  let wellH = $state(288);

  // Register the blit sink with the controller once the canvas exists.
  $effect(() => {
    if (screen) emu.attachScreen((shades) => screen!.blit(shades));
  });

  // Apply the LCD-effect display preference to the shader.
  $effect(() => {
    if (screen) screen.setLcdEffect(settings.lcdEffect);
  });

  // Fit a 160:144 well inside the panel with breathing room.
  $effect(() => {
    if (!stageEl) return;
    const fit = () => {
      const rect = stageEl!.getBoundingClientRect();
      const maxW = Math.max(160, rect.width - 24);
      const maxH = Math.max(144, rect.height - 24);
      const w = Math.min(maxW, (maxH * 160) / 144);
      wellW = w;
      wellH = (w * 144) / 160;
    };
    fit();
    const ro = new ResizeObserver(fit);
    ro.observe(stageEl);
    return () => ro.disconnect();
  });

  async function onFileSelect() {
    const file = fileInput?.files?.[0];
    if (!file) return;
    await emu.loadRomBytes(await file.arrayBuffer());
  }
</script>

<div class="screen-stage" bind:this={stageEl}>
  <div
    class="screen-well"
    class:screen-off={!emu.romLoaded}
    style:width="{wellW}px"
    style:height="{wellH}px"
  >
    <Screen bind:this={screen} />
  </div>

  {#if !emu.romLoaded}
    <div class="hero">
      <!-- <span class="hero-title">fragile-canvas</span> -->
      <input
        bind:this={fileInput}
        type="file"
        accept=".gb,.gbc,.bin"
        class="hidden-input"
        onchange={onFileSelect}
      />
      <button class="hero-btn" onclick={() => fileInput?.click()}>
        <FolderOpen size={14} /> Load a ROM
      </button>
      {#if emu.bundledRoms.length > 0}
        <div class="hero-bundled">
          {#each emu.bundledRoms as rom (rom.id)}
            <button class="hero-rom" onclick={() => emu.loadBundled(rom.id)}>
              <Gamepad2 size={12} />
              <span>{rom.title}</span>
            </button>
          {/each}
        </div>
      {/if}
      <span class="hero-hint">or drop a .gb file anywhere</span>
    </div>
  {/if}
</div>

<style>
  .screen-stage {
    position: relative;
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--surface-1);
  }

  .screen-well {
    position: relative;
    border-radius: 3px;
    overflow: hidden;
    box-shadow:
      inset 1.5px 1.5px 3px oklch(0 0 0 / 40%),
      inset 0 0 0 1px oklch(0 0 0 / 25%);
  }

  /* Powered-off LCD: darkest DMG green, barely visible (spec §Hero). */
  .screen-off {
    opacity: 0.35;
  }

  .hero {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 14px;
    background: oklch(0 0 0 / 35%);
  }

  .hero-title {
    font-family: var(--font-mono);
    font-size: 15px;
    font-weight: 500;
    color: var(--accent);
    letter-spacing: 0.02em;
  }

  .hero-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 7px 16px;
    font-family: var(--font-sans);
    font-size: 12px;
    font-weight: 500;
    color: oklch(0.18 0.01 115);
    background: var(--accent-oklch, var(--interactive));
    border: 1px solid oklch(0.82 0.2 115);
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: background 0.15s ease;
  }

  .hero-btn:hover {
    background: var(--interactive-hi);
  }

  .hero-bundled {
    display: flex;
    flex-direction: column;
    gap: 4px;
    align-items: center;
  }

  .hero-rom {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 12px;
    font-family: var(--font-sans);
    font-size: 11px;
    color: var(--text-mid);
    background: var(--fill-lo);
    border: 1px solid var(--stroke-mid);
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: background 0.1s ease, color 0.1s ease;
  }

  .hero-rom:hover {
    color: var(--text-hi);
    background: var(--fill-mid);
  }

  .hero-hint {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-faint);
  }

  .hidden-input {
    position: absolute;
    opacity: 0;
    width: 0;
    height: 0;
    pointer-events: none;
  }
</style>

<script lang="ts">
  /**
   * The game screen panel — the permanent center anchor of the workbench.
   * Wraps the WebGL2 Screen (DMG LCD shader); the dock's keep-alive panel
   * layer keeps the GL context mounted across panel moves and floats.
   * Shows the hero / ROM-load state until a ROM is running.
   */
  import Screen from "../../components/Screen.svelte";
  import { FolderOpen, Gamepad2, Play } from "lucide-svelte";
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
    // Reset so re-picking the SAME file still fires onchange (else silent no-op).
    if (fileInput) fileInput.value = "";
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
      <input
        bind:this={fileInput}
        type="file"
        accept=".gb,.gbc,.bin"
        class="hidden-input"
        onchange={onFileSelect}
      />
      <div class="hero-content">
        {#if emu.canAssemble}
          <!-- Assembly is the primary path: run what's already in the editor. -->
          <button class="hero-btn" onclick={() => void emu.launchCurrent()}>
            <Play size={14} /> Launch
          </button>
          <span class="hero-sub">
            Runs the program in the editor <kbd class="hero-kbd">⌘↵</kbd>
          </span>
          <div class="hero-or"><span>or</span></div>
          <div class="hero-alt">
            <button class="hero-rom" onclick={() => fileInput?.click()}>
              <FolderOpen size={12} />
              <span>Load a ROM</span>
            </button>
            {#each emu.bundledRoms as rom (rom.id)}
              <button class="hero-rom" onclick={() => emu.loadBundled(rom.id)}>
                <Gamepad2 size={12} />
                <span>{rom.title}</span>
              </button>
            {/each}
          </div>
          <span class="hero-hint">drop a .gb file anywhere</span>
        {:else}
          <!-- No assembler (e.g. desktop): keep ROM-loading as the primary path. -->
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
        {/if}
      </div>
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
    align-items: center;
    justify-content: center;
    /* Vignette: lighter over the CTA, darker at the edges → draws the eye in. */
    background: radial-gradient(
      ellipse 60% 60% at center,
      oklch(0 0 0 / 20%) 0%,
      oklch(0 0 0 / 48%) 100%
    );
  }

  .hero-content {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 13px;
    animation: hero-rise 0.4s cubic-bezier(0.2, 0.8, 0.2, 1) both;
  }

  @keyframes hero-rise {
    from {
      opacity: 0;
      transform: translateY(7px);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }

  .hero-sub {
    margin-top: -5px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-family: var(--font-sans);
    font-size: 11px;
    color: var(--text-subtle);
  }

  .hero-kbd {
    font-family: var(--font-mono);
    font-size: 9px;
    line-height: 1;
    padding: 2px 4px;
    color: var(--text-mid);
    background: var(--fill-mid);
    border: 1px solid var(--stroke-mid);
    border-radius: 3px;
  }

  /* "or" hairline divider between the Launch CTA and the ROM options. */
  .hero-or {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 150px;
    margin-top: 3px;
    font-family: var(--font-sans);
    font-size: 10px;
    color: var(--text-faint);
  }

  .hero-or::before,
  .hero-or::after {
    content: "";
    flex: 1;
    height: 1px;
    background: var(--stroke-mid);
  }

  /* Secondary ROM options, quieter than the Launch CTA and grouped in a row. */
  .hero-alt {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 6px;
  }

  .hero-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 8px 18px;
    font-family: var(--font-sans);
    font-size: 12px;
    font-weight: 600;
    color: oklch(0.18 0.01 115);
    background: var(--accent-oklch, var(--interactive));
    border: 1px solid oklch(0.82 0.2 115);
    border-radius: var(--radius-sm);
    cursor: pointer;
    box-shadow:
      0 4px 12px oklch(0.76 0.18 115 / 14%),
      0 2px 6px oklch(0 0 0 / 30%);
    transition: background 0.15s ease, box-shadow 0.15s ease, transform 0.15s ease;
  }

  .hero-btn:hover {
    background: var(--interactive-hi);
    box-shadow:
      0 5px 14px oklch(0.76 0.18 115 / 20%),
      0 2px 6px oklch(0 0 0 / 30%);
    transform: translateY(-1px);
  }

  .hero-btn:active {
    transform: translateY(0);
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

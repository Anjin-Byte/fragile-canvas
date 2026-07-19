<script lang="ts">
  /**
   * Workbench toolbar — transport controls, ROM loading, layout reset.
   * Shortcuts (wired in Workbench): F5 run/pause, F10 step, F6 frame.
   */
  import {
    Play, Pause, StepForward, SkipForward, RotateCcw,
    Upload, Gamepad2, ChevronDown, LayoutGrid,
  } from "lucide-svelte";
  import type { EmuController } from "./emu.svelte.js";

  let {
    emu,
    onresetlayout,
  }: {
    emu: EmuController;
    onresetlayout: () => void;
  } = $props();

  let fileInput = $state<HTMLInputElement>();
  let romMenuOpen = $state(false);
  let menuEl = $state<HTMLDivElement>();

  async function onFileSelect() {
    const file = fileInput?.files?.[0];
    if (!file) return;
    await emu.loadRomBytes(await file.arrayBuffer());
  }

  $effect(() => {
    if (!romMenuOpen) return;
    const close = (e: PointerEvent) => {
      if (menuEl && !menuEl.contains(e.target as Node)) romMenuOpen = false;
    };
    window.addEventListener("pointerdown", close, true);
    return () => window.removeEventListener("pointerdown", close, true);
  });
</script>

<header class="toolbar">
  <span class="brand">fragile-canvas</span>

  <div class="transport">
    <button
      class="tb-btn"
      class:accent={emu.running}
      title={emu.running ? "Pause (F5)" : "Run (F5)"}
      disabled={!emu.romLoaded}
      onclick={() => emu.toggle()}
    >
      {#if emu.running}<Pause size={13} />{:else}<Play size={13} />{/if}
    </button>
    <button
      class="tb-btn"
      title="Step 1 M-cycle (F10)"
      disabled={!emu.romLoaded || emu.running}
      onclick={() => emu.stepCycles(1)}
    ><StepForward size={13} /></button>
    <button
      class="tb-btn tb-btn-text"
      title="Step 100 M-cycles"
      disabled={!emu.romLoaded || emu.running}
      onclick={() => emu.stepCycles(100)}
    >×100</button>
    <button
      class="tb-btn"
      title="Step one frame (F6)"
      disabled={!emu.romLoaded || emu.running}
      onclick={() => emu.stepFrame()}
    ><SkipForward size={13} /></button>
    <button
      class="tb-btn"
      title="Reset"
      disabled={!emu.romLoaded}
      onclick={() => emu.reset()}
    ><RotateCcw size={13} /></button>
  </div>

  <div class="spacer"></div>

  <input
    bind:this={fileInput}
    type="file"
    accept=".gb,.gbc,.bin"
    class="hidden-input"
    onchange={onFileSelect}
  />
  <button class="tb-btn tb-btn-labeled" title="Load a ROM file" onclick={() => fileInput?.click()}>
    <Upload size={13} /> Open
  </button>

  <div class="rom-menu" bind:this={menuEl}>
    <button
      class="tb-btn tb-btn-labeled"
      title="Bundled games"
      onclick={() => (romMenuOpen = !romMenuOpen)}
    >
      <Gamepad2 size={13} /> Bundled <ChevronDown size={11} />
    </button>
    {#if romMenuOpen}
      <div class="rom-menu-list">
        {#each emu.bundledRoms as rom (rom.id)}
          <button
            class="rom-menu-item"
            onclick={() => {
              romMenuOpen = false;
              emu.loadBundled(rom.id);
            }}
          >
            <span class="rom-menu-title">{rom.title}</span>
            <span class="rom-menu-author">{rom.author}</span>
          </button>
        {/each}
      </div>
    {/if}
  </div>

  <button class="tb-btn" title="Reset panel layout" onclick={onresetlayout}>
    <LayoutGrid size={13} />
  </button>
</header>

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 38px;
    padding: 0 12px;
    background: var(--surface-0);
    border-bottom: 1px solid var(--stroke-lo);
    flex-shrink: 0;
    user-select: none;
  }

  .brand {
    font-family: var(--font-mono);
    font-size: 12px;
    font-weight: 500;
    color: var(--accent);
    letter-spacing: 0.02em;
    margin-right: 8px;
  }

  .transport {
    display: flex;
    gap: 2px;
    padding: 2px;
    background: var(--fill-lo);
    border-radius: var(--radius-sm);
  }

  .spacer {
    flex: 1;
  }

  .tb-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    min-width: 26px;
    height: 24px;
    padding: 0 6px;
    font-family: var(--font-sans);
    font-size: 11px;
    font-weight: 500;
    color: var(--text-subtle);
    background: none;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: color 0.1s ease, background 0.1s ease;
  }

  .tb-btn:hover:not(:disabled) {
    color: var(--text-hi);
    background: var(--fill-mid);
  }

  .tb-btn:disabled {
    opacity: 0.35;
    cursor: default;
  }

  .tb-btn.accent {
    color: var(--interactive);
  }

  .tb-btn-text {
    font-family: var(--font-mono);
    font-size: 10px;
  }

  .tb-btn-labeled {
    padding: 0 9px;
  }

  .hidden-input {
    position: absolute;
    opacity: 0;
    width: 0;
    height: 0;
    pointer-events: none;
  }

  .rom-menu {
    position: relative;
  }

  .rom-menu-list {
    position: absolute;
    top: calc(100% + 4px);
    right: 0;
    min-width: 220px;
    background: var(--surface-3);
    border: 1px solid var(--stroke-mid);
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-overlay);
    overflow: hidden;
    z-index: 200;
  }

  .rom-menu-item {
    display: flex;
    flex-direction: column;
    gap: 1px;
    width: 100%;
    padding: 8px 12px;
    background: none;
    border: none;
    border-bottom: 1px solid var(--stroke-lo);
    cursor: pointer;
    text-align: left;
    transition: background 0.1s ease;
  }

  .rom-menu-item:last-child {
    border-bottom: none;
  }

  .rom-menu-item:hover {
    background: var(--fill-mid);
  }

  .rom-menu-title {
    font-family: var(--font-sans);
    font-size: 12px;
    font-weight: 500;
    color: var(--text-hi);
  }

  .rom-menu-author {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-subtle);
  }
</style>

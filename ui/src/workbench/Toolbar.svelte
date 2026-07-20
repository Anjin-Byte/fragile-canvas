<script lang="ts">
  /**
   * Workbench toolbar — transport controls, ROM loading, layout reset.
   * Shortcuts (wired in Workbench): F5 run/pause, F10 step, F6 frame.
   */
  import type { Snippet, Component } from "svelte";
  import {
    Play, Pause, StepForward, SkipForward, RotateCcw,
    FolderOpen, Gamepad2, ChevronDown,
    Cpu, Code, MemoryStick, Grid3x3, Music, Gauge, Volume2, VolumeX,
    FastForward, Command as CommandIcon, Settings,
  } from "lucide-svelte";
  import type { EmuController } from "./emu.svelte.js";
  import type { Command } from "./commands.svelte.js";

  let {
    emu,
    commands,
    layouts,
    openPalette,
    openPreferences,
  }: {
    emu: EmuController;
    /** Command registry — panel toggles, machine + audio render as inline buttons. */
    commands: Command[];
    /** Right-side layout control (the LayoutsMenu). */
    layouts?: Snippet;
    /** Open the command palette (Cmd+K). */
    openPalette?: () => void;
    /** Open the preferences dialog. */
    openPreferences?: () => void;
  } = $props();

  // Panel id → toolbar icon (Screen is permanent, so it has no toggle).
  const PANEL_ICON: Record<string, Component> = {
    cpu: Cpu, disasm: Code, memory: MemoryStick,
    ppu: Grid3x3, apu: Music, io: Gauge,
  };

  // Visible panel-visibility toggles (permanent panels report enabled()===false).
  const viewButtons = $derived(
    commands
      .filter((c) => c.group === "View" && c.kind === "toggle" && (c.enabled ? c.enabled() : true))
      .map((c) => ({ cmd: c as Extract<Command, { kind: "toggle" }>, icon: PANEL_ICON[c.id.replace(/^view\./, "")] })),
  );
  const volumeCmd = $derived(
    commands.find((c): c is Extract<Command, { kind: "value" }> => c.id === "audio.volume"),
  );
  const muteCmd = $derived(
    commands.find((c): c is Extract<Command, { kind: "toggle" }> => c.id === "audio.mute"),
  );
  const audioOn = $derived(!!volumeCmd && (volumeCmd.enabled ? volumeCmd.enabled() : true));

  // Machine cluster: speed presets + fast-forward + boot ROM.
  const speedCmd = $derived(
    commands.find((c): c is Extract<Command, { kind: "choice" }> => c.id === "machine.speed"),
  );
  const turboCmd = $derived(
    commands.find((c): c is Extract<Command, { kind: "toggle" }> => c.id === "machine.turbo"),
  );

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

  <!-- Panel visibility toggles (active = open; click to show/hide). -->
  {#if viewButtons.length > 0}
    <div class="tb-divider"></div>
    <div class="tb-panels">
      {#each viewButtons as vb (vb.cmd.id)}
        {@const Icon = vb.icon}
        <button
          class="tb-btn"
          class:active={vb.cmd.checked()}
          title={vb.cmd.label}
          aria-pressed={vb.cmd.checked()}
          onclick={() => vb.cmd.run()}
        >
          {#if Icon}<Icon size={14} />{:else}{vb.cmd.label}{/if}
        </button>
      {/each}
    </div>
  {/if}

  <!-- Master volume: mute button + inline slider. -->
  {#if audioOn && volumeCmd && muteCmd}
    <div class="tb-divider"></div>
    <div class="tb-volume">
      <button
        class="tb-btn"
        title={muteCmd.checked() ? "Unmute" : "Mute"}
        aria-pressed={muteCmd.checked()}
        onclick={() => muteCmd.run()}
      >
        {#if muteCmd.checked()}<VolumeX size={14} />{:else}<Volume2 size={14} />{/if}
      </button>
      <input
        class="tb-vol"
        type="range"
        min={volumeCmd.min}
        max={volumeCmd.max}
        step={volumeCmd.step}
        value={volumeCmd.value()}
        disabled={muteCmd.checked()}
        aria-label="Master volume"
        oninput={(e) => volumeCmd.set(+e.currentTarget.value)}
      />
    </div>
  {/if}

  <!-- Machine: speed presets + fast-forward. -->
  {#if speedCmd && turboCmd}
    <div class="tb-divider"></div>
    <div class="tb-machine">
      <div class="tb-seg" role="group" aria-label="Emulation speed">
        {#each speedCmd.options as opt (opt.value)}
          <button
            class="tb-seg-btn"
            class:active={speedCmd.value() === opt.value}
            title={`Speed ${opt.label}`}
            onclick={() => speedCmd.select(opt.value)}
          >{opt.label}</button>
        {/each}
      </div>
      <button
        class="tb-btn"
        class:active={turboCmd.checked()}
        title="Fast-forward"
        aria-pressed={turboCmd.checked()}
        disabled={turboCmd.enabled ? !turboCmd.enabled() : false}
        onclick={() => turboCmd.run()}
      ><FastForward size={14} /></button>
    </div>
  {/if}

  <div class="spacer"></div>

  <input
    bind:this={fileInput}
    type="file"
    accept=".gb,.gbc,.bin"
    class="hidden-input"
    onchange={onFileSelect}
  />
  <button class="tb-btn tb-btn-labeled" title="Load a ROM file" onclick={() => fileInput?.click()}>
    <FolderOpen size={13} /> Open
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

  {@render layouts?.()}

  <div class="tb-divider"></div>
  <button
    class="tb-btn"
    title="Command palette (⌘K)"
    aria-label="Command palette"
    onclick={() => openPalette?.()}
  ><CommandIcon size={14} /></button>
  <button
    class="tb-btn"
    title="Preferences"
    aria-label="Preferences"
    onclick={() => openPreferences?.()}
  ><Settings size={15} /></button>
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

  /* Panel toggle in the "open" state. */
  .tb-btn.active {
    color: var(--interactive);
    background: var(--interactive-fill);
  }

  .tb-divider {
    width: 1px;
    height: 18px;
    background: var(--stroke-mid);
    margin: 0 2px;
    flex-shrink: 0;
  }

  .tb-panels {
    display: flex;
    gap: 1px;
  }

  .tb-volume {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .tb-machine {
    display: flex;
    align-items: center;
    gap: 3px;
  }

  /* Segmented speed presets — a grouped pill like .transport. */
  .tb-seg {
    display: flex;
    gap: 2px;
    padding: 2px;
    background: var(--fill-lo);
    border-radius: var(--radius-sm);
  }

  .tb-seg-btn {
    min-width: 24px;
    height: 20px;
    padding: 0 5px;
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 500;
    color: var(--text-subtle);
    background: none;
    border: none;
    border-radius: calc(var(--radius-sm) - 1px);
    cursor: pointer;
    transition: color 0.1s ease, background 0.1s ease;
  }

  .tb-seg-btn:hover {
    color: var(--text-hi);
  }

  .tb-seg-btn.active {
    color: var(--interactive);
    background: var(--interactive-fill);
  }

  .tb-vol {
    width: 68px;
    accent-color: var(--interactive);
    cursor: pointer;
  }

  .tb-vol:disabled {
    opacity: 0.4;
    cursor: default;
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

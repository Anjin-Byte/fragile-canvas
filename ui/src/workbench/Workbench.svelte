<script lang="ts">
  /**
   * Workbench — the dock-based debugger shell (gui_spec.md blockout).
   *
   * Toolbar → DockLayout → Footer. The game screen is a permanent
   * center panel; debugger panels dock around it or float above it.
   * One EmuController instance is the reactive source of truth for
   * every panel; layout persists via the dock's persistKey.
   */
  import { onDestroy, untrack } from "svelte";
  import {
    DockLayout,
    DockModel,
    type PanelDef,
  } from "@gestalt/phi";
  import type { EmulatorBackend } from "../types";
  import { EmuController } from "./emu.svelte.js";
  import { DEFAULT_LAYOUT } from "./layouts.js";
  import { Settings } from "./settings.svelte.js";
  import { createCommands } from "./commands.svelte.js";
  import Toolbar from "./Toolbar.svelte";
  import LayoutsMenu from "./LayoutsMenu.svelte";
  import CommandPalette from "./CommandPalette.svelte";
  import PreferencesModal from "./PreferencesModal.svelte";
  import Footer from "./Footer.svelte";
  import ScreenPanel from "./panels/ScreenPanel.svelte";
  import CpuPanel from "./panels/CpuPanel.svelte";
  import MemoryPanel from "./panels/MemoryPanel.svelte";
  import DisasmPanel from "./panels/DisasmPanel.svelte";
  import EditorPanel from "./panels/EditorPanel.svelte";
  import PpuPanel from "./panels/PpuPanel.svelte";
  import ApuPanel from "./panels/ApuPanel.svelte";
  import IoPanel from "./panels/IoPanel.svelte";

  let { backend }: { backend: EmulatorBackend } = $props();

  // The backend is injected once and never reassigned — capture it
  // non-reactively so it isn't treated as a live dependency.
  const emu = new EmuController(untrack(() => backend));
  onDestroy(() => emu.destroy());

  // ─── Dock model ────────────────────────────────────────────────────────

  const LAYOUT_KEY = "fc-workbench-v1";

  const PANEL_DEFS: PanelDef[] = [
    { id: "screen", title: "Screen", closable: false, minWidth: 220, minHeight: 240 },
    { id: "cpu", title: "CPU", minWidth: 190 },
    { id: "editor", title: "Editor", minWidth: 260 },
    { id: "disasm", title: "Disassembly", minWidth: 230 },
    { id: "memory", title: "Memory", minWidth: 360 },
    { id: "ppu", title: "PPU", minWidth: 220 },
    { id: "apu", title: "APU", minWidth: 200 },
    { id: "io", title: "I/O", minWidth: 200 },
  ];
  const PANEL_IDS = PANEL_DEFS.map((d) => d.id);

  let model = $state(DockModel.fromStorage(LAYOUT_KEY, DEFAULT_LAYOUT));
  const getModel = () => model;
  const setModel = (m: DockModel) => (model = m);

  // ─── Settings + command registry ───────────────────────────────────────

  const settings = new Settings();
  const commands = createCommands({ emu, getModel, setModel, settings, panelDefs: PANEL_DEFS });

  // Overlay surfaces driven from the toolbar / keyboard.
  let paletteOpen = $state(false);
  let prefsOpen = $state(false);

  // Applier: push master volume (0 when muted) to the audio backend.
  $effect(() => {
    emu.backend.setMasterVolume?.(settings.muted ? 0 : settings.masterVolume);
  });
  // Appliers: keep the emulator's speed + boot flag in sync with settings.
  $effect(() => {
    emu.speedFactor = settings.speed;
  });
  $effect(() => {
    emu.skipBoot = settings.skipBoot;
  });

  // Reveal requests: bring the target panel to front (opening it if closed) so
  // the panel's own effect can scroll to the requested spot. Each is an
  // independent seq-countered signal ("Show in Memory", "Reveal in source", …).
  let lastMemSeq = 0;
  let lastSourceSeq = 0;
  let lastDisasmSeq = 0;
  $effect(() => {
    const r = emu.memoryRequest;
    if (r.seq !== lastMemSeq) {
      lastMemSeq = r.seq;
      model.openPanel("memory");
    }
  });
  $effect(() => {
    const r = emu.sourceRequest;
    if (r.seq !== lastSourceSeq) {
      lastSourceSeq = r.seq;
      model.openPanel("editor");
    }
  });
  $effect(() => {
    const r = emu.disasmRequest;
    if (r.seq !== lastDisasmSeq) {
      lastDisasmSeq = r.seq;
      model.openPanel("disasm");
    }
  });

  // ─── Keyboard: joypad + transport ──────────────────────────────────────

  const KEY_MAP: Record<string, { group: "action" | "dpad"; bit: number }> = {
    z: { group: "action", bit: 1 },          // A
    x: { group: "action", bit: 2 },          // B
    Shift: { group: "action", bit: 4 },      // Select
    Enter: { group: "action", bit: 8 },      // Start
    ArrowRight: { group: "dpad", bit: 1 },
    ArrowLeft: { group: "dpad", bit: 2 },
    ArrowUp: { group: "dpad", bit: 4 },
    ArrowDown: { group: "dpad", bit: 8 },
  };

  function isEditableTarget(e: KeyboardEvent): boolean {
    const el = e.target as HTMLElement | null;
    if (!el) return false;
    return (
      el.tagName === "INPUT" ||
      el.tagName === "TEXTAREA" ||
      el.tagName === "SELECT" ||
      el.isContentEditable
    );
  }

  function handleKey(e: KeyboardEvent, pressed: boolean) {
    // Command palette — Cmd/Ctrl+K, from anywhere (including inputs).
    if (pressed && (e.metaKey || e.ctrlKey) && (e.key === "k" || e.key === "K")) {
      e.preventDefault();
      paletteOpen = !paletteOpen;
      return;
    }

    // Transport shortcuts work anywhere except editable fields.
    if (pressed && !isEditableTarget(e)) {
      if (e.key === "F5") {
        e.preventDefault();
        emu.toggle();
        return;
      }
      if (e.key === "F10") {
        e.preventDefault();
        if (!emu.running) void emu.stepCycles(1);
        return;
      }
      if (e.key === "F6") {
        e.preventDefault();
        if (!emu.running) void emu.stepFrame();
        return;
      }
    }

    if (!emu.romLoaded || isEditableTarget(e)) return;

    if (e.key === " " && pressed) {
      e.preventDefault();
      emu.toggle();
      return;
    }

    const mapping = KEY_MAP[e.key];
    if (!mapping) return;
    e.preventDefault();
    emu.setButton(mapping.group, mapping.bit, pressed);
  }

  // ─── ROM drag-and-drop (files only — dock tab drags must pass through) ─

  let dragging = $state(false);
  let dragCounter = 0;

  function isFileDrag(e: DragEvent): boolean {
    return e.dataTransfer?.types.includes("Files") ?? false;
  }

  function onDragEnter(e: DragEvent) {
    if (!isFileDrag(e)) return;
    e.preventDefault();
    dragCounter++;
    dragging = true;
  }

  function onDragOver(e: DragEvent) {
    if (isFileDrag(e)) e.preventDefault();
  }

  function onDragLeave(e: DragEvent) {
    if (!isFileDrag(e)) return;
    dragCounter--;
    if (dragCounter <= 0) {
      dragCounter = 0;
      dragging = false;
    }
  }

  async function onDrop(e: DragEvent) {
    if (!isFileDrag(e)) return;
    e.preventDefault();
    dragCounter = 0;
    dragging = false;
    const file = e.dataTransfer?.files?.[0];
    if (!file) return;
    await emu.loadRomBytes(await file.arrayBuffer());
  }
</script>

<svelte:window
  onkeydown={(e) => handleKey(e, true)}
  onkeyup={(e) => handleKey(e, false)}
  ondragenter={onDragEnter}
  ondragover={onDragOver}
  ondragleave={onDragLeave}
  ondrop={onDrop}
/>

<div class="workbench">
  <Toolbar
    {emu}
    {commands}
    openPalette={() => (paletteOpen = true)}
    openPreferences={() => (prefsOpen = true)}
  >
    {#snippet layouts()}
      <LayoutsMenu {model} panelIds={PANEL_IDS} onapply={setModel} />
    {/snippet}
  </Toolbar>

  <main class="dock-area">
    <DockLayout {model} panelDefs={PANEL_DEFS} persistKey={LAYOUT_KEY}>
      {#snippet panel(id: string)}
        {#if id === "screen"}
          <ScreenPanel {emu} {settings} />
        {:else if id === "cpu"}
          <CpuPanel {emu} />
        {:else if id === "memory"}
          <MemoryPanel {emu} />
        {:else if id === "editor"}
          <EditorPanel {emu} />
        {:else if id === "disasm"}
          <DisasmPanel {emu} />
        {:else if id === "ppu"}
          <PpuPanel {emu} />
        {:else if id === "apu"}
          <ApuPanel {emu} />
        {:else if id === "io"}
          <IoPanel {emu} />
        {/if}
      {/snippet}
    </DockLayout>
  </main>

  <Footer />

  {#if dragging}
    <div class="drop-overlay">
      <span class="drop-text">Drop ROM</span>
    </div>
  {/if}

  {#if emu.error}
    <div class="error-toast">{emu.error}</div>
  {/if}

  <CommandPalette {commands} open={paletteOpen} onclose={() => (paletteOpen = false)} />
  <PreferencesModal {commands} open={prefsOpen} onclose={() => (prefsOpen = false)} />
</div>

<style>
  .workbench {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--surface-2);
    position: relative;
  }

  .dock-area {
    flex: 1;
    min-height: 0;
    padding: 4px;
  }

  .drop-overlay {
    position: fixed;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: oklch(0 0 0 / 60%);
    backdrop-filter: blur(4px);
    z-index: 300;
    pointer-events: none;
  }

  .drop-text {
    font-family: var(--font-mono);
    font-size: 18px;
    font-weight: 500;
    color: var(--accent);
    padding: 16px 32px;
    border: 2px dashed oklch(0.76 0.18 115 / 40%);
    border-radius: var(--radius-lg);
    background: oklch(0.76 0.18 115 / 6%);
  }

  .error-toast {
    position: fixed;
    bottom: 36px;
    left: 50%;
    transform: translateX(-50%);
    padding: 8px 16px;
    background: var(--surface-4);
    border: 1px solid var(--color-destructive);
    border-radius: var(--radius-md);
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-destructive);
    z-index: 250;
  }
</style>

<script lang="ts">
  /**
   * Workbench — the dock-based debugger shell (gui_spec.md blockout).
   *
   * Toolbar → DockLayout → StatusBar. The game screen is a permanent
   * center panel; debugger panels dock around it or float above it.
   * One EmuController instance is the reactive source of truth for
   * every panel; layout persists via the dock's persistKey.
   */
  import { onDestroy, untrack } from "svelte";
  import {
    DockLayout,
    DockModel,
    type PanelDef,
    type SerializedNode,
  } from "@gestalt/phi";
  import type { EmulatorBackend } from "../types";
  import { EmuController } from "./emu.svelte.js";
  import Toolbar from "./Toolbar.svelte";
  import StatusBar from "./StatusBar.svelte";
  import ScreenPanel from "./panels/ScreenPanel.svelte";
  import CpuPanel from "./panels/CpuPanel.svelte";
  import MemoryPanel from "./panels/MemoryPanel.svelte";
  import DisasmPanel from "./panels/DisasmPanel.svelte";
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

  const DEFAULT_LAYOUT: SerializedNode = {
    type: "branch",
    orientation: "row",
    children: [
      {
        type: "branch",
        orientation: "column",
        fraction: 0.22,
        children: [
          { type: "leaf", id: "left-top", panels: ["cpu"] },
          { type: "leaf", id: "left-bottom", panels: ["disasm"], fraction: 1.4 },
        ],
      },
      { type: "leaf", id: "center", panels: ["screen"], fraction: 0.5 },
      {
        type: "branch",
        orientation: "column",
        fraction: 0.28,
        children: [
          { type: "leaf", id: "right-top", panels: ["ppu", "apu", "io"] },
          { type: "leaf", id: "right-bottom", panels: ["memory"] },
        ],
      },
    ],
  };

  const PANEL_DEFS: PanelDef[] = [
    { id: "screen", title: "Screen", closable: false, minWidth: 220, minHeight: 240 },
    { id: "cpu", title: "CPU", minWidth: 190 },
    { id: "disasm", title: "Disassembly", minWidth: 230 },
    { id: "memory", title: "Memory", minWidth: 360 },
    { id: "ppu", title: "PPU", minWidth: 220 },
    { id: "apu", title: "APU", minWidth: 200 },
    { id: "io", title: "I/O", minWidth: 200 },
  ];

  let model = $state(DockModel.fromStorage(LAYOUT_KEY, DEFAULT_LAYOUT));

  function resetLayout() {
    localStorage.removeItem(LAYOUT_KEY);
    model = new DockModel(DEFAULT_LAYOUT);
  }

  // Bring the Memory tab to front when another panel requests a view of it
  // (e.g. "Show in Memory" from the Disassembly panel).
  let lastMemSeq = 0;
  $effect(() => {
    const r = emu.memoryRequest;
    if (r.seq === lastMemSeq) return;
    lastMemSeq = r.seq;
    const leaf = model.findPanel("memory");
    if (leaf) model.activatePanel(leaf.id, "memory");
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
  <Toolbar {emu} onresetlayout={resetLayout} />

  <main class="dock-area">
    <DockLayout {model} panelDefs={PANEL_DEFS} persistKey={LAYOUT_KEY}>
      {#snippet panel(id: string)}
        {#if id === "screen"}
          <ScreenPanel {emu} />
        {:else if id === "cpu"}
          <CpuPanel {emu} />
        {:else if id === "memory"}
          <MemoryPanel {emu} />
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

  <StatusBar {emu} />

  {#if dragging}
    <div class="drop-overlay">
      <span class="drop-text">Drop ROM</span>
    </div>
  {/if}

  {#if emu.error}
    <div class="error-toast">{emu.error}</div>
  {/if}
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

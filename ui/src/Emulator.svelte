<script lang="ts">
  import { onMount } from "svelte";
  import type { CpuState, EmulatorBackend, BundledRomInfo } from "./types";
  import Screen from "./components/Screen.svelte";
  import { Play, StepForward, SkipForward, Upload, Gamepad2, ChevronDown } from "lucide-svelte";

  // ── Props ──
  let { backend }: { backend: EmulatorBackend } = $props();

  // ── State ──
  let cpu = $state<CpuState | null>(null);
  let loaded = $state(false);
  let error = $state<string | null>(null);
  let running = $state(false);
  let paused = $state(false);
  let dragging = $state(false);
  let romPickerOpen = $state(false);
  let bundledRoms = $state<BundledRomInfo[]>([]);
  let fps = $state(0);
  let frameCount = $state(0);

  // ── Non-reactive refs ──
  let runningRef = false;
  let rafRef = 0;
  let screen = $state<Screen>();
  let fileInput = $state<HTMLInputElement>();
  let stageEl = $state<HTMLDivElement>();
  let bezelW = $state(0);
  let bezelH = $state(0);
  let wellW = $state(480);
  let wellH = $state(432);
  let fpsFrames = 0;
  let fpsLast = 0;

  // ── Layout fitting ──
  // Sizes the bezel and screen-well together so the entire unit
  // scales proportionally within the stage area.
  // Chrome = fixed-pixel bezel frame around the screen.
  const CHROME_W = 32;  // bezel-screen-area horizontal padding (16px × 2)
  const CHROME_H = 80;  // bezel-top (~30) + screen-area v-padding (22) + bezel-bottom (~28)

  function fitLayout() {
    if (!stageEl) return;
    const rect = stageEl.getBoundingClientRect();
    const maxW = rect.width * 0.90;
    const maxH = rect.height * 0.85;
    // Maximum screen dimensions after subtracting chrome
    const maxScreenW = maxW - CHROME_W;
    const maxScreenH = maxH - CHROME_H;
    // Fit 160:144 screen within available area
    const fitW = maxScreenH * 160 / 144;
    if (fitW <= maxScreenW) {
      wellW = fitW;
      wellH = maxScreenH;
    } else {
      wellW = maxScreenW;
      wellH = maxScreenW * 144 / 160;
    }
    bezelW = wellW + CHROME_W;
    bezelH = wellH + CHROME_H;
  }

  // ── Joypad ──
  const BTN_A = 1, BTN_B = 2, BTN_SELECT = 4, BTN_START = 8;
  const DPAD_RIGHT = 1, DPAD_LEFT = 2, DPAD_UP = 4, DPAD_DOWN = 8;

  const KEY_MAP: Record<string, { group: "action" | "dpad"; bit: number }> = {
    z:          { group: "action", bit: BTN_A },
    x:          { group: "action", bit: BTN_B },
    Shift:      { group: "action", bit: BTN_SELECT },
    Enter:      { group: "action", bit: BTN_START },
    ArrowRight: { group: "dpad",   bit: DPAD_RIGHT },
    ArrowLeft:  { group: "dpad",   bit: DPAD_LEFT },
    ArrowUp:    { group: "dpad",   bit: DPAD_UP },
    ArrowDown:  { group: "dpad",   bit: DPAD_DOWN },
  };

  let actionBits = 0;
  let dpadBits = 0;

  function handleKey(e: KeyboardEvent, pressed: boolean) {
    if (!loaded) return;

    // Space toggles pause
    if (e.key === " " && pressed) {
      e.preventDefault();
      if (runningRef) { stopLoop(); paused = true; }
      else { paused = false; startLoop(); }
      return;
    }

    const mapping = KEY_MAP[e.key];
    if (!mapping) return;
    e.preventDefault();

    if (mapping.group === "action") {
      actionBits = pressed ? (actionBits | mapping.bit) : (actionBits & ~mapping.bit);
    } else {
      dpadBits = pressed ? (dpadBits | mapping.bit) : (dpadBits & ~mapping.bit);
    }
    backend.setButtons(actionBits, dpadBits);
  }

  // ── Frame rendering ──
  async function updateFrame() {
    const frameData = await backend.getFrame();
    if (frameData && screen) screen.blit(frameData);
  }

  // ── Run loop ──
  function startLoop() {
    if (runningRef) return;
    runningRef = true;
    running = true;
    backend.resetGovernor();
    fpsLast = performance.now();
    fpsFrames = 0;

    let last = performance.now();
    function frame(now: number) {
      if (!runningRef) return;
      const dt = now - last;
      last = now;

      // FPS counter
      fpsFrames++;
      const fpsDt = now - fpsLast;
      if (fpsDt >= 1000) {
        fps = Math.round(fpsFrames * 1000 / fpsDt);
        fpsFrames = 0;
        fpsLast = now;
      }

      const elapsedNs = BigInt(Math.round(dt * 1_000_000));
      backend
        .tickFrame(elapsedNs)
        .then(async (state) => {
          if (!runningRef) return;
          cpu = state;
          frameCount++;
          await updateFrame();
          rafRef = requestAnimationFrame(frame);
        })
        .catch((e) => {
          runningRef = false;
          running = false;
          error = String(e);
        });
    }
    rafRef = requestAnimationFrame(frame);
  }

  function stopLoop() {
    runningRef = false;
    running = false;
    cancelAnimationFrame(rafRef);
  }

  // ── ROM loading ──
  async function loadRomData(data: ArrayBuffer) {
    try {
      const state = await backend.loadRom(new Uint8Array(data));
      cpu = state;
      loaded = true;
      error = null;
      paused = false;
      startLoop();
    } catch (e) {
      error = String(e);
    }
  }

  async function onFileSelect() {
    if (!fileInput?.files?.[0]) return;
    const buf = await fileInput.files[0].arrayBuffer();
    await loadRomData(buf);
  }

  async function loadDefault() {
    try {
      const state = await backend.loadDefaultRom();
      cpu = state;
      loaded = true;
      error = null;
      paused = false;
      startLoop();
    } catch (e) {
      error = String(e);
    }
  }

  async function loadBundled(id: string) {
    romPickerOpen = false;
    try {
      const state = await backend.loadBundledRom(id);
      cpu = state;
      loaded = true;
      error = null;
      paused = false;
      startLoop();
    } catch (e) {
      error = String(e);
    }
  }

  function toggleRomPicker() {
    romPickerOpen = !romPickerOpen;
  }

  function onCardClick() {
    fileInput?.click();
  }

  // ── Drag and drop ──
  // Counter tracks nested dragenter/dragleave pairs so the overlay
  // stays visible when the cursor moves over child elements.
  let dragCounter = 0;

  function onDragEnter(e: DragEvent) {
    e.preventDefault();
    dragCounter++;
    dragging = true;
  }

  function onDragOver(e: DragEvent) {
    e.preventDefault();
  }

  function onDragLeave() {
    dragCounter--;
    if (dragCounter <= 0) {
      dragCounter = 0;
      dragging = false;
    }
  }

  async function onDrop(e: DragEvent) {
    e.preventDefault();
    dragCounter = 0;
    dragging = false;
    const file = e.dataTransfer?.files?.[0];
    if (!file) return;
    const buf = await file.arrayBuffer();
    await loadRomData(buf);
  }

  // ── Status line helpers ──
  function hex16(n: number): string {
    return n.toString(16).toUpperCase().padStart(4, "0");
  }

  // ── Lifecycle ──

  // Fit bezel + screen to stage when the playing view appears.
  // $effect re-runs when stageEl becomes available (loaded → true).
  $effect(() => {
    if (!stageEl) return;
    fitLayout();
    const ro = new ResizeObserver(fitLayout);
    ro.observe(stageEl);
    return () => ro.disconnect();
  });

  onMount(() => {
    bundledRoms = backend.listBundledRoms();

    const onDown = (e: KeyboardEvent) => handleKey(e, true);
    const onUp = (e: KeyboardEvent) => handleKey(e, false);
    const onClickOutside = (e: MouseEvent) => {
      if (romPickerOpen && !(e.target as Element)?.closest(".rom-picker")) {
        romPickerOpen = false;
      }
    };
    window.addEventListener("keydown", onDown);
    window.addEventListener("keyup", onUp);
    window.addEventListener("pointerdown", onClickOutside);

    return () => {
      window.removeEventListener("keydown", onDown);
      window.removeEventListener("keyup", onUp);
      window.removeEventListener("pointerdown", onClickOutside);
      runningRef = false;
      cancelAnimationFrame(rafRef);
    };
  });
</script>

<svelte:window
  ondragenter={onDragEnter}
  ondragover={onDragOver}
  ondragleave={onDragLeave}
  ondrop={onDrop}
/>

<div class="app" class:dragging>
  {#if !loaded}
    <!-- ── Hero state ── -->
    <div class="hero">
      <h1 class="hero-title">Okra</h1>

      <input
        bind:this={fileInput}
        type="file"
        accept=".gb,.gbc,.bin"
        class="hidden-input"
        onchange={onFileSelect}
      />

      <div class="hero-actions">
        <button class="hero-btn hero-btn-primary" onclick={onCardClick}>
          <Upload size={16} /> Load a ROM file
        </button>
        <span class="hero-divider">or</span>
        <div class="rom-picker">
          <button class="hero-btn hero-btn-secondary" onclick={toggleRomPicker}>
            <Gamepad2 size={16} />
            Play a bundled game
            <span class="rom-picker-chevron" class:open={romPickerOpen}>
              <ChevronDown size={14} />
            </span>
          </button>
          {#if romPickerOpen}
            <div class="rom-picker-menu">
              {#each bundledRoms as rom}
                <button class="rom-picker-item" onclick={() => loadBundled(rom.id)}>
                  <span class="rom-picker-title">{rom.title}</span>
                  <span class="rom-picker-author">{rom.author}</span>
                </button>
              {/each}
            </div>
          {/if}
        </div>
      </div>

      <p class="hero-hint">or drop a .gb file anywhere</p>
    </div>
  {:else}
    <!-- ── Playing state ── -->
    <div class="stage" bind:this={stageEl}>
      <div class="screen-bezel" style:width="{bezelW}px" style:height="{bezelH}px">
        <div class="bezel-top">
        <div class="power-led" class:led-on={running}></div>
          <span class="bezel-brand-text">DOT MATRIX WITH STEREO SOUND</span>
        </div>
        <div class="bezel-screen-area">
          <div class="screen-well" style:width="{wellW}px" style:height="{wellH}px">
            <Screen bind:this={screen} />
          </div>
        </div>
        <div class="bezel-bottom">
          <div class="bezel-ridges">
            <div class="bezel-ridge"></div>
            <div class="bezel-ridge"></div>
          </div>
        </div>
      </div>

      {#if paused}
        <div class="pause-overlay">
          <div class="pause-bar">
            <button class="ctrl-btn" onclick={() => { paused = false; startLoop(); }}>
              <Play size={12} /> Resume
            </button>
            <button class="ctrl-btn" onclick={async () => { cpu = await backend.step(1); await updateFrame(); }}>
              <StepForward size={12} /> Step
            </button>
            <button class="ctrl-btn" onclick={async () => { cpu = await backend.step(100); await updateFrame(); }}>
              <StepForward size={12} /> ×100
            </button>
            <button class="ctrl-btn" onclick={async () => {
              for (let i = 0; i < 17556; i++) { cpu = await backend.step(1); }
              await updateFrame();
            }}>
              <SkipForward size={12} /> Frame
            </button>
          </div>
        </div>
      {/if}

      {#if cpu}
        <div class="status-line">
          <span class="status-segment">PC:{hex16(cpu.pc)}</span>
          <span class="status-sep">│</span>
          <span class="status-segment">Frame {frameCount}</span>
          <span class="status-sep">│</span>
          <span class="status-segment">{fps}fps</span>
          <span class="status-sep">│</span>
          <span class="status-segment">{running ? "▶ Running" : paused ? "⏸ Paused" : "Ready"}</span>
        </div>
      {/if}
    </div>
  {/if}

  {#if dragging}
    <div class="drop-overlay">
      <span class="drop-text">Drop ROM</span>
    </div>
  {/if}

  {#if error}
    <div class="error-toast">{error}</div>
  {/if}
</div>

<style>
  .app {
    height: 100%;
    position: relative;
    display: flex;
    flex-direction: column;
  }

  /* ── Hero state ── */
  .hero {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 32px;
    position: relative;
  }

  /* Ambient glow — dimmer than playing state (Game Boy is off) */
  .hero::before {
    content: "";
    position: absolute;
    top: 50%;
    left: 50%;
    width: 700px;
    height: 650px;
    transform: translate(-50%, -55%);
    z-index: -1;
    border-radius: 50%;
    background: radial-gradient(
      ellipse at center,
      oklch(0.76 0.06 115 / 3%) 0%,
      oklch(0.76 0.03 115 / 1%) 45%,
      transparent 70%
    );
    pointer-events: none;
  }

  .hero-title {
    font-family: var(--font-mono);
    font-size: 20px;
    font-weight: 500;
    color: var(--accent);
    letter-spacing: 0.02em;
  }

  /* ── Hero actions ── */
  .hero-actions {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
  }

  .hero-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 10px 24px;
    min-height: 40px;
    font-family: var(--font-sans);
    font-size: 14px;
    font-weight: 500;
    border-radius: var(--radius-md);
    cursor: pointer;
    transition: background 0.15s ease, border-color 0.15s ease,
                box-shadow 0.15s ease, transform 0.1s ease;
    user-select: none;
    white-space: nowrap;
  }

  .hero-btn:focus-visible {
    outline: none;
    box-shadow: 0 0 0 2px var(--interactive-ring);
  }

  .hero-btn:active {
    transform: scale(0.98);
  }

  /* Primary — green accent, the main CTA */
  .hero-btn-primary {
    color: oklch(0.18 0.01 115);
    background: var(--accent-oklch);
    border: 1px solid oklch(0.82 0.20 115);
  }

  .hero-btn-primary:hover {
    background: var(--interactive-hi);
    box-shadow: 0 0 12px oklch(0.76 0.18 115 / 20%);
  }

  /* Secondary — surface-level, understated */
  .hero-btn-secondary {
    color: var(--text-mid);
    background: var(--surface-3);
    border: 1px solid var(--stroke-mid);
  }

  .hero-btn-secondary:hover {
    color: var(--text-hi);
    background: var(--surface-4);
    border-color: var(--stroke-hi);
  }

  /* ── Divider ── */
  .hero-divider {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-faint);
    letter-spacing: 0.05em;
    position: relative;
    padding: 0 16px;
  }

  .hero-divider::before,
  .hero-divider::after {
    content: "";
    position: absolute;
    top: 50%;
    width: 40px;
    height: 1px;
    background: var(--stroke-lo);
  }

  .hero-divider::before { right: 100%; }
  .hero-divider::after  { left: 100%; }

  /* ── Hint text ── */
  .hero-hint {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-faint);
    letter-spacing: 0.02em;
  }

  /* ── ROM picker dropdown ── */
  .rom-picker {
    position: relative;
  }

  .rom-picker-chevron {
    display: inline-flex;
    transition: transform 0.15s ease;
  }

  .rom-picker-chevron.open {
    transform: rotate(180deg);
  }

  .rom-picker-menu {
    position: absolute;
    top: calc(100% + 6px);
    left: 0;
    right: 0;
    min-width: max(100%, 240px);
    background: var(--surface-3);
    border: 1px solid var(--stroke-mid);
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-overlay);
    overflow: hidden;
    z-index: 20;
  }

  .rom-picker-item {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 100%;
    padding: 10px 16px;
    background: none;
    border: none;
    border-bottom: 1px solid var(--stroke-lo);
    cursor: pointer;
    text-align: left;
    transition: background 0.1s ease;
  }

  .rom-picker-item:last-child {
    border-bottom: none;
  }

  .rom-picker-item:hover {
    background: var(--fill-mid);
  }

  .rom-picker-title {
    font-family: var(--font-sans);
    font-size: 13px;
    font-weight: 500;
    color: var(--text-hi);
  }

  .rom-picker-author {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-subtle);
    letter-spacing: 0.02em;
  }

  .hidden-input {
    position: absolute;
    opacity: 0;
    width: 0;
    height: 0;
    pointer-events: none;
  }

  /* ── Playing state ── */
  .stage {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0;
    position: relative;
    padding-bottom: 24px; /* reserve space for the fixed status bar */
  }

  .screen-bezel {
    position: relative;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    border-radius: 15px;
    background: linear-gradient(
      180deg,
      oklch(0.24 0.007 250) 0%,
      oklch(0.22 0.007 250) 40%,
      oklch(0.21 0.007 250) 100%
    );
    box-shadow:
      0 1px 0 0 oklch(1 0 0 / 3%),
      0 6px 24px oklch(0 0 0 / 25%);
  }

  .bezel-top {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 16px 25px 0;
  }

  .bezel-brand-text {
    font-family: var(--font-sans);
    font-size: 11px;
    font-weight: 700;
    font-style: italic;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-lo);
  }

  .bezel-screen-area {
    flex: 1;
    min-height: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 12px 16px 10px;
  }

  .power-led {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: oklch(0.26 0.008 250);
    box-shadow: inset 0 0.5px 1px oklch(0 0 0 / 40%);
    transition: background 0.3s ease, box-shadow 0.3s ease;
  }

  .led-on {
    background: oklch(0.72 0.22 115);
    box-shadow:
      inset 0 0.5px 1px oklch(1 0 0 / 20%),
      0 0 3px oklch(0.72 0.22 115 / 60%),
      0 0 10px oklch(0.72 0.22 115 / 20%);
  }

  /* ── Screen well — recessed LCD cutout ──
     Plain rectangle. Chamfer is on the bezel, not here. */
  .screen-well {
    position: relative;
    border-radius: 2px;
    overflow: hidden;
  }

  /* Inner shadow — bevel for the recessed screen */
  .screen-well::after {
    content: "";
    position: absolute;
    inset: 0;
    pointer-events: none;
    border-radius: 2px;
    box-shadow:
      inset 1.5px 1.5px 3px oklch(0 0 0 / 40%),
      inset 0 0 0 1px oklch(0 0 0 / 25%);
    z-index: 1;
  }

  /* ── Bottom area — decorative ridges ── */
  .bezel-bottom {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 8px 52px 14px;
  }

  .bezel-ridges {
    display: flex;
    flex-direction: column;
    gap: 3px;
    width: 100%;
  }

  .bezel-ridge {
    height: 1px;
    border-radius: 0.5px;
    background: oklch(0 0 0 / 10%);
    box-shadow: 0 1px 0 0 oklch(1 0 0 / 2%);
  }

  /* ── Soft ambient glow behind bezel ── */
  .stage::before {
    content: "";
    position: absolute;
    top: 50%;
    left: 50%;
    width: 700px;
    height: 650px;
    transform: translate(-50%, -55%);
    z-index: -1;
    border-radius: 50%;
    background: radial-gradient(
      ellipse at center,
      oklch(0.76 0.10 115 / 6%) 0%,
      oklch(0.76 0.06 115 / 2%) 45%,
      transparent 70%
    );
    pointer-events: none;
  }

  /* ── Pause controls ── */
  .pause-overlay {
    position: absolute;
    bottom: 40px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 10;
  }

  .pause-bar {
    display: flex;
    gap: 2px;
  }

  .ctrl-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 400;
    color: var(--text-subtle);
    background: none;
    border: none;
    border-radius: var(--radius-sm);
    padding: 4px 8px;
    cursor: pointer;
    transition: color 0.1s ease, background 0.1s ease;
  }

  .ctrl-btn:hover {
    color: var(--text-hi);
    background: var(--fill-lo);
  }

  /* ── Status line ── */
  .status-line {
    position: fixed;
    bottom: 0;
    left: 0;
    right: 0;
    height: 24px;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0;
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-subtle);
    background: var(--surface-0);
    border-top: 1px solid var(--stroke-lo);
  }

  .status-segment {
    padding: 0 10px;
  }

  .status-sep {
    color: var(--text-faint);
  }

  /* ── Drag overlay ── */
  .drop-overlay {
    position: fixed;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: oklch(0 0 0 / 60%);
    backdrop-filter: blur(4px);
    z-index: 100;
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
    background: linear-gradient(
      180deg,
      oklch(0.76 0.18 115 / 8%) 0%,
      oklch(0.76 0.18 115 / 4%) 100%
    );
  }

  /* ── Error toast ── */
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
    z-index: 50;
  }

  /* ── Drag state on app ── */
  .dragging {
    outline: 2px solid var(--interactive);
    outline-offset: -2px;
  }
</style>

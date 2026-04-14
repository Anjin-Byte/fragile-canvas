<script lang="ts">
  import { onMount } from "svelte";
  import type { CpuState, EmulatorBackend } from "./types";
  import Screen from "./components/Screen.svelte";
  import { Play, StepForward, SkipForward, Upload, Gamepad2 } from "lucide-svelte";

  // ── Props ──
  let { backend }: { backend: EmulatorBackend } = $props();

  // ── State ──
  let cpu = $state<CpuState | null>(null);
  let loaded = $state(false);
  let error = $state<string | null>(null);
  let running = $state(false);
  let paused = $state(false);
  let dragging = $state(false);
  let fps = $state(0);
  let frameCount = $state(0);

  // ── Non-reactive refs ──
  let runningRef = false;
  let rafRef = 0;
  let screen: Screen;
  let fileInput: HTMLInputElement;
  let fpsFrames = 0;
  let fpsLast = 0;

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

  function onCardClick() {
    fileInput?.click();
  }

  // ── Drag and drop ──
  function onDragOver(e: DragEvent) {
    e.preventDefault();
    dragging = true;
  }

  function onDragLeave() {
    dragging = false;
  }

  async function onDrop(e: DragEvent) {
    e.preventDefault();
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
  onMount(() => {
    const onDown = (e: KeyboardEvent) => handleKey(e, true);
    const onUp = (e: KeyboardEvent) => handleKey(e, false);
    window.addEventListener("keydown", onDown);
    window.addEventListener("keyup", onUp);

    return () => {
      window.removeEventListener("keydown", onDown);
      window.removeEventListener("keyup", onUp);
      runningRef = false;
      cancelAnimationFrame(rafRef);
    };
  });
</script>

<svelte:window
  ondragover={onDragOver}
  ondragleave={onDragLeave}
  ondrop={onDrop}
/>

<div class="app" class:dragging>
  {#if !loaded}
    <!-- ── Hero state ── -->
    <div class="hero">
      <div class="hero-card" role="button" tabindex="0" onclick={onCardClick} onkeydown={(e) => e.key === 'Enter' && onCardClick()}>
        <div class="hero-screen-ghost"></div>
        <h1 class="hero-title">fragile-canvas</h1>
        <p class="hero-prompt"><Upload size={14} /> Drop a ROM anywhere, or click to browse</p>
        <input
          bind:this={fileInput}
          type="file"
          accept=".gb,.gbc,.bin"
          class="hidden-input"
          onchange={onFileSelect}
        />
      </div>
      <div class="hero-divider">or</div>
      <button class="ctrl-btn demo-btn" onclick={loadDefault}>
        <Gamepad2 size={14} /> Play Tobu Tobu Girl
      </button>
    </div>
  {:else}
    <!-- ── Playing state ── -->
    <div class="stage">
      <div class="screen-bezel">
        <div class="bezel-label-strip">
          <span class="bezel-spec-text">DOT MATRIX WITH STEREO SOUND</span>
        </div>
        <div class="bezel-brand">
          <span class="bezel-brand-text">fragile-canvas</span>
        </div>
        <div class="bezel-screen-area">
          <div class="power-led" class:led-on={running}></div>
          <div class="screen-well">
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
    align-items: center;
    justify-content: center;
  }

  .hero-card {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 16px;
    padding: 56px 64px;
    background: linear-gradient(
      180deg,
      oklch(0.25 0.008 250) 0%,
      oklch(0.21 0.008 250) 100%
    );
    border: 1px solid var(--stroke-lo);
    border-top-color: oklch(1 0 0 / 5%);
    border-radius: var(--radius-lg);
    cursor: pointer;
    transition: border-color 0.15s ease, box-shadow 0.15s ease;
    overflow: hidden;
  }

  .hero-card:hover {
    border-color: var(--stroke-mid);
    border-top-color: oklch(1 0 0 / 8%);
  }

  .hero-card:focus-visible {
    outline: none;
    box-shadow: 0 0 0 2px var(--interactive-ring);
  }

  .hero-screen-ghost {
    width: 160px;
    height: 144px;
    border-radius: var(--radius-sm);
    background: oklch(0.06 0.008 115);  /* barely-visible DMG green tint */
    margin-bottom: 8px;
  }

  .hero-title {
    font-family: var(--font-mono);
    font-size: 20px;
    font-weight: 500;
    color: var(--accent);
    letter-spacing: 0.02em;
  }

  .hero-prompt {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
    color: var(--text-subtle);
    font-weight: 400;
  }

  .hidden-input {
    position: absolute;
    opacity: 0;
    width: 0;
    height: 0;
    pointer-events: none;
  }

  .hero-divider {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-faint);
    text-transform: lowercase;
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
  }

  /* ── DMG-inspired bezel ──
     Proportions from the Game Boy DMG-01 hardware, scaled to 35%.
     Ratios: label/screen = 0.23, sides/screen = 0.11, chamfer/width = 0.07.
     The chamfer is on the bezel body bottom-right corner. */
  .screen-bezel {
    position: relative;
    flex-shrink: 0;
    /* DMG shape: subtle radius on top corners and bottom-left,
       larger curved chamfer on bottom-right (matches the real hardware's
       softened diagonal cut). */
    border-radius: 15px 15px 70px 15px;
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

  /* ── Top label strip — darker recessed band (DMG logo area) ──
     Height ratio: 0.23 × screen_h at 80% scale = 82px */
  .bezel-label-strip {
    height: 35px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: linear-gradient(
      180deg,
      oklch(0.17 0.008 250) 0%,
      oklch(0.19 0.008 250) 100%
    );
    box-shadow:
      inset 0 1px 2px oklch(0 0 0 / 20%),
      0 1px 0 0 oklch(1 0 0 / 3%);
    border-bottom: 1px solid oklch(0 0 0 / 12%);
    border-radius: 10px 10px 0 0;
  }

  /* "DOT MATRIX WITH STEREO SOUND" — in the dark strip */
  .bezel-spec-text {
    font-family: var(--font-sans);
    font-size: 7px;
    font-weight: 600;
    letter-spacing: 0.18em;
    text-transform: uppercase;
    color: oklch(0.35 0.005 250);
    user-select: none;
  }

  /* Brand name — on bezel surface below the strip, left-aligned
     like "Nintendo GAME BOY" on the real hardware */
  .bezel-brand {
    padding: 6px 80px 0;
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

  /* ── Screen area — contains LED + well ──
     Padding ratios from DMG at 80%: top 49px, sides 41px, bottom 41px */
  .bezel-screen-area {
    position: relative;
    padding: 30px 80px 10px 80px;
  }

  /* Power LED — left of screen, vertically centered with screen top area. */
  .power-led {
    position: absolute;
    top: 150px;
    left: 30px;
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
    width: 480px;
    height: 432px;
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

  /* ── Bottom area — spec text + decorative ridges ── */
  .bezel-bottom {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 8px 80px 14px;
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

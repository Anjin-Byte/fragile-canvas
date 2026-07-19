// ─── EmuController — reactive emulator session for the workbench ────────────
// Extracts the run-loop/ROM/joypad logic from Emulator.svelte into a runes
// class so every panel reads one reactive source of truth. `frameCount` is
// the refresh heartbeat: it bumps once per rendered frame AND once per
// manual step, so panels can $effect on it (throttled while running,
// immediate when stepping/paused).

import type { CpuState, DisasmLine, EmulatorBackend, BundledRomInfo } from "../types";

/** M-cycles per DMG frame (70224 T-cycles / 4). */
export const MCYCLES_PER_FRAME = 17556;

export class EmuController {
  readonly backend: EmulatorBackend;

  cpu = $state<CpuState | null>(null);
  romLoaded = $state(false);
  running = $state(false);
  paused = $state(false);
  error = $state<string | null>(null);
  fps = $state(0);
  frameCount = $state(0);
  bundledRoms = $state<BundledRomInfo[]>([]);

  #runningRef = false;
  #raf = 0;
  #fpsFrames = 0;
  #fpsLast = 0;
  #screenBlit: ((shades: Uint8Array) => void) | null = null;
  #actionBits = 0;
  #dpadBits = 0;
  /** Last ROM source, kept so reset() can re-load (the web backend's
   *  reset() tears the machine down entirely). */
  #lastRom: { kind: "bytes"; data: ArrayBuffer } | { kind: "bundled"; id: string } | null = null;

  constructor(backend: EmulatorBackend) {
    this.backend = backend;
    this.bundledRoms = backend.listBundledRoms();
  }

  destroy(): void {
    this.#runningRef = false;
    cancelAnimationFrame(this.#raf);
  }

  // ─── Screen hookup ────────────────────────────────────────────────────

  /** ScreenPanel registers the WebGL blit sink here. */
  attachScreen(blit: (shades: Uint8Array) => void): void {
    this.#screenBlit = blit;
  }

  async #drawFrame(): Promise<void> {
    const frame = await this.backend.getFrame();
    if (frame && this.#screenBlit) this.#screenBlit(frame);
  }

  // ─── ROM loading ──────────────────────────────────────────────────────

  async loadRomBytes(data: ArrayBuffer): Promise<void> {
    this.#lastRom = { kind: "bytes", data };
    await this.#load(() => this.backend.loadRom(new Uint8Array(data)));
  }

  async loadBundled(id: string): Promise<void> {
    this.#lastRom = { kind: "bundled", id };
    await this.#load(() => this.backend.loadBundledRom(id));
  }

  async #load(loader: () => Promise<CpuState>): Promise<void> {
    try {
      this.cpu = await loader();
      this.romLoaded = true;
      this.error = null;
      this.paused = false;
      this.start();
    } catch (e) {
      this.error = String(e);
    }
  }

  // ─── Execution control ────────────────────────────────────────────────

  start(): void {
    if (this.#runningRef || !this.romLoaded) return;
    this.#runningRef = true;
    this.running = true;
    this.paused = false;
    this.backend.resetGovernor();
    this.#fpsLast = performance.now();
    this.#fpsFrames = 0;

    let last = performance.now();
    const frame = (now: number) => {
      if (!this.#runningRef) return;
      const dt = now - last;
      last = now;

      this.#fpsFrames++;
      const fpsDt = now - this.#fpsLast;
      if (fpsDt >= 1000) {
        this.fps = Math.round((this.#fpsFrames * 1000) / fpsDt);
        this.#fpsFrames = 0;
        this.#fpsLast = now;
      }

      const elapsedNs = BigInt(Math.round(dt * 1_000_000));
      this.backend
        .tickFrame(elapsedNs)
        .then(async (state) => {
          if (!this.#runningRef) return;
          this.cpu = state;
          this.frameCount++;
          await this.#drawFrame();
          this.#raf = requestAnimationFrame(frame);
        })
        .catch((e) => {
          this.#runningRef = false;
          this.running = false;
          this.error = String(e);
        });
    };
    this.#raf = requestAnimationFrame(frame);
  }

  pause(): void {
    if (!this.#runningRef) return;
    this.#runningRef = false;
    this.running = false;
    this.paused = true;
    cancelAnimationFrame(this.#raf);
  }

  toggle(): void {
    if (this.running) this.pause();
    else this.start();
  }

  /** Step N M-cycles (while paused). Bumps the panel heartbeat. */
  async stepCycles(n: number): Promise<void> {
    if (this.running || !this.romLoaded) return;
    try {
      this.cpu = await this.backend.step(n);
      this.frameCount++;
      await this.#drawFrame();
    } catch (e) {
      this.error = String(e);
    }
  }

  /** Step one full frame's worth of M-cycles. */
  async stepFrame(): Promise<void> {
    await this.stepCycles(MCYCLES_PER_FRAME);
  }

  /** Reset = re-load the last ROM (the backend's reset() is a teardown). */
  async reset(): Promise<void> {
    if (!this.romLoaded || !this.#lastRom) return;
    this.pause();
    try {
      await this.backend.reset();
      const last = this.#lastRom;
      if (last.kind === "bytes") {
        await this.#load(() => this.backend.loadRom(new Uint8Array(last.data)));
      } else {
        await this.#load(() => this.backend.loadBundledRom(last.id));
      }
    } catch (e) {
      this.error = String(e);
    }
  }

  // ─── Panel data access ────────────────────────────────────────────────

  /** Bus read passthrough for inspector panels. */
  read(addr: number, length: number): Promise<number[]> {
    return this.backend.readMemory(addr, length);
  }

  /** Whether the backend can disassemble (needs the sm83-isa export). */
  get canDisasm(): boolean {
    return typeof this.backend.disassemble === "function";
  }

  /** Disassemble `count` instructions from `addr`; [] when unsupported. */
  async disasm(addr: number, count: number): Promise<DisasmLine[]> {
    if (!this.backend.disassemble || !this.romLoaded) return [];
    return this.backend.disassemble(addr, count);
  }

  // ─── Joypad ───────────────────────────────────────────────────────────

  setButton(group: "action" | "dpad", bit: number, pressed: boolean): void {
    if (group === "action") {
      this.#actionBits = pressed ? this.#actionBits | bit : this.#actionBits & ~bit;
    } else {
      this.#dpadBits = pressed ? this.#dpadBits | bit : this.#dpadBits & ~bit;
    }
    this.backend.setButtons(this.#actionBits, this.#dpadBits);
  }
}

// ─── Shared formatting helpers ──────────────────────────────────────────────

export function hex16(n: number): string {
  return n.toString(16).toUpperCase().padStart(4, "0");
}

export function hex8(n: number): string {
  return n.toString(16).toUpperCase().padStart(2, "0");
}

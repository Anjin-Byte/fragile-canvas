import type {
  CpuState,
  DisasmLine,
  EmulatorBackend,
  BundledRomInfo,
  AssembleResult,
  RunResult,
  TickResult,
} from "@fragile-canvas/ui";
import init, { EmulatorWasm } from "../../wasm/pkg/fragile_canvas_wasm";
import { AudioManager } from "./audio";

let emu: EmulatorWasm | null = null;
// Dedicated instance for pure assembly (live diagnostics), so it never
// disturbs the running machine.
let asmEmu: EmulatorWasm | null = null;
let initialized = false;
const audio = new AudioManager();

async function ensureInit() {
  if (!initialized) {
    await init();
    initialized = true;
  }
}

/**
 * Replace the active emulator with a fresh instance, freeing the previous
 * one's wasm memory first. Every `new EmulatorWasm()` allocates its own linear
 * memory; without `free()` each ROM/snippet load leaked the prior machine.
 */
function swapEmu(): EmulatorWasm {
  emu?.free();
  emu = new EmulatorWasm();
  return emu;
}

/**
 * Start/resume audio as a best-effort side effect. Deliberately fire-and-forget:
 * `AudioContext.resume()` on a suspended context hangs indefinitely when there's
 * no live user activation (autoplay policy). If a load method ever `await`s that,
 * a hung resume silently swallows the ROM load — so audio must never gate a load.
 * Worst case here: the ROM runs with no sound until the next real user gesture.
 */
function kickAudio() {
  void audio
    .init()
    .then(() => audio.resume())
    .catch(() => {
      /* audio unavailable — the machine still runs */
    });
}

function drainAndPushAudio() {
  if (!emu) return;
  const wasmSamples = emu.drainAudioSamples();
  if (wasmSamples.length > 0) {
    // Copy from WASM memory into a transferable ArrayBuffer
    const samples = new Float32Array(wasmSamples);
    audio.pushSamples(samples);
  }
}

export const wasmBackend: EmulatorBackend = {
  async loadRom(cartRom: Uint8Array): Promise<CpuState> {
    await ensureInit();
    const m = swapEmu();
    const state = m.loadRom(cartRom) as CpuState; // load first — never gate on audio
    kickAudio();
    return state;
  },

  async loadRomNoBoot(cartRom: Uint8Array): Promise<CpuState> {
    await ensureInit();
    const m = swapEmu();
    const state = m.loadRomNoBoot(cartRom) as CpuState;
    kickAudio();
    return state;
  },

  async loadDefaultRom(): Promise<CpuState> {
    await ensureInit();
    const m = swapEmu();
    const state = m.loadDefaultRom() as CpuState;
    kickAudio();
    return state;
  },

  listBundledRoms(): BundledRomInfo[] {
    // Static list — matches BUNDLED_ROMS in sm83/src/lib.rs.
    // Avoids needing a WASM instance just to enumerate ROMs.
    return [
      { id: "tobu-tobu-girl-dx", title: "Tobu Tobu Girl DX", author: "Tangram Games" },
    ];
  },

  async loadBundledRom(id: string): Promise<CpuState> {
    await ensureInit();
    const m = swapEmu();
    // loadBundledRom is available after WASM rebuild; fall back to loadDefaultRom
    const state = (
      typeof (m as any).loadBundledRom === "function"
        ? (m as any).loadBundledRom(id)
        : m.loadDefaultRom()
    ) as CpuState;
    kickAudio();
    return state;
  },

  async loadBundledRomNoBoot(id: string): Promise<CpuState> {
    await ensureInit();
    const m = swapEmu();
    // Available after a WASM rebuild; fall back to the boot path otherwise.
    let state: CpuState;
    if (typeof (m as any).loadBundledRomNoBoot === "function") {
      state = (m as any).loadBundledRomNoBoot(id) as CpuState;
    } else if (typeof (m as any).loadBundledRom === "function") {
      state = (m as any).loadBundledRom(id) as CpuState;
    } else {
      state = m.loadDefaultRom() as CpuState;
    }
    kickAudio();
    return state;
  },

  async step(ticks: number): Promise<CpuState> {
    if (!emu) throw new Error("no ROM loaded");
    const state = emu.step(ticks) as CpuState;
    drainAndPushAudio();
    return state;
  },

  async stepInstruction(): Promise<CpuState> {
    if (!emu) throw new Error("no ROM loaded");
    // No audio drain — debugger stepping / run-to is fast-forward, not
    // playback (avoids flooding the worklet during run-to's step loop).
    if (typeof (emu as any).stepInstruction !== "function") {
      return emu.step(1) as CpuState; // pre-rebuild fallback
    }
    return (emu as any).stepInstruction() as CpuState;
  },

  async tickFrame(elapsedNs: bigint): Promise<CpuState> {
    if (!emu) throw new Error("no ROM loaded");
    const state = emu.tickFrame(elapsedNs) as CpuState;
    drainAndPushAudio();
    return state;
  },

  async tickFrameUntil(
    elapsedNs: bigint,
    breakpoints: Uint16Array,
    exemptFirst: boolean,
  ): Promise<TickResult> {
    if (!emu) throw new Error("no ROM loaded");
    const res = emu.tickFrameUntil(elapsedNs, breakpoints, exemptFirst) as TickResult;
    drainAndPushAudio();
    return res;
  },

  async setButtons(action: number, direction: number): Promise<void> {
    if (emu) emu.setButtons(action, direction);
  },

  async resetGovernor(): Promise<void> {
    if (emu) emu.resetGovernor();
  },

  async getState(): Promise<CpuState> {
    if (!emu) throw new Error("no ROM loaded");
    return emu.getState() as CpuState;
  },

  async readMemory(addr: number, length: number): Promise<number[]> {
    if (!emu) throw new Error("no ROM loaded");
    return emu.readMemory(addr, length) as number[];
  },

  async reset(): Promise<void> {
    await audio.close();
    if (emu) {
      emu.reset();
      emu = null;
    }
  },

  async getFrame(): Promise<Uint8Array | null> {
    if (!emu) return null;
    const data = emu.getFrame();
    return data ? new Uint8Array(data) : null;
  },

  setMasterVolume(volume: number): void {
    audio.setVolume(volume);
  },

  async disassemble(addr: number, count: number): Promise<DisasmLine[]> {
    if (!emu) throw new Error("no ROM loaded");
    // Available after a WASM rebuild; feature-detect like loadBundledRom.
    if (typeof (emu as any).disassemble !== "function") return [];
    return (emu as any).disassemble(addr, count) as DisasmLine[];
  },

  async assemble(source: string): Promise<AssembleResult> {
    await ensureInit();
    // Pure — needs a wasm instance but no loaded ROM. A dedicated instance
    // keeps live diagnostics from touching the running machine.
    asmEmu ??= new EmulatorWasm();
    return asmEmu.assemble(source) as AssembleResult;
  },

  async loadCode(origin: number, bytes: Uint8Array, entry: number): Promise<CpuState> {
    await ensureInit();
    const m = swapEmu(); // fresh machine per Run
    const state = m.loadCode(origin, bytes, entry) as CpuState;
    kickAudio();
    return state;
  },

  async runCode(
    budget: number,
    breakpoints: Uint16Array,
    lo: number,
    hi: number,
  ): Promise<RunResult> {
    if (!emu) throw new Error("no code loaded");
    const res = emu.runCode(budget, breakpoints, lo, hi) as RunResult;
    drainAndPushAudio(); // in case the snippet produced sound
    return res;
  },
};

import type { CpuState, DisasmLine, EmulatorBackend, BundledRomInfo } from "@fragile-canvas/ui";
import init, { EmulatorWasm } from "../../wasm/pkg/fragile_canvas_wasm";
import { AudioManager } from "./audio";

let emu: EmulatorWasm | null = null;
let initialized = false;
const audio = new AudioManager();

async function ensureInit() {
  if (!initialized) {
    await init();
    initialized = true;
  }
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
    emu = new EmulatorWasm();
    await audio.init();
    await audio.resume();
    return emu.loadRom(cartRom) as CpuState;
  },

  async loadRomNoBoot(cartRom: Uint8Array): Promise<CpuState> {
    await ensureInit();
    emu = new EmulatorWasm();
    await audio.init();
    await audio.resume();
    return emu.loadRomNoBoot(cartRom) as CpuState;
  },

  async loadDefaultRom(): Promise<CpuState> {
    await ensureInit();
    emu = new EmulatorWasm();
    await audio.init();
    await audio.resume();
    return emu.loadDefaultRom() as CpuState;
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
    emu = new EmulatorWasm();
    await audio.init();
    await audio.resume();
    // loadBundledRom is available after WASM rebuild; fall back to loadDefaultRom
    if (typeof (emu as any).loadBundledRom === "function") {
      return (emu as any).loadBundledRom(id) as CpuState;
    }
    return emu.loadDefaultRom() as CpuState;
  },

  async loadBundledRomNoBoot(id: string): Promise<CpuState> {
    await ensureInit();
    emu = new EmulatorWasm();
    await audio.init();
    await audio.resume();
    // Available after a WASM rebuild; fall back to the boot path otherwise.
    if (typeof (emu as any).loadBundledRomNoBoot === "function") {
      return (emu as any).loadBundledRomNoBoot(id) as CpuState;
    }
    if (typeof (emu as any).loadBundledRom === "function") {
      return (emu as any).loadBundledRom(id) as CpuState;
    }
    return emu.loadDefaultRom() as CpuState;
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
};

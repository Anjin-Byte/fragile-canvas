import type { CpuState, EmulatorBackend } from "@fragile-canvas/ui";
import init, { EmulatorWasm } from "../../wasm/pkg/fragile_canvas_wasm";

let emu: EmulatorWasm | null = null;
let initialized = false;

async function ensureInit() {
  if (!initialized) {
    await init();
    initialized = true;
  }
}

export const wasmBackend: EmulatorBackend = {
  async loadRom(cartRom: Uint8Array): Promise<CpuState> {
    await ensureInit();
    emu = new EmulatorWasm();
    return emu.loadRom(cartRom) as CpuState;
  },

  async loadDefaultRom(): Promise<CpuState> {
    await ensureInit();
    emu = new EmulatorWasm();
    return emu.loadDefaultRom() as CpuState;
  },

  async step(ticks: number): Promise<CpuState> {
    if (!emu) throw new Error("no ROM loaded");
    return emu.step(ticks) as CpuState;
  },

  async tickFrame(elapsedNs: bigint): Promise<CpuState> {
    if (!emu) throw new Error("no ROM loaded");
    return emu.tickFrame(elapsedNs) as CpuState;
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
    if (emu) {
      emu.reset();
      emu = null;
    }
  },
};

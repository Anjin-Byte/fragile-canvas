import { invoke } from "@tauri-apps/api/core";
import type { CpuState, EmulatorBackend } from "@fragile-canvas/ui";

export const tauriBackend: EmulatorBackend = {
  async loadRom(cartRom: Uint8Array): Promise<CpuState> {
    return invoke<CpuState>("load_rom", {
      cartRom: Array.from(cartRom),
    });
  },

  async loadDefaultRom(): Promise<CpuState> {
    return invoke<CpuState>("load_default_rom");
  },

  async step(ticks: number): Promise<CpuState> {
    return invoke<CpuState>("step", { ticks });
  },

  async getState(): Promise<CpuState> {
    return invoke<CpuState>("get_state");
  },

  async readMemory(addr: number, length: number): Promise<number[]> {
    return invoke<number[]>("read_memory", { addr, length });
  },

  async reset(): Promise<void> {
    await invoke("reset");
  },
};

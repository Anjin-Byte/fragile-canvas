import { invoke } from "@tauri-apps/api/core";
import type { CpuState, EmulatorBackend, BundledRomInfo } from "@fragile-canvas/ui";
import { AudioManager } from "./audio";

const audio = new AudioManager();

async function drainAndPushAudio() {
  const samples: number[] = await invoke("drain_audio_samples");
  if (samples.length > 0) {
    audio.pushSamples(new Float32Array(samples));
  }
}

export const tauriBackend: EmulatorBackend = {
  async loadRom(cartRom: Uint8Array): Promise<CpuState> {
    await audio.init();
    await audio.resume();
    return invoke<CpuState>("load_rom", {
      cartRom: Array.from(cartRom),
    });
  },

  async loadRomNoBoot(cartRom: Uint8Array): Promise<CpuState> {
    await audio.init();
    await audio.resume();
    return invoke<CpuState>("load_rom_no_boot", {
      cartRom: Array.from(cartRom),
    });
  },

  async loadDefaultRom(): Promise<CpuState> {
    await audio.init();
    await audio.resume();
    return invoke<CpuState>("load_default_rom");
  },

  listBundledRoms(): BundledRomInfo[] {
    // Synchronous — the ROM list is compiled into the binary.
    // Tauri invoke is async, so we cache it eagerly.
    // For now, mirror the static list from the sm83 crate.
    return [
      { id: "tobu-tobu-girl-dx", title: "Tobu Tobu Girl DX", author: "Tangram Games" },
    ];
  },

  async loadBundledRom(id: string): Promise<CpuState> {
    await audio.init();
    await audio.resume();
    return invoke<CpuState>("load_bundled_rom", { id });
  },

  async step(ticks: number): Promise<CpuState> {
    const state = await invoke<CpuState>("step", { ticks });
    await drainAndPushAudio();
    return state;
  },

  async tickFrame(elapsedNs: bigint): Promise<CpuState> {
    const state = await invoke<CpuState>("tick_frame", { elapsedNs: Number(elapsedNs) });
    await drainAndPushAudio();
    return state;
  },

  async setButtons(action: number, direction: number): Promise<void> {
    await invoke("set_buttons", { action, direction });
  },

  async resetGovernor(): Promise<void> {
    await invoke("reset_governor");
  },

  async getState(): Promise<CpuState> {
    return invoke<CpuState>("get_state");
  },

  async readMemory(addr: number, length: number): Promise<number[]> {
    return invoke<number[]>("read_memory", { addr, length });
  },

  async reset(): Promise<void> {
    await audio.close();
    await invoke("reset");
  },

  async getFrame(): Promise<Uint8Array | null> {
    const data = await invoke<number[] | null>("get_frame");
    return data ? new Uint8Array(data) : null;
  },
};

export interface CpuState {
  pc: number;
  sp: number;
  af: number;
  bc: number;
  de: number;
  hl: number;
  ir: number;
  ie: number;
  halted: boolean;
}

export interface BundledRomInfo {
  id: string;
  title: string;
  author: string;
}

export interface DisasmLine {
  addr: number;
  /** Raw bytes as spaced hex, e.g. "CD 61 01". */
  bytes: string;
  /** Formatted instruction, e.g. "CALL $0161". */
  text: string;
  len: number;
}

export interface EmulatorBackend {
  loadRom(cartRom: Uint8Array): Promise<CpuState>;
  /** Load a ROM skipping the boot ROM (starts at PC=0x0100). */
  loadRomNoBoot(cartRom: Uint8Array): Promise<CpuState>;
  loadDefaultRom(): Promise<CpuState>;
  /** List all ROMs bundled into the binary. */
  listBundledRoms(): BundledRomInfo[];
  /** Load a bundled ROM by id. */
  loadBundledRom(id: string): Promise<CpuState>;
  /** Load a bundled ROM by id, skipping the boot ROM. Optional — backends
   *  without the export omit it and the boot toggle falls back to booting. */
  loadBundledRomNoBoot?(id: string): Promise<CpuState>;
  step(ticks: number): Promise<CpuState>;
  /** Execute exactly one instruction. Optional — backends without it fall
   *  back to M-cycle stepping. */
  stepInstruction?(): Promise<CpuState>;
  /** Set master output volume, 0–1. Optional — backends without an audio
   *  gain stage omit it (the Audio menu then shows disabled). */
  setMasterVolume?(volume: number): void;
  tickFrame(elapsedNs: bigint): Promise<CpuState>;
  /** Set joypad button state. action: A=1,B=2,Select=4,Start=8. direction: Right=1,Left=2,Up=4,Down=8. */
  setButtons(action: number, direction: number): Promise<void>;
  resetGovernor(): Promise<void>;
  getState(): Promise<CpuState>;
  readMemory(addr: number, length: number): Promise<number[]>;
  reset(): Promise<void>;
  /** Drain the latest completed PPU frame. Returns 160×144 shade indices (0-3)
   *  or null if no new frame is ready. */
  getFrame(): Promise<Uint8Array | null>;
  /** Disassemble `count` instructions starting at `addr`. Optional —
   *  backends without the sm83-isa export omit it and the DisasmPanel
   *  falls back to its placeholder. */
  disassemble?(addr: number, count: number): Promise<DisasmLine[]>;
}

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

export interface EmulatorBackend {
  loadRom(cartRom: Uint8Array): Promise<CpuState>;
  loadDefaultRom(): Promise<CpuState>;
  step(ticks: number): Promise<CpuState>;
  tickFrame(elapsedNs: bigint): Promise<CpuState>;
  resetGovernor(): Promise<void>;
  getState(): Promise<CpuState>;
  readMemory(addr: number, length: number): Promise<number[]>;
  reset(): Promise<void>;
  /** Drain the latest completed PPU frame. Returns 160×144 shade indices (0-3)
   *  or null if no new frame is ready. */
  getFrame(): Promise<Uint8Array | null>;
}

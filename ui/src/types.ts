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
}

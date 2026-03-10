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
  loadRom(bootRom: Uint8Array, cartRom: Uint8Array): Promise<CpuState>;
  step(ticks: number): Promise<CpuState>;
  getState(): Promise<CpuState>;
  readMemory(addr: number, length: number): Promise<number[]>;
  reset(): Promise<void>;
}

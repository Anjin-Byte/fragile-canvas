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

/**
 * A code location a breakpoint anchors to. Line-anchored bps (set in the
 * editor) follow source edits and re-map on re-assemble; address-anchored bps
 * (set in the disassembly / a file ROM with no source) stay at their address.
 * The source map bridges the two; both project into either gutter.
 */
export type Location = { kind: "line"; line: number } | { kind: "addr"; addr: number };

/** One emitted source line → the byte range it produced. */
export interface SrcSpan {
  /** 1-based source line. */
  line: number;
  addr: number;
  len: number;
}

export interface AsmDiagnostic {
  /** 1-based source line (0 = whole-program). */
  line: number;
  msg: string;
}

export interface AssembleResult {
  ok: boolean;
  /** Lowest segment address of the flattened image, or null if empty. */
  origin: number | null;
  /** Flattened image (ORG gaps NOP-filled), or null if nothing assembled. */
  bytes: number[] | null;
  /** User labels / EQUs → value. */
  symbols: Record<string, number>;
  diagnostics: AsmDiagnostic[];
  /** One span per emitting source line, in source order. */
  sourceMap: SrcSpan[];
}

export type StopReason = "halt" | "breakpoint" | "leftRange" | "selfLoop" | "budget";

/** Result of a bounded snippet run: the final CPU state + why it stopped. */
export interface RunResult {
  state: CpuState;
  stopReason: StopReason;
  steps: number;
}

/** Result of a real-time slice (`tickFrameUntil`) that may stop at a breakpoint. */
export interface TickResult {
  state: CpuState;
  /** Stopped at an enforced breakpoint (paused before executing it). */
  hit: boolean;
  /** Instructions executed this slice. */
  steps: number;
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
  /** Governed real-time tick that stops at an enforced breakpoint. Optional —
   *  backends without it (desktop) fall back to `tickFrame` (no enforcement). */
  tickFrameUntil?(
    elapsedNs: bigint,
    breakpoints: Uint16Array,
    exemptFirst: boolean,
  ): Promise<TickResult>;
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
  /** Assemble SM83 source → bytes + diagnostics + source map. Needs no ROM.
   *  Optional — web implements it; desktop may omit. */
  assemble?(source: string): Promise<AssembleResult>;
  /** Load assembled bytes into a fresh boot-skipped machine, PC=entry. */
  loadCode?(origin: number, bytes: Uint8Array, entry: number): Promise<CpuState>;
  /** Run from the current PC to a stop condition, enforcing the given
   *  breakpoint addresses. Returns the final state + why it stopped. */
  runCode?(budget: number, breakpoints: Uint16Array, lo: number, hi: number): Promise<RunResult>;
}

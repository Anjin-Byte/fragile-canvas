import { beforeEach, describe, expect, it } from "vitest";
import { EmuController } from "./emu.svelte.js";
import type {
  AssembleResult,
  BundledRomInfo,
  CpuState,
  EmulatorBackend,
  RunResult,
  TickResult,
} from "../types";

function cpu(pc: number): CpuState {
  return { pc, sp: 0, af: 0, bc: 0, de: 0, hl: 0, ir: 0, ie: 0, halted: false };
}

/**
 * A stub backend: it never assembles for real — the test hands it a canned
 * `AssembleResult` and `RunResult`, and it records loadCode/reset calls so the
 * snippet lifecycle can be asserted without wasm.
 */
class FakeBackend implements EmulatorBackend {
  cur: CpuState = cpu(0);
  loadCodeCalls: { origin: number; bytes: Uint8Array; entry: number }[] = [];
  resetCount = 0;
  canned: AssembleResult = {
    ok: true,
    origin: 0x100,
    bytes: [0x3e, 0x05, 0x76], // LD A,$05 ; HALT
    symbols: { main: 0x100 },
    diagnostics: [],
    sourceMap: [
      { line: 1, addr: 0x100, len: 2 },
      { line: 2, addr: 0x102, len: 1 },
    ],
  };
  runResult: RunResult = { state: cpu(0x102), stopReason: "halt", steps: 2 };

  async loadRom(): Promise<CpuState> {
    return (this.cur = cpu(0x100));
  }
  async loadRomNoBoot(): Promise<CpuState> {
    return (this.cur = cpu(0x100));
  }
  async loadDefaultRom(): Promise<CpuState> {
    return (this.cur = cpu(0x100));
  }
  listBundledRoms(): BundledRomInfo[] {
    return [];
  }
  async loadBundledRom(): Promise<CpuState> {
    return (this.cur = cpu(0x100));
  }
  async step(): Promise<CpuState> {
    return this.cur;
  }
  // Scripted PC sequence: each stepInstruction advances `cur` to the next PC.
  stepSequence: number[] = [];
  async stepInstruction(): Promise<CpuState> {
    const pc = this.stepSequence.shift();
    if (pc !== undefined) this.cur = cpu(pc);
    return this.cur;
  }
  async tickFrame(): Promise<CpuState> {
    return this.cur;
  }
  // Real-time break engine stub: records the exemption flag it was called with
  // and returns a canned result the test controls.
  tickResult: TickResult = { state: cpu(0), hit: false, steps: 1 };
  lastExemptFirst: boolean | null = null;
  async tickFrameUntil(
    _elapsedNs: bigint,
    _breakpoints: Uint16Array,
    exemptFirst: boolean,
  ): Promise<TickResult> {
    this.lastExemptFirst = exemptFirst;
    return this.tickResult;
  }
  async setButtons(): Promise<void> {}
  async resetGovernor(): Promise<void> {}
  async getState(): Promise<CpuState> {
    return this.cur;
  }
  // Byte-addressable memory the test can seed (opcodes for step-over).
  mem = new Map<number, number>();
  async readMemory(addr: number, length: number): Promise<number[]> {
    return Array.from({ length }, (_, i) => this.mem.get(addr + i) ?? 0);
  }
  async reset(): Promise<void> {
    this.resetCount++;
  }
  async getFrame(): Promise<Uint8Array | null> {
    return null;
  }
  async assemble(): Promise<AssembleResult> {
    return this.canned;
  }
  async loadCode(origin: number, bytes: Uint8Array, entry: number): Promise<CpuState> {
    this.loadCodeCalls.push({ origin, bytes, entry });
    return (this.cur = cpu(entry));
  }
  runCodeBreakpoints: number[] | null = null;
  async runCode(_budget: number, breakpoints: Uint16Array): Promise<RunResult> {
    this.runCodeBreakpoints = [...breakpoints];
    return this.runResult;
  }
}

beforeEach(() => localStorage.clear());

describe("EmuController — assembly snippet lifecycle", () => {
  it("runCurrent enters code mode and blocks free-run", async () => {
    const be = new FakeBackend();
    const emu = new EmuController(be);
    await emu.runCurrent();

    expect(emu.mode).toBe("code");
    expect(emu.romLoaded).toBe(true);
    expect(emu.canFreeRun).toBe(false);
    expect(emu.lastRun).toEqual({ reason: "halt", steps: 2 });

    // start() must no-op in code mode (no RAF free-run of the synthetic cart).
    emu.start();
    expect(emu.running).toBe(false);
  });

  it("reset rewinds the snippet to its entry, paused, without tearing down the backend", async () => {
    const be = new FakeBackend();
    const emu = new EmuController(be);
    await emu.runCurrent();
    expect(emu.cpu?.pc).toBe(0x102); // stopped at HALT

    be.loadCodeCalls.length = 0;
    await emu.reset();

    expect(be.resetCount).toBe(0); // skips the audio-tearing backend.reset()
    expect(be.loadCodeCalls).toHaveLength(1);
    expect(emu.cpu?.pc).toBe(0x100); // entry = `main`
    expect(emu.paused).toBe(true);
    expect(emu.running).toBe(false);
    expect(emu.mode).toBe("code");
    expect(emu.lastRun).toBeNull();
  });

  it("a full cartridge stays free-runnable", async () => {
    const be = new FakeBackend();
    const emu = new EmuController(be);
    await emu.loadRomBytes(new ArrayBuffer(8));
    expect(emu.mode).toBe("rom");
    expect(emu.canFreeRun).toBe(true);
    emu.destroy(); // stop the RAF loop so it can't leak into later tests
  });

  it("launchCurrent enters live mode and free-runs, like a ROM", async () => {
    const be = new FakeBackend();
    const emu = new EmuController(be);
    await emu.launchCurrent();
    expect(emu.mode).toBe("live");
    expect(emu.romLoaded).toBe(true);
    expect(emu.canFreeRun).toBe(true); // Play/Step/Reset apply
    expect(emu.running).toBe(true); // the real-time RAF loop started
    expect(emu.machineLabel).toBe("Editor");
    emu.pause();
  });

  it("reset relaunches a live program from its entry", async () => {
    const be = new FakeBackend();
    const emu = new EmuController(be);
    await emu.launchCurrent();
    emu.pause(); // stop the loop for a deterministic check
    be.loadCodeCalls.length = 0;
    await emu.reset();
    expect(be.resetCount).toBe(0); // skips the audio-tearing backend.reset()
    expect(be.loadCodeCalls).toHaveLength(1);
    expect(emu.cpu?.pc).toBe(0x100); // reloaded at entry (main)
    expect(emu.mode).toBe("live");
    expect(emu.running).toBe(true); // relaunched real-time
    emu.pause();
  });
});

describe("EmuController — source map & breakpoints", () => {
  it("currentSourceLine maps PC to a line and re-derives when the map changes", async () => {
    const be = new FakeBackend();
    const emu = new EmuController(be);

    const before = emu.mapVersion;
    await emu.assemble("whatever");
    expect(emu.mapVersion).toBeGreaterThan(before); // M1: map rebuild is observable

    emu.cpu = cpu(0x100);
    expect(emu.currentSourceLine).toBe(1);
    emu.cpu = cpu(0x102);
    expect(emu.currentSourceLine).toBe(2);
    emu.cpu = cpu(0x500);
    expect(emu.currentSourceLine).toBeNull();
  });

  it("enforcedBreakpoints unions address breakpoints with mapped line breakpoints", async () => {
    const be = new FakeBackend();
    const emu = new EmuController(be);
    await emu.assemble("whatever"); // builds line→addr (line 2 → $102)

    emu.toggleBreakpoint(0x200); // raw address
    emu.toggleLineBreakpoint(2); // line → $102 via the map

    const bps = emu.enforcedBreakpoints().sort((a, b) => a - b);
    expect(bps).toEqual([0x102, 0x200]);
  });
});

describe("EmuController — editor default seeding", () => {
  it("adopts the shipped default on a fresh browser", () => {
    localStorage.clear();
    const emu = new EmuController(new FakeBackend());
    expect(emu.source).toContain("marquee"); // current STARTER_SOURCE
  });

  it("migrates an un-edited legacy default (pre-seed-marker) to the current one", () => {
    localStorage.clear();
    localStorage.setItem(
      "fc-asm-source",
      "; The original Game Boy (DMG) boot ROM — 256 bytes\n  NOP\n",
    );
    // No seed marker — the pre-migration state.
    const emu = new EmuController(new FakeBackend());
    expect(emu.source).not.toContain("boot ROM");
    expect(emu.source).toContain("marquee");
  });

  it("preserves a user's edited source", () => {
    localStorage.clear();
    localStorage.setItem("fc-asm-source", "; my own program\n  LD A, $42\n  HALT\n");
    const emu = new EmuController(new FakeBackend());
    expect(emu.source).toContain("my own program");
  });
});

describe("EmuController — unified breakpoint model", () => {
  it("projects a line breakpoint into the disasm gutter, and an address bp into the editor", async () => {
    localStorage.clear();
    const emu = new EmuController(new FakeBackend());
    await emu.assemble("whatever"); // map: line1→$100/len2, line2→$102/len1

    emu.toggleBreakpointAtLine(2);
    expect(emu.hasBreakpointAtLine(2)).toBe(true);
    expect(emu.hasBreakpointAtAddr(0x102)).toBe(true); // shows in the disasm too

    emu.toggleBreakpointAtAddr(0x100);
    expect(emu.hasBreakpointAtAddr(0x100)).toBe(true);
    expect(emu.hasBreakpointAtLine(1)).toBe(true); // shows in the editor too

    expect(emu.enforcedBreakpoints().sort((a, b) => a - b)).toEqual([0x100, 0x102]);
  });

  it("editor toggles make line anchors; disasm toggles make address anchors", async () => {
    localStorage.clear();
    const emu = new EmuController(new FakeBackend());
    await emu.assemble("whatever");
    emu.toggleBreakpointAtLine(2);
    emu.toggleBreakpointAtAddr(0x500); // outside the map → an address anchor
    const locs = [...emu.breakpointLocations.values()];
    expect(locs).toContainEqual({ kind: "line", line: 2 });
    expect(locs).toContainEqual({ kind: "addr", addr: 0x500 });
    expect(emu.enforcedBreakpoints().sort((a, b) => a - b)).toEqual([0x102, 0x500]);
  });

  it("removing a co-located line+address breakpoint clears it from BOTH gutters", async () => {
    localStorage.clear();
    // Legacy migration can produce both anchors at the same resolved address.
    localStorage.setItem("fc-breakpoints", JSON.stringify([0x102]));
    localStorage.setItem("fc-line-breakpoints", JSON.stringify([2]));
    const emu = new EmuController(new FakeBackend());
    await emu.assemble("whatever"); // addr $102 ⟷ line 2
    expect(emu.hasBreakpointAtLine(2)).toBe(true);
    expect(emu.hasBreakpointAtAddr(0x102)).toBe(true);

    // Click it off in the editor — must remove the co-located address anchor too.
    emu.toggleBreakpointAtLine(2);
    expect(emu.hasBreakpointAtLine(2)).toBe(false);
    expect(emu.hasBreakpointAtAddr(0x102)).toBe(false);
    expect(emu.enforcedBreakpoints()).toEqual([]);
  });

  it("migrates the two legacy stores into the unified v2 store, eagerly", async () => {
    localStorage.clear();
    localStorage.setItem("fc-breakpoints", JSON.stringify([0x200]));
    localStorage.setItem("fc-line-breakpoints", JSON.stringify([2]));
    const emu = new EmuController(new FakeBackend());
    const v2 = JSON.parse(localStorage.getItem("fc-breakpoints-v2")!); // written at construction
    expect(v2).toContainEqual({ kind: "addr", addr: 0x200 });
    expect(v2).toContainEqual({ kind: "line", line: 2 });
    await emu.assemble("whatever");
    expect(emu.enforcedBreakpoints().sort((a, b) => a - b)).toEqual([0x102, 0x200]);
  });

  it("keeps but does not enforce a breakpoint on a non-emitting line", async () => {
    localStorage.clear();
    const emu = new EmuController(new FakeBackend());
    await emu.assemble("whatever"); // only lines 1,2 emit
    emu.toggleBreakpointAtLine(99);
    expect(emu.breakpointLocations.size).toBe(1); // kept in the store
    expect(emu.hasBreakpointAtLine(99)).toBe(true); // shown (panel dims it)
    expect(emu.enforcedBreakpoints()).toEqual([]); // but not enforced
  });
});

describe("EmuController — run-to-cursor (editor)", () => {
  it("runs to the line's mapped address on a loaded machine", async () => {
    localStorage.clear();
    const emu = new EmuController(new FakeBackend());
    await emu.runCurrent(); // bounded run: loads code + map; cpu ends at $102
    expect(await emu.runToLine(2)).toBe(true); // addrForLine(2)=$102, already there
  });

  it("is a no-op for a non-emitting line", async () => {
    localStorage.clear();
    const emu = new EmuController(new FakeBackend());
    await emu.runCurrent();
    expect(await emu.runToLine(99)).toBe(false);
  });

  it("is a no-op with no machine loaded", async () => {
    localStorage.clear();
    const emu = new EmuController(new FakeBackend());
    await emu.assemble("whatever"); // map built, but nothing loaded
    expect(emu.romLoaded).toBe(false);
    expect(await emu.runToLine(2)).toBe(false);
  });

  it("uses the native runner with the target as a breakpoint (no stepping loop → no freeze)", async () => {
    localStorage.clear();
    const be = new FakeBackend();
    const emu = new EmuController(be);
    await emu.runCurrent();
    be.runCodeBreakpoints = null;
    await emu.runToLine(2); // addrForLine(2) = $102
    expect(be.runCodeBreakpoints).toEqual([0x102]); // one native run, target as bp
  });
});

describe("EmuController — source-level stepping", () => {
  // Canned map: line 1 → $100 (len 2, covers $100-$101), line 2 → $102 (len 1).
  async function loaded() {
    localStorage.clear();
    const be = new FakeBackend();
    const emu = new EmuController(be);
    await emu.runCurrent(); // loads code + map, mode=code, running=false
    emu.cpu = cpu(0x100); // park at line 1
    return { be, emu };
  }

  it("stepSourceLine advances to the next source line", async () => {
    const { be, emu } = await loaded();
    be.stepSequence = [0x102]; // one step lands on line 2
    expect(await emu.stepSourceLine()).toBe(2);
    expect(emu.cpu?.pc).toBe(0x102);
  });

  it("steps through instructions on the same line until the line changes", async () => {
    const { be, emu } = await loaded();
    be.stepSequence = [0x101, 0x102]; // $101 is still line 1; $102 is line 2
    expect(await emu.stepSourceLine()).toBe(2);
    expect(be.stepSequence).toHaveLength(0); // consumed both steps
  });

  it("stops at a self-loop instead of spinning", async () => {
    const { be, emu } = await loaded();
    be.stepSequence = [0x100, 0x100, 0x100]; // PC never advances (jr $)
    expect(await emu.stepSourceLine()).toBe(1);
    expect(be.stepSequence).toHaveLength(2); // stopped after the first step
  });

  it("returns null when the step leaves the source map", async () => {
    const { be, emu } = await loaded();
    be.stepSequence = [0x500]; // unmapped
    expect(await emu.stepSourceLine()).toBeNull();
  });

  it("is a no-op while running or with nothing loaded", async () => {
    const { emu } = await loaded();
    emu.running = true;
    expect(await emu.stepSourceLine()).toBeNull();

    localStorage.clear();
    const fresh = new EmuController(new FakeBackend());
    expect(await fresh.stepSourceLine()).toBeNull();
  });

  it("clears a paused-at-breakpoint state (forward progress)", async () => {
    const { be, emu } = await loaded();
    emu.pausedAtBreakpoint = true;
    emu.breakpointPc = 0x100;
    be.stepSequence = [0x102];
    await emu.stepSourceLine();
    expect(emu.pausedAtBreakpoint).toBe(false);
    expect(emu.breakpointPc).toBeNull();
  });

  it("stepOverLine steps a non-CALL like a source step", async () => {
    const { be, emu } = await loaded();
    be.mem.set(0x100, 0x00); // NOP at PC → not a CALL
    be.stepSequence = [0x102];
    be.runCodeBreakpoints = null;
    expect(await emu.stepOverLine()).toBe(2);
    expect(be.runCodeBreakpoints).toBeNull(); // did NOT run-to (it stepped)
  });

  it("stepOverLine runs a CALL to its return address via the native runner", async () => {
    const { be, emu } = await loaded();
    be.mem.set(0x100, 0xcd); // CALL nn at PC
    be.runCodeBreakpoints = null;
    await emu.stepOverLine();
    expect(be.runCodeBreakpoints).toEqual([0x103]); // return address = PC + 3
  });

  it("recognizes every CALL form for step-over", async () => {
    for (const op of [0xcd, 0xc4, 0xcc, 0xd4, 0xdc]) {
      const { be, emu } = await loaded();
      be.mem.set(0x100, op);
      be.runCodeBreakpoints = null;
      await emu.stepOverLine();
      expect(be.runCodeBreakpoints).toEqual([0x103]);
    }
  });

  it("stepOverLine is a no-op with nothing loaded", async () => {
    localStorage.clear();
    const emu = new EmuController(new FakeBackend());
    expect(await emu.stepOverLine()).toBeNull();
  });
});

describe("EmuController — reveal-in-other", () => {
  it("revealInSource resolves an address anchor to its line and signals once", async () => {
    localStorage.clear();
    const emu = new EmuController(new FakeBackend());
    await emu.assemble("whatever");
    const before = emu.sourceRequest.seq;
    emu.revealInSource({ kind: "addr", addr: 0x102 });
    expect(emu.sourceRequest.line).toBe(2);
    expect(emu.sourceRequest.seq).toBe(before + 1);
  });

  it("revealInDisasm resolves a line anchor to its address and signals once", async () => {
    localStorage.clear();
    const emu = new EmuController(new FakeBackend());
    await emu.assemble("whatever");
    const before = emu.disasmRequest.seq;
    emu.revealInDisasm({ kind: "line", line: 2 });
    expect(emu.disasmRequest.addr).toBe(0x102);
    expect(emu.disasmRequest.seq).toBe(before + 1);
  });

  it("is a no-op when the location does not resolve", async () => {
    localStorage.clear();
    const emu = new EmuController(new FakeBackend());
    await emu.assemble("whatever");
    const s = emu.sourceRequest.seq;
    const d = emu.disasmRequest.seq;
    emu.revealInSource({ kind: "addr", addr: 0x999 }); // no source line here
    emu.revealInDisasm({ kind: "line", line: 99 }); // emits no address
    expect(emu.sourceRequest.seq).toBe(s);
    expect(emu.disasmRequest.seq).toBe(d);
  });
});

// ── Real-time break engine: drive the RAF loop deterministically ────────────
/** Capture scheduled rAF callbacks so a test can fire frames one at a time. */
function captureRaf() {
  const cbs: FrameRequestCallback[] = [];
  const origRaf = globalThis.requestAnimationFrame;
  const origCancel = globalThis.cancelAnimationFrame;
  globalThis.requestAnimationFrame = ((cb: FrameRequestCallback) => cbs.push(cb)) as typeof requestAnimationFrame;
  globalThis.cancelAnimationFrame = (() => {}) as typeof cancelAnimationFrame;
  const flush = async () => {
    await new Promise((r) => setTimeout(r, 0));
    await new Promise((r) => setTimeout(r, 0));
  };
  return {
    pending: () => cbs.length,
    async fireNext() {
      const cb = cbs.shift();
      if (!cb) throw new Error("no rAF scheduled");
      cb(performance.now());
      await flush(); // let tickFrameUntil + the .then(#drawFrame) chain settle
    },
    restore() {
      globalThis.requestAnimationFrame = origRaf;
      globalThis.cancelAnimationFrame = origCancel;
    },
  };
}

describe("EmuController — real-time breakpoints", () => {
  it("free-run stops and pauses at a breakpoint, without rescheduling", async () => {
    localStorage.clear();
    const be = new FakeBackend();
    const raf = captureRaf();
    try {
      const emu = new EmuController(be);
      emu.toggleBreakpoint(0x0102);
      be.tickResult = { state: cpu(0x0102), hit: true, steps: 3 };
      await emu.launchCurrent(); // mode live → start() schedules a frame
      await raf.fireNext();

      expect(emu.running).toBe(false);
      expect(emu.paused).toBe(true);
      expect(emu.pausedAtBreakpoint).toBe(true);
      expect(emu.breakpointPc).toBe(0x0102);
      expect(emu.cpu?.pc).toBe(0x0102);
      expect(raf.pending()).toBe(0); // a hit does not reschedule the loop
    } finally {
      raf.restore();
    }
  });

  it("stops at an entry breakpoint on Launch, then exempts it on resume", async () => {
    localStorage.clear();
    const be = new FakeBackend();
    const raf = captureRaf();
    try {
      const emu = new EmuController(be);
      emu.toggleBreakpoint(0x0100); // bp at the entry (main)
      be.tickResult = { state: cpu(0x0100), hit: true, steps: 0 };
      await emu.launchCurrent();
      await raf.fireNext();
      // Fresh Launch does NOT exempt → the entry breakpoint stops immediately.
      expect(be.lastExemptFirst).toBe(false);
      expect(emu.pausedAtBreakpoint).toBe(true);
      expect(emu.breakpointPc).toBe(0x0100);

      // Resume (Play) exempts the first instruction so it makes progress.
      be.tickResult = { state: cpu(0x0100), hit: false, steps: 1 };
      emu.toggle(); // paused → start()
      await raf.fireNext();
      expect(be.lastExemptFirst).toBe(true);
      expect(emu.running).toBe(true);
      emu.pause();
    } finally {
      raf.restore();
    }
  });
});

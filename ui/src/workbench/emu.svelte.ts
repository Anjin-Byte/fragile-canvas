// ─── EmuController — reactive emulator session for the workbench ────────────
// Extracts the run-loop/ROM/joypad logic from Emulator.svelte into a runes
// class so every panel reads one reactive source of truth. `frameCount` is
// the refresh heartbeat: it bumps once per rendered frame AND once per
// manual step, so panels can $effect on it (throttled while running,
// immediate when stepping/paused).

import { SvelteMap } from "svelte/reactivity";
import { STARTER_SOURCE } from "./starterProgram.js";
import type {
  CpuState,
  DisasmLine,
  EmulatorBackend,
  BundledRomInfo,
  AssembleResult,
  Location,
  SrcSpan,
  StopReason,
} from "../types";

/** Unified breakpoint store (Location[]); supersedes the two legacy sets. */
const BP_STORE_KEY = "fc-breakpoints-v2";
/** Legacy keys, read once for migration into `BP_STORE_KEY`. */
const LEGACY_BP_KEY = "fc-breakpoints";
const LEGACY_LINE_BP_KEY = "fc-line-breakpoints";

/** Stable identity for a Location (dedup within a kind). */
function locKey(loc: Location): string {
  return loc.kind === "line" ? `line:${loc.line}` : `addr:${loc.addr}`;
}
const ASM_SOURCE_KEY = "fc-asm-source";
/** The shipped default last written, so a NEW default can supersede an
 *  un-edited old one without ever clobbering real edits. */
const ASM_SEED_KEY = "fc-asm-seed";
/** Header of a previously-shipped default (the DMG boot ROM), so an un-edited
 *  copy left in a browser before the seed marker existed still migrates. */
const LEGACY_SEED_HEADER = "; The original Game Boy (DMG) boot ROM";

/** Entry-label names recognized by `resolveEntry` (case-insensitive). */
const ENTRY_LABELS = ["main", "_start", "start"];

/** Resolve the run entry point: a `main`/`_start`/`start` label if present,
 *  else the image origin. Keeps the assembler pure (no magic entry symbol). */
function resolveEntry(symbols: Record<string, number>, origin: number): number {
  for (const [k, v] of Object.entries(symbols)) {
    if (ENTRY_LABELS.includes(k.toLowerCase())) return v;
  }
  return origin;
}

/** M-cycles per DMG frame (70224 T-cycles / 4). */
export const MCYCLES_PER_FRAME = 17556;

/** Effective multiplier while fast-forward (turbo) is held on. */
export const TURBO_FACTOR = 8;
/**
 * Ceiling on the wall-time handed to a single `tickFrame`, in ns. The clock
 * governor runs *every* cycle it's asked for (no internal cap), so this bounds
 * the synchronous work per tick — protecting against turbo/high-speed and
 * against catch-up bursts after a tab stall.
 */
export const MAX_TICK_NS = 100_000_000; // 100 ms ≈ 6 frames of work

/**
 * Wall-time (ms) scaled by the speed multiplier → clamped tick nanoseconds.
 * The clamp bounds the synchronous work per tick (the clock governor runs
 * every cycle it's given) and guards against negative/NaN dt.
 */
export function scaledTickNs(dtMs: number, factor: number): bigint {
  const ns = Math.min(dtMs * 1_000_000 * factor, MAX_TICK_NS);
  return BigInt(Math.round(Math.max(0, ns) || 0));
}

export class EmuController {
  readonly backend: EmulatorBackend;

  cpu = $state<CpuState | null>(null);
  romLoaded = $state(false);
  running = $state(false);
  paused = $state(false);
  error = $state<string | null>(null);
  fps = $state(0);
  frameCount = $state(0);
  bundledRoms = $state<BundledRomInfo[]>([]);
  /**
   * The single breakpoint store: `Location`s keyed by `locKey`. Editor clicks
   * add line anchors, disassembly clicks add address anchors; the source map
   * bridges them so a breakpoint set in either panel projects into both.
   * Enforced by both engines (bounded `run_code` + real-time `tickFrameUntil`).
   */
  breakpointLocations = $state(new SvelteMap<string, Location>());
  /** Line numbers that carry a breakpoint (line anchors + address anchors whose
   *  address reverse-maps to a line). Drives the editor gutter — O(1) lookup. */
  breakpointLines = $derived.by(() => {
    void this.mapVersion;
    const s = new Set<number>();
    for (const loc of this.breakpointLocations.values()) {
      if (loc.kind === "line") s.add(loc.line);
      else {
        const l = this.lineForAddr(loc.addr);
        if (l != null) s.add(l);
      }
    }
    return s;
  });
  /** Addresses that carry a breakpoint (address anchors + line anchors resolved
   *  to their first byte). Drives the disasm gutter AND enforcement. */
  breakpointAddrs = $derived.by(() => {
    void this.mapVersion;
    const s = new Set<number>();
    for (const loc of this.breakpointLocations.values()) {
      if (loc.kind === "addr") s.add(loc.addr);
      else {
        const a = this.addrForLine(loc.line);
        if (a !== undefined) s.add(a);
      }
    }
    return s;
  });
  /** Latest assemble result (diagnostics/symbols/source map) for the editor. */
  asmResult = $state<AssembleResult | null>(null);
  /** Editor document — the shared source of truth so the palette can run code
   *  without the editor focused. Persisted to localStorage. */
  source = $state<string>("");
  /**
   * How the current machine was loaded:
   * - `rom`  — a cartridge from a file/bundle; free-runs via the transport.
   * - `live` — assembled editor code *launched* into real time; also free-runs
   *   (it's just a ROM whose provenance is the editor).
   * - `code` — an assembled snippet run to a bounded stop (`run_code`); shows a
   *   frozen result and must NOT free-run the synth cart from its HALT PC.
   */
  mode = $state<"rom" | "live" | "code" | null>(null);
  /** Bumped whenever the source map is rebuilt, so `currentSourceLine` (which
   *  reads the plain `#spansByAddr`) re-derives after a live re-assemble. */
  mapVersion = $state(0);
  /** Why the last snippet run stopped, and how many instructions it took. */
  lastRun = $state<{ reason: StopReason; steps: number } | null>(null);
  /** True while the real-time run is paused AT a breakpoint (vs a manual
   *  pause). Drives the debugger's "stopped at breakpoint" state. */
  pausedAtBreakpoint = $state(false);
  /** The address the run stopped at when `pausedAtBreakpoint`, else null. */
  breakpointPc = $state<number | null>(null);
  /** Fast-forward — reactive so the toolbar button reflects it. */
  turbo = $state(false);
  /**
   * Base speed multiplier (from Settings via an applier). Plain field: it is
   * read inside the RAF closure, not in a reactive context.
   */
  speedFactor = 1;
  /** Skip the boot ROM on load (from Settings via an applier). */
  skipBoot = false;
  /**
   * Cross-panel "reveal" requests. Each is a seq-countered signal so a consumer
   * (the panel scrolls; Workbench brings the tab to front) reacts to every
   * request without a shared-clear race. Memory / source line / disasm address.
   */
  memoryRequest = $state<{ addr: number; seq: number }>({ addr: 0, seq: 0 });
  sourceRequest = $state<{ line: number; seq: number }>({ line: 0, seq: 0 });
  disasmRequest = $state<{ addr: number; seq: number }>({ addr: 0, seq: 0 });

  #runningRef = false;
  #raf = 0;
  #fpsFrames = 0;
  #fpsLast = 0;
  /** Whether the next free-run frame exempts its first instruction from the
   *  breakpoint check (armed on resume-from-break; see `start()`). */
  #exemptFirst = false;
  #screenBlit: ((shades: Uint8Array) => void) | null = null;
  #actionBits = 0;
  #dpadBits = 0;
  /** Last thing loaded, kept so reset() can re-load (the web backend's reset()
   *  tears the machine down entirely). A `code` snippet rewinds in place. */
  #lastRom:
    | { kind: "bytes"; data: ArrayBuffer }
    | { kind: "bundled"; id: string }
    | { kind: "code"; origin: number; bytes: number[]; entry: number }
    | null = null;
  /** Source map from the last assemble: line→address, and address-sorted spans
   *  for the reverse (PC→line) lookup. */
  #lineToAddr = new Map<number, number>();
  #spansByAddr: SrcSpan[] = [];

  constructor(backend: EmulatorBackend) {
    this.backend = backend;
    this.bundledRoms = backend.listBundledRoms();
    // Breakpoints: prefer the unified v2 store; else migrate the two legacy
    // sets (addresses → addr anchors, lines → line anchors) and write v2 once.
    try {
      const v2 = localStorage.getItem(BP_STORE_KEY);
      if (v2 != null) {
        for (const loc of JSON.parse(v2) as Location[]) {
          this.breakpointLocations.set(locKey(loc), loc);
        }
      } else {
        const parseNums = (key: string): number[] => {
          try {
            const raw = localStorage.getItem(key);
            return raw ? (JSON.parse(raw) as number[]) : [];
          } catch {
            return [];
          }
        };
        for (const addr of parseNums(LEGACY_BP_KEY)) {
          this.breakpointLocations.set(`addr:${addr}`, { kind: "addr", addr });
        }
        for (const line of parseNums(LEGACY_LINE_BP_KEY)) {
          this.breakpointLocations.set(`line:${line}`, { kind: "line", line });
        }
        this.#saveBreakpoints(); // eager v2 write; legacy keys are now read-only
      }
    } catch {
      /* corrupt/absent — start empty */
    }
    // Prefer the user's saved source, but let a new shipped default replace an
    // un-edited old one: `saved` counts as un-edited when it equals the seed we
    // last wrote (or, pre-seed-marker, matches the legacy boot-ROM header).
    try {
      const saved = localStorage.getItem(ASM_SOURCE_KEY);
      const seed = localStorage.getItem(ASM_SEED_KEY);
      const unedited =
        saved == null ||
        (seed != null && saved === seed) ||
        (seed == null && saved.startsWith(LEGACY_SEED_HEADER));
      if (unedited) {
        this.source = STARTER_SOURCE;
        try {
          localStorage.setItem(ASM_SEED_KEY, STARTER_SOURCE);
        } catch {
          /* best-effort */
        }
      } else {
        this.source = saved ?? STARTER_SOURCE;
        // Record a seed for future migrations of content we're now preserving.
        if (seed == null) {
          try {
            localStorage.setItem(ASM_SEED_KEY, saved ?? STARTER_SOURCE);
          } catch {
            /* best-effort */
          }
        }
      }
    } catch {
      this.source = STARTER_SOURCE;
    }
  }

  destroy(): void {
    this.#runningRef = false;
    cancelAnimationFrame(this.#raf);
  }

  // ─── Screen hookup ────────────────────────────────────────────────────

  /** ScreenPanel registers the WebGL blit sink here. */
  attachScreen(blit: (shades: Uint8Array) => void): void {
    this.#screenBlit = blit;
  }

  async #drawFrame(): Promise<void> {
    const frame = await this.backend.getFrame();
    if (frame && this.#screenBlit) this.#screenBlit(frame);
  }

  // ─── ROM loading ──────────────────────────────────────────────────────

  async loadRomBytes(data: ArrayBuffer): Promise<void> {
    this.#lastRom = { kind: "bytes", data };
    await this.#load(() => this.#loadFileBackend(data));
  }

  async loadBundled(id: string): Promise<void> {
    this.#lastRom = { kind: "bundled", id };
    await this.#load(() => this.#loadBundledBackend(id));
  }

  /** Load file-ROM bytes, honoring `skipBoot`. */
  #loadFileBackend(data: ArrayBuffer): Promise<CpuState> {
    const bytes = new Uint8Array(data);
    return this.skipBoot ? this.backend.loadRomNoBoot(bytes) : this.backend.loadRom(bytes);
  }

  /**
   * Load a bundled ROM, honoring `skipBoot`. Falls back to the boot path when
   * the backend has no no-boot bundled variant (e.g. desktop until its Tauri
   * command lands).
   */
  #loadBundledBackend(id: string): Promise<CpuState> {
    if (this.skipBoot && this.backend.loadBundledRomNoBoot) {
      return this.backend.loadBundledRomNoBoot(id);
    }
    return this.backend.loadBundledRom(id);
  }

  async #load(loader: () => Promise<CpuState>): Promise<void> {
    try {
      this.cpu = await loader();
      this.romLoaded = true;
      this.mode = "rom";
      this.error = null;
      this.paused = false;
      this.#clearBreakPause(); // fresh load: not resuming from a break
      this.start();
    } catch (e) {
      this.error = String(e);
    }
  }

  // ─── Execution control ────────────────────────────────────────────────

  start(): void {
    // `canFreeRun` is false in code mode, so an assembled snippet can be
    // stepped/re-run but never RAF-loops the synthetic cart from its HALT PC.
    if (this.#runningRef || !this.canFreeRun) return;
    // Exempt the first instruction ONLY when resuming from a breakpoint (so
    // Play makes progress instead of instantly re-tripping it). A fresh
    // Launch/Load does NOT exempt, so a breakpoint on the entry PC stops there.
    this.#exemptFirst = this.pausedAtBreakpoint;
    this.pausedAtBreakpoint = false;
    this.breakpointPc = null;
    this.#runningRef = true;
    this.running = true;
    this.paused = false;
    this.backend.resetGovernor();
    this.#fpsLast = performance.now();
    this.#fpsFrames = 0;

    let last = performance.now();
    const frame = (now: number) => {
      if (!this.#runningRef) return;
      const dt = now - last;
      last = now;

      this.#fpsFrames++;
      const fpsDt = now - this.#fpsLast;
      if (fpsDt >= 1000) {
        this.fps = Math.round((this.#fpsFrames * 1000) / fpsDt);
        this.#fpsFrames = 0;
        this.#fpsLast = now;
      }

      // Scale wall-time by the speed multiplier (turbo overrides the base),
      // then clamp so one tick can't run an unbounded number of cycles.
      const factor = this.turbo ? TURBO_FACTOR : this.speedFactor;
      const elapsedNs = scaledTickNs(dt, factor);

      // Enforce breakpoints only when there ARE some and the backend supports
      // the break engine (desktop falls back to the plain, unenforced tick).
      const hasBps = this.breakpointLocations.size > 0;
      const tick: Promise<{ state: CpuState; hit: boolean; steps: number }> =
        hasBps && this.backend.tickFrameUntil
          ? this.backend.tickFrameUntil(
              elapsedNs,
              Uint16Array.from(this.enforcedBreakpoints()),
              this.#exemptFirst,
            )
          : this.backend.tickFrame(elapsedNs).then((state) => ({ state, hit: false, steps: 1 }));

      tick
        .then(async (res) => {
          if (!this.#runningRef) return;
          this.cpu = res.state;
          this.frameCount++;
          // Clear the exemption once we've actually executed something — but
          // NOT after a zero-cycle frame, or a resume could deadlock at the bp.
          if (res.steps > 0) this.#exemptFirst = false;
          if (res.hit) {
            // Stop-and-pause: set the flags BEFORE drawing so the debugger
            // never renders one flush of "running" at the breakpoint PC.
            this.#runningRef = false;
            this.running = false;
            this.paused = true;
            this.pausedAtBreakpoint = true;
            this.breakpointPc = res.state.pc;
            cancelAnimationFrame(this.#raf);
            await this.#drawFrame();
            return;
          }
          await this.#drawFrame();
          if (this.#runningRef) this.#raf = requestAnimationFrame(frame);
        })
        .catch((e) => {
          this.#runningRef = false;
          this.running = false;
          this.error = String(e);
        });
    };
    this.#raf = requestAnimationFrame(frame);
  }

  pause(): void {
    if (!this.#runningRef) return;
    this.#runningRef = false;
    this.running = false;
    this.paused = true;
    cancelAnimationFrame(this.#raf);
  }

  toggle(): void {
    if (this.running) this.pause();
    else this.start();
  }

  /** Step N M-cycles (while paused). Bumps the panel heartbeat. */
  async stepCycles(n: number): Promise<void> {
    if (this.running || !this.romLoaded) return;
    this.#clearBreakPause();
    try {
      this.cpu = await this.backend.step(n);
      this.frameCount++;
      await this.#drawFrame();
    } catch (e) {
      this.error = String(e);
    }
  }

  /** Step one full frame's worth of M-cycles. */
  async stepFrame(): Promise<void> {
    await this.stepCycles(MCYCLES_PER_FRAME);
  }

  /** Step exactly one instruction (while paused). */
  async stepInstruction(): Promise<void> {
    if (this.running || !this.romLoaded || !this.backend.stepInstruction) return;
    this.#clearBreakPause();
    try {
      this.cpu = await this.backend.stepInstruction();
      this.frameCount++;
      await this.#drawFrame();
    } catch (e) {
      this.error = String(e);
    }
  }

  /**
   * Run until PC reaches `addr` (stopping before executing it), or the program
   * halts / self-loops / the `budget` is spent. Returns whether the target was
   * hit, leaving the machine paused there.
   *
   * Uses the NATIVE runner (`run_code` with `addr` as a breakpoint) — one wasm
   * call. The old JS `stepInstruction` loop did up to a million awaited wasm
   * round-trips on the main thread and hard-froze the page whenever the target
   * wasn't reached quickly (e.g. the program HALTs or self-loops first).
   */
  async runTo(addr: number, budget = 5_000_000): Promise<boolean> {
    if (!this.romLoaded || !this.backend.runCode) return false;
    this.pause();
    try {
      // Full-space range: real code never executes at $FFFF (the IE register),
      // so LeftRange can't false-fire — only the breakpoint / halt / self-loop
      // / budget stop the run.
      const res = await this.backend.runCode(budget, Uint16Array.of(addr), 0x0000, 0xffff);
      this.cpu = res.state;
      this.running = false;
      this.paused = true;
      this.frameCount++;
      await this.#drawFrame();
      return res.state.pc === addr;
    } catch (e) {
      this.error = String(e);
      return false;
    }
  }

  get canStepInstruction(): boolean {
    return typeof this.backend.stepInstruction === "function";
  }

  /** Whether the RAF free-run loop may drive this machine. False in code mode:
   *  an assembled snippet must not free-run the synthetic cart from a HALT /
   *  self-loop PC (Run re-assembles instead). */
  get canFreeRun(): boolean {
    return this.romLoaded && this.mode !== "code";
  }

  /** Whether the backend enforces breakpoints during real-time free-run
   *  (desktop lacks the break engine and falls back to unenforced ticks). */
  get canBreakRealtime(): boolean {
    return typeof this.backend.tickFrameUntil === "function";
  }

  /** Forget any "paused at a breakpoint" state — call on any forward progress
   *  (fresh load, step, reset) so the debugger banner doesn't go stale. */
  #clearBreakPause(): void {
    this.pausedAtBreakpoint = false;
    this.breakpointPc = null;
  }

  /** A short label for what the current machine is running, or null. Lets the
   *  UI show provenance (file ROM vs bundled vs editor code) so one transport
   *  over one machine is never ambiguous. */
  get machineLabel(): string | null {
    if (!this.romLoaded || !this.#lastRom) return null;
    switch (this.#lastRom.kind) {
      case "code":
        return "Editor";
      case "bytes":
        return "ROM file";
      case "bundled": {
        const id = this.#lastRom.id;
        return this.bundledRoms.find((r) => r.id === id)?.title ?? "Bundled";
      }
    }
  }

  /**
   * Reset. A file/bundled cartridge re-loads the last ROM (the backend's
   * reset() is a teardown). Assembled editor code re-loads to its entry from
   * the remembered bytes — skipping the audio-tearing backend.reset() since
   * loadCode builds a fresh machine — and then either **relaunches** in real
   * time (it was launched, `live`) or **rewinds paused** (a bounded `code` run:
   * Run re-runs, Reset rewinds).
   */
  async reset(): Promise<void> {
    if (!this.romLoaded || !this.#lastRom) return;
    const wasLive = this.mode === "live";
    this.pause();
    this.#clearBreakPause(); // rewind to entry, not resuming from a break
    const last = this.#lastRom;
    try {
      if (last.kind === "code") {
        if (!this.backend.loadCode) return;
        this.cpu = await this.backend.loadCode(last.origin, Uint8Array.from(last.bytes), last.entry);
        this.romLoaded = true;
        this.lastRun = null;
        this.error = null;
        if (wasLive) {
          this.mode = "live";
          this.start(); // relaunch real-time from entry
        } else {
          this.mode = "code";
          this.running = false;
          this.paused = true;
          this.frameCount++;
          await this.#drawFrame();
        }
        return;
      }
      await this.backend.reset();
      if (last.kind === "bytes") {
        await this.#load(() => this.#loadFileBackend(last.data));
      } else {
        await this.#load(() => this.#loadBundledBackend(last.id));
      }
    } catch (e) {
      this.error = String(e);
    }
  }

  // ─── Panel data access ────────────────────────────────────────────────

  /** Bus read passthrough for inspector panels. */
  read(addr: number, length: number): Promise<number[]> {
    return this.backend.readMemory(addr, length);
  }

  /** Whether the backend can disassemble (needs the sm83-isa export). */
  get canDisasm(): boolean {
    return typeof this.backend.disassemble === "function";
  }

  /** Disassemble `count` instructions from `addr`; [] when unsupported. */
  async disasm(addr: number, count: number): Promise<DisasmLine[]> {
    if (!this.backend.disassemble || !this.romLoaded) return [];
    return this.backend.disassemble(addr, count);
  }

  // ─── Assembly (write → assemble → run) ─────────────────────────────────

  /** Whether the backend can assemble (needs the sm83-isa wasm export). */
  get canAssemble(): boolean {
    return typeof this.backend.assemble === "function";
  }

  /** Assemble the editor document (`this.source`) for live diagnostics. */
  assembleCurrent(): Promise<AssembleResult | null> {
    this.#saveSource();
    return this.assemble(this.source);
  }

  /** Assemble + run the editor document to a bounded stop (fast check). */
  runCurrent(budget?: number): Promise<void> {
    this.#saveSource();
    return this.assembleAndRun(this.source, budget);
  }

  /** Assemble + launch the editor document into real time (like a ROM). */
  launchCurrent(): Promise<void> {
    this.#saveSource();
    return this.assembleAndLaunch(this.source);
  }

  #saveSource(): void {
    try {
      localStorage.setItem(ASM_SOURCE_KEY, this.source);
    } catch {
      /* storage unavailable — best-effort */
    }
  }

  /** Assemble `source` for live diagnostics; stores the result + rebuilds the
   *  source map. Pure — does not touch the running machine. */
  async assemble(source: string): Promise<AssembleResult | null> {
    if (!this.backend.assemble) return null;
    try {
      const r = await this.backend.assemble(source);
      this.asmResult = r;
      this.#rebuildSourceMap(r.sourceMap);
      return r;
    } catch (e) {
      // A real backend throw (assembler panic) — surface it in the problems
      // list as a program-level diagnostic, not the global error toast.
      this.asmResult = {
        ok: false,
        origin: null,
        bytes: null,
        symbols: {},
        diagnostics: [{ line: 0, msg: `assembler error: ${String(e)}` }],
        sourceMap: [],
      };
      this.#rebuildSourceMap([]);
      return null;
    }
  }

  #rebuildSourceMap(map: SrcSpan[]): void {
    const lineToAddr = new Map<number, number>();
    for (const s of map) if (!lineToAddr.has(s.line)) lineToAddr.set(s.line, s.addr);
    this.#lineToAddr = lineToAddr;
    this.#spansByAddr = [...map].sort((a, b) => a.addr - b.addr);
    // Invalidate reactive consumers of the map (currentSourceLine reads the
    // plain #spansByAddr, so it needs this signal to re-derive).
    this.mapVersion++;
  }

  /** The source line whose emitted span contains `addr`, or null. The reusable
   *  reverse (address→line) half of the source map. */
  lineForAddr(addr: number | null | undefined): number | null {
    void this.mapVersion; // re-derive when the map is rebuilt
    const spans = this.#spansByAddr;
    if (addr == null || spans.length === 0) return null;
    let lo = 0;
    let hi = spans.length - 1;
    while (lo <= hi) {
      const mid = (lo + hi) >> 1;
      const s = spans[mid];
      if (addr < s.addr) hi = mid - 1;
      else if (addr >= s.addr + s.len) lo = mid + 1;
      else return s.line;
    }
    return null;
  }

  /** The source line the CPU's PC is currently within, via the source map. */
  get currentSourceLine(): number | null {
    return this.lineForAddr(this.cpu?.pc);
  }

  /**
   * Assemble `source` and, if clean, load it into a fresh boot-skipped machine
   * at its entry. Records `#lastRom` (so Reset can rebuild) and marks the
   * machine loaded. Returns the `[lo, hi)` code range, or null if the source
   * had diagnostics / overflowed the ROM window. Shared by Run and Launch.
   */
  async #assembleAndLoad(source: string): Promise<{ lo: number; hi: number } | null> {
    const r = await this.assemble(source);
    if (!r || r.diagnostics.length > 0 || r.bytes == null || r.origin == null) return null;
    const lo = r.origin;
    const hi = r.origin + r.bytes.length;
    // Pre-check the ROM window here — otherwise the backend allocates a fresh
    // machine before load_code rejects it, leaving the current one destroyed.
    if (hi > 0x8000) {
      this.error = "assembled code exceeds the 32 KiB ROM window";
      return null;
    }
    const entry = resolveEntry(r.symbols, r.origin);
    this.cpu = await this.backend.loadCode!(r.origin, Uint8Array.from(r.bytes), entry);
    // Remember the program so Reset can rebuild it from `entry` (see reset()).
    this.#lastRom = { kind: "code", origin: r.origin, bytes: r.bytes, entry };
    this.romLoaded = true;
    this.#clearBreakPause(); // fresh assemble+load: not resuming from a break
    return { lo, hi };
  }

  /**
   * Assemble `source` and, if it's clean, run it to a bounded stop condition
   * (HALT / self-loop / breakpoint / left-range / budget) and show the frozen
   * result — the fast "does my routine compute X?" path. Diagnostics surface
   * via `asmResult`; a program with errors does not run.
   */
  async assembleAndRun(source: string, budget = 2_000_000): Promise<void> {
    if (!this.backend.assemble || !this.backend.loadCode || !this.backend.runCode) return;
    this.pause();
    try {
      const range = await this.#assembleAndLoad(source);
      if (!range) return;
      this.mode = "code";
      this.running = false;
      this.paused = false;
      const res = await this.backend.runCode(
        budget,
        Uint16Array.from(this.enforcedBreakpoints()),
        range.lo,
        range.hi,
      );
      this.cpu = res.state;
      this.lastRun = { reason: res.stopReason, steps: res.steps };
      this.frameCount++;
      await this.#drawFrame();
    } catch (e) {
      this.error = String(e);
    }
  }

  /**
   * Assemble `source` and, if it's clean, **launch** it into real time — the
   * program runs like a ROM (live frames, joypad, the global transport). The
   * "does my program *do* the thing?" path. Diagnostics surface via `asmResult`;
   * a program with errors does not launch.
   */
  async assembleAndLaunch(source: string): Promise<void> {
    if (!this.backend.assemble || !this.backend.loadCode) return;
    this.pause();
    try {
      const range = await this.#assembleAndLoad(source);
      if (!range) return;
      this.mode = "live"; // free-runnable (canFreeRun is true for "live")
      this.lastRun = null;
      this.error = null;
      this.start(); // hand off to the real-time RAF loop
    } catch (e) {
      this.error = String(e);
    }
  }

  // ─── Breakpoints (one Location store; projected into both gutters) ─────

  /** The enforceable address for a source line (its first emitted byte), or
   *  undefined when the line emits nothing / no map exists. */
  addrForLine(line: number): number | undefined {
    void this.mapVersion;
    return this.#lineToAddr.get(line);
  }

  /** A breakpoint is set on this source line (line anchor, or an address anchor
   *  whose address falls on this line). O(1) via the derived index. */
  hasBreakpointAtLine(line: number): boolean {
    return this.breakpointLines.has(line);
  }

  /** A breakpoint is set at this instruction address (address anchor, or a line
   *  anchor resolved to its first byte). O(1) via the derived index. */
  hasBreakpointAtAddr(addr: number): boolean {
    return this.breakpointAddrs.has(addr);
  }

  /** Editor gutter: toggle a LINE-anchored breakpoint. Removing clears every
   *  Location that resolves to this line (so a co-located address anchor can't
   *  linger as an un-removable ghost). */
  toggleBreakpointAtLine(line: number): void {
    const resolving: string[] = [];
    for (const [k, loc] of this.breakpointLocations) {
      const l = loc.kind === "line" ? loc.line : this.lineForAddr(loc.addr);
      if (l === line) resolving.push(k);
    }
    if (resolving.length > 0) for (const k of resolving) this.breakpointLocations.delete(k);
    else this.breakpointLocations.set(`line:${line}`, { kind: "line", line });
    this.#saveBreakpoints();
  }

  /** Disasm gutter: toggle an ADDRESS-anchored breakpoint. Removing clears every
   *  Location that resolves to this address. */
  toggleBreakpointAtAddr(addr: number): void {
    const resolving: string[] = [];
    for (const [k, loc] of this.breakpointLocations) {
      const a = loc.kind === "addr" ? loc.addr : this.addrForLine(loc.line);
      if (a === addr) resolving.push(k);
    }
    if (resolving.length > 0) for (const k of resolving) this.breakpointLocations.delete(k);
    else this.breakpointLocations.set(`addr:${addr}`, { kind: "addr", addr });
    this.#saveBreakpoints();
  }

  /** Back-compat aliases (existing callers / disasm gutter / editor gutter). */
  toggleBreakpoint(addr: number): void {
    this.toggleBreakpointAtAddr(addr);
  }
  toggleLineBreakpoint(line: number): void {
    this.toggleBreakpointAtLine(line);
  }

  #saveBreakpoints(): void {
    try {
      localStorage.setItem(BP_STORE_KEY, JSON.stringify([...this.breakpointLocations.values()]));
    } catch {
      /* storage unavailable — best-effort */
    }
  }

  /** Addresses the runner enforces — the resolved-address view of the store. */
  enforcedBreakpoints(): number[] {
    return [...this.breakpointAddrs];
  }

  /** Ask the Memory panel to reveal `addr` (and bring its tab to front). */
  requestMemoryView(addr: number): void {
    this.memoryRequest = { addr: addr & 0xffff, seq: this.memoryRequest.seq + 1 };
  }

  /** Reveal a Location in the source editor (its line, mapping an addr anchor
   *  through the source map). No-op if it doesn't resolve to a line. */
  revealInSource(loc: Location): void {
    const line = loc.kind === "line" ? loc.line : this.lineForAddr(loc.addr);
    if (line != null) this.sourceRequest = { line, seq: this.sourceRequest.seq + 1 };
  }

  /** Reveal a Location in the disassembly (its address, mapping a line anchor
   *  through the source map). No-op if it doesn't resolve to an address. */
  revealInDisasm(loc: Location): void {
    const addr = loc.kind === "addr" ? loc.addr : this.addrForLine(loc.line);
    if (addr != null) this.disasmRequest = { addr: addr & 0xffff, seq: this.disasmRequest.seq + 1 };
  }

  /** Run the loaded machine to a source line's address (run-to-cursor from the
   *  editor). Returns whether the target was reached; no-op if the line emits
   *  nothing or no machine is loaded. */
  runToLine(line: number): Promise<boolean> {
    const addr = this.addrForLine(line);
    if (addr === undefined || !this.romLoaded) return Promise.resolve(false);
    return this.runTo(addr);
  }

  // ─── Joypad ───────────────────────────────────────────────────────────

  setButton(group: "action" | "dpad", bit: number, pressed: boolean): void {
    if (group === "action") {
      this.#actionBits = pressed ? this.#actionBits | bit : this.#actionBits & ~bit;
    } else {
      this.#dpadBits = pressed ? this.#dpadBits | bit : this.#dpadBits & ~bit;
    }
    this.backend.setButtons(this.#actionBits, this.#dpadBits);
  }
}

// ─── Shared formatting helpers ──────────────────────────────────────────────

export function hex16(n: number): string {
  return n.toString(16).toUpperCase().padStart(4, "0");
}

export function hex8(n: number): string {
  return n.toString(16).toUpperCase().padStart(2, "0");
}

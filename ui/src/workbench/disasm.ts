// ─── Disassembly windowing ──────────────────────────────────────────────────
// Pure helpers over an injected async disassemble function. Backward motion
// on SM83 is ambiguous (no fixed instruction length), so `prevInstr` scans a
// few bytes back for an alignment that ends exactly at the target. The
// forward window is always synced through a known address, so the current-PC
// row is exact even when the leading context rows are heuristic.

import type { DisasmLine } from "../types";

export type DisasmFn = (addr: number, count: number) => Promise<DisasmLine[]>;

/**
 * Address of the instruction immediately before `addr`. Tries 1..3 bytes
 * back, preferring the longest instruction that ends exactly at `addr`.
 * Falls back to a 1-byte step when nothing aligns (data bytes).
 */
export async function prevInstr(disasm: DisasmFn, addr: number): Promise<number> {
  if (addr <= 0) return 0;
  for (let b = Math.min(3, addr); b >= 1; b--) {
    const start = addr - b;
    const [line] = await disasm(start, 1);
    if (line && line.len === b) return start;
  }
  return addr - 1;
}

/** Walk `n` instructions back from `addr`. */
export async function backUp(disasm: DisasmFn, addr: number, n: number): Promise<number> {
  let a = addr;
  for (let i = 0; i < n && a > 0; i++) {
    a = await prevInstr(disasm, a);
  }
  return a;
}

export interface DisasmWindow {
  lines: DisasmLine[];
  /** Index of the row whose addr === focus, or -1 if not present. */
  focusIndex: number;
}

/**
 * A window of instructions with up to `before` rows of context above
 * `focus` and `after` rows below, guaranteed to contain the `focus` row.
 *
 * Backward context is only reliable while the bytes before `focus` are
 * real code: near a code/data boundary (e.g. the header just below the
 * $0150 entry point) the forward stream from a data anchor can misalign
 * and skip `focus`. We detect that and shrink the context until `focus`
 * reappears — at worst `focus` is the first row (no context above).
 */
export async function windowAround(
  disasm: DisasmFn,
  focus: number,
  before: number,
  after: number,
): Promise<DisasmWindow> {
  for (let b = before; b > 0; b = b > 2 ? b >> 1 : b - 1) {
    const anchor = await backUp(disasm, focus, b);
    const lines = await disasm(anchor, b + after + 1);
    const focusIndex = lines.findIndex((l) => l.addr === focus);
    if (focusIndex >= 0) return { lines, focusIndex };
  }
  // No aligned context available — anchor exactly on focus.
  const lines = await disasm(focus, after + 1);
  return { lines, focusIndex: 0 };
}

/** A plain forward window from `top` (free-scroll mode). */
export async function windowFrom(
  disasm: DisasmFn,
  top: number,
  count: number,
): Promise<DisasmLine[]> {
  return disasm(top, count);
}

// Live listing: turn an assemble result into per-source-line address + bytes,
// so the editor gutter can show what each line assembles to (like an assembler
// listing). Pure — derived entirely from `AssembleResult`.

import type { AssembleResult } from "../types";

export interface LineListing {
  /** 1-based source line. */
  line: number;
  /** Address of the line's first emitted byte. */
  addr: number;
  /** The exact bytes this line emitted. */
  bytes: number[];
}

/** One `LineListing` per emitting source line, in source order. Non-emitting
 *  lines (blank / comment / EQU) contribute nothing. */
export function buildLineListing(asm: AssembleResult | null): LineListing[] {
  if (!asm || asm.origin == null || asm.bytes == null) return [];
  const origin = asm.origin;
  const image = asm.bytes;
  const out: LineListing[] = [];
  for (const s of asm.sourceMap) {
    const start = s.addr - origin;
    out.push({ line: s.line, addr: s.addr, bytes: image.slice(start, start + s.len) });
  }
  return out;
}

/** Index the listing by source line for O(1) gutter lookup. */
export function listingByLine(asm: AssembleResult | null): Map<number, LineListing> {
  const m = new Map<number, LineListing>();
  for (const l of buildLineListing(asm)) m.set(l.line, l);
  return m;
}

/** `$XXXX` — a 4-digit hex address. */
export function formatAddr(addr: number): string {
  return "$" + addr.toString(16).toUpperCase().padStart(4, "0");
}

/** Space-separated hex bytes, truncated to `max` with an ellipsis. */
export function formatBytes(bytes: number[], max = 4): string {
  const shown = bytes.slice(0, max).map((b) => b.toString(16).toUpperCase().padStart(2, "0"));
  return shown.join(" ") + (bytes.length > max ? "…" : "");
}

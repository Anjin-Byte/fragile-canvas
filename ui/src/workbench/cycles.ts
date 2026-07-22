// Canonical SM83 T-cycle counts (Pandocs values), ported faithfully from
// sm83-isa/src/cycles.rs — an algebraic decode over the x/y/z/q opcode fields,
// not a 512-entry table. `taken == notTaken` for everything except conditional
// JR/JP/CALL/RET. Illegal opcodes return 0/0.

export interface Cycles {
  /** T-cycles when a conditional branch is taken (== notTaken otherwise). */
  taken: number;
  notTaken: number;
}

const ILLEGAL_OPCODES = new Set([
  0xd3, 0xdb, 0xdd, 0xe3, 0xe4, 0xeb, 0xec, 0xed, 0xf4, 0xfc, 0xfd,
]);

const both = (t: number): Cycles => ({ taken: t, notTaken: t });
const cond = (taken: number, notTaken: number): Cycles => ({ taken, notTaken });

/** True for a conditional-branch instruction (taken ≠ not-taken cost). */
export function isConditional(c: Cycles): boolean {
  return c.taken !== c.notTaken;
}

/** T-cycles for the instruction whose opcode is `op` (`cb` = second byte when
 *  `op === 0xCB`). */
export function opcodeCycles(op: number, cb?: number): Cycles {
  if (op === 0xcb) return cbCycles(cb ?? 0);
  if (ILLEGAL_OPCODES.has(op)) return both(0);

  const x = op >> 6;
  const y = (op >> 3) & 7;
  const z = op & 7;
  const q = y & 1;

  if (x === 0) {
    switch (z) {
      case 0:
        if (y === 0 || y === 2) return both(4); // NOP, STOP
        if (y === 1) return both(20); // LD (a16),SP
        if (y === 3) return both(12); // JR
        return cond(12, 8); // JR cc
      case 1:
        return q === 0 ? both(12) : both(8); // LD rr,nn / ADD HL,rr
      case 2:
        return both(8); // LD (rr),A / LD A,(rr)
      case 3:
        return both(8); // INC/DEC rr
      case 4:
      case 5:
        return y === 6 ? both(12) : both(4); // INC/DEC r / (HL)
      case 6:
        return y === 6 ? both(12) : both(8); // LD r,n / LD (HL),n
      default:
        return both(4); // rotates A / DAA / CPL / SCF / CCF
    }
  }
  if (x === 1) {
    if (y === 6 && z === 6) return both(4); // HALT
    if (y === 6 || z === 6) return both(8); // LD r,(HL) / LD (HL),r
    return both(4); // LD r,r
  }
  if (x === 2) {
    return z === 6 ? both(8) : both(4); // ALU r / (HL)
  }
  // x === 3
  switch (z) {
    case 0:
      if (y <= 3) return cond(20, 8); // RET cc
      if (y === 4 || y === 6) return both(12); // LDH (a8),A / LDH A,(a8)
      if (y === 5) return both(16); // ADD SP,e8
      return both(12); // LD HL,SP+e8
    case 1:
      if (q === 0) return both(12); // POP
      switch (y >> 1) {
        case 0:
        case 1:
          return both(16); // RET / RETI
        case 2:
          return both(4); // JP HL
        default:
          return both(8); // LD SP,HL
      }
    case 2:
      if (y <= 3) return cond(16, 12); // JP cc
      if (y === 4 || y === 6) return both(8); // LDH (C),A / LDH A,(C)
      return both(16); // LD (a16),A / LD A,(a16)
    case 3:
      return y === 0 ? both(16) : both(4); // JP a16 / DI / EI
    case 4:
      return cond(24, 12); // CALL cc
    case 5:
      return q === 0 ? both(16) : both(24); // PUSH / CALL
    case 6:
      return both(8); // ALU n
    default:
      return both(16); // RST
  }
}

function cbCycles(cb: number): Cycles {
  const x = cb >> 6;
  const z = cb & 7;
  if (z === 6) return x === 1 ? both(12) : both(16); // BIT (HL)=12; RMW (HL)=16
  return both(8);
}

/** T-cycles for the instruction encoded at the start of `bytes` (handles the
 *  0xCB prefix). Empty input → 0/0. */
export function bytesCycles(bytes: number[]): Cycles {
  if (bytes.length === 0) return both(0);
  return bytes[0] === 0xcb ? opcodeCycles(0xcb, bytes[1]) : opcodeCycles(bytes[0]);
}

/** Sum a list of instruction costs (e.g. a multi-line selection). */
export function sumCycles(list: Cycles[]): Cycles {
  return list.reduce((acc, c) => cond(acc.taken + c.taken, acc.notTaken + c.notTaken), both(0));
}

/** M-cycles = T-cycles / 4 (SM83 is always a whole number of M-cycles). */
export const mCycles = (t: number): number => t / 4;

/** Compact label: "8" or "12/8" (taken/not-taken) in T-cycles. */
export function formatCycles(c: Cycles): string {
  return isConditional(c) ? `${c.taken}/${c.notTaken}` : `${c.taken}`;
}

// ─── Game Boy hardware symbol table ─────────────────────────────────────────
// Address → canonical name for I/O registers, interrupt vectors, RST targets,
// and the cartridge entry point. Drives the disassembly comment column and
// jump-target labels. Reusable by other panels (IoPanel etc.).

import type { DisasmLine } from "../types";

export const HARDWARE_SYMBOLS: Map<number, string> = new Map([
  // ── Fixed ROM addresses ──
  [0x0000, "RST_00"],
  [0x0008, "RST_08"],
  [0x0010, "RST_10"],
  [0x0018, "RST_18"],
  [0x0020, "RST_20"],
  [0x0028, "RST_28"],
  [0x0030, "RST_30"],
  [0x0038, "RST_38"],
  [0x0040, "VBlank"],
  [0x0048, "STAT_int"],
  [0x0050, "Timer_int"],
  [0x0058, "Serial_int"],
  [0x0060, "Joypad_int"],
  [0x0100, "entry"],
  // ── I/O registers (FF00-FF7F) ──
  [0xff00, "P1"],
  [0xff01, "SB"],
  [0xff02, "SC"],
  [0xff04, "DIV"],
  [0xff05, "TIMA"],
  [0xff06, "TMA"],
  [0xff07, "TAC"],
  [0xff0f, "IF"],
  [0xff10, "NR10"],
  [0xff11, "NR11"],
  [0xff12, "NR12"],
  [0xff13, "NR13"],
  [0xff14, "NR14"],
  [0xff16, "NR21"],
  [0xff17, "NR22"],
  [0xff18, "NR23"],
  [0xff19, "NR24"],
  [0xff1a, "NR30"],
  [0xff1b, "NR31"],
  [0xff1c, "NR32"],
  [0xff1d, "NR33"],
  [0xff1e, "NR34"],
  [0xff20, "NR41"],
  [0xff21, "NR42"],
  [0xff22, "NR43"],
  [0xff23, "NR44"],
  [0xff24, "NR50"],
  [0xff25, "NR51"],
  [0xff26, "NR52"],
  [0xff40, "LCDC"],
  [0xff41, "STAT"],
  [0xff42, "SCY"],
  [0xff43, "SCX"],
  [0xff44, "LY"],
  [0xff45, "LYC"],
  [0xff46, "DMA"],
  [0xff47, "BGP"],
  [0xff48, "OBP0"],
  [0xff49, "OBP1"],
  [0xff4a, "WY"],
  [0xff4b, "WX"],
  [0xff4d, "KEY1"],
  [0xff4f, "VBK"],
  [0xff50, "BOOT"],
  [0xff51, "HDMA1"],
  [0xff52, "HDMA2"],
  [0xff53, "HDMA3"],
  [0xff54, "HDMA4"],
  [0xff55, "HDMA5"],
  [0xff56, "RP"],
  [0xff68, "BCPS"],
  [0xff69, "BCPD"],
  [0xff6a, "OCPS"],
  [0xff6b, "OCPD"],
  [0xff70, "SVBK"],
  [0xffff, "IE"],
]);

/** Wave RAM occupies FF30-FF3F. */
export function symbolFor(addr: number): string | undefined {
  const named = HARDWARE_SYMBOLS.get(addr);
  if (named) return named;
  if (addr >= 0xff30 && addr <= 0xff3f) return `WAVE_${(addr - 0xff30).toString(16).toUpperCase()}`;
  return undefined;
}

// ─── Instruction reference parsing ──────────────────────────────────────────
// The disassembler's output is a deterministic canonical form, so a small
// regex reliably recovers operand addresses:
//   LDH ($FF44), A   →  mem ref $FF44
//   LD A, ($C000)    →  mem ref $C000
//   JP $0150         →  target $0150
//   JR NZ, $0154     →  target $0154 (JR resolved to absolute by wasm)
//   CALL Z, $2000    →  target $2000
//   RST $38          →  target $0038

const CONTROL_FLOW = /^(JP|JR|CALL|RST)\b/;

function firstHex(text: string): number | undefined {
  // Prefer a parenthesized memory operand; else the last bare $hhhh.
  const mem = text.match(/\(\$([0-9A-Fa-f]{2,4})\)/);
  if (mem) return parseInt(mem[1], 16);
  const bare = text.match(/\$([0-9A-Fa-f]{2,4})/);
  return bare ? parseInt(bare[1], 16) : undefined;
}

/** Absolute target address of a JP/JR/CALL/RST, for jump navigation. */
export function refTarget(line: DisasmLine): number | undefined {
  const m = line.text.match(CONTROL_FLOW);
  if (!m) return undefined;
  if (m[1] === "RST") {
    const t = line.text.match(/\$([0-9A-Fa-f]{2})/);
    return t ? parseInt(t[1], 16) : undefined;
  }
  const t = line.text.match(/\$([0-9A-Fa-f]{2,4})\s*$/);
  return t ? parseInt(t[1], 16) : undefined;
}

/** Comment for a disassembly line: register name, or resolved jump target. */
export function commentFor(line: DisasmLine): string | undefined {
  const target = refTarget(line);
  if (target !== undefined) {
    const sym = symbolFor(target);
    return sym ? `→ ${sym}` : undefined;
  }
  // Memory access: name the referenced register/address.
  const mem = line.text.match(/\(\$([0-9A-Fa-f]{2,4})\)/);
  if (mem) {
    const addr = parseInt(mem[1], 16);
    const sym = symbolFor(addr);
    if (sym) return sym;
  }
  return undefined;
}

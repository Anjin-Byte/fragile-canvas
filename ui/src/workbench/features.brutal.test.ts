// Brutal integration tests for the editor features: adversarial input, cross-
// module invariants, exhaustive opcode sweeps, and a realistic assemble-result
// run through the whole pipeline. Distinct from the per-module unit tests.

import { describe, it, expect } from "vitest";
import { opcodeCycles, bytesCycles, sumCycles } from "./cycles.js";
import { buildLineListing, listingByLine, formatAddr } from "./listing.js";
import { tokenize, isInstructionLine } from "./highlight.js";
import { inlayHints } from "./inlay.js";
import { formatLine, formatSource } from "./format.js";
import type { AssembleResult, SrcSpan } from "../types";

const mkAsm = (
  origin: number | null,
  bytes: number[] | null,
  sourceMap: SrcSpan[],
  symbols: Record<string, number> = {},
): AssembleResult => ({ ok: true, origin, bytes, symbols, diagnostics: [], sourceMap });

// The 16 conditional-branch opcodes (JR/RET/JP/CALL cc) — the only ones whose
// taken and not-taken costs differ.
const CONDITIONAL = new Set([
  0x20, 0x28, 0x30, 0x38, // JR cc
  0xc0, 0xc8, 0xd0, 0xd8, // RET cc
  0xc2, 0xca, 0xd2, 0xda, // JP cc
  0xc4, 0xcc, 0xd4, 0xdc, // CALL cc
]);
const ILLEGAL = new Set([0xd3, 0xdb, 0xdd, 0xe3, 0xe4, 0xeb, 0xec, 0xed, 0xf4, 0xfc, 0xfd]);

// ─── Cycles: exhaustive sweeps + degenerate input ───────────────────────────

it("BRUTAL cycles: every base opcode has a hardware-plausible cost", () => {
  const legal = new Set([4, 8, 12, 16, 20, 24]);
  for (let op = 0; op <= 0xff; op++) {
    const c = opcodeCycles(op);
    if (ILLEGAL.has(op)) {
      expect(c).toEqual({ taken: 0, notTaken: 0 });
      continue;
    }
    expect(legal.has(c.taken), `op $${op.toString(16)} taken=${c.taken}`).toBe(true);
    if (CONDITIONAL.has(op)) expect(c.taken).toBeGreaterThan(c.notTaken);
    else expect(c.taken).toBe(c.notTaken);
  }
});

it("BRUTAL cycles: every CB opcode obeys the (HL)-vs-register rule", () => {
  for (let cb = 0; cb <= 0xff; cb++) {
    const c = opcodeCycles(0xcb, cb).taken;
    const z = cb & 7;
    const x = cb >> 6;
    const expected = z === 6 ? (x === 1 ? 12 : 16) : 8; // BIT (HL)=12, RMW (HL)=16
    expect(c, `CB $${cb.toString(16)}`).toBe(expected);
  }
});

it("BRUTAL cycles: robust to truncated / empty / illegal / data bytes", () => {
  expect(() => bytesCycles([0xcb])).not.toThrow(); // CB with no second byte
  expect(typeof bytesCycles([0xcb]).taken).toBe("number");
  expect(bytesCycles([]).taken).toBe(0);
  expect(bytesCycles([0xd3]).taken).toBe(0); // illegal
  expect(typeof bytesCycles([0x42, 0x99, 0x00]).taken).toBe("number"); // arbitrary data
});

it("BRUTAL cycles: sumCycles is exact across all 256 opcodes, both branches", () => {
  const all = Array.from({ length: 256 }, (_, op) => opcodeCycles(op));
  const total = sumCycles(all);
  expect(total.taken).toBe(all.reduce((a, c) => a + c.taken, 0));
  expect(total.notTaken).toBe(all.reduce((a, c) => a + c.notTaken, 0));
  expect(sumCycles([])).toEqual({ taken: 0, notTaken: 0 });
});

// ─── Listing: malformed spans, gaps, offsets ────────────────────────────────

it("BRUTAL listing: tolerates a span running past the end of the image", () => {
  const r = buildLineListing(mkAsm(0x0000, [0x3e, 0x05], [{ line: 1, addr: 0, len: 8 }]));
  expect(() => r).not.toThrow();
  expect(r[0].bytes).toEqual([0x3e, 0x05]); // clamped to what exists
});

it("BRUTAL listing: absolute addresses across a large ORG gap + non-zero origin", () => {
  const image = new Array(0x8000).fill(0);
  image[0] = 0x11; // at $0200
  image[0x7fff - 0x0200] = 0x22; // at $7FFF
  const r = listingByLine(
    mkAsm(0x0200, image, [
      { line: 1, addr: 0x0200, len: 1 },
      { line: 9, addr: 0x7fff, len: 1 },
    ]),
  );
  expect(r.get(1)).toEqual({ line: 1, addr: 0x0200, bytes: [0x11] });
  expect(r.get(9)).toEqual({ line: 9, addr: 0x7fff, bytes: [0x22] });
  expect(formatAddr(0x7fff)).toBe("$7FFF");
});

// ─── Highlight: lossless over pathological input ────────────────────────────

const PATHOLOGICAL = [
  '\tLD   A, $05   ; ok',
  'DB "a;b", $00 ; real',
  'DB "unterminated ; not a comment',
  '; a comment with "fake string and ; semicolon',
  'RET:  RET',
  'HL HLX hl.local',
  'DB $ , % , @ , 0x , 0xZ , 42abc',
  'main:JR NZ,loop',
  'X = $FF00+C',
  '   ',
  '',
  'LDH ($FF00+C), A',
  'SCREEN\tEQU\t$9800\r',
];

it("BRUTAL highlight: tokenizer is lossless over every pathological line", () => {
  for (const line of PATHOLOGICAL) {
    expect(tokenize(line).map((t) => t.text).join(""), JSON.stringify(line)).toBe(line);
  }
});

it("BRUTAL highlight: strings and comments swallow their contents", () => {
  // ; inside a string is not a comment.
  expect(tokenize('DB "a;b"').some((t) => t.kind === "comment")).toBe(false);
  // An unterminated string runs to end-of-line as one token.
  const un = tokenize('DB "abc');
  expect(un.find((t) => t.kind === "string")?.text).toBe('"abc');
  // A comment swallows a fake string opener.
  const cm = tokenize('NOP ; "not a string');
  expect(cm.filter((t) => t.kind === "string")).toHaveLength(0);
  expect(cm.find((t) => t.kind === "comment")?.text).toBe('; "not a string');
});

it("BRUTAL highlight: keyword / label / identifier resolution is unambiguous", () => {
  const kind = (line: string, text: string) =>
    tokenize(line).find((t) => t.text === text)?.kind;
  expect(kind("RET: RET", "RET")).toBe("label"); // colon wins → label token "RET"
  expect(tokenize("RET: RET").filter((t) => t.text === "RET").map((t) => t.kind)).toEqual([
    "label",
    "mnemonic",
  ]);
  expect(kind("LD HL, HLX", "HLX")).toBe("ident"); // HLX is not a register
  expect(kind("LD HL, HLX", "HL")).toBe("register");
  expect(kind("x equ 5", "equ")).toBe("directive");
});

it("BRUTAL highlight: degenerate number literals don't crash and stay lossless", () => {
  for (const line of ["DB $", "DB $G", "DB %", "DB %2", "DB 0x", "DB @", "DB 42x"]) {
    const toks = tokenize(line);
    expect(toks.map((t) => t.text).join("")).toBe(line);
    expect(toks[0].kind).toBe("directive");
  }
});

// ─── Inlay: cross-checked with the tokenizer's comment/string handling ───────

it("BRUTAL inlay: never hints a symbol appearing only in a comment or string", () => {
  const syms = { SCREEN: 0x9800, loop: 0x0154 };
  expect(inlayHints('  DB "SCREEN"\n', syms)).toEqual([]);
  expect(inlayHints("  NOP ; jump to loop and SCREEN\n", syms)).toEqual([]);
});

it("BRUTAL inlay: whole-token match only — no substring hinting", () => {
  expect(inlayHints("  LDH (rLCDC), A\n", { LCD: 0x40 })).toEqual([]); // LCD ⊄ rLCDC as a token
  expect(inlayHints("  LDH (rLCDC), A\n", { rLCDC: 0x40 })).toHaveLength(1);
});

it("BRUTAL inlay: case-sensitive, and definition sites are suppressed but later uses hint", () => {
  // Distinct symbols by case; only the exact one referenced hints.
  expect(inlayHints("  LD A, SCREEN\n", { screen: 0x1 })).toEqual([]);
  // EQU/= definition suppressed; the forward reference on the next line hints.
  const h = inlayHints("X EQU $FF\n        LD A, X\n", { X: 0xff });
  expect(h.map((x) => x.line)).toEqual([2]);
});

// ─── Format: idempotency + semantics preservation ───────────────────────────

const FORMAT_CASES = [
  "\tmain:LD  A,$05\t; go",
  "   ; indented comment",
  "loop:  ; label with a comment, no code",
  'DB "a;b" ; real',
  "verylonglabelname: NOP",
  "",
  "        ORG $0150",
  "\tJR   NZ ,  loop",
];

it("BRUTAL format: idempotent over a battery of nasty lines", () => {
  for (const line of FORMAT_CASES) {
    const once = formatLine(line);
    expect(formatLine(once), JSON.stringify(line)).toBe(once);
  }
});

it("BRUTAL format: preserves the code's token kinds (never corrupts semantics)", () => {
  const nonWs = (s: string) => tokenize(s).filter((t) => t.kind !== "ws").map((t) => t.kind);
  for (const line of FORMAT_CASES) {
    expect(nonWs(formatLine(line)), JSON.stringify(line)).toEqual(nonWs(line));
  }
});

it("BRUTAL format: a string with a semicolon plus a trailing comment aligns", () => {
  const out = formatLine('DB "a;b" ; real');
  expect(out).toContain('DB "a;b"'); // string kept whole
  expect(out.indexOf("; real")).toBe(32); // comment aligned
});

// ─── Cross-feature integration ──────────────────────────────────────────────

it("BRUTAL integration: formatting is stable under re-highlight and re-format", () => {
  const program = FORMAT_CASES.join("\n");
  const formatted = formatSource(program);
  // Re-formatting is a fixpoint...
  expect(formatSource(formatted)).toBe(formatted);
  // ...and every formatted line still tokenizes losslessly.
  for (const line of formatted.split("\n")) {
    expect(tokenize(line).map((t) => t.text).join("")).toBe(line);
  }
});

it("BRUTAL integration: a realistic assemble result agrees across listing, cycles, inlay", () => {
  // main: LD A,$05 ; INC A ; JR NZ,main ; HALT   at $0150, with a label `main`.
  const bytes = [0x3e, 0x05, 0x3c, 0x20, 0xfb, 0x76];
  const asm = mkAsm(
    0x0150,
    bytes,
    [
      { line: 1, addr: 0x0150, len: 2 }, // main: LD A,$05
      { line: 2, addr: 0x0152, len: 1 }, // INC A
      { line: 3, addr: 0x0153, len: 2 }, // JR NZ, main
      { line: 4, addr: 0x0155, len: 1 }, // HALT
    ],
    { main: 0x0150 },
  );
  const l = listingByLine(asm);
  expect(l.get(1)!.addr).toBe(0x0150);
  expect(l.get(3)!.addr).toBe(0x0153);
  // cycles from each line's bytes: LD A,n=8, INC A=4, JR cc=12/8, HALT=4
  expect(bytesCycles(l.get(1)!.bytes)).toEqual({ taken: 8, notTaken: 8 });
  expect(bytesCycles(l.get(2)!.bytes)).toEqual({ taken: 4, notTaken: 4 });
  expect(bytesCycles(l.get(3)!.bytes)).toEqual({ taken: 12, notTaken: 8 });
  expect(bytesCycles(l.get(4)!.bytes)).toEqual({ taken: 4, notTaken: 4 });
  // inlay: the JR references `main` → $0150
  const src = "main:   LD A, $05\n        INC A\n        JR NZ, main\n        HALT\n";
  const hints = inlayHints(src, asm.symbols);
  expect(hints.map((h) => [h.line, h.symbol, h.value])).toEqual([[3, "main", 0x0150]]);
});

it("BRUTAL isInstructionLine: distinguishes code from directives / labels / comments", () => {
  expect(isInstructionLine("        NOP")).toBe(true);
  expect(isInstructionLine("main:   LD A, B")).toBe(true);
  expect(isInstructionLine(".loop:  JR .loop")).toBe(true);
  expect(isInstructionLine("main:")).toBe(false); // label only
  expect(isInstructionLine("        DB $FF")).toBe(false); // directive
  expect(isInstructionLine("SCREEN EQU $9800")).toBe(false);
  expect(isInstructionLine("        ; comment")).toBe(false);
  expect(isInstructionLine("")).toBe(false);
});

it("BRUTAL integration: a selection cycle-sum equals the per-line total", () => {
  const bytes = [0x3e, 0x05, 0x3c, 0x20, 0xfb, 0x76];
  const l = listingByLine(
    mkAsm(0x0150, bytes, [
      { line: 1, addr: 0x0150, len: 2 },
      { line: 2, addr: 0x0152, len: 1 },
      { line: 3, addr: 0x0153, len: 2 },
      { line: 4, addr: 0x0155, len: 1 },
    ]),
  );
  const costs = [1, 2, 3, 4].map((n) => bytesCycles(l.get(n)!.bytes));
  const total = sumCycles(costs);
  expect(total.taken).toBe(8 + 4 + 12 + 4);
  expect(total.notTaken).toBe(8 + 4 + 8 + 4);
});

import { describe, it, expect } from "vitest";
import { inlayHints, formatHintValue } from "./inlay.js";

const SYMS = { SCREEN: 0x9800, loop: 0x0154, COUNT: 8, rLCDC: 0x40 };

describe("inlay — hints on references", () => {
  it("hints a label reference with its resolved value", () => {
    const h = inlayHints("        LD HL, SCREEN\n", SYMS);
    expect(h).toHaveLength(1);
    expect(h[0]).toMatchObject({ line: 1, symbol: "SCREEN", value: 0x9800 });
  });

  it("hints an EQU-constant reference", () => {
    const h = inlayHints("        LD B, COUNT\n", SYMS);
    expect(h.map((x) => x.symbol)).toEqual(["COUNT"]);
    expect(h[0].value).toBe(8);
  });

  it("hints a branch-target reference", () => {
    const h = inlayHints("        JR NZ, loop\n", SYMS);
    expect(h.map((x) => x.symbol)).toEqual(["loop"]);
    expect(h[0].value).toBe(0x0154);
  });

  it("does NOT hint the name being defined by EQU", () => {
    expect(inlayHints("SCREEN EQU $9800\n", SYMS)).toEqual([]);
  });

  it("does NOT hint the name being defined by `=`", () => {
    expect(inlayHints("COUNT = 8\n", SYMS)).toEqual([]);
  });

  it("does NOT hint a label definition (foo:)", () => {
    // `loop:` is a label token, not an ident reference.
    expect(inlayHints("loop:  DEC B\n", SYMS)).toEqual([]);
  });

  it("hints every reference on a line (multiple)", () => {
    const h = inlayHints("        LD HL, SCREEN\n        LD DE, loop\n", SYMS);
    expect(h.map((x) => [x.line, x.symbol])).toEqual([
      [1, "SCREEN"],
      [2, "loop"],
    ]);
  });

  it("hints two references on one line", () => {
    const h = inlayHints("  LDH (rLCDC), A  ; SCREEN\n", { ...SYMS });
    // rLCDC is referenced; SCREEN only appears in a comment (not tokenized as ident).
    expect(h.map((x) => x.symbol)).toEqual(["rLCDC"]);
  });

  it("ignores unknown identifiers (not in the symbol table)", () => {
    expect(inlayHints("        JR NZ, nowhere\n", SYMS)).toEqual([]);
  });

  it("reports a 1-based line and the column just after the token", () => {
    const src = "\nLD HL, SCREEN"; // reference on line 2
    const h = inlayHints(src, SYMS);
    expect(h[0].line).toBe(2);
    // col is the index just past "SCREEN" in "LD HL, SCREEN"
    expect(h[0].col).toBe("LD HL, SCREEN".length);
  });

  it("does not hint identifiers inside comments or strings", () => {
    expect(inlayHints('  DB "SCREEN"  ; loop\n', SYMS)).toEqual([]);
  });
});

describe("inlay — value formatting", () => {
  it("formats addresses wide and small constants narrow", () => {
    expect(formatHintValue(0x9800)).toBe("$9800");
    expect(formatHintValue(0x40)).toBe("$40");
    expect(formatHintValue(8)).toBe("$08");
    expect(formatHintValue(-1)).toBe("$FFFF");
  });
});

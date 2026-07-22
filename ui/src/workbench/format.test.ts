import { describe, it, expect } from "vitest";
import { formatLine, formatSource } from "./format.js";

describe("format — column alignment", () => {
  it("aligns a bare mnemonic to column 8", () => {
    expect(formatLine("  ORG  $0150")).toBe("        ORG $0150");
    expect(formatLine("NOP")).toBe("        NOP");
  });

  it("places a label at column 0 and its mnemonic at column 8", () => {
    expect(formatLine("main:LD A,$05")).toBe("main:   LD A,$05");
  });

  it("aligns the comment to column 32", () => {
    const out = formatLine("NOP ; go");
    expect(out.indexOf("; go")).toBe(32);
    expect(out.startsWith("        NOP")).toBe(true);
  });

  it("gives a single space after an over-long label", () => {
    expect(formatLine("verylonglabel: NOP")).toBe("verylonglabel: NOP");
  });

  it("keeps a label-only line intact", () => {
    expect(formatLine("loop:")).toBe("loop:");
  });

  it("left-aligns a full-line comment", () => {
    expect(formatLine("      ; a note")).toBe("; a note");
  });

  it("preserves blank and whitespace-only lines as blank", () => {
    expect(formatLine("")).toBe("");
    expect(formatLine("   ")).toBe("");
  });

  it("formats a directive with a string operand", () => {
    expect(formatLine('  DB "GB!", $00')).toBe('        DB "GB!", $00');
  });

  it("never splits a semicolon inside a string as a comment", () => {
    expect(formatLine('DB "a;b"')).toBe('        DB "a;b"');
  });

  it("does not rewrite operand internals (only aligns columns)", () => {
    // Comma spacing inside operands is preserved verbatim.
    expect(formatLine("main:  LD  A,$05")).toBe("main:   LD A,$05");
  });
});

describe("format — whole document", () => {
  const program = [
    "main:LD A,$05",
    "  LD B, $03",
    "        ADD A,B ; sum",
    "",
    "loop: JR loop",
  ].join("\n");

  it("aligns every line of a program", () => {
    const out = formatSource(program).split("\n");
    expect(out[0]).toBe("main:   LD A,$05");
    expect(out[1]).toBe("        LD B, $03");
    expect(out[2].startsWith("        ADD A,B")).toBe(true);
    expect(out[2].indexOf("; sum")).toBe(32);
    expect(out[3]).toBe(""); // blank preserved
    expect(out[4]).toBe("loop:   JR loop");
  });

  it("is idempotent (formatting formatted output is a no-op)", () => {
    const once = formatSource(program);
    expect(formatSource(once)).toBe(once);
  });
});

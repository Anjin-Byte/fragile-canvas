import { describe, it, expect } from "vitest";
import { tokenize, type TokenKind } from "./highlight.js";

/** The kinds present, and the concatenation, for compact assertions. */
function kinds(line: string): TokenKind[] {
  return tokenize(line).map((t) => t.kind);
}
function pick(line: string, kind: TokenKind): string[] {
  return tokenize(line)
    .filter((t) => t.kind === kind)
    .map((t) => t.text);
}

describe("highlight — tokenizer", () => {
  it("is lossless: joined token text reproduces the line", () => {
    for (const line of [
      "\tLD   A, $05      ; load",
      'DB "GB!", $00',
      "main:  JR NZ, loop",
      "SCREEN EQU $9800",
      "",
      "   ",
    ]) {
      expect(tokenize(line).map((t) => t.text).join("")).toBe(line);
    }
  });

  it("classifies a comment to end of line", () => {
    expect(pick("NOP ; hello world", "comment")).toEqual(["; hello world"]);
  });

  it("keeps a semicolon inside a string out of the comment", () => {
    const toks = tokenize('DB "a;b", $00');
    expect(pick('DB "a;b", $00', "string")).toEqual(['"a;b"']);
    expect(toks.some((t) => t.kind === "comment")).toBe(false);
  });

  it("recognizes a label definition (ident + colon)", () => {
    const toks = tokenize("loop:  DEC B");
    expect(toks[0]).toEqual({ text: "loop", kind: "label" });
    expect(toks[1]).toEqual({ text: ":", kind: "punct" });
  });

  it("recognizes directives (case-insensitive)", () => {
    expect(pick("ORG $150", "directive")).toEqual(["ORG"]);
    expect(pick("db 1", "directive")).toEqual(["db"]);
    expect(pick("X equ 5", "directive")).toEqual(["equ"]);
  });

  it("recognizes mnemonics (case-insensitive)", () => {
    expect(pick("LD A,B", "mnemonic")).toEqual(["LD"]);
    expect(pick("jr nz, x", "mnemonic")).toEqual(["jr"]);
    expect(pick("RETI", "mnemonic")).toEqual(["RETI"]);
  });

  it("recognizes registers and conditions", () => {
    expect(pick("LD HL, SP", "register")).toEqual(["HL", "SP"]);
    expect(pick("JR NZ, x", "register")).toEqual(["NZ"]);
    expect(pick("ADD A, B", "register")).toEqual(["A", "B"]);
  });

  it("recognizes number literals: $hex, %bin, 0x, decimal, @", () => {
    expect(pick("LD A, $FF", "number")).toEqual(["$FF"]);
    expect(pick("DB %1010", "number")).toEqual(["%1010"]);
    expect(pick("DB 0x2A", "number")).toEqual(["0x2A"]);
    expect(pick("DB 42", "number")).toEqual(["42"]);
    expect(pick("DW @", "number")).toEqual(["@"]);
  });

  it("treats % as an operator when not followed by a binary digit", () => {
    const toks = tokenize("DB 7 % 2");
    const pct = toks.find((t) => t.text === "%");
    expect(pct?.kind).toBe("punct");
  });

  it("treats an unknown identifier as an ident (label reference)", () => {
    expect(pick("JR NZ, loop", "ident")).toEqual(["loop"]);
    expect(pick("LD HL, SCREEN", "ident")).toEqual(["SCREEN"]);
  });

  it("emits punctuation for commas, parens, and operators", () => {
    expect(pick("LD (HL), A", "punct")).toEqual(["(", ")", ","]);
    expect(pick("DB $FF00+C", "punct")).toEqual(["+"]);
  });

  it("tokenizes a full instruction line into the expected kind sequence", () => {
    // "\tLD A, $05 ; go"
    expect(kinds("\tLD A, $05 ; go")).toEqual([
      "ws", // \t
      "mnemonic", // LD
      "ws",
      "register", // A
      "punct", // ,
      "ws",
      "number", // $05
      "ws",
      "comment", // ; go
    ]);
  });
});

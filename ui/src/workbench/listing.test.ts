import { describe, it, expect } from "vitest";
import { buildLineListing, listingByLine, formatAddr, formatBytes } from "./listing.js";
import type { AssembleResult, SrcSpan } from "../types";

function asm(
  origin: number | null,
  bytes: number[] | null,
  sourceMap: SrcSpan[],
): AssembleResult {
  return { ok: true, origin, bytes, symbols: {}, diagnostics: [], sourceMap };
}

describe("listing — buildLineListing", () => {
  it("slices each line's bytes out of the flattened image", () => {
    // $0150: LD A,$05 ; $0152: LD B,$03 ; $0154: ADD A,B ; $0155: HALT
    const r = buildLineListing(
      asm(0x0150, [0x3e, 0x05, 0x06, 0x03, 0x80, 0x76], [
        { line: 2, addr: 0x0150, len: 2 },
        { line: 3, addr: 0x0152, len: 2 },
        { line: 4, addr: 0x0154, len: 1 },
        { line: 5, addr: 0x0155, len: 1 },
      ]),
    );
    expect(r).toEqual([
      { line: 2, addr: 0x0150, bytes: [0x3e, 0x05] },
      { line: 3, addr: 0x0152, bytes: [0x06, 0x03] },
      { line: 4, addr: 0x0154, bytes: [0x80] },
      { line: 5, addr: 0x0155, bytes: [0x76] },
    ]);
  });

  it("handles a multi-byte DB line", () => {
    const r = buildLineListing(asm(0x0000, [0xde, 0xad, 0xbe, 0xef], [{ line: 1, addr: 0, len: 4 }]));
    expect(r[0].bytes).toEqual([0xde, 0xad, 0xbe, 0xef]);
  });

  it("handles a DW line (little-endian, 2 bytes)", () => {
    const r = buildLineListing(asm(0x0000, [0x34, 0x12], [{ line: 1, addr: 0, len: 2 }]));
    expect(r[0].bytes).toEqual([0x34, 0x12]);
  });

  it("handles a DS reservation of many bytes", () => {
    const r = buildLineListing(asm(0x0000, [0, 0, 0, 0, 0], [{ line: 1, addr: 0, len: 5 }]));
    expect(r[0].bytes).toHaveLength(5);
  });

  it("resolves absolute addresses across an ORG gap", () => {
    // DB $AA @ $00 ; (gap) ; DB $BB @ $04 — flatten fills the gap.
    const r = buildLineListing(
      asm(0x0000, [0xaa, 0xff, 0xff, 0xff, 0xbb], [
        { line: 1, addr: 0x00, len: 1 },
        { line: 3, addr: 0x04, len: 1 },
      ]),
    );
    expect(r[0]).toEqual({ line: 1, addr: 0x00, bytes: [0xaa] });
    expect(r[1]).toEqual({ line: 3, addr: 0x04, bytes: [0xbb] });
  });

  it("offsets correctly when origin is non-zero", () => {
    const r = buildLineListing(asm(0x0200, [0x11, 0x22, 0x33], [{ line: 1, addr: 0x0201, len: 1 }]));
    expect(r[0].bytes).toEqual([0x22]); // image[0x0201-0x0200] = image[1]
  });

  it("returns [] when nothing assembled (null origin/bytes)", () => {
    expect(buildLineListing(asm(null, null, []))).toEqual([]);
    expect(buildLineListing(asm(0, null, [{ line: 1, addr: 0, len: 1 }]))).toEqual([]);
    expect(buildLineListing(null)).toEqual([]);
  });

  it("returns [] when the source map is empty", () => {
    expect(buildLineListing(asm(0, [0x00], []))).toEqual([]);
  });
});

describe("listing — listingByLine", () => {
  it("indexes by source line for O(1) lookup", () => {
    const m = listingByLine(
      asm(0x0150, [0x00, 0x00], [
        { line: 4, addr: 0x0150, len: 1 },
        { line: 7, addr: 0x0151, len: 1 },
      ]),
    );
    expect(m.get(4)?.addr).toBe(0x0150);
    expect(m.get(7)?.addr).toBe(0x0151);
    expect(m.has(5)).toBe(false);
  });
});

describe("listing — formatting", () => {
  it("formats addresses as 4-digit hex", () => {
    expect(formatAddr(0x0150)).toBe("$0150");
    expect(formatAddr(0xffff)).toBe("$FFFF");
    expect(formatAddr(0)).toBe("$0000");
  });

  it("formats bytes as spaced hex, truncating with an ellipsis", () => {
    expect(formatBytes([0x3e, 0x05])).toBe("3E 05");
    expect(formatBytes([0xff])).toBe("FF");
    expect(formatBytes([1, 2, 3, 4, 5], 4)).toBe("01 02 03 04…");
    expect(formatBytes([])).toBe("");
  });
});

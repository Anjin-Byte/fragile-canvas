import { describe, it, expect } from "vitest";
import {
  opcodeCycles,
  bytesCycles,
  sumCycles,
  isConditional,
  formatCycles,
  mCycles,
} from "./cycles.js";

const t = (op: number, cb?: number) => opcodeCycles(op, cb).taken;

describe("cycles — fixed single-M-cycle-class ops", () => {
  it("NOP, STOP, HALT, register LD/ALU are 4 T", () => {
    expect(t(0x00)).toBe(4); // NOP
    expect(t(0x10)).toBe(4); // STOP
    expect(t(0x76)).toBe(4); // HALT
    expect(t(0x41)).toBe(4); // LD B,C
    expect(t(0x80)).toBe(4); // ADD A,B
    expect(t(0x07)).toBe(4); // RLCA
  });
});

describe("cycles — immediates and (HL) forms", () => {
  it("LD r,n=8, LD (HL),n=12, LD rr,nn=12, ALU n=8", () => {
    expect(t(0x06)).toBe(8); // LD B,n
    expect(t(0x36)).toBe(12); // LD (HL),n
    expect(t(0x01)).toBe(12); // LD BC,nn
    expect(t(0xc6)).toBe(8); // ADD A,n
  });

  it("(HL) ALU=8, INC/DEC (HL)=12, INC/DEC r=4, ADD HL,rr=8", () => {
    expect(t(0x86)).toBe(8); // ADD A,(HL)
    expect(t(0x34)).toBe(12); // INC (HL)
    expect(t(0x04)).toBe(4); // INC B
    expect(t(0x09)).toBe(8); // ADD HL,BC
    expect(t(0x46)).toBe(8); // LD B,(HL)
  });
});

describe("cycles — unconditional control flow", () => {
  it("JR=12, JP=16, CALL=24, RET=16, RST=16, JP HL=4, LD SP,HL=8", () => {
    expect(t(0x18)).toBe(12); // JR
    expect(t(0xc3)).toBe(16); // JP nn
    expect(t(0xcd)).toBe(24); // CALL nn
    expect(t(0xc9)).toBe(16); // RET
    expect(t(0xd9)).toBe(16); // RETI
    expect(t(0xff)).toBe(16); // RST 38
    expect(t(0xe9)).toBe(4); // JP HL
    expect(t(0xf9)).toBe(8); // LD SP,HL
  });
});

describe("cycles — conditional branches carry two costs", () => {
  it("JR cc=12/8, JP cc=16/12, CALL cc=24/12, RET cc=20/8", () => {
    expect(opcodeCycles(0x20)).toEqual({ taken: 12, notTaken: 8 }); // JR NZ
    expect(opcodeCycles(0xc2)).toEqual({ taken: 16, notTaken: 12 }); // JP NZ
    expect(opcodeCycles(0xc4)).toEqual({ taken: 24, notTaken: 12 }); // CALL NZ
    expect(opcodeCycles(0xc0)).toEqual({ taken: 20, notTaken: 8 }); // RET NZ
    expect(isConditional(opcodeCycles(0x20))).toBe(true);
    expect(isConditional(opcodeCycles(0x18))).toBe(false);
  });
});

describe("cycles — stack ops", () => {
  it("PUSH=16, POP=12", () => {
    expect(t(0xc5)).toBe(16); // PUSH BC
    expect(t(0xc1)).toBe(12); // POP BC
  });
});

describe("cycles — high page, SP, and 16-bit address forms", () => {
  it("LDH (a8),A=12, LDH A,(a8)=12, LDH (C),A=8, LD (a16),A=16, LD (a16),SP=20", () => {
    expect(t(0xe0)).toBe(12); // LDH (a8),A
    expect(t(0xf0)).toBe(12); // LDH A,(a8)
    expect(t(0xe2)).toBe(8); // LDH (C),A
    expect(t(0xea)).toBe(16); // LD (a16),A
    expect(t(0x08)).toBe(20); // LD (a16),SP
  });

  it("ADD SP,e8=16, LD HL,SP+e8=12, DI/EI=4", () => {
    expect(t(0xe8)).toBe(16); // ADD SP,e8
    expect(t(0xf8)).toBe(12); // LD HL,SP+e8
    expect(t(0xf3)).toBe(4); // DI
    expect(t(0xfb)).toBe(4); // EI
  });
});

describe("cycles — CB-prefixed", () => {
  it("register ops=8, BIT (HL)=12, RMW (HL)=16", () => {
    expect(t(0xcb, 0x00)).toBe(8); // RLC B
    expect(t(0xcb, 0x37)).toBe(8); // SWAP A
    expect(t(0xcb, 0x46)).toBe(12); // BIT 0,(HL)
    expect(t(0xcb, 0x86)).toBe(16); // RES 0,(HL)
    expect(t(0xcb, 0xc6)).toBe(16); // SET 0,(HL)
  });
});

describe("cycles — illegal opcodes cost nothing", () => {
  it("every illegal opcode is 0/0", () => {
    for (const op of [0xd3, 0xdb, 0xdd, 0xe3, 0xe4, 0xeb, 0xec, 0xed, 0xf4, 0xfc, 0xfd]) {
      expect(opcodeCycles(op)).toEqual({ taken: 0, notTaken: 0 });
    }
  });
});

describe("cycles — byte-level helpers", () => {
  it("bytesCycles reads the CB prefix from the stream", () => {
    expect(bytesCycles([0x00]).taken).toBe(4); // NOP
    expect(bytesCycles([0xcb, 0x46]).taken).toBe(12); // BIT 0,(HL)
    expect(bytesCycles([]).taken).toBe(0);
  });

  it("sumCycles totals a selection (taken and not-taken tracked)", () => {
    const total = sumCycles([opcodeCycles(0x00), opcodeCycles(0x06), opcodeCycles(0x20)]);
    expect(total.taken).toBe(4 + 8 + 12);
    expect(total.notTaken).toBe(4 + 8 + 8);
  });
});

describe("cycles — formatting", () => {
  it("formats fixed and conditional costs; M = T/4", () => {
    expect(formatCycles(opcodeCycles(0x00))).toBe("4");
    expect(formatCycles(opcodeCycles(0x20))).toBe("12/8");
    expect(mCycles(24)).toBe(6);
  });
});

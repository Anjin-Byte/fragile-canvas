//! Bytes → `Instruction`. A total function: illegal opcodes and truncated
//! buffers decode to `Mnemonic::Data` (formatted as `DB $xx`), so linear
//! disassembly never fails and listings always reassemble byte-identically.
//!
//! Decoding follows the octal field decomposition the SM83 encoding is
//! built around (the same structure `sm83`'s microcode decoder uses):
//! `x = op>>6`, `y = (op>>3)&7`, `z = op&7`, `p = y>>1`, `q = y&1`.

use crate::model::*;

/// Illegal opcodes on SM83 (hard-lock the CPU on hardware).
pub const ILLEGAL_OPCODES: [u8; 11] = [
    0xD3, 0xDB, 0xDD, 0xE3, 0xE4, 0xEB, 0xEC, 0xED, 0xF4, 0xFC, 0xFD,
];

/// Encoded length (1-3 bytes) of the instruction starting with `op`.
/// `0xCB` returns 2 (prefix + opcode). Illegal opcodes return 1.
pub fn instruction_len(op: u8) -> u8 {
    match op {
        // LD rr,nn / LD (a16),SP / JP / JP cc / CALL / CALL cc / LD (a16),A / LD A,(a16)
        0x01 | 0x11 | 0x21 | 0x31 | 0x08 => 3,
        0xC3 | 0xC2 | 0xCA | 0xD2 | 0xDA => 3,
        0xCD | 0xC4 | 0xCC | 0xD4 | 0xDC => 3,
        0xEA | 0xFA => 3,
        // LD r,n (x=0, z=6)
        0x06 | 0x0E | 0x16 | 0x1E | 0x26 | 0x2E | 0x36 | 0x3E => 2,
        // STOP consumes a padding byte
        0x10 => 2,
        // JR / JR cc
        0x18 | 0x20 | 0x28 | 0x30 | 0x38 => 2,
        // ALU n
        0xC6 | 0xCE | 0xD6 | 0xDE | 0xE6 | 0xEE | 0xF6 | 0xFE => 2,
        // LDH (a8),A / LDH A,(a8) / ADD SP,e8 / LD HL,SP+e8
        0xE0 | 0xF0 | 0xE8 | 0xF8 => 2,
        // CB prefix
        0xCB => 2,
        _ => 1,
    }
}

fn r8_operand(bits: u8) -> Operand {
    match bits & 7 {
        0 => Operand::R8(Reg8::B),
        1 => Operand::R8(Reg8::C),
        2 => Operand::R8(Reg8::D),
        3 => Operand::R8(Reg8::E),
        4 => Operand::R8(Reg8::H),
        5 => Operand::R8(Reg8::L),
        6 => Operand::MemHL,
        _ => Operand::R8(Reg8::A),
    }
}

fn rp(p: u8) -> Reg16 {
    match p & 3 {
        0 => Reg16::BC,
        1 => Reg16::DE,
        2 => Reg16::HL,
        _ => Reg16::SP,
    }
}

fn rp2(p: u8) -> Reg16 {
    match p & 3 {
        0 => Reg16::BC,
        1 => Reg16::DE,
        2 => Reg16::HL,
        _ => Reg16::AF,
    }
}

fn cond(bits: u8) -> Cond {
    match bits & 3 {
        0 => Cond::NZ,
        1 => Cond::Z,
        2 => Cond::NC,
        _ => Cond::C,
    }
}

/// ALU instruction in canonical operand form: ADD/ADC/SBC take an explicit
/// A destination (`ADD A, x`); SUB/AND/XOR/OR/CP take the source alone.
fn alu(y: u8, src: Operand) -> Instruction {
    match y & 7 {
        0 => Instruction::binary(Mnemonic::Add, Operand::R8(Reg8::A), src),
        1 => Instruction::binary(Mnemonic::Adc, Operand::R8(Reg8::A), src),
        2 => Instruction::unary(Mnemonic::Sub, src),
        3 => Instruction::binary(Mnemonic::Sbc, Operand::R8(Reg8::A), src),
        4 => Instruction::unary(Mnemonic::And, src),
        5 => Instruction::unary(Mnemonic::Xor, src),
        6 => Instruction::unary(Mnemonic::Or, src),
        _ => Instruction::unary(Mnemonic::Cp, src),
    }
}

fn data_byte(b: u8) -> Decoded {
    Decoded { instr: Instruction::unary(Mnemonic::Data, Operand::Imm8(b)), len: 1 }
}

/// Decode one instruction from the start of `bytes`.
///
/// Returns `None` only for an empty slice. A truncated multi-byte
/// instruction at the end of a buffer decodes as `Data` (one byte).
pub fn decode(bytes: &[u8]) -> Option<Decoded> {
    let &op = bytes.first()?;
    let len = instruction_len(op);
    if (bytes.len() as u8) < len {
        return Some(data_byte(op));
    }
    if ILLEGAL_OPCODES.contains(&op) {
        return Some(data_byte(op));
    }

    let imm8 = || bytes[1];
    let simm8 = || bytes[1] as i8;
    let imm16 = || u16::from_le_bytes([bytes[1], bytes[2]]);

    let x = op >> 6;
    let y = (op >> 3) & 7;
    let z = op & 7;
    let p = y >> 1;
    let q = y & 1;

    use Mnemonic as M;
    use Operand as O;

    let instr = match (x, z) {
        // ── x=0 ──────────────────────────────────────────────────────────
        (0, 0) => match y {
            0 => Instruction::nullary(M::Nop),
            1 => Instruction::binary(M::Ld, O::MemImm16(imm16()), O::R16(Reg16::SP)),
            // STOP carries its padding byte (usually $00, but ROM data can
            // put anything there) so byte roundtrips stay exact.
            2 => Instruction::unary(M::Stop, O::Imm8(imm8())),
            3 => Instruction::unary(M::Jr, O::Rel(simm8())),
            _ => Instruction::binary(M::Jr, O::Cond(cond(y - 4)), O::Rel(simm8())),
        },
        (0, 1) => {
            if q == 0 {
                Instruction::binary(M::Ld, O::R16(rp(p)), O::Imm16(imm16()))
            } else {
                Instruction::binary(M::Add, O::R16(Reg16::HL), O::R16(rp(p)))
            }
        }
        (0, 2) => {
            let mem = [O::MemBC, O::MemDE, O::MemHLInc, O::MemHLDec][p as usize];
            if q == 0 {
                Instruction::binary(M::Ld, mem, O::R8(Reg8::A))
            } else {
                Instruction::binary(M::Ld, O::R8(Reg8::A), mem)
            }
        }
        (0, 3) => {
            let m = if q == 0 { M::Inc } else { M::Dec };
            Instruction::unary(m, O::R16(rp(p)))
        }
        (0, 4) => Instruction::unary(M::Inc, r8_operand(y)),
        (0, 5) => Instruction::unary(M::Dec, r8_operand(y)),
        (0, 6) => Instruction::binary(M::Ld, r8_operand(y), O::Imm8(imm8())),
        (0, 7) => {
            let m = [M::Rlca, M::Rrca, M::Rla, M::Rra, M::Daa, M::Cpl, M::Scf, M::Ccf][y as usize];
            Instruction::nullary(m)
        }

        // ── x=1: LD r,r' (HALT at the (HL),(HL) hole) ────────────────────
        (1, _) => {
            if y == 6 && z == 6 {
                Instruction::nullary(M::Halt)
            } else {
                Instruction::binary(M::Ld, r8_operand(y), r8_operand(z))
            }
        }

        // ── x=2: ALU A, r ────────────────────────────────────────────────
        (2, _) => alu(y, r8_operand(z)),

        // ── x=3 ──────────────────────────────────────────────────────────
        (3, 0) => match y {
            0..=3 => Instruction::unary(M::Ret, O::Cond(cond(y))),
            4 => Instruction::binary(M::Ldh, O::HighImm(imm8()), O::R8(Reg8::A)),
            5 => Instruction::binary(M::Add, O::R16(Reg16::SP), O::Simm8(simm8())),
            6 => Instruction::binary(M::Ldh, O::R8(Reg8::A), O::HighImm(imm8())),
            _ => Instruction::binary(M::Ld, O::R16(Reg16::HL), O::SpPlus(simm8())),
        },
        (3, 1) => {
            if q == 0 {
                Instruction::unary(M::Pop, O::R16(rp2(p)))
            } else {
                match p {
                    0 => Instruction::nullary(M::Ret),
                    1 => Instruction::nullary(M::Reti),
                    2 => Instruction::unary(M::Jp, O::R16(Reg16::HL)),
                    _ => Instruction::binary(M::Ld, O::R16(Reg16::SP), O::R16(Reg16::HL)),
                }
            }
        }
        (3, 2) => match y {
            0..=3 => Instruction::binary(M::Jp, O::Cond(cond(y)), O::Imm16(imm16())),
            4 => Instruction::binary(M::Ldh, O::HighC, O::R8(Reg8::A)),
            5 => Instruction::binary(M::Ld, O::MemImm16(imm16()), O::R8(Reg8::A)),
            6 => Instruction::binary(M::Ldh, O::R8(Reg8::A), O::HighC),
            _ => Instruction::binary(M::Ld, O::R8(Reg8::A), O::MemImm16(imm16())),
        },
        (3, 3) => match y {
            0 => Instruction::unary(M::Jp, O::Imm16(imm16())),
            1 => return Some(decode_cb(bytes[1])),
            6 => Instruction::nullary(M::Di),
            7 => Instruction::nullary(M::Ei),
            _ => unreachable!("illegal opcodes handled above"),
        },
        (3, 4) => match y {
            0..=3 => Instruction::binary(M::Call, O::Cond(cond(y)), O::Imm16(imm16())),
            _ => unreachable!("illegal opcodes handled above"),
        },
        (3, 5) => {
            if q == 0 {
                Instruction::unary(M::Push, O::R16(rp2(p)))
            } else {
                match p {
                    0 => Instruction::unary(M::Call, O::Imm16(imm16())),
                    _ => unreachable!("illegal opcodes handled above"),
                }
            }
        }
        (3, 6) => alu(y, O::Imm8(imm8())),
        (3, 7) => Instruction::unary(M::Rst, O::Rst(y * 8)),

        _ => unreachable!("x and z are 2- and 3-bit fields"),
    };

    Some(Decoded { instr, len })
}

/// Decode the second byte of a `CB`-prefixed instruction.
fn decode_cb(op: u8) -> Decoded {
    use Mnemonic as M;
    let x = op >> 6;
    let y = (op >> 3) & 7;
    let z = op & 7;
    let target = r8_operand(z);

    let instr = match x {
        0 => {
            let m = [M::Rlc, M::Rrc, M::Rl, M::Rr, M::Sla, M::Sra, M::Swap, M::Srl][y as usize];
            Instruction::unary(m, target)
        }
        1 => Instruction::binary(M::Bit, Operand::Bit(y), target),
        2 => Instruction::binary(M::Res, Operand::Bit(y), target),
        _ => Instruction::binary(M::Set, Operand::Bit(y), target),
    };
    Decoded { instr, len: 2 }
}

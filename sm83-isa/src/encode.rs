//! `Instruction` → bytes. The hand-inverse of `decode`; the exhaustive
//! roundtrip tests enumerate every opcode, so the two cannot drift.
//!
//! Encoding is strict: it accepts the canonical IR produced by `decode`
//! and `parse` (which normalizes surface forms like `SUB A, B` → `SUB B`).
//! Unencodable operand combinations return `EncodeError`, which the
//! assembler surfaces as a diagnostic.

use crate::model::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodeError(pub &'static str);

impl std::fmt::Display for EncodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cannot encode: {}", self.0)
    }
}
impl std::error::Error for EncodeError {}

/// Encoded instruction bytes (1-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Encoded {
    bytes: [u8; 3],
    len: u8,
}

impl Encoded {
    fn one(a: u8) -> Self {
        Self { bytes: [a, 0, 0], len: 1 }
    }
    fn two(a: u8, b: u8) -> Self {
        Self { bytes: [a, b, 0], len: 2 }
    }
    fn three(a: u8, b: u8, c: u8) -> Self {
        Self { bytes: [a, b, c], len: 3 }
    }
    fn imm16(a: u8, v: u16) -> Self {
        let [lo, hi] = v.to_le_bytes();
        Self::three(a, lo, hi)
    }
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes[..self.len as usize]
    }
    pub fn len(&self) -> u8 {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        false
    }
}

/// 3-bit register index for r8/(HL) operand positions.
fn r_index(op: &Operand) -> Option<u8> {
    Some(match op {
        Operand::R8(Reg8::B) => 0,
        Operand::R8(Reg8::C) => 1,
        Operand::R8(Reg8::D) => 2,
        Operand::R8(Reg8::E) => 3,
        Operand::R8(Reg8::H) => 4,
        Operand::R8(Reg8::L) => 5,
        Operand::MemHL => 6,
        Operand::R8(Reg8::A) => 7,
        _ => return None,
    })
}

fn rp_index(r: Reg16) -> Option<u8> {
    Some(match r {
        Reg16::BC => 0,
        Reg16::DE => 1,
        Reg16::HL => 2,
        Reg16::SP => 3,
        Reg16::AF => return None,
    })
}

fn rp2_index(r: Reg16) -> Option<u8> {
    Some(match r {
        Reg16::BC => 0,
        Reg16::DE => 1,
        Reg16::HL => 2,
        Reg16::AF => 3,
        Reg16::SP => return None,
    })
}

fn cond_index(c: Cond) -> u8 {
    match c {
        Cond::NZ => 0,
        Cond::Z => 1,
        Cond::NC => 2,
        Cond::C => 3,
    }
}

pub fn encode(instr: &Instruction) -> Result<Encoded, EncodeError> {
    use Mnemonic as M;
    use Operand as O;

    let err = |m: &'static str| Err(EncodeError(m));
    let ops = (&instr.ops[0], &instr.ops[1]);

    match instr.mnemonic {
        // ── Nullary ──────────────────────────────────────────────────────
        M::Nop => Ok(Encoded::one(0x00)),
        M::Stop => match ops {
            (None, None) => Ok(Encoded::two(0x10, 0x00)),
            (Some(O::Imm8(pad)), None) => Ok(Encoded::two(0x10, *pad)),
            _ => err("STOP takes at most a padding byte"),
        },
        M::Halt => Ok(Encoded::one(0x76)),
        M::Di => Ok(Encoded::one(0xF3)),
        M::Ei => Ok(Encoded::one(0xFB)),
        M::Daa => Ok(Encoded::one(0x27)),
        M::Cpl => Ok(Encoded::one(0x2F)),
        M::Scf => Ok(Encoded::one(0x37)),
        M::Ccf => Ok(Encoded::one(0x3F)),
        M::Rlca => Ok(Encoded::one(0x07)),
        M::Rrca => Ok(Encoded::one(0x0F)),
        M::Rla => Ok(Encoded::one(0x17)),
        M::Rra => Ok(Encoded::one(0x1F)),
        M::Reti => Ok(Encoded::one(0xD9)),

        // ── LD ───────────────────────────────────────────────────────────
        M::Ld => match ops {
            (Some(O::R16(rr)), Some(O::Imm16(v))) => match rp_index(*rr) {
                Some(p) => Ok(Encoded::imm16(0x01 | (p << 4), *v)),
                None => err("LD AF, nn does not exist"),
            },
            (Some(O::MemImm16(v)), Some(O::R16(Reg16::SP))) => Ok(Encoded::imm16(0x08, *v)),
            (Some(O::MemBC), Some(O::R8(Reg8::A))) => Ok(Encoded::one(0x02)),
            (Some(O::MemDE), Some(O::R8(Reg8::A))) => Ok(Encoded::one(0x12)),
            (Some(O::MemHLInc), Some(O::R8(Reg8::A))) => Ok(Encoded::one(0x22)),
            (Some(O::MemHLDec), Some(O::R8(Reg8::A))) => Ok(Encoded::one(0x32)),
            (Some(O::R8(Reg8::A)), Some(O::MemBC)) => Ok(Encoded::one(0x0A)),
            (Some(O::R8(Reg8::A)), Some(O::MemDE)) => Ok(Encoded::one(0x1A)),
            (Some(O::R8(Reg8::A)), Some(O::MemHLInc)) => Ok(Encoded::one(0x2A)),
            (Some(O::R8(Reg8::A)), Some(O::MemHLDec)) => Ok(Encoded::one(0x3A)),
            (Some(O::MemImm16(v)), Some(O::R8(Reg8::A))) => Ok(Encoded::imm16(0xEA, *v)),
            (Some(O::R8(Reg8::A)), Some(O::MemImm16(v))) => Ok(Encoded::imm16(0xFA, *v)),
            (Some(O::R16(Reg16::SP)), Some(O::R16(Reg16::HL))) => Ok(Encoded::one(0xF9)),
            (Some(O::R16(Reg16::HL)), Some(O::SpPlus(e))) => Ok(Encoded::two(0xF8, *e as u8)),
            (Some(dst), Some(O::Imm8(v))) => match r_index(dst) {
                Some(y) => Ok(Encoded::two(0x06 | (y << 3), *v)),
                None => err("LD immediate destination must be r8 or (HL)"),
            },
            (Some(dst), Some(src)) => match (r_index(dst), r_index(src)) {
                (Some(6), Some(6)) => err("LD (HL), (HL) does not exist"),
                (Some(y), Some(z)) => Ok(Encoded::one(0x40 | (y << 3) | z)),
                _ => err("unsupported LD operands"),
            },
            _ => err("LD needs two operands"),
        },

        // ── LDH ──────────────────────────────────────────────────────────
        M::Ldh => match ops {
            (Some(O::HighImm(n)), Some(O::R8(Reg8::A))) => Ok(Encoded::two(0xE0, *n)),
            (Some(O::R8(Reg8::A)), Some(O::HighImm(n))) => Ok(Encoded::two(0xF0, *n)),
            (Some(O::HighC), Some(O::R8(Reg8::A))) => Ok(Encoded::one(0xE2)),
            (Some(O::R8(Reg8::A)), Some(O::HighC)) => Ok(Encoded::one(0xF2)),
            _ => err("LDH transfers A to/from the $FF00 page"),
        },

        // ── INC / DEC ────────────────────────────────────────────────────
        M::Inc => match ops {
            (Some(O::R16(rr)), None) => match rp_index(*rr) {
                Some(p) => Ok(Encoded::one(0x03 | (p << 4))),
                None => err("INC AF does not exist"),
            },
            (Some(t), None) => match r_index(t) {
                Some(y) => Ok(Encoded::one(0x04 | (y << 3))),
                None => err("INC target must be r8, (HL), or rr"),
            },
            _ => err("INC takes one operand"),
        },
        M::Dec => match ops {
            (Some(O::R16(rr)), None) => match rp_index(*rr) {
                Some(p) => Ok(Encoded::one(0x0B | (p << 4))),
                None => err("DEC AF does not exist"),
            },
            (Some(t), None) => match r_index(t) {
                Some(y) => Ok(Encoded::one(0x05 | (y << 3))),
                None => err("DEC target must be r8, (HL), or rr"),
            },
            _ => err("DEC takes one operand"),
        },

        // ── ALU ──────────────────────────────────────────────────────────
        M::Add => match ops {
            (Some(O::R16(Reg16::HL)), Some(O::R16(rr))) => match rp_index(*rr) {
                Some(p) => Ok(Encoded::one(0x09 | (p << 4))),
                None => err("ADD HL, AF does not exist"),
            },
            (Some(O::R16(Reg16::SP)), Some(O::Simm8(e))) => Ok(Encoded::two(0xE8, *e as u8)),
            (Some(O::R8(Reg8::A)), Some(O::Imm8(v))) => Ok(Encoded::two(0xC6, *v)),
            (Some(O::R8(Reg8::A)), Some(src)) => match r_index(src) {
                Some(z) => Ok(Encoded::one(0x80 | z)),
                None => err("ADD source must be r8, (HL), or n8"),
            },
            _ => err("unsupported ADD operands"),
        },
        M::Adc => alu_a(ops, 0x88, 0xCE),
        M::Sbc => alu_a(ops, 0x98, 0xDE),
        M::Sub => alu_bare(ops, 0x90, 0xD6),
        M::And => alu_bare(ops, 0xA0, 0xE6),
        M::Xor => alu_bare(ops, 0xA8, 0xEE),
        M::Or => alu_bare(ops, 0xB0, 0xF6),
        M::Cp => alu_bare(ops, 0xB8, 0xFE),

        // ── Control flow ─────────────────────────────────────────────────
        M::Jp => match ops {
            (Some(O::Imm16(v)), None) => Ok(Encoded::imm16(0xC3, *v)),
            (Some(O::R16(Reg16::HL)), None) => Ok(Encoded::one(0xE9)),
            (Some(O::Cond(c)), Some(O::Imm16(v))) => {
                Ok(Encoded::imm16(0xC2 | (cond_index(*c) << 3), *v))
            }
            _ => err("unsupported JP operands"),
        },
        M::Jr => match ops {
            (Some(O::Rel(d)), None) => Ok(Encoded::two(0x18, *d as u8)),
            (Some(O::Cond(c)), Some(O::Rel(d))) => {
                Ok(Encoded::two(0x20 | (cond_index(*c) << 3), *d as u8))
            }
            _ => err("unsupported JR operands"),
        },
        M::Call => match ops {
            (Some(O::Imm16(v)), None) => Ok(Encoded::imm16(0xCD, *v)),
            (Some(O::Cond(c)), Some(O::Imm16(v))) => {
                Ok(Encoded::imm16(0xC4 | (cond_index(*c) << 3), *v))
            }
            _ => err("unsupported CALL operands"),
        },
        M::Ret => match ops {
            (None, None) => Ok(Encoded::one(0xC9)),
            (Some(O::Cond(c)), None) => Ok(Encoded::one(0xC0 | (cond_index(*c) << 3))),
            _ => err("unsupported RET operands"),
        },
        M::Rst => match ops {
            (Some(O::Rst(t)), None) if *t <= 0x38 && t % 8 == 0 => {
                Ok(Encoded::one(0xC7 | (t & 0x38)))
            }
            _ => err("RST target must be $00, $08, ... $38"),
        },
        M::Push => match ops {
            (Some(O::R16(rr)), None) => match rp2_index(*rr) {
                Some(p) => Ok(Encoded::one(0xC5 | (p << 4))),
                None => err("PUSH SP does not exist"),
            },
            _ => err("PUSH takes a register pair"),
        },
        M::Pop => match ops {
            (Some(O::R16(rr)), None) => match rp2_index(*rr) {
                Some(p) => Ok(Encoded::one(0xC1 | (p << 4))),
                None => err("POP SP does not exist"),
            },
            _ => err("POP takes a register pair"),
        },

        // ── CB-prefixed ──────────────────────────────────────────────────
        M::Rlc => cb_rot(ops, 0x00),
        M::Rrc => cb_rot(ops, 0x08),
        M::Rl => cb_rot(ops, 0x10),
        M::Rr => cb_rot(ops, 0x18),
        M::Sla => cb_rot(ops, 0x20),
        M::Sra => cb_rot(ops, 0x28),
        M::Swap => cb_rot(ops, 0x30),
        M::Srl => cb_rot(ops, 0x38),
        M::Bit => cb_bit(ops, 0x40),
        M::Res => cb_bit(ops, 0x80),
        M::Set => cb_bit(ops, 0xC0),

        // ── Raw data ─────────────────────────────────────────────────────
        M::Data => match ops {
            (Some(O::Imm8(b)), None) => Ok(Encoded::one(*b)),
            _ => Err(EncodeError("DB pseudo-instruction takes one byte")),
        },
    }
}

type Ops<'a> = (&'a Option<Operand>, &'a Option<Operand>);

/// ADC/SBC: explicit A destination.
fn alu_a(ops: Ops, base_r: u8, imm_op: u8) -> Result<Encoded, EncodeError> {
    match ops {
        (Some(Operand::R8(Reg8::A)), Some(Operand::Imm8(v))) => Ok(Encoded::two(imm_op, *v)),
        (Some(Operand::R8(Reg8::A)), Some(src)) => match r_index(src) {
            Some(z) => Ok(Encoded::one(base_r | z)),
            None => Err(EncodeError("ALU source must be r8, (HL), or n8")),
        },
        _ => Err(EncodeError("ALU op needs A destination and a source")),
    }
}

/// SUB/AND/XOR/OR/CP: bare source operand.
fn alu_bare(ops: Ops, base_r: u8, imm_op: u8) -> Result<Encoded, EncodeError> {
    match ops {
        (Some(Operand::Imm8(v)), None) => Ok(Encoded::two(imm_op, *v)),
        (Some(src), None) => match r_index(src) {
            Some(z) => Ok(Encoded::one(base_r | z)),
            None => Err(EncodeError("ALU source must be r8, (HL), or n8")),
        },
        _ => Err(EncodeError("ALU op takes one source operand")),
    }
}

fn cb_rot(ops: Ops, base: u8) -> Result<Encoded, EncodeError> {
    match ops {
        (Some(t), None) => match r_index(t) {
            Some(z) => Ok(Encoded::two(0xCB, base | z)),
            None => Err(EncodeError("rotate/shift target must be r8 or (HL)")),
        },
        _ => Err(EncodeError("rotate/shift takes one operand")),
    }
}

fn cb_bit(ops: Ops, base: u8) -> Result<Encoded, EncodeError> {
    match ops {
        (Some(Operand::Bit(b)), Some(t)) if *b <= 7 => match r_index(t) {
            Some(z) => Ok(Encoded::two(0xCB, base | (b << 3) | z)),
            None => Err(EncodeError("bit target must be r8 or (HL)")),
        },
        _ => Err(EncodeError("bit op takes a bit index 0-7 and a target")),
    }
}

//! One instruction line → `Instruction`.
//!
//! Permissive reader for the canonical formatter's output and common
//! hand-written variants: case-insensitive, `(..)` or `[..]` indirection,
//! `$`/`0x`/`%`/decimal literals, `(HLI)`/`(HLD)` synonyms, optional-A ALU
//! forms (`SUB A, B` ≡ `SUB B`, `ADD B` ≡ `ADD A, B`), `LDH ($FF00+n)` or
//! `($FFxx)` or `($xx)` high-page forms, and `@±$XX` raw JR displacements.
//!
//! Numeric-only: label/expression resolution lives in the two-pass
//! assembler (`asm`), which parses operands itself and feeds resolved
//! values through the same context rules via `coerce`.

use crate::model::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub msg: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.msg)
    }
}
impl std::error::Error for ParseError {}

fn err<T>(msg: impl Into<String>) -> Result<T, ParseError> {
    Err(ParseError { msg: msg.into() })
}

/// Operand before context coercion: bare numbers keep their value until
/// the mnemonic decides imm8/imm16/bit/rst width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawOperand {
    Op(Operand),
    Num(i32),
    /// (number) — indirect; LD → (a16), LDH → high page.
    Mem(i32),
}

pub fn mnemonic_from_str(s: &str) -> Option<Mnemonic> {
    use Mnemonic::*;
    Some(match s.to_ascii_uppercase().as_str() {
        "NOP" => Nop,
        "LD" => Ld,
        "LDH" => Ldh,
        "INC" => Inc,
        "DEC" => Dec,
        "ADD" => Add,
        "ADC" => Adc,
        "SUB" => Sub,
        "SBC" => Sbc,
        "AND" => And,
        "XOR" => Xor,
        "OR" => Or,
        "CP" => Cp,
        "JP" => Jp,
        "JR" => Jr,
        "CALL" => Call,
        "RET" => Ret,
        "RETI" => Reti,
        "RST" => Rst,
        "PUSH" => Push,
        "POP" => Pop,
        "RLCA" => Rlca,
        "RRCA" => Rrca,
        "RLA" => Rla,
        "RRA" => Rra,
        "DAA" => Daa,
        "CPL" => Cpl,
        "SCF" => Scf,
        "CCF" => Ccf,
        "HALT" => Halt,
        "STOP" => Stop,
        "DI" => Di,
        "EI" => Ei,
        "RLC" => Rlc,
        "RRC" => Rrc,
        "RL" => Rl,
        "RR" => Rr,
        "SLA" => Sla,
        "SRA" => Sra,
        "SWAP" => Swap,
        "SRL" => Srl,
        "BIT" => Bit,
        "RES" => Res,
        "SET" => Set,
        "DB" => Data,
        _ => return None,
    })
}

/// Parse a numeric literal: `$1F`, `0x1F`, `%1010`, `31`, with optional
/// leading sign.
pub fn parse_number(s: &str) -> Result<i32, ParseError> {
    let s = s.trim();
    let (neg, rest) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let rest = rest.trim();
    let value = if let Some(hex) = rest.strip_prefix('$') {
        i64::from_str_radix(hex, 16)
    } else if let Some(hex) = rest.strip_prefix("0x").or_else(|| rest.strip_prefix("0X")) {
        i64::from_str_radix(hex, 16)
    } else if let Some(bin) = rest.strip_prefix('%') {
        i64::from_str_radix(bin, 2)
    } else {
        rest.parse::<i64>()
    };
    match value {
        Ok(v) if (-65536..=65535).contains(&v) => Ok(if neg { -v as i32 } else { v as i32 }),
        Ok(_) => err(format!("number out of range: {s}")),
        Err(_) => err(format!("bad number: {s}")),
    }
}

pub(crate) fn parse_raw_operand(s: &str) -> Result<RawOperand, ParseError> {
    use Operand as O;
    let s = s.trim();
    let up = s.to_ascii_uppercase();

    // Registers / conditions (bare)
    match up.as_str() {
        "A" => return Ok(RawOperand::Op(O::R8(Reg8::A))),
        "B" => return Ok(RawOperand::Op(O::R8(Reg8::B))),
        "C" => return Ok(RawOperand::Op(O::R8(Reg8::C))), // may become Cond(C) in context
        "D" => return Ok(RawOperand::Op(O::R8(Reg8::D))),
        "E" => return Ok(RawOperand::Op(O::R8(Reg8::E))),
        "H" => return Ok(RawOperand::Op(O::R8(Reg8::H))),
        "L" => return Ok(RawOperand::Op(O::R8(Reg8::L))),
        "AF" => return Ok(RawOperand::Op(O::R16(Reg16::AF))),
        "BC" => return Ok(RawOperand::Op(O::R16(Reg16::BC))),
        "DE" => return Ok(RawOperand::Op(O::R16(Reg16::DE))),
        "HL" => return Ok(RawOperand::Op(O::R16(Reg16::HL))),
        "SP" => return Ok(RawOperand::Op(O::R16(Reg16::SP))),
        "NZ" => return Ok(RawOperand::Op(O::Cond(Cond::NZ))),
        "Z" => return Ok(RawOperand::Op(O::Cond(Cond::Z))),
        "NC" => return Ok(RawOperand::Op(O::Cond(Cond::NC))),
        _ => {}
    }

    // Indirection: (..) or [..]
    let indirect = (s.starts_with('(') && s.ends_with(')'))
        || (s.starts_with('[') && s.ends_with(']'));
    if indirect {
        let inner = s[1..s.len() - 1].trim();
        let inner_up: String = inner
            .to_ascii_uppercase()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        return Ok(match inner_up.as_str() {
            "BC" => RawOperand::Op(O::MemBC),
            "DE" => RawOperand::Op(O::MemDE),
            "HL" => RawOperand::Op(O::MemHL),
            "HL+" | "HLI" => RawOperand::Op(O::MemHLInc),
            "HL-" | "HLD" => RawOperand::Op(O::MemHLDec),
            "C" | "$FF00+C" | "0XFF00+C" => RawOperand::Op(O::HighC),
            _ => {
                // ($FF00+n) → high page; anything else numeric → Mem
                if let Some(tail) = inner_up.strip_prefix("$FF00+").or_else(|| inner_up.strip_prefix("0XFF00+")) {
                    let n = parse_number(tail)?;
                    if !(0..=0xFF).contains(&n) {
                        return err(format!("high-page offset out of range: {inner}"));
                    }
                    RawOperand::Op(O::HighImm(n as u8))
                } else {
                    RawOperand::Mem(parse_number(inner)?)
                }
            }
        });
    }

    // SP+e8 / SP-e8 (LD HL, SP±e) — whitespace-tolerant
    let compact: String = up.chars().filter(|c| !c.is_whitespace()).collect();
    if let Some(tail) = compact.strip_prefix("SP+").or_else(|| compact.strip_prefix("SP-")) {
        let mag = parse_number(tail)?;
        let signed = if compact.as_bytes()[2] == b'-' { -mag } else { mag };
        if !(-128..=127).contains(&signed) {
            return err(format!("SP offset out of range: {s}"));
        }
        return Ok(RawOperand::Op(O::SpPlus(signed as i8)));
    }

    // @±disp — raw relative displacement
    if let Some(tail) = up.strip_prefix('@') {
        let d = parse_number(tail)?;
        if !(-128..=127).contains(&d) {
            return err(format!("relative displacement out of range: {s}"));
        }
        return Ok(RawOperand::Op(O::Rel(d as i8)));
    }

    Ok(RawOperand::Num(parse_number(s)?))
}

/// Split an operand list on top-level commas (parens/brackets shield none
/// in this grammar, but tolerate them anyway).
pub(crate) fn split_operands(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                out.push(s[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    let last = s[start..].trim();
    if !last.is_empty() {
        out.push(last);
    }
    out
}

fn imm8_from(n: i32, what: &str) -> Result<Operand, ParseError> {
    if (-128..=255).contains(&n) {
        Ok(Operand::Imm8(n as u8))
    } else {
        err(format!("{what} out of 8-bit range: {n}"))
    }
}

fn imm16_from(n: i32, what: &str) -> Result<Operand, ParseError> {
    // Accept unsigned $0000-$FFFF and signed -32768..-1 (two's complement),
    // mirroring `imm8_from` — so `LD BC, -1` folds to $FFFF just as `LD A, -1`
    // folds to $FF, instead of being rejected.
    if (-0x8000..=0xFFFF).contains(&n) {
        Ok(Operand::Imm16(n as u16))
    } else {
        err(format!("{what} out of 16-bit range: {n}"))
    }
}

/// Apply mnemonic context to raw operands, producing the canonical IR.
/// Shared by the line parser and the two-pass assembler (which resolves
/// expressions to `Num`/`Mem` first).
pub fn coerce(mnemonic: Mnemonic, raw: &[RawOperand]) -> Result<Instruction, ParseError> {
    use Mnemonic as M;
    use Operand as O;

    if raw.len() > 2 {
        return err("too many operands");
    }

    let mut ops: Vec<RawOperand> = raw.to_vec();

    // Optional-A ALU surface forms; bare STOP gets its canonical padding.
    match mnemonic {
        M::Sub | M::And | M::Xor | M::Or | M::Cp => {
            if ops.len() == 2 && ops[0] == RawOperand::Op(O::R8(Reg8::A)) {
                ops.remove(0); // SUB A, B → SUB B
            }
        }
        M::Add | M::Adc | M::Sbc => {
            if ops.len() == 1 {
                ops.insert(0, RawOperand::Op(O::R8(Reg8::A))); // ADD B → ADD A, B
            }
        }
        M::Stop => {
            if ops.is_empty() {
                ops.push(RawOperand::Num(0)); // STOP ≡ STOP $00 (decode canonical)
            }
        }
        _ => {}
    }

    // Bare C as a condition where the grammar wants one.
    let wants_cond = matches!(
        (mnemonic, ops.len()),
        (M::Jr | M::Jp | M::Call, 2) | (M::Ret, 1)
    );
    if wants_cond {
        if let RawOperand::Op(O::R8(Reg8::C)) = ops[0] {
            ops[0] = RawOperand::Op(O::Cond(Cond::C));
        }
    }

    // Mnemonic-specific numeric coercions.
    let resolved: Result<Vec<Operand>, ParseError> = ops
        .iter()
        .enumerate()
        .map(|(i, r)| match *r {
            RawOperand::Op(op) => Ok(op),
            RawOperand::Mem(n) => match mnemonic {
                M::Ldh => {
                    if (0xFF00..=0xFFFF).contains(&n) {
                        Ok(O::HighImm((n - 0xFF00) as u8))
                    } else if (0..=0xFF).contains(&n) {
                        Ok(O::HighImm(n as u8))
                    } else {
                        err(format!("LDH address must be in $FF00-$FFFF: ${n:04X}"))
                    }
                }
                _ => {
                    if (0..=0xFFFF).contains(&n) {
                        Ok(O::MemImm16(n as u16))
                    } else {
                        err(format!("address out of range: {n}"))
                    }
                }
            },
            RawOperand::Num(n) => match mnemonic {
                M::Bit | M::Res | M::Set if i == 0 => {
                    if (0..=7).contains(&n) {
                        Ok(O::Bit(n as u8))
                    } else {
                        err(format!("bit index must be 0-7: {n}"))
                    }
                }
                M::Rst => {
                    if (0..=0x38).contains(&n) && n % 8 == 0 {
                        Ok(O::Rst(n as u8))
                    } else {
                        err(format!("RST target must be $00, $08, ... $38: {n}"))
                    }
                }
                M::Jp | M::Call => imm16_from(n, "jump target"),
                M::Jr => err("JR takes @±disp here; absolute targets need the assembler"),
                M::Add if matches!(ops[0], RawOperand::Op(O::R16(Reg16::SP))) => {
                    if (-128..=127).contains(&n) {
                        Ok(O::Simm8(n as i8))
                    } else {
                        err(format!("ADD SP offset out of range: {n}"))
                    }
                }
                M::Ld if matches!(ops[0], RawOperand::Op(O::R16(_))) && i == 1 => {
                    imm16_from(n, "16-bit immediate")
                }
                M::Stop => imm8_from(n, "STOP padding"),
                _ => imm8_from(n, "immediate"),
            },
        })
        .collect();
    let resolved = resolved?;

    let instr = match resolved.len() {
        0 => Instruction::nullary(mnemonic),
        1 => Instruction::unary(mnemonic, resolved[0]),
        _ => Instruction::binary(mnemonic, resolved[0], resolved[1]),
    };
    Ok(instr)
}

/// Parse one instruction line (no label, no comment) into the IR.
pub fn parse_instruction(line: &str) -> Result<Instruction, ParseError> {
    let line = match line.find(';') {
        Some(i) => &line[..i],
        None => line,
    };
    let line = line.trim();
    if line.is_empty() {
        return err("empty line");
    }

    let (mnem_str, rest) = match line.find(char::is_whitespace) {
        Some(i) => (&line[..i], line[i..].trim()),
        None => (line, ""),
    };
    let mnemonic = mnemonic_from_str(mnem_str)
        .ok_or_else(|| ParseError { msg: format!("unknown mnemonic: {mnem_str}") })?;

    let raw: Result<Vec<RawOperand>, ParseError> =
        split_operands(rest).into_iter().map(parse_raw_operand).collect();
    coerce(mnemonic, &raw?)
}

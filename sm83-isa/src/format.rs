//! `Instruction` → canonical text.
//!
//! Canonical style (matches Pandocs / the debugger spec): uppercase
//! mnemonics and registers, `$` hex, parens for indirection, one space
//! after commas. Examples:
//!
//! ```text
//! LD A, ($FF44)      JR NZ, $0150       LDH ($FF44), A
//! LD A, (HL+)        ADD SP, -$02       BIT 7, (HL)
//! ```
//!
//! `JR` targets render as absolute addresses when the instruction's own
//! address is supplied; otherwise as a raw signed displacement `@±$XX`
//! (relative to the instruction end), which the assembler also accepts.

use crate::model::*;

/// Resolves an absolute address to a symbol name (labels, I/O registers).
pub type SymbolResolver<'a> = &'a dyn Fn(u16) -> Option<String>;

#[derive(Default)]
pub struct FormatOptions<'a> {
    /// Address of the instruction's first byte — enables absolute `JR` targets.
    pub addr: Option<u16>,
    /// Optional symbol lookup for absolute addresses.
    pub symbols: Option<SymbolResolver<'a>>,
}

fn reg8(r: Reg8) -> &'static str {
    match r {
        Reg8::A => "A",
        Reg8::B => "B",
        Reg8::C => "C",
        Reg8::D => "D",
        Reg8::E => "E",
        Reg8::H => "H",
        Reg8::L => "L",
    }
}

fn reg16(r: Reg16) -> &'static str {
    match r {
        Reg16::AF => "AF",
        Reg16::BC => "BC",
        Reg16::DE => "DE",
        Reg16::HL => "HL",
        Reg16::SP => "SP",
    }
}

fn cond(c: Cond) -> &'static str {
    match c {
        Cond::NZ => "NZ",
        Cond::Z => "Z",
        Cond::NC => "NC",
        Cond::C => "C",
    }
}

pub fn mnemonic_str(m: Mnemonic) -> &'static str {
    use Mnemonic::*;
    match m {
        Nop => "NOP",
        Ld => "LD",
        Ldh => "LDH",
        Inc => "INC",
        Dec => "DEC",
        Add => "ADD",
        Adc => "ADC",
        Sub => "SUB",
        Sbc => "SBC",
        And => "AND",
        Xor => "XOR",
        Or => "OR",
        Cp => "CP",
        Jp => "JP",
        Jr => "JR",
        Call => "CALL",
        Ret => "RET",
        Reti => "RETI",
        Rst => "RST",
        Push => "PUSH",
        Pop => "POP",
        Rlca => "RLCA",
        Rrca => "RRCA",
        Rla => "RLA",
        Rra => "RRA",
        Daa => "DAA",
        Cpl => "CPL",
        Scf => "SCF",
        Ccf => "CCF",
        Halt => "HALT",
        Stop => "STOP",
        Di => "DI",
        Ei => "EI",
        Rlc => "RLC",
        Rrc => "RRC",
        Rl => "RL",
        Rr => "RR",
        Sla => "SLA",
        Sra => "SRA",
        Swap => "SWAP",
        Srl => "SRL",
        Bit => "BIT",
        Res => "RES",
        Set => "SET",
        Data => "DB",
    }
}

fn abs16(addr: u16, opts: &FormatOptions) -> String {
    if let Some(resolve) = opts.symbols {
        if let Some(name) = resolve(addr) {
            return name;
        }
    }
    format!("${addr:04X}")
}

fn operand(op: &Operand, instr_len: u8, opts: &FormatOptions) -> String {
    use Operand as O;
    match *op {
        O::R8(r) => reg8(r).into(),
        O::R16(r) => reg16(r).into(),
        O::Imm8(v) => format!("${v:02X}"),
        O::Imm16(v) => abs16(v, opts),
        O::MemBC => "(BC)".into(),
        O::MemDE => "(DE)".into(),
        O::MemHL => "(HL)".into(),
        O::MemHLInc => "(HL+)".into(),
        O::MemHLDec => "(HL-)".into(),
        O::MemImm16(v) => format!("({})", abs16(v, opts)),
        O::HighImm(n) => format!("({})", abs16(0xFF00 + n as u16, opts)),
        O::HighC => "(C)".into(),
        O::SpPlus(e) => {
            if e < 0 {
                format!("SP-${:02X}", -(e as i16))
            } else {
                format!("SP+${e:02X}")
            }
        }
        O::Simm8(e) => {
            if e < 0 {
                format!("-${:02X}", -(e as i16))
            } else {
                format!("${e:02X}")
            }
        }
        O::Rel(d) => match opts.addr {
            Some(base) => {
                let target = base.wrapping_add(instr_len as u16).wrapping_add(d as i16 as u16);
                abs16(target, opts)
            }
            None => {
                if d < 0 {
                    format!("@-${:02X}", -(d as i16))
                } else {
                    format!("@+${d:02X}")
                }
            }
        },
        O::Cond(c) => cond(c).into(),
        O::Bit(b) => b.to_string(),
        O::Rst(t) => format!("${t:02X}"),
    }
}

/// Format one instruction in canonical style.
pub fn format_instruction(instr: &Instruction, opts: &FormatOptions) -> String {
    // STOP's padding byte is shown only when it deviates from $00.
    if instr.mnemonic == Mnemonic::Stop && instr.ops[0] == Some(Operand::Imm8(0)) {
        return "STOP".into();
    }
    // Rel operands need the instruction length; JR is the only user (2
    // bytes for both forms).
    let len = 2;
    let mut out = String::from(mnemonic_str(instr.mnemonic));
    for (i, op) in instr.operands().enumerate() {
        out.push_str(if i == 0 { " " } else { ", " });
        out.push_str(&operand(op, len, opts));
    }
    out
}

/// Format with no address context (relative branches shown as `@±$XX`).
pub fn format_simple(instr: &Instruction) -> String {
    format_instruction(instr, &FormatOptions::default())
}

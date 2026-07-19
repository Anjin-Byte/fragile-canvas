//! The instruction IR — the pivot between bytes and text.
//!
//! `Instruction` is a fully decoded, register-typed value. Four functions
//! surround it: `decode` (bytes → IR), `encode` (IR → bytes), `format`
//! (IR → text), `parse` (text → IR). Disassembly is decode+format;
//! assembly is parse+encode. All four agree by construction of the
//! exhaustive roundtrip tests in `tests/roundtrip.rs`.

/// 8-bit ISA registers (operand positions only — no emulator internals).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Reg8 {
    A,
    B,
    C,
    D,
    E,
    H,
    L,
}

/// 16-bit register pairs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Reg16 {
    AF,
    BC,
    DE,
    HL,
    SP,
}

/// Jump/call/return conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Cond {
    NZ,
    Z,
    NC,
    C,
}

/// One instruction operand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Operand {
    R8(Reg8),
    R16(Reg16),
    Imm8(u8),
    Imm16(u16),
    /// (BC)
    MemBC,
    /// (DE)
    MemDE,
    /// (HL)
    MemHL,
    /// (HL+) — post-increment
    MemHLInc,
    /// (HL-) — post-decrement
    MemHLDec,
    /// (a16) — absolute indirect
    MemImm16(u16),
    /// LDH ($FF00+n) — high-page indirect by immediate
    HighImm(u8),
    /// LDH ($FF00+C) — high-page indirect by C
    HighC,
    /// LD HL, SP+e8
    SpPlus(i8),
    /// Signed 8-bit immediate (ADD SP, e8)
    Simm8(i8),
    /// JR displacement (relative to the address AFTER the instruction)
    Rel(i8),
    Cond(Cond),
    /// BIT/RES/SET bit index 0-7
    Bit(u8),
    /// RST target: $00, $08, ... $38
    Rst(u8),
}

/// Instruction mnemonics. `Data` represents an illegal/unknown opcode byte,
/// formatted as a `DB` directive so listings always reassemble.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mnemonic {
    Nop,
    Ld,
    Ldh,
    Inc,
    Dec,
    Add,
    Adc,
    Sub,
    Sbc,
    And,
    Xor,
    Or,
    Cp,
    Jp,
    Jr,
    Call,
    Ret,
    Reti,
    Rst,
    Push,
    Pop,
    Rlca,
    Rrca,
    Rla,
    Rra,
    Daa,
    Cpl,
    Scf,
    Ccf,
    Halt,
    Stop,
    Di,
    Ei,
    // CB-prefixed
    Rlc,
    Rrc,
    Rl,
    Rr,
    Sla,
    Sra,
    Swap,
    Srl,
    Bit,
    Res,
    Set,
    /// Raw data byte (illegal opcode or trailing fragment).
    Data,
}

/// A decoded instruction: mnemonic + up to two operands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Instruction {
    pub mnemonic: Mnemonic,
    pub ops: [Option<Operand>; 2],
}

impl Instruction {
    pub const fn nullary(mnemonic: Mnemonic) -> Self {
        Self { mnemonic, ops: [None, None] }
    }

    pub const fn unary(mnemonic: Mnemonic, a: Operand) -> Self {
        Self { mnemonic, ops: [Some(a), None] }
    }

    pub const fn binary(mnemonic: Mnemonic, a: Operand, b: Operand) -> Self {
        Self { mnemonic, ops: [Some(a), Some(b)] }
    }

    /// Operands in order, skipping empty slots.
    pub fn operands(&self) -> impl Iterator<Item = &Operand> {
        self.ops.iter().filter_map(|o| o.as_ref())
    }

    pub fn op_count(&self) -> usize {
        self.ops.iter().filter(|o| o.is_some()).count()
    }
}

/// A decode result: the instruction plus its encoded length in bytes (1-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decoded {
    pub instr: Instruction,
    pub len: u8,
}

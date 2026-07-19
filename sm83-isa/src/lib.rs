//! # sm83-isa
//!
//! SM83 (Game Boy CPU) instruction set as data: disassembler, assembler,
//! and instruction metadata. Standalone — the `sm83` emulator crate is a
//! dev-dependency only (used as a length oracle in tests).
//!
//! The core is an instruction IR ([`Instruction`]) with four functions
//! around it:
//!
//! ```text
//!  bytes ──decode──▶ Instruction ──format──▶ text
//!  bytes ◀─encode── Instruction ◀──parse──  text
//! ```
//!
//! Disassembly = decode + format. Assembly = parse + encode; the two-pass
//! assembler in [`asm`] layers labels, directives, and expressions on top.
//! `encode(decode(x)) == x` for every opcode is enforced by exhaustive
//! tests, which is what keeps the two directions from drifting.

pub mod asm;
pub mod cycles;
pub mod decode;
pub mod encode;
pub mod format;
pub mod model;
pub mod parse;

pub use cycles::{cycles, Cycles};
pub use decode::{decode, instruction_len, ILLEGAL_OPCODES};
pub use encode::{encode, EncodeError, Encoded};
pub use format::{format_instruction, format_simple, mnemonic_str, FormatOptions};
pub use model::{Cond, Decoded, Instruction, Mnemonic, Operand, Reg16, Reg8};
pub use parse::{parse_instruction, ParseError};

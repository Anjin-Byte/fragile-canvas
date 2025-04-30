use crate::cpu::microcode::*;
use crate::cpu::registers::{RegisterFile, Reg8};
use std::collections::VecDeque;

pub fn decode_instruction(opcode: u8, _rf: &RegisterFile) -> VecDeque<MicroOp> {
    match opcode {
        0x00 => VecDeque::new(), // NOP
        0x3E => VecDeque::from([
            MicroOp::ReadImmediate8 { reg: Reg8::A },
        ]),
        _ => panic!("Unimplemented opcode: {:02X}", opcode),
    }
}
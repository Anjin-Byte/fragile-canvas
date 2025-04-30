use crate::cpu::CPU;

use super::registers::{Reg8, Reg16, Flag};

mod cycles {
    const CYCLES_LOOKUP: [u8; 256] = [
    // x0  x1  x2  x3  x4  x5  x6  x7  x8  x9  xA  xB  xC  xD  xE  xF
        4, 12,  8,  8,  4,  4,  8,  4, 20,  8,  8,  8,  4,  4,  8,  4,  // 0x
        4, 12,  8,  8,  4,  4,  8,  4,  8,  8,  8,  8,  4,  4,  8,  4,  // 1x
        8, 12,  8,  8,  4,  4,  8,  4,  8,  8,  8,  8,  4,  4,  8,  4,  // 2x
        8, 12,  8,  8, 12, 12, 12,  4,  8,  8,  8,  8,  4,  4,  8,  4,  // 3x
        4,  4,  4,  4,  4,  4,  8,  4,  4,  4,  4,  4,  4,  4,  8,  4,  // 4x
        4,  4,  4,  4,  4,  4,  8,  4,  4,  4,  4,  4,  4,  4,  8,  4,  // 5x
        4,  4,  4,  4,  4,  4,  8,  4,  4,  4,  4,  4,  4,  4,  8,  4,  // 6x
        8,  8,  8,  8,  8,  8,  4,  8,  4,  4,  4,  4,  4,  4,  8,  4,  // 7x
        4,  4,  4,  4,  4,  4,  8,  4,  4,  4,  4,  4,  4,  4,  8,  4,  // 8x
        4,  4,  4,  4,  4,  4,  8,  4,  4,  4,  4,  4,  4,  4,  8,  4,  // 9x
        4,  4,  4,  4,  4,  4,  8,  4,  4,  4,  4,  4,  4,  4,  8,  4,  // Ax
        4,  4,  4,  4,  4,  4,  8,  4,  4,  4,  4,  4,  4,  4,  8,  4,  // Bx
        8, 12, 12, 12, 12, 16,  8, 32,  8,  8, 12,  8, 12, 12,  8, 32,  // Cx
        8, 12, 12,  0, 12, 16,  8, 32,  8,  8, 12,  0, 12,  0,  8, 32,  // Dx
        12, 12,  8,  0,  0, 16,  8, 32, 16,  4, 16,  0,  0,  0,  8, 32, // Ex
        12, 12,  8,  4,  0, 16,  8, 32, 12,  8, 16,  4,  0,  0,  8, 32, // Fx
    ];
}

#[derive(Debug)]
pub enum MicroOp {
    ReadImmediate8 { reg: Reg8 },
    ReadImmediate16 { reg: Reg16 },

    ReadReg8 { reg: Reg8 },
    WriteReg8 { dst: Reg8, val: u8 },

    ReadReg16 { reg: Reg16 },
    WriteReg16 { dst: Reg16, val: u16 },

    IncrementReg8 { reg: Reg8 },
    DecrementReg8 { reg: Reg8 },

    IncrementReg16 { reg: Reg16 },
    DecrementReg16 { reg: Reg16 },

    ReadMem { addr: u16 },
    WriteMem { addr: u16, val: u8 },

    // Indirect addressing through registers (like [HL])
    ReadMemReg { addr_reg: Reg16 },
    WriteMemReg { addr_reg: Reg16, val: Reg8 },

    AluOp { kind: AluOpKind, lhs: Reg8, rhs: Operand8 },

    SetFlag { flag: Flag, value: bool },
    UpdateFlags { result: u8, kind: AluOpKind },

    PushStack16 { src: Reg16 },
    PopStack16 { dst: Reg16 },

    UpdatePC { addr: u16 },
    AddRelativePC { offset: i8 },

    // **Interrupt and Control Flow Operations**
    EnableInterrupts,   // EI instruction
    DisableInterrupts,  // DI instruction
    CheckInterrupts,    // Check for pending interrupts
    ServiceInterrupt { vector: u16 },

    // **Pipeline and Timing Control**
    Delay,              // Used explicitly to model pipeline delays or idle cycles
}

#[derive(Debug)]
pub enum AluOpKind { Add, Sub, Xor, And, Or, }

#[derive(Debug)]
pub enum Operand8 { Reg(Reg8), Imm(u8) }

pub fn execute_microop(cpu: &mut CPU, op: MicroOp) {
    match op {
        MicroOp::ReadImmediate8 { reg } => todo!(),
        MicroOp::ReadImmediate16 { reg } => todo!(),
        MicroOp::ReadReg8 { reg } => todo!(),
        MicroOp::WriteReg8 { dst, val } => todo!(),
        MicroOp::ReadReg16 { reg } => todo!(),
        MicroOp::WriteReg16 { dst, val } => todo!(),
        MicroOp::IncrementReg8 { reg } => todo!(),
        MicroOp::DecrementReg8 { reg } => todo!(),
        MicroOp::IncrementReg16 { reg } => todo!(),
        MicroOp::DecrementReg16 { reg } => todo!(),
        MicroOp::ReadMem { addr } => todo!(),
        MicroOp::WriteMem { addr, val } => todo!(),
        MicroOp::ReadMemReg { addr_reg } => todo!(),
        MicroOp::WriteMemReg { addr_reg, val } => todo!(),
        MicroOp::AluOp { kind, lhs, rhs } => todo!(),
        MicroOp::SetFlag { flag, value } => todo!(),
        MicroOp::UpdateFlags { result, kind } => todo!(),
        MicroOp::PushStack16 { src } => todo!(),
        MicroOp::PopStack16 { dst } => todo!(),
        MicroOp::UpdatePC { addr } => todo!(),
        MicroOp::AddRelativePC { offset } => todo!(),
        MicroOp::EnableInterrupts => todo!(),
        MicroOp::DisableInterrupts => todo!(),
        MicroOp::CheckInterrupts => todo!(),
        MicroOp::ServiceInterrupt { vector } => todo!(),
        MicroOp::Delay => todo!(),
    }
}
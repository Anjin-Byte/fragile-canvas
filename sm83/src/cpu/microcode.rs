use crate::cpu::CPU;

use super::{alu::alu, registers::{Flag, Reg16, Reg8}};

#[derive(Debug)]
pub enum MicroOp {
    ReadImmediate8 { into: Reg8 },
    ReadImmediate16 { into: Reg16 },

    WriteReg8 { dst: Reg8, val: u8 },
    WriteReg16 { dst: Reg16, val: u16 },

    IncrementReg8 { reg: Reg8 },
    DecrementReg8 { reg: Reg8 },

    IncrementReg16 { reg: Reg16 },
    DecrementReg16 { reg: Reg16 },

    WriteMem { addr: u16, val: u8 },
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
        MicroOp::ReadImmediate8 { into: reg } => { // x2 memory access 
            let pc = cpu.register_file.get_16bit(&Reg16::PC);
            let val = cpu.bus.borrow_mut().read(pc);  // memory access
            cpu.register_file.inc16(&Reg16::PC); 

            cpu.register_file.set_8bit(&reg, val);
        },
        MicroOp::ReadImmediate16 { into: reg } => {
            let pc = cpu.register_file.get_16bit(&Reg16::PC);
            let lo = cpu.bus.borrow_mut().read(pc);
            cpu.register_file.inc16(&Reg16::PC);

            let pc = cpu.register_file.get_16bit(&Reg16::PC);
            let hi = cpu.bus.borrow_mut().read(pc);  // memory access
            cpu.register_file.inc16(&Reg16::PC);

            let value = u16::from_le_bytes([lo, hi]);
            cpu.register_file.set_16bit(&reg, value);
        },
        MicroOp::WriteReg8 { dst, val } => {
            cpu.register_file.set_8bit(&dst, val);    
        },
        MicroOp::WriteReg16 { dst, val } => {
            cpu.register_file.set_16bit(&dst, val);
        },
        MicroOp::IncrementReg8 { reg } => {
            cpu.register_file.inc8(&reg);
        },
        MicroOp::DecrementReg8 { reg } => {
            cpu.register_file.dec8(&reg);
        },
        MicroOp::IncrementReg16 { reg } => {
            cpu.register_file.inc16(&reg);
        },
        MicroOp::DecrementReg16 { reg } => {
            cpu.register_file.dec16(&reg);
        },
        MicroOp::WriteMem { addr, val } => {
            cpu.bus.borrow_mut().write(addr, val);
        }
        MicroOp::WriteMemReg { addr_reg, val } => {
            let addr = cpu.register_file.get_16bit(&addr_reg);
            let v = cpu.register_file.get_8bit(&val);
            cpu.bus.borrow_mut().write(addr, v);
        }
        MicroOp::AluOp { kind, lhs, rhs } => {
            let a = cpu.register_file.get_8bit(&lhs);
            let b = match rhs {
              Operand8::Reg(r) => cpu.register_file.get_8bit(&r),
              Operand8::Imm(i)  => i,
            };

            let res = alu(kind, a, b);
            cpu.register_file.set_8bit(&lhs, res.value);

            cpu.register_file.write_flag(Flag::Zero,      res.z);
            cpu.register_file.write_flag(Flag::Subtract,  res.n);
            cpu.register_file.write_flag(Flag::HalfCarry, res.h);
            cpu.register_file.write_flag(Flag::Carry,     res.c);
        }
        MicroOp::SetFlag { flag, value } => todo!(),
        MicroOp::UpdateFlags { result, kind } => todo!(),
        MicroOp::PushStack16 { src } => todo!(),
        MicroOp::PopStack16 { dst } => todo!(),
        MicroOp::UpdatePC { addr } => {
            cpu.register_file.set_16bit(&Reg16::PC, addr);
        }
        MicroOp::AddRelativePC { offset } => {
            let pc = cpu.register_file.get_16bit(&Reg16::PC);
            let new_pc = pc.wrapping_add(offset as i16 as u16);
            cpu.register_file.set_16bit(&Reg16::PC, new_pc);
        }
        MicroOp::EnableInterrupts => todo!(),
        MicroOp::DisableInterrupts => todo!(),
        MicroOp::CheckInterrupts => todo!(),
        MicroOp::ServiceInterrupt { vector } => todo!(),
        MicroOp::Delay => todo!(),
    }
}
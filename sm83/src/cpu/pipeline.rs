use crate::cpu::{CPU, microcode::MicroOp};
use std::collections::VecDeque;

use super::registers::Reg16;

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

#[derive(Debug)]
pub enum PipelineState {
    Fetch,
    Decode(u8),
    Execute(VecDeque<MicroOp>),
    Halted,
    InterruptService(VecDeque<MicroOp>),
}

pub fn pipeline(cpu: &mut CPU) {
    match &mut cpu.state {
        PipelineState::Fetch => {
            let pc = cpu.register_file.get_16bit(&Reg16::PC);
            let opcode = cpu.bus.borrow_mut().read(pc);
            cpu.register_file.inc16(&Reg16::PC);

            cpu.state = PipelineState::Decode(opcode);
        }
        PipelineState::Decode(opcode) => {
            let micro_ops = crate::cpu::decoder::decode_instruction(*opcode, &cpu.register_file);
            cpu.state = PipelineState::Execute(micro_ops);
        }
        PipelineState::Execute(ops) => {
            if let Some(op) = ops.pop_front() {
                crate::cpu::microcode::execute_microop(cpu, op);
            } else {
                cpu.state = PipelineState::Fetch;
            }
        }
        PipelineState::InterruptService(ops) => {
            if let Some(op) = ops.pop_front() {
                crate::cpu::microcode::execute_microop(cpu, op);
            } else {
                // cpu.interrupts.ime = true;
                cpu.state = PipelineState::Fetch;
            }
        }
        PipelineState::Halted => {
        }
    }
}
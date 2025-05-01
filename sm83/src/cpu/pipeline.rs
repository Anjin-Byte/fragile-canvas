use crate::cpu::{CPU, microcode::MicroOp};
use std::collections::VecDeque;

use super::registers::Reg16;

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
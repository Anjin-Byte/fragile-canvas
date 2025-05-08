use crate::cpu::{CPU, microcode::MicroOp};
use std::collections::VecDeque;

use super::{decoder, microcode, registers::Reg16};

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
            /* 
            microcode::execute(cpu, MicroOp::WriteMemReg { 
                addr_reg: Reg16::PC, 
                val: super::registers::Reg8::IR 
            });
            */
            cpu.state = PipelineState::Decode(opcode);
        }
        PipelineState::Decode(opcode) => {
            /*
            Not technically what I had in mind - 
            Could have a seperate stage in the pipeline for
            CB decode but this works as well for now. 
             */
            let micro_ops = if *opcode == 0xCB {
                let pc = cpu.register_file.get_16bit(&Reg16::PC);
                let cb_code = cpu.bus.borrow_mut().read(pc);
                cpu.register_file.inc16(&Reg16::PC);
        
                decoder::decode_cb_instruction(cb_code)
            } else {
                decoder::decode_instruction(*opcode)
            };

            cpu.state = PipelineState::Execute(micro_ops);
        }
        PipelineState::Execute(ops) => {
            if let Some(op) = ops.pop_front() {
                microcode::execute(cpu, op);
            } else {
                cpu.state = PipelineState::Fetch;
            }
        }
        PipelineState::InterruptService(ops) => {
            if let Some(op) = ops.pop_front() {
                microcode::execute(cpu, op);
            } else {
                // cpu.interrupts.ime = true;
                cpu.state = PipelineState::Fetch;
            }
        }
        PipelineState::Halted => {
        }
    }
}
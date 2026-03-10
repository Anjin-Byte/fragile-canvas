pub mod alu;
pub mod decoder;
pub mod interrupts;
pub mod microcode;
pub mod pipeline;
pub mod registers;

use crate::memory::bus::Bus;
use crate::trace::Tracer;
use pipeline::PipelineState;
use registers::{Reg16, RegisterFile};

pub struct CPU<B: Bus> {
    pub register_file: RegisterFile,
    pub bus: B,
    pub state: PipelineState,
    pub ime: bool,
    pub ime_defer: bool,
    pub tracer: Tracer,
}

impl<B: Bus> CPU<B> {
    pub fn new(bus: B, tracer: Tracer) -> Self {
        Self {
            register_file: RegisterFile::new(),
            bus,
            state: PipelineState::Fetch,
            ime: false,
            ime_defer: false,
            tracer,
        }
    }

    pub fn tick(&mut self) {
        match &mut self.state {
            PipelineState::Fetch => {
                let pc = self.register_file.get_16bit(Reg16::PC);
                let opcode = self.bus.read(pc);
                self.register_file.inc16(Reg16::PC);
                self.state = PipelineState::Decode(opcode);
            }
            PipelineState::Decode(opcode) => {
                let micro_ops = if *opcode == 0xCB {
                    let pc = self.register_file.get_16bit(Reg16::PC);
                    let cb_code = self.bus.read(pc);
                    self.register_file.inc16(Reg16::PC);
                    let ops = decoder::decode_cb_instruction(cb_code);
                    if self.tracer.enabled() {
                        self.tracer.instruction(
                            &self.register_file,
                            &format!("CB {:02X}", cb_code),
                            &ops,
                        );
                    }
                    ops
                } else {
                    let ops = decoder::decode_instruction(*opcode);
                    if self.tracer.enabled() {
                        self.tracer.instruction(
                            &self.register_file,
                            &format!("{:02X}", *opcode),
                            &ops,
                        );
                    }
                    ops
                };
                self.state = PipelineState::Execute(micro_ops);
            }
            PipelineState::Execute(ops) => {
                if !ops.is_empty() {
                    let op = ops.remove(0);
                    microcode::execute(self, op);
                } else {
                    self.state = PipelineState::Fetch;
                }
            }
            PipelineState::InterruptService(ops) => {
                if !ops.is_empty() {
                    let op = ops.remove(0);
                    microcode::execute(self, op);
                } else {
                    self.state = PipelineState::Fetch;
                }
            }
            PipelineState::Halted => {}
        }
    }
}


#[cfg(test)]
mod integration_tests {
    use crate::{cpu::registers::Reg16, memory::mmu::MMU};
    use crate::memory::bus::Bus;
    use crate::trace::Tracer;

    use super::*;
    use indicatif::ProgressBar;
    use rand::Rng;

    #[test]
    fn test_memory_register_integration() {
        let mut cpu = CPU::new(MMU::new(), Tracer::off());
        let mut rng = rand::thread_rng();

        let n: u64 = 1000000;
        let bar = ProgressBar::new(n);
        for _ in 0..=n {
            bar.inc(1);
            let bc_addr_data = rng.gen_range(0x8000..=0x9FFF);
            let de_addr_data = rng.gen_range(0xA000..=0xBFFF);
            let hl_addr_data = rng.gen_range(0xC000..=0xDFFF);

            cpu.register_file.set_16bit(Reg16::BC, bc_addr_data);
            cpu.register_file.set_16bit(Reg16::DE, de_addr_data);
            cpu.register_file.set_16bit(Reg16::HL, hl_addr_data);

            let bc_memory_dummy = rng.gen_range(0x00..0xFF);
            let de_memory_dummy = rng.gen_range(0x00..0xFF);
            let hl_memory_dummy = rng.gen_range(0x00..0xFF);

            cpu.bus.write(cpu.register_file.get_16bit(Reg16::BC), bc_memory_dummy);
            cpu.bus.write(cpu.register_file.get_16bit(Reg16::DE), de_memory_dummy);
            cpu.bus.write(cpu.register_file.get_16bit(Reg16::HL), hl_memory_dummy);

            assert_eq!(
                cpu.bus.read(bc_addr_data),
                bc_memory_dummy,
                "Register BC failed integration test..."
            );
            assert_eq!(
                cpu.bus.read(de_addr_data),
                de_memory_dummy,
                "Register DE failed integration test..."
            );
            assert_eq!(
                cpu.bus.read(hl_addr_data),
                hl_memory_dummy,
                "Register HL failed integration test..."
            );
        }
        bar.finish();
    }
}

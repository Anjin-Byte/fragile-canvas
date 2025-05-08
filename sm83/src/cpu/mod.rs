pub mod alu;
pub mod decoder;
pub mod interrupts;
pub mod microcode;
pub mod pipeline;
pub mod registers;

use crate::memory::bus::Bus;
use pipeline::{pipeline, PipelineState};
use registers::RegisterFile;
use std::cell::RefCell;
use std::rc::Rc;

pub struct CPU {
    pub register_file: RegisterFile,
    pub bus: Rc<RefCell<dyn Bus>>,
    pub state: PipelineState,
}

impl CPU {
    pub fn new(bus: Rc<RefCell<dyn Bus>>) -> Self {
        Self {
            register_file: RegisterFile::new(),
            bus,
            state: PipelineState::Fetch,
        }
    }

    pub fn tick(&mut self) {
        self.log(">");
        pipeline(self);
    }

    fn log(&self, message: &str) {
        println!("Pipeline stage: {:?}", self.state);
        println!(
            "{}{}",
            message,
            self.register_file
        );
    }
}


#[cfg(test)]
mod integration_tests {
    use crate::{cpu::registers::Reg16, memory::mmu::MMU};

    use super::*;
    use indicatif::ProgressBar;
    use rand::Rng;

    #[test]
    fn test_memory_register_integration() {
        let mmu = MMU::new();
        let shared_bus = Rc::new(RefCell::new(mmu));
        let mut cpu = CPU::new(shared_bus);
        let mut rng = rand::thread_rng();

        let n: u64 = 1000000;
        let bar = ProgressBar::new(n);
        for _ in 0..=n {
            bar.inc(1);
            let bc_addr_data = rng.gen_range(0x8000..=0x9FFF);
            let de_addr_data = rng.gen_range(0xA000..=0xBFFF);
            let hl_addr_data = rng.gen_range(0xC000..=0xDFFF);

            cpu.register_file.set_16bit(&Reg16::BC, bc_addr_data);
            cpu.register_file.set_16bit(&Reg16::DE, de_addr_data);
            cpu.register_file.set_16bit(&Reg16::HL, hl_addr_data);

            let bc_memory_dummy = rng.gen_range(0x00..0xFF);
            let de_memory_dummy = rng.gen_range(0x00..0xFF);
            let hl_memory_dummy = rng.gen_range(0x00..0xFF);

            cpu.bus
                .borrow_mut()
                .write(cpu.register_file.get_16bit(&Reg16::BC), bc_memory_dummy);
            cpu.bus
                .borrow_mut()
                .write(cpu.register_file.get_16bit(&Reg16::DE), de_memory_dummy);
            cpu.bus
                .borrow_mut()
                .write(cpu.register_file.get_16bit(&Reg16::HL), hl_memory_dummy);

            assert_eq!(
                cpu.bus.borrow_mut().read(bc_addr_data),
                bc_memory_dummy,
                "Register BC failed integration test..."
            );
            assert_eq!(
                cpu.bus.borrow_mut().read(de_addr_data),
                de_memory_dummy,
                "Register DE failed integration test..."
            );
            assert_eq!(
                cpu.bus.borrow_mut().read(hl_addr_data),
                hl_memory_dummy,
                "Register HL failed integration test..."
            );
        }
        bar.finish();
    }
}
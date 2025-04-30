pub mod alu;
pub mod decoder;
pub mod interrupts;
pub mod microcode;
pub mod pipeline;
pub mod registers;

use crate::memory::bus::Bus;
use pipeline::{pipeline, PipelineState};
use registers::{Reg16, Reg8, RegisterFile};
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
        self.log("register_file: ");
        pipeline(self);
    }

    fn log(&self, message: &str) {
        println!(
            "{:<015} | PC: {:#^04X} | IR: {:#^02X} | mem[PC]: {:#X}",
            message,
            self.register_file.get_16bit(&Reg16::PC),
            self.register_file.get_8bit(&Reg8::IR),
            self.bus
                .borrow_mut()
                .read(self.register_file.get_16bit(&Reg16::PC))
        );
    }
}

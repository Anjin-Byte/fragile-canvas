#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::fs::File;
    use std::io::{BufWriter, Write};

    use crate::cpu::decoder::MicrocodeQueue;
    use crate::cpu::registers::{Reg16, RegisterFile};

    pub struct Tracer {
        writer: Option<BufWriter<File>>,
    }

    impl Tracer {
        pub fn off() -> Self {
            Self { writer: None }
        }

        pub fn to_file(file: File) -> Self {
            Self { writer: Some(BufWriter::new(file)) }
        }

        pub fn enabled(&self) -> bool {
            self.writer.is_some()
        }

        pub fn instruction(&mut self, regs: &RegisterFile, opcode: &str, ops: &MicrocodeQueue) {
            if let Some(w) = &mut self.writer {
                let _ = writeln!(
                    w,
                    "PC:{:04X} SP:{:04X} AF:{:04X} BC:{:04X} DE:{:04X} HL:{:04X} | {:<7} {:?}",
                    regs.get_16bit(Reg16::PC),
                    regs.get_16bit(Reg16::SP),
                    regs.get_16bit(Reg16::AF),
                    regs.get_16bit(Reg16::BC),
                    regs.get_16bit(Reg16::DE),
                    regs.get_16bit(Reg16::HL),
                    opcode,
                    ops,
                );
            }
        }

        pub fn event(&mut self, message: &str) {
            if let Some(w) = &mut self.writer {
                let _ = writeln!(w, "--- {} ---", message);
            }
        }

        pub fn flush(&mut self) {
            if let Some(w) = &mut self.writer {
                let _ = w.flush();
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use crate::cpu::decoder::MicrocodeQueue;
    use crate::cpu::registers::RegisterFile;

    pub struct Tracer;

    impl Tracer {
        pub fn off() -> Self { Self }
        pub fn enabled(&self) -> bool { false }
        pub fn instruction(&mut self, _regs: &RegisterFile, _opcode: &str, _ops: &MicrocodeQueue) {}
        pub fn event(&mut self, _message: &str) {}
        pub fn flush(&mut self) {}
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::Tracer;

#[cfg(target_arch = "wasm32")]
pub use wasm::Tracer;

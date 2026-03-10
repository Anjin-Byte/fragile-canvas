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

#[cfg(test)]
mod boot_rom_tests {
    use crate::cpu::registers::{Reg8, Reg16};
    use crate::memory::bus::Bus;
    use crate::memory::mmu::MMU;
    use crate::trace::Tracer;

    use super::*;

    const MAX_TICKS: u64 = 10_000_000;

    // Nintendo logo bytes that the boot ROM expects at 0x0104-0x0133 in the cartridge
    const NINTENDO_LOGO: [u8; 48] = [
        0xCE, 0xED, 0x66, 0x66, 0xCC, 0x0D, 0x00, 0x0B,
        0x03, 0x73, 0x00, 0x83, 0x00, 0x0C, 0x00, 0x0D,
        0x00, 0x08, 0x11, 0x1F, 0x88, 0x89, 0x00, 0x0E,
        0xDC, 0xCC, 0x6E, 0xE6, 0xDD, 0xDD, 0xD9, 0x99,
        0xBB, 0xBB, 0x67, 0x63, 0x6E, 0x0E, 0xEC, 0xCC,
        0xDD, 0xDC, 0x99, 0x9F, 0xBB, 0xB9, 0x33, 0x3E,
    ];

    fn make_boot_cpu() -> CPU<MMU> {
        let mut mmu = MMU::new();
        mmu.load_boot_rom(crate::BOOT_ROM).unwrap();

        // Build a minimal cartridge with a valid Nintendo logo at 0x0104
        let mut cart = vec![0u8; 0x150];
        cart[0x0104..0x0134].copy_from_slice(&NINTENDO_LOGO);

        // Header checksum at 0x014D: sum of -(byte+1) for 0x0134..=0x014C
        // All 25 bytes are zero, so checksum = (-1)*25 = 0xE7
        cart[0x014D] = 0xE7;

        mmu.load_cartridge(&cart);

        // Pre-set LY (0xFF44) to 0x90 so the boot ROM's VBlank wait loop
        // exits immediately. Without a PPU, LY would stay at 0 forever.
        mmu.write(0xFF44, 0x90);

        CPU::new(mmu, Tracer::off())
    }

    fn tick_until(cpu: &mut CPU<MMU>, pc: u16) -> u64 {
        for t in 0..MAX_TICKS {
            // Only match PC at Fetch boundaries so the previous instruction
            // has fully completed (all its microops have executed).
            if matches!(cpu.state, PipelineState::Fetch)
                && cpu.register_file.get_16bit(Reg16::PC) == pc
            {
                return t;
            }
            cpu.tick();
            cpu.bus.write(0xFF44, 0x90);
        }
        panic!("CPU did not reach PC={:#06X} within {} ticks", pc, MAX_TICKS);
    }

    #[test]
    fn vram_zeroed_after_boot_init() {
        let mut cpu = make_boot_cpu();
        // The boot ROM zeroes VRAM (0x8000-0x9FFF) early on.
        // The zero loop ends when HL wraps from 0x9FFF back around.
        // After the loop, PC moves past the VRAM-clear routine (~0x000C).
        tick_until(&mut cpu, 0x000C);

        for addr in 0x8000..=0x9FFFu16 {
            assert_eq!(
                cpu.bus.read(addr), 0,
                "VRAM at {:#06X} was not zeroed", addr
            );
        }
    }

    #[test]
    fn nintendo_logo_tiles_in_vram() {
        let mut cpu = make_boot_cpu();
        // The boot ROM copies logo tile data into VRAM at 0x8010-0x809F.
        // By the time PC reaches the scroll/display section (~0x0040),
        // the tiles should be written.
        tick_until(&mut cpu, 0x0040);

        let mut all_zero = true;
        for addr in 0x8010..=0x809Fu16 {
            if cpu.bus.read(addr) != 0 {
                all_zero = false;
                break;
            }
        }
        assert!(!all_zero, "Nintendo logo tiles were not written to VRAM");
    }

    #[test]
    fn logo_comparison_passes() {
        let mut cpu = make_boot_cpu();
        // If the logo comparison fails, the boot ROM locks into an infinite
        // loop and never reaches 0x00E0+. Getting past 0x00E0 means it passed.
        tick_until(&mut cpu, 0x00E8);
    }

    #[test]
    fn boot_rom_unmapped() {
        let mut cpu = make_boot_cpu();
        tick_until(&mut cpu, 0x0100);

        // 0xFF50 bit 0 set means boot ROM is unmapped
        assert_ne!(
            cpu.bus.read(0xFF50) & 1, 0,
            "boot ROM was not unmapped (0xFF50 bit 0 not set)"
        );
    }

    #[test]
    fn pc_reaches_cartridge_entry() {
        let mut cpu = make_boot_cpu();
        tick_until(&mut cpu, 0x0100);
    }

    #[test]
    fn registers_after_boot() {
        let mut cpu = make_boot_cpu();
        tick_until(&mut cpu, 0x0100);

        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x01, "A != 0x01");
        assert_eq!(cpu.register_file.get_8bit(Reg8::F), 0xB0, "F != 0xB0");
        assert_eq!(cpu.register_file.get_8bit(Reg8::B), 0x00, "B != 0x00");
        assert_eq!(cpu.register_file.get_8bit(Reg8::C), 0x13, "C != 0x13");
        assert_eq!(cpu.register_file.get_8bit(Reg8::D), 0x00, "D != 0x00");
        assert_eq!(cpu.register_file.get_8bit(Reg8::E), 0xD8, "E != 0xD8");
        assert_eq!(cpu.register_file.get_8bit(Reg8::H), 0x01, "H != 0x01");
        assert_eq!(cpu.register_file.get_8bit(Reg8::L), 0x4D, "L != 0x4D");
        assert_eq!(cpu.register_file.get_16bit(Reg16::SP), 0xFFFE, "SP != 0xFFFE");
    }
}

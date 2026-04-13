pub mod alu;
pub mod decoder;
pub mod interrupts;
pub mod mcycle_dispatch;
pub mod microcode;
pub mod pipeline;
pub mod registers;

use crate::memory::bus::Bus;
use crate::trace::Tracer;
use registers::{Reg8, Reg16, RegisterFile};

/// Interrupt dispatch takes 5 M-cycles = 20 T-cycles on the SM83.
const INTERRUPT_T_CYCLES: u8 = 20;

pub struct CPU {
    pub register_file: RegisterFile,
    pub halted: bool,
    pub ime: bool,
    pub ime_defer: bool,
    /// Set to false by CheckCond when the condition fails.
    /// The tick loop uses this to stop executing remaining micro-ops
    /// and to choose taken vs not-taken cycle counts.
    pub condition_taken: bool,
    pub tracer: Tracer,

    // ── M-cycle state machine fields ─────────────────────────────────
    // These track progress within multi-M-cycle instructions for the
    // new `step_m()` pipeline.  See `assets/docs/m_cycle_design/`.

    /// Opcode being executed. 0x000-0x0FF = base, 0x100-0x1FF = CB.
    current_opcode: u16,
    /// Which M-cycle of the current instruction (0 = ready to fetch).
    mcycle: u8,
    /// Temporary byte storage for values spanning M-cycles.
    temp_lo: u8,
    temp_hi: u8,
    /// Assembled 16-bit address from temp_lo/temp_hi.
    temp_addr: u16,
    /// True when executing the 5-M-cycle interrupt dispatch sequence.
    in_interrupt_dispatch: bool,
    /// Interrupt vector for the current dispatch.
    interrupt_vector: u16,
    /// HALT bug: when HALT executes with IME=0 and an interrupt pending,
    /// the CPU wakes without servicing the interrupt and the PC fails to
    /// increment on the next fetch, causing the following byte to be read
    /// twice.  This flag is consumed after one fetch.
    halt_bug_active: bool,
}

impl CPU {
    pub fn new(tracer: Tracer) -> Self {
        Self {
            register_file: RegisterFile::new(),
            halted: false,
            ime: false,
            ime_defer: false,
            condition_taken: true,
            tracer,
            current_opcode: 0,
            mcycle: 0,
            temp_lo: 0,
            temp_hi: 0,
            temp_addr: 0,
            in_interrupt_dispatch: false,
            interrupt_vector: 0,
            halt_bug_active: false,
        }
    }

    /// Execute one full instruction and return the number of T-cycles consumed.
    ///
    /// On a real SM83, fetch + decode happens in 1 M-cycle, and every
    /// additional memory access or internal operation is 1 more M-cycle.
    /// We execute all micro-ops at once and use the lookup table for the
    /// authoritative cycle count.
    pub fn tick(&mut self, bus: &mut Bus) -> u8 {
        // 1. Check for pending interrupts (runs before instruction fetch).
        //    An interrupt wakes the CPU from HALT regardless of IME.
        //    IE is memory-mapped at 0xFFFF; read it from the bus so that
        //    game writes via LD (0xFFFF),A are immediately visible here.
        let ie = bus.read(0xFFFF);
        let if_reg = bus.read(0xFF0F);
        let pending = ie & if_reg & 0x1F;

        if pending != 0 {
            self.halted = false;

            if self.ime {
                // Service highest-priority (lowest bit) interrupt.
                let bit = pending.trailing_zeros() as u8;
                let vector = 0x0040 + (bit as u16) * 0x08;

                bus.write(0xFF0F, if_reg & !(1 << bit));
                self.ime = false;

                // Push PC and jump to vector.
                let pc = self.register_file.get_16bit(Reg16::PC);
                let [hi, lo] = pc.to_be_bytes();
                self.register_file.dec16(Reg16::SP);
                let sp = self.register_file.get_16bit(Reg16::SP);
                bus.write(sp, hi);
                self.register_file.dec16(Reg16::SP);
                let sp = self.register_file.get_16bit(Reg16::SP);
                bus.write(sp, lo);

                self.register_file.set_16bit(Reg16::PC, vector);
                return INTERRUPT_T_CYCLES;
            }
        }

        // 2. If halted (and no interrupt woke us), burn 1 M-cycle.
        if self.halted {
            return 4;
        }

        // 3. Fetch opcode.
        let pc = self.register_file.get_16bit(Reg16::PC);
        let opcode = bus.read(pc);
        self.register_file.inc16(Reg16::PC);

        // 4. Decode (CB prefix reads a second byte).
        let (ops, cb_opcode) = if opcode == 0xCB {
            let pc2 = self.register_file.get_16bit(Reg16::PC);
            let cb_code = bus.read(pc2);
            self.register_file.inc16(Reg16::PC);
            let ops = decoder::decode_cb_instruction(cb_code);
            if self.tracer.enabled() {
                self.tracer.instruction(
                    &self.register_file,
                    &format!("CB {:02X}", cb_code),
                    &ops,
                );
            }
            (ops, Some(cb_code))
        } else {
            let ops = decoder::decode_instruction(opcode);
            if self.tracer.enabled() {
                self.tracer.instruction(
                    &self.register_file,
                    &format!("{:02X}", opcode),
                    &ops,
                );
            }
            (ops, None)
        };

        // 5. Execute all micro-ops.
        self.condition_taken = true;
        for i in 0..ops.len() {
            microcode::execute(self, bus, ops[i]);
            if !self.condition_taken {
                // Condition failed — skip unread immediates in the remaining ops.
                let skip: u16 = ops[i+1..].iter().map(|op| match op {
                    microcode::MicroOp::JumpRelImm => 1,
                    microcode::MicroOp::JumpAbsImm | microcode::MicroOp::CallImm => 2,
                    _ => 0,
                }).sum();
                if skip > 0 {
                    let cur_pc = self.register_file.get_16bit(Reg16::PC);
                    self.register_file.set_16bit(Reg16::PC, cur_pc.wrapping_add(skip));
                }
                break;
            }
        }

        // 6. Handle EI deferred IME enable.
        //    EI sets ime_defer; IME becomes true after the *next* instruction.
        //    We check *before* promoting so the instruction after EI still
        //    executes with IME off, and the one after that sees IME on.
        if self.ime_defer {
            self.ime_defer = false;
            self.ime = true;
        }

        // 7. Return authoritative T-cycle count from lookup table.
        if let Some(cb) = cb_opcode {
            decoder::cb_t_cycles(cb)
        } else if self.condition_taken {
            decoder::taken_t_cycles(opcode)
        } else {
            decoder::t_cycles(opcode)
        }
    }

    /// Execute exactly one M-cycle (4 T-cycles) of CPU work.
    ///
    /// Returns `MCycleResult::Continue` if the instruction is still in
    /// progress, `InstructionComplete` when it finishes, or `HaltBurn`
    /// if the CPU is halted and no interrupt woke it.
    pub fn step_m(&mut self, bus: &mut Bus) -> pipeline::MCycleResult {
        // ── Interrupt dispatch (5-cycle sequence) ──
        if self.in_interrupt_dispatch {
            return self.step_interrupt(bus);
        }

        // ── HALT ──
        if self.halted {
            let ie = bus.read(0xFFFF);
            let pending = ie & bus.if_reg & 0x1F;
            if pending != 0 {
                self.halted = false;
                if self.ime {
                    self.begin_interrupt_dispatch(bus);
                    return self.step_interrupt(bus);
                }
                // Wake without IME: fall through to fetch next instruction
            } else {
                return pipeline::MCycleResult::HaltBurn;
            }
        }

        // ── M-cycle 0: fetch next opcode ──
        if self.mcycle == 0 {
            // Check for pending interrupts before fetch.
            // Note: EI defer is NOT consumed here — the instruction after EI
            // must execute fully with IME=0.  The defer is consumed when that
            // instruction completes (see InstructionComplete handling below).
            if self.ime {
                let ie = bus.read(0xFFFF);
                let pending = ie & bus.if_reg & 0x1F;
                if pending != 0 {
                    self.begin_interrupt_dispatch(bus);
                    return self.step_interrupt(bus);
                }
            }

            // Fetch opcode
            let pc = self.register_file.get_16bit(Reg16::PC);
            let opcode = bus.read(pc);
            // HALT bug: suppress the PC increment for one fetch
            if self.halt_bug_active {
                self.halt_bug_active = false;
            } else {
                self.register_file.inc16(Reg16::PC);
            }

            // CB prefix: need another fetch cycle
            if opcode == 0xCB {
                self.current_opcode = 0xCB00;
                self.mcycle = 1;
                return pipeline::MCycleResult::Continue;
            }

            self.current_opcode = opcode as u16;

            // 1-M instructions complete immediately during fetch
            if self.is_single_mcycle(opcode) {
                self.execute_m1(opcode, bus);
                // Consume EI defer after the instruction completes.
                // This ensures EI → HALT sees IME=0 during HALT, and
                // IME becomes 1 only at the start of the NEXT instruction.
                if self.ime_defer {
                    self.ime_defer = false;
                    self.ime = true;
                }
                return pipeline::MCycleResult::InstructionComplete {
                    opcode: opcode as u16,
                };
            }

            // Multi-M instruction: advance to M2
            self.mcycle = 1;
            return pipeline::MCycleResult::Continue;
        }

        // ── M-cycles 1+: per-opcode dispatch ──
        let result = self.execute_mcycle(bus);
        if matches!(result, pipeline::MCycleResult::InstructionComplete { .. }) {
            self.mcycle = 0;
            // Consume EI defer after instruction completes.
            if self.ime_defer {
                self.ime_defer = false;
                self.ime = true;
            }
        } else {
            self.mcycle += 1;
        }
        result
    }

    /// Returns true if the opcode is a 1-M-cycle instruction (completes
    /// during the fetch cycle with no additional bus accesses).
    fn is_single_mcycle(&self, opcode: u8) -> bool {
        match opcode {
            0x00 => true, // NOP
            0x10 => true, // STOP
            0x76 => true, // HALT
            0xF3 => true, // DI
            0xFB => true, // EI
            0xE9 => true, // JP HL
            0x27 | 0x2F | 0x37 | 0x3F => true, // DAA, CPL, SCF, CCF
            0x07 | 0x0F | 0x17 | 0x1F => true, // RLCA, RRCA, RLA, RRA
            // INC r (not INC (HL) = 0x34)
            0x04 | 0x0C | 0x14 | 0x1C | 0x24 | 0x2C | 0x3C => true,
            // DEC r (not DEC (HL) = 0x35)
            0x05 | 0x0D | 0x15 | 0x1D | 0x25 | 0x2D | 0x3D => true,
            // LD r, r' (0x40-0x7F excluding 0x76 HALT and column 6 (HL) loads)
            op @ 0x40..=0x7F if op != 0x76 && (op & 0x07) != 6 && ((op >> 3) & 0x07) != 6 => true,
            // ALU A, r (0x80-0xBF excluding column 6)
            op @ 0x80..=0xBF if (op & 0x07) != 6 => true,
            _ => false,
        }
    }
}


#[cfg(test)]
mod integration_tests {
    use crate::cpu::registers::Reg16;
    use crate::memory::bus::Bus;
    use crate::trace::Tracer;

    use super::*;
    use indicatif::ProgressBar;
    use rand::Rng;

    #[test]
    fn test_memory_register_integration() {
        let mut cpu = CPU::new(Tracer::off());
        let mut bus = Bus::new();
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

            bus.write(cpu.register_file.get_16bit(Reg16::BC), bc_memory_dummy);
            bus.write(cpu.register_file.get_16bit(Reg16::DE), de_memory_dummy);
            bus.write(cpu.register_file.get_16bit(Reg16::HL), hl_memory_dummy);

            assert_eq!(
                bus.read(bc_addr_data),
                bc_memory_dummy,
                "Register BC failed integration test..."
            );
            assert_eq!(
                bus.read(de_addr_data),
                de_memory_dummy,
                "Register DE failed integration test..."
            );
            assert_eq!(
                bus.read(hl_addr_data),
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

    fn make_boot_cpu() -> (CPU, Bus) {
        let mut bus = Bus::new();
        bus.load_boot_rom(crate::BOOT_ROM).unwrap();

        // Build a minimal cartridge with a valid Nintendo logo at 0x0104
        let mut cart = vec![0u8; 0x150];
        cart[0x0104..0x0134].copy_from_slice(&NINTENDO_LOGO);

        // Header checksum at 0x014D: sum of -(byte+1) for 0x0134..=0x014C
        // All 25 bytes are zero, so checksum = (-1)*25 = 0xE7
        cart[0x014D] = 0xE7;

        bus.load_cartridge(&cart);

        // Pre-set LY (0xFF44) to 0x90 so the boot ROM's VBlank wait loop
        // exits immediately. Without a PPU, LY would stay at 0 forever.
        bus.write(0xFF44, 0x90);

        (CPU::new(Tracer::off()), bus)
    }

    fn tick_until(cpu: &mut CPU, bus: &mut Bus, pc: u16) -> u64 {
        let mut total_t: u64 = 0;
        for _ in 0..MAX_TICKS {
            if cpu.register_file.get_16bit(Reg16::PC) == pc {
                return total_t;
            }
            let t = cpu.tick(bus);
            total_t += t as u64;
            bus.write(0xFF44, 0x90);
        }
        panic!("CPU did not reach PC={:#06X} within {} ticks", pc, MAX_TICKS);
    }

    #[test]
    fn vram_zeroed_after_boot_init() {
        let (mut cpu, mut bus) = make_boot_cpu();
        tick_until(&mut cpu, &mut bus, 0x000C);

        for addr in 0x8000..=0x9FFFu16 {
            assert_eq!(
                bus.read(addr), 0,
                "VRAM at {:#06X} was not zeroed", addr
            );
        }
    }

    #[test]
    fn nintendo_logo_tiles_in_vram() {
        let (mut cpu, mut bus) = make_boot_cpu();
        tick_until(&mut cpu, &mut bus, 0x0040);

        let mut all_zero = true;
        for addr in 0x8010..=0x809Fu16 {
            if bus.read(addr) != 0 {
                all_zero = false;
                break;
            }
        }
        assert!(!all_zero, "Nintendo logo tiles were not written to VRAM");
    }

    #[test]
    fn logo_comparison_passes() {
        let (mut cpu, mut bus) = make_boot_cpu();
        tick_until(&mut cpu, &mut bus, 0x00E8);
    }

    #[test]
    fn boot_rom_unmapped() {
        let (mut cpu, mut bus) = make_boot_cpu();
        tick_until(&mut cpu, &mut bus, 0x0100);

        assert_ne!(
            bus.read(0xFF50) & 1, 0,
            "boot ROM was not unmapped (0xFF50 bit 0 not set)"
        );
    }

    #[test]
    fn pc_reaches_cartridge_entry() {
        let (mut cpu, mut bus) = make_boot_cpu();
        tick_until(&mut cpu, &mut bus, 0x0100);
    }

    #[test]
    fn registers_after_boot() {
        let (mut cpu, mut bus) = make_boot_cpu();
        tick_until(&mut cpu, &mut bus, 0x0100);

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

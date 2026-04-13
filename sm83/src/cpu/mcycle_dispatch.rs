/// M-cycle dispatch for the SM83 CPU state machine.
///
/// Each match arm in `execute_mcycle()` handles exactly one M-cycle of one
/// instruction.  Bus reads/writes happen at the correct M-cycle boundary
/// so subsystems (timer, PPU) see intermediate state between M-cycles.
///
/// See `assets/docs/m_cycle_design/` for the full architecture.

use crate::cpu::alu::{alu, AluOpKind, AluResult};
use crate::cpu::microcode::{Condition, FLAG_C, FLAG_H, FLAG_N, FLAG_Z};
use crate::cpu::pipeline::MCycleResult;
use crate::cpu::registers::{Reg16, Reg8};
use crate::cpu::CPU;
use crate::memory::bus::Bus;

// ── Opcode decoding helpers ─────────────────────────────────────────────

fn reg8_from_bits(bits: u8) -> Reg8 {
    match bits & 0x07 {
        0 => Reg8::B, 1 => Reg8::C, 2 => Reg8::D, 3 => Reg8::E,
        4 => Reg8::H, 5 => Reg8::L, 7 => Reg8::A,
        _ => unreachable!(),
    }
}

fn reg16_from_bits(bits: u8) -> Reg16 {
    match bits & 0x03 {
        0 => Reg16::BC, 1 => Reg16::DE, 2 => Reg16::HL, 3 => Reg16::SP,
        _ => unreachable!(),
    }
}

fn reg16_push_pop(bits: u8) -> Reg16 {
    match bits & 0x03 {
        0 => Reg16::BC, 1 => Reg16::DE, 2 => Reg16::HL, 3 => Reg16::AF,
        _ => unreachable!(),
    }
}

fn alu_kind_from_bits(bits: u8) -> AluOpKind {
    match bits & 0x07 {
        0 => AluOpKind::Add, 1 => AluOpKind::Adc, 2 => AluOpKind::Sub,
        3 => AluOpKind::Sbc, 4 => AluOpKind::And, 5 => AluOpKind::Xor,
        6 => AluOpKind::Or,  7 => AluOpKind::Cp,
        _ => unreachable!(),
    }
}

fn cond_from_bits(bits: u8) -> Condition {
    match bits & 0x03 {
        0 => Condition::NZ, 1 => Condition::Z,
        2 => Condition::NC, 3 => Condition::C,
        _ => unreachable!(),
    }
}

#[inline]
fn flags(z: bool, n: bool, h: bool, c: bool) -> u8 {
    (if z { FLAG_Z } else { 0 })
        | (if n { FLAG_N } else { 0 })
        | (if h { FLAG_H } else { 0 })
        | (if c { FLAG_C } else { 0 })
}

fn done(opcode: u16) -> MCycleResult {
    MCycleResult::InstructionComplete { opcode }
}

// ── CPU impl ────────────────────────────────────────────────────────────

impl CPU {
    // ── Small helpers ────────────────────────────────────────────────────

    /// Read [PC] and increment PC.
    fn read_pc_inc(&mut self, bus: &Bus) -> u8 {
        let pc = self.register_file.get_16bit(Reg16::PC);
        let val = bus.read(pc);
        self.register_file.inc16(Reg16::PC);
        val
    }

    /// Assemble temp_addr from temp_lo (low) and temp_hi (high).
    fn assemble_temp_addr(&mut self) {
        self.temp_addr = u16::from_le_bytes([self.temp_lo, self.temp_hi]);
    }

    /// Compute H and C flags for ADD SP,e / LD HL,SP+e.
    fn sp_offset_flags(&mut self, sp: u16, e: u8) {
        let eu = e as u16;
        let h = (sp & 0x000F) + (eu & 0x000F) > 0x000F;
        let c = (sp & 0x00FF) + eu > 0x00FF;
        self.register_file.set_8bit(Reg8::F, flags(false, false, h, c));
    }

    /// Run an ALU op: A = A op operand, set flags.
    fn alu_a(&mut self, kind: AluOpKind, operand: u8) {
        let a = self.register_file.get_8bit(Reg8::A);
        let carry_in = (self.register_file.get_8bit(Reg8::F) & FLAG_C) != 0;
        let is_cp = matches!(kind, AluOpKind::Cp);
        let AluResult { value, code } = alu(kind, a, operand, carry_in);
        if !is_cp {
            self.register_file.set_8bit(Reg8::A, value);
        }
        self.register_file
            .set_8bit(Reg8::F, flags(code.z, code.n, code.h, code.c));
    }

    // ── 1M fast path ────────────────────────────────────────────────────

    /// Execute a 1-M-cycle instruction during the fetch cycle.
    pub(crate) fn execute_m1(&mut self, opcode: u8, _bus: &mut Bus) {
        match opcode {
            // NOP
            0x00 => {}

            // STOP (2-byte opcode: 0x10 0x00 — skip the trailing byte)
            0x10 => {
                self.register_file.inc16(Reg16::PC);
                self.halted = true;
            }

            // LD r, r' (0x40-0x7F block, excluding 0x76=HALT and column 6=(HL))
            op @ 0x40..=0x7F if op != 0x76 && (op & 0x07) != 6 && ((op >> 3) & 0x07) != 6 => {
                let dst = reg8_from_bits((op >> 3) & 0x07);
                let src = reg8_from_bits(op & 0x07);
                let v = self.register_file.get_8bit(src);
                self.register_file.set_8bit(dst, v);
            }

            // HALT
            0x76 => {
                let ie = _bus.read(0xFFFF);
                let pending = ie & _bus.if_reg & 0x1F;
                if !self.ime && pending != 0 {
                    // HALT bug: IME=0 with a pending interrupt.
                    // Don't enter halt mode; instead suppress the PC
                    // increment on the next fetch so the following byte
                    // is read twice.
                    self.halt_bug_active = true;
                } else {
                    self.halted = true;
                }
            }

            // ALU A, r (0x80-0xBF, excluding column 6=(HL))
            op @ 0x80..=0xBF if (op & 0x07) != 6 => {
                let kind = alu_kind_from_bits((op >> 3) & 0x07);
                let src = reg8_from_bits(op & 0x07);
                let val = self.register_file.get_8bit(src);
                self.alu_a(kind, val);
            }

            // INC r
            0x04 | 0x0C | 0x14 | 0x1C | 0x24 | 0x2C | 0x3C => {
                let r = reg8_from_bits((opcode >> 3) & 0x07);
                let old = self.register_file.get_8bit(r);
                let res = old.wrapping_add(1);
                self.register_file.set_8bit(r, res);
                let old_c = self.register_file.get_8bit(Reg8::F) & FLAG_C;
                self.register_file.set_8bit(
                    Reg8::F,
                    flags(res == 0, false, (old & 0x0F) + 1 > 0x0F, false) | old_c,
                );
            }

            // DEC r
            0x05 | 0x0D | 0x15 | 0x1D | 0x25 | 0x2D | 0x3D => {
                let r = reg8_from_bits((opcode >> 3) & 0x07);
                let old = self.register_file.get_8bit(r);
                let res = old.wrapping_sub(1);
                self.register_file.set_8bit(r, res);
                let old_c = self.register_file.get_8bit(Reg8::F) & FLAG_C;
                self.register_file.set_8bit(
                    Reg8::F,
                    flags(res == 0, true, (old & 0x0F) == 0, false) | old_c,
                );
            }

            // RLCA
            0x07 => {
                let a = self.register_file.get_8bit(Reg8::A);
                let carry = a >> 7;
                self.register_file.set_8bit(Reg8::A, (a << 1) | carry);
                self.register_file
                    .set_8bit(Reg8::F, flags(false, false, false, carry != 0));
            }
            // RRCA
            0x0F => {
                let a = self.register_file.get_8bit(Reg8::A);
                let carry = a & 1;
                self.register_file
                    .set_8bit(Reg8::A, (a >> 1) | (carry << 7));
                self.register_file
                    .set_8bit(Reg8::F, flags(false, false, false, carry != 0));
            }
            // RLA
            0x17 => {
                let a = self.register_file.get_8bit(Reg8::A);
                let old_c = (self.register_file.get_8bit(Reg8::F) & FLAG_C) != 0;
                let new_c = a >> 7;
                self.register_file
                    .set_8bit(Reg8::A, (a << 1) | (old_c as u8));
                self.register_file
                    .set_8bit(Reg8::F, flags(false, false, false, new_c != 0));
            }
            // RRA
            0x1F => {
                let a = self.register_file.get_8bit(Reg8::A);
                let old_c = (self.register_file.get_8bit(Reg8::F) & FLAG_C) != 0;
                let new_c = a & 1;
                self.register_file
                    .set_8bit(Reg8::A, (a >> 1) | ((old_c as u8) << 7));
                self.register_file
                    .set_8bit(Reg8::F, flags(false, false, false, new_c != 0));
            }

            // DAA
            0x27 => {
                let mut a = self.register_file.get_8bit(Reg8::A);
                let f = self.register_file.get_8bit(Reg8::F);
                let n = f & FLAG_N != 0;
                let h = f & FLAG_H != 0;
                let c = f & FLAG_C != 0;
                let mut adj: u8 = 0;
                let mut new_c = c;
                if !n {
                    if h || (a & 0x0F) > 9 { adj |= 0x06; }
                    if c || a > 0x99 { adj |= 0x60; new_c = true; }
                    a = a.wrapping_add(adj);
                } else {
                    if h { adj |= 0x06; }
                    if c { adj |= 0x60; }
                    a = a.wrapping_sub(adj);
                }
                self.register_file.set_8bit(Reg8::A, a);
                self.register_file
                    .set_8bit(Reg8::F, flags(a == 0, n, false, new_c));
            }
            // CPL
            0x2F => {
                let a = self.register_file.get_8bit(Reg8::A);
                self.register_file.set_8bit(Reg8::A, !a);
                let prev = self.register_file.get_8bit(Reg8::F);
                self.register_file
                    .set_8bit(Reg8::F, (prev & (FLAG_Z | FLAG_C)) | FLAG_N | FLAG_H);
            }
            // SCF
            0x37 => {
                let z = self.register_file.get_8bit(Reg8::F) & FLAG_Z;
                self.register_file.set_8bit(Reg8::F, z | FLAG_C);
            }
            // CCF
            0x3F => {
                let prev = self.register_file.get_8bit(Reg8::F);
                let z = prev & FLAG_Z;
                let new_c = if prev & FLAG_C == 0 { FLAG_C } else { 0 };
                self.register_file.set_8bit(Reg8::F, z | new_c);
            }

            // JP HL
            0xE9 => {
                let hl = self.register_file.get_16bit(Reg16::HL);
                self.register_file.set_16bit(Reg16::PC, hl);
            }

            // DI
            0xF3 => { self.ime = false; }

            // EI
            0xFB => { self.ime_defer = true; }

            _ => panic!("execute_m1: unhandled opcode {:#04X}", opcode),
        }
    }

    // ── CB helpers ──────────────────────────────────────────────────────

    /// Apply a CB rotate/shift/swap/bit/set/res op to a value.
    /// Returns the result byte and sets flags.  For BIT ops the result
    /// is meaningless (not written back).
    fn apply_cb_op(&mut self, cb: u8, val: u8) -> u8 {
        match cb >> 6 {
            // 0x00-0x3F: rotates and shifts
            0 => {
                let (result, carry) = match (cb >> 3) & 0x07 {
                    0 => { let c = val >> 7; ((val << 1) | c, c != 0) }               // RLC
                    1 => { let c = val & 1; ((val >> 1) | (c << 7), c != 0) }         // RRC
                    2 => { // RL
                        let old_c = (self.register_file.get_8bit(Reg8::F) & FLAG_C) != 0;
                        let c = val >> 7;
                        ((val << 1) | (old_c as u8), c != 0)
                    }
                    3 => { // RR
                        let old_c = (self.register_file.get_8bit(Reg8::F) & FLAG_C) != 0;
                        let c = val & 1;
                        ((val >> 1) | ((old_c as u8) << 7), c != 0)
                    }
                    4 => { let c = val >> 7; (val << 1, c != 0) }                     // SLA
                    5 => { let c = val & 1; ((val >> 1) | (val & 0x80), c != 0) }     // SRA
                    6 => { ((val >> 4) | (val << 4), false) }                          // SWAP
                    7 => { let c = val & 1; (val >> 1, c != 0) }                       // SRL
                    _ => unreachable!(),
                };
                let is_swap = (cb >> 3) & 0x07 == 6;
                self.register_file.set_8bit(
                    Reg8::F,
                    flags(result == 0, false, false, if is_swap { false } else { carry }),
                );
                result
            }
            // 0x40-0x7F: BIT
            1 => {
                let bit = (cb >> 3) & 0x07;
                let is_zero = (val & (1 << bit)) == 0;
                let old_c = self.register_file.get_8bit(Reg8::F) & FLAG_C;
                self.register_file
                    .set_8bit(Reg8::F, flags(is_zero, false, true, false) | old_c);
                val // not written back
            }
            // 0x80-0xBF: RES
            2 => {
                let bit = (cb >> 3) & 0x07;
                val & !(1 << bit)
                // no flag changes
            }
            // 0xC0-0xFF: SET
            3 => {
                let bit = (cb >> 3) & 0x07;
                val | (1 << bit)
                // no flag changes
            }
            _ => unreachable!(),
        }
    }

    /// Execute a CB-prefixed register op (operand bits 2:0 != 6).
    pub(crate) fn execute_cb_register(&mut self, cb: u8) {
        let reg = reg8_from_bits(cb & 0x07);
        let val = self.register_file.get_8bit(reg);
        let result = self.apply_cb_op(cb, val);
        // BIT ops (0x40-0x7F) don't write back
        if cb < 0x40 || cb >= 0x80 {
            self.register_file.set_8bit(reg, result);
        }
    }

    // ── Interrupt dispatch ──────────────────────────────────────────────

    pub(crate) fn begin_interrupt_dispatch(&mut self, bus: &mut Bus) {
        let ie = bus.read(0xFFFF);
        let if_reg = bus.if_reg;
        let pending = ie & if_reg & 0x1F;
        let bit = pending.trailing_zeros() as u8;

        bus.if_reg &= !(1 << bit);
        self.ime = false;
        self.in_interrupt_dispatch = true;
        self.interrupt_vector = 0x0040 + (bit as u16) * 0x08;
        self.mcycle = 0;
    }

    pub(crate) fn step_interrupt(&mut self, bus: &mut Bus) -> MCycleResult {
        match self.mcycle {
            0 => { self.mcycle = 1; MCycleResult::Continue }
            1 => { self.mcycle = 2; MCycleResult::Continue }
            2 => {
                // Push PC high byte
                let pc = self.register_file.get_16bit(Reg16::PC);
                self.register_file.dec16(Reg16::SP);
                let sp = self.register_file.get_16bit(Reg16::SP);
                bus.write(sp, (pc >> 8) as u8);
                self.mcycle = 3;
                MCycleResult::Continue
            }
            3 => {
                // Push PC low byte
                let pc = self.register_file.get_16bit(Reg16::PC);
                self.register_file.dec16(Reg16::SP);
                let sp = self.register_file.get_16bit(Reg16::SP);
                bus.write(sp, pc as u8);
                self.mcycle = 4;
                MCycleResult::Continue
            }
            4 => {
                // Set PC to vector
                self.register_file
                    .set_16bit(Reg16::PC, self.interrupt_vector);
                self.in_interrupt_dispatch = false;
                self.mcycle = 0;
                done(0xFFFF)
            }
            _ => unreachable!(),
        }
    }

    // ── Main dispatch ───────────────────────────────────────────────────

    pub(crate) fn execute_mcycle(&mut self, bus: &mut Bus) -> MCycleResult {
        let op = self.current_opcode;
        let m = self.mcycle;

        match (op, m) {
            // ════════════════════════════════════════════════════════════
            // CB prefix fetch (sentinel 0xCB00)
            // ════════════════════════════════════════════════════════════
            (0xCB00, 1) => {
                let cb = self.read_pc_inc(bus);
                if cb & 0x07 != 6 {
                    // Register operand: execute + complete
                    self.execute_cb_register(cb);
                    done(0x100 | cb as u16)
                } else {
                    // (HL) operand: need more M-cycles
                    self.current_opcode = 0x100 | cb as u16;
                    MCycleResult::Continue
                }
            }

            // ════════════════════════════════════════════════════════════
            // CB BIT b,(HL) — 3M total (mcycle 2 = read + test)
            // ════════════════════════════════════════════════════════════
            (0x146 | 0x14E | 0x156 | 0x15E | 0x166 | 0x16E | 0x176 | 0x17E, 2) => {
                let cb = (op & 0xFF) as u8;
                let addr = self.register_file.get_16bit(Reg16::HL);
                let val = bus.read(addr);
                self.apply_cb_op(cb, val); // sets flags, no writeback
                done(op)
            }

            // ════════════════════════════════════════════════════════════
            // CB shift/swap/set/res (HL) — 4M total
            // mcycle 2 = read, mcycle 3 = modify + write
            // ════════════════════════════════════════════════════════════
            (0x100..=0x1FF, 2) if (op & 0x07) == 6 => {
                // Read (HL)
                let addr = self.register_file.get_16bit(Reg16::HL);
                self.temp_lo = bus.read(addr);
                MCycleResult::Continue
            }
            (0x100..=0x1FF, 3) if (op & 0x07) == 6 => {
                // Modify + write back
                let cb = (op & 0xFF) as u8;
                let result = self.apply_cb_op(cb, self.temp_lo);
                let addr = self.register_file.get_16bit(Reg16::HL);
                bus.write(addr, result);
                done(op)
            }

            // ════════════════════════════════════════════════════════════
            // 2M — LD r, n
            // ════════════════════════════════════════════════════════════
            (0x06 | 0x0E | 0x16 | 0x1E | 0x26 | 0x2E | 0x3E, 1) => {
                let r = reg8_from_bits(((op as u8) >> 3) & 0x07);
                let val = self.read_pc_inc(bus);
                self.register_file.set_8bit(r, val);
                done(op)
            }

            // ════════════════════════════════════════════════════════════
            // 2M — LD r, (HL)
            // ════════════════════════════════════════════════════════════
            (0x46 | 0x4E | 0x56 | 0x5E | 0x66 | 0x6E | 0x7E, 1) => {
                let r = reg8_from_bits(((op as u8) >> 3) & 0x07);
                let addr = self.register_file.get_16bit(Reg16::HL);
                let val = bus.read(addr);
                self.register_file.set_8bit(r, val);
                done(op)
            }

            // ════════════════════════════════════════════════════════════
            // 2M — LD (HL), r
            // ════════════════════════════════════════════════════════════
            (0x70..=0x75 | 0x77, 1) => {
                let r = reg8_from_bits((op as u8) & 0x07);
                let addr = self.register_file.get_16bit(Reg16::HL);
                let val = self.register_file.get_8bit(r);
                bus.write(addr, val);
                done(op)
            }

            // ════════════════════════════════════════════════════════════
            // 2M — LD A, (BC/DE) and LD A, (HL+/HL-)
            // ════════════════════════════════════════════════════════════
            (0x0A, 1) => { // LD A,(BC)
                let addr = self.register_file.get_16bit(Reg16::BC);
                self.register_file.set_8bit(Reg8::A, bus.read(addr));
                done(op)
            }
            (0x1A, 1) => { // LD A,(DE)
                let addr = self.register_file.get_16bit(Reg16::DE);
                self.register_file.set_8bit(Reg8::A, bus.read(addr));
                done(op)
            }
            (0x2A, 1) => { // LD A,(HL+)
                let addr = self.register_file.get_16bit(Reg16::HL);
                self.register_file.set_8bit(Reg8::A, bus.read(addr));
                self.register_file.inc16(Reg16::HL);
                done(op)
            }
            (0x3A, 1) => { // LD A,(HL-)
                let addr = self.register_file.get_16bit(Reg16::HL);
                self.register_file.set_8bit(Reg8::A, bus.read(addr));
                self.register_file.dec16(Reg16::HL);
                done(op)
            }

            // ════════════════════════════════════════════════════════════
            // 2M — LD (BC/DE), A and LD (HL+/HL-), A
            // ════════════════════════════════════════════════════════════
            (0x02, 1) => { // LD (BC),A
                let addr = self.register_file.get_16bit(Reg16::BC);
                bus.write(addr, self.register_file.get_8bit(Reg8::A));
                done(op)
            }
            (0x12, 1) => { // LD (DE),A
                let addr = self.register_file.get_16bit(Reg16::DE);
                bus.write(addr, self.register_file.get_8bit(Reg8::A));
                done(op)
            }
            (0x22, 1) => { // LD (HL+),A
                let addr = self.register_file.get_16bit(Reg16::HL);
                bus.write(addr, self.register_file.get_8bit(Reg8::A));
                self.register_file.inc16(Reg16::HL);
                done(op)
            }
            (0x32, 1) => { // LD (HL-),A
                let addr = self.register_file.get_16bit(Reg16::HL);
                bus.write(addr, self.register_file.get_8bit(Reg8::A));
                self.register_file.dec16(Reg16::HL);
                done(op)
            }

            // ════════════════════════════════════════════════════════════
            // 2M — LDH (C),A / LDH A,(C)
            // ════════════════════════════════════════════════════════════
            (0xE2, 1) => {
                let c = self.register_file.get_8bit(Reg8::C) as u16;
                bus.write(0xFF00 | c, self.register_file.get_8bit(Reg8::A));
                done(op)
            }
            (0xF2, 1) => {
                let c = self.register_file.get_8bit(Reg8::C) as u16;
                self.register_file.set_8bit(Reg8::A, bus.read(0xFF00 | c));
                done(op)
            }

            // ════════════════════════════════════════════════════════════
            // 2M — ALU A, (HL)
            // ════════════════════════════════════════════════════════════
            (0x86 | 0x8E | 0x96 | 0x9E | 0xA6 | 0xAE | 0xB6 | 0xBE, 1) => {
                let kind = alu_kind_from_bits(((op as u8) >> 3) & 0x07);
                let addr = self.register_file.get_16bit(Reg16::HL);
                let val = bus.read(addr);
                self.alu_a(kind, val);
                done(op)
            }

            // ════════════════════════════════════════════════════════════
            // 2M — ALU A, n
            // ════════════════════════════════════════════════════════════
            (0xC6 | 0xCE | 0xD6 | 0xDE | 0xE6 | 0xEE | 0xF6 | 0xFE, 1) => {
                let kind = alu_kind_from_bits(((op as u8) >> 3) & 0x07);
                let val = self.read_pc_inc(bus);
                self.alu_a(kind, val);
                done(op)
            }

            // ════════════════════════════════════════════════════════════
            // 2M — INC/DEC rr (internal cycle)
            // ════════════════════════════════════════════════════════════
            (0x03 | 0x13 | 0x23 | 0x33, 1) => {
                let rr = reg16_from_bits(((op as u8) >> 4) & 0x03);
                self.register_file.inc16(rr);
                done(op)
            }
            (0x0B | 0x1B | 0x2B | 0x3B, 1) => {
                let rr = reg16_from_bits(((op as u8) >> 4) & 0x03);
                self.register_file.dec16(rr);
                done(op)
            }

            // ════════════════════════════════════════════════════════════
            // 2M — LD SP, HL (internal cycle)
            // ════════════════════════════════════════════════════════════
            (0xF9, 1) => {
                let hl = self.register_file.get_16bit(Reg16::HL);
                self.register_file.set_16bit(Reg16::SP, hl);
                done(op)
            }

            // ════════════════════════════════════════════════════════════
            // 2M — ADD HL, rr (internal cycle)
            // ════════════════════════════════════════════════════════════
            (0x09 | 0x19 | 0x29 | 0x39, 1) => {
                let rr = reg16_from_bits(((op as u8) >> 4) & 0x03);
                let a = self.register_file.get_16bit(Reg16::HL);
                let b = self.register_file.get_16bit(rr);
                self.register_file.set_16bit(Reg16::HL, a.wrapping_add(b));
                let old_z = self.register_file.get_8bit(Reg8::F) & FLAG_Z;
                self.register_file.set_8bit(
                    Reg8::F,
                    flags(false, false, (a & 0x0FFF) + (b & 0x0FFF) > 0x0FFF,
                          (a as u32 + b as u32) > 0xFFFF)
                        | old_z,
                );
                done(op)
            }

            // ════════════════════════════════════════════════════════════
            // 3M — LD rr, nn
            // ════════════════════════════════════════════════════════════
            (0x01 | 0x11 | 0x21 | 0x31, 1) => {
                self.temp_lo = self.read_pc_inc(bus);
                MCycleResult::Continue
            }
            (0x01 | 0x11 | 0x21 | 0x31, 2) => {
                self.temp_hi = self.read_pc_inc(bus);
                let rr = reg16_from_bits(((op as u8) >> 4) & 0x03);
                self.register_file
                    .set_16bit(rr, u16::from_le_bytes([self.temp_lo, self.temp_hi]));
                done(op)
            }

            // ════════════════════════════════════════════════════════════
            // 3M — LD (HL), n
            // ════════════════════════════════════════════════════════════
            (0x36, 1) => { self.temp_lo = self.read_pc_inc(bus); MCycleResult::Continue }
            (0x36, 2) => {
                let addr = self.register_file.get_16bit(Reg16::HL);
                bus.write(addr, self.temp_lo);
                done(op)
            }

            // ════════════════════════════════════════════════════════════
            // 3M — INC (HL) / DEC (HL)
            // ════════════════════════════════════════════════════════════
            (0x34 | 0x35, 1) => {
                let addr = self.register_file.get_16bit(Reg16::HL);
                self.temp_lo = bus.read(addr);
                MCycleResult::Continue
            }
            (0x34, 2) => { // INC (HL) — write back
                let old = self.temp_lo;
                let res = old.wrapping_add(1);
                let old_c = self.register_file.get_8bit(Reg8::F) & FLAG_C;
                self.register_file.set_8bit(
                    Reg8::F,
                    flags(res == 0, false, (old & 0x0F) + 1 > 0x0F, false) | old_c,
                );
                let addr = self.register_file.get_16bit(Reg16::HL);
                bus.write(addr, res);
                done(op)
            }
            (0x35, 2) => { // DEC (HL) — write back
                let old = self.temp_lo;
                let res = old.wrapping_sub(1);
                let old_c = self.register_file.get_8bit(Reg8::F) & FLAG_C;
                self.register_file.set_8bit(
                    Reg8::F,
                    flags(res == 0, true, (old & 0x0F) == 0, false) | old_c,
                );
                let addr = self.register_file.get_16bit(Reg16::HL);
                bus.write(addr, res);
                done(op)
            }

            // ════════════════════════════════════════════════════════════
            // 3M — LDH (n), A / LDH A, (n)
            // ════════════════════════════════════════════════════════════
            (0xE0 | 0xF0, 1) => { self.temp_lo = self.read_pc_inc(bus); MCycleResult::Continue }
            (0xE0, 2) => {
                bus.write(0xFF00 | self.temp_lo as u16, self.register_file.get_8bit(Reg8::A));
                done(op)
            }
            (0xF0, 2) => {
                self.register_file
                    .set_8bit(Reg8::A, bus.read(0xFF00 | self.temp_lo as u16));
                done(op)
            }

            // ════════════════════════════════════════════════════════════
            // 3M — JR e (unconditional)
            // ════════════════════════════════════════════════════════════
            (0x18, 1) => { self.temp_lo = self.read_pc_inc(bus); MCycleResult::Continue }
            (0x18, 2) => {
                let offset = self.temp_lo as i8;
                let pc = self.register_file.get_16bit(Reg16::PC);
                self.register_file
                    .set_16bit(Reg16::PC, pc.wrapping_add(offset as u16));
                done(op)
            }

            // ════════════════════════════════════════════════════════════
            // 2-3M — JR cc, e (conditional)
            // ════════════════════════════════════════════════════════════
            (0x20 | 0x28 | 0x30 | 0x38, 1) => {
                self.temp_lo = self.read_pc_inc(bus);
                let cond = cond_from_bits(((op as u8) >> 3) & 0x03);
                let f = self.register_file.get_8bit(Reg8::F);
                if cond.eval(f) {
                    self.condition_taken = true;
                    MCycleResult::Continue // M3: apply jump
                } else {
                    self.condition_taken = false;
                    done(op) // 2M: not taken
                }
            }
            (0x20 | 0x28 | 0x30 | 0x38, 2) => {
                let offset = self.temp_lo as i8;
                let pc = self.register_file.get_16bit(Reg16::PC);
                self.register_file
                    .set_16bit(Reg16::PC, pc.wrapping_add(offset as u16));
                done(op)
            }

            // ════════════════════════════════════════════════════════════
            // 3M — POP rr
            // ════════════════════════════════════════════════════════════
            (0xC1 | 0xD1 | 0xE1 | 0xF1, 1) => {
                let sp = self.register_file.get_16bit(Reg16::SP);
                self.temp_lo = bus.read(sp);
                self.register_file.inc16(Reg16::SP);
                MCycleResult::Continue
            }
            (0xC1 | 0xD1 | 0xE1 | 0xF1, 2) => {
                let sp = self.register_file.get_16bit(Reg16::SP);
                self.temp_hi = bus.read(sp);
                self.register_file.inc16(Reg16::SP);
                let rr = reg16_push_pop(((op as u8) >> 4) & 0x03);
                self.register_file
                    .set_16bit(rr, u16::from_le_bytes([self.temp_lo, self.temp_hi]));
                if rr == Reg16::AF {
                    let f = self.register_file.get_8bit(Reg8::F) & 0xF0;
                    self.register_file.set_8bit(Reg8::F, f);
                }
                done(op)
            }

            // ════════════════════════════════════════════════════════════
            // 3M — LD HL, SP+e
            // ════════════════════════════════════════════════════════════
            (0xF8, 1) => { self.temp_lo = self.read_pc_inc(bus); MCycleResult::Continue }
            (0xF8, 2) => {
                let sp = self.register_file.get_16bit(Reg16::SP);
                let e = self.temp_lo as i8;
                let result = sp.wrapping_add(e as i16 as u16);
                self.register_file.set_16bit(Reg16::HL, result);
                self.sp_offset_flags(sp, self.temp_lo);
                done(op)
            }

            // ════════════════════════════════════════════════════════════
            // 4M — JP nn
            // ════════════════════════════════════════════════════════════
            (0xC3, 1) => { self.temp_lo = self.read_pc_inc(bus); MCycleResult::Continue }
            (0xC3, 2) => { self.temp_hi = self.read_pc_inc(bus); MCycleResult::Continue }
            (0xC3, 3) => {
                self.assemble_temp_addr();
                self.register_file.set_16bit(Reg16::PC, self.temp_addr);
                done(op)
            }

            // ════════════════════════════════════════════════════════════
            // 3-4M — JP cc, nn (conditional)
            // ════════════════════════════════════════════════════════════
            (0xC2 | 0xCA | 0xD2 | 0xDA, 1) => {
                self.temp_lo = self.read_pc_inc(bus);
                MCycleResult::Continue
            }
            (0xC2 | 0xCA | 0xD2 | 0xDA, 2) => {
                self.temp_hi = self.read_pc_inc(bus);
                let cond = cond_from_bits(((op as u8) >> 3) & 0x03);
                let f = self.register_file.get_8bit(Reg8::F);
                if cond.eval(f) {
                    self.condition_taken = true;
                    MCycleResult::Continue // M4: apply jump
                } else {
                    self.condition_taken = false;
                    done(op) // 3M: not taken
                }
            }
            (0xC2 | 0xCA | 0xD2 | 0xDA, 3) => {
                self.assemble_temp_addr();
                self.register_file.set_16bit(Reg16::PC, self.temp_addr);
                done(op)
            }

            // ════════════════════════════════════════════════════════════
            // 4M — RET
            // ════════════════════════════════════════════════════════════
            (0xC9, 1) => {
                let sp = self.register_file.get_16bit(Reg16::SP);
                self.temp_lo = bus.read(sp);
                self.register_file.inc16(Reg16::SP);
                MCycleResult::Continue
            }
            (0xC9, 2) => {
                let sp = self.register_file.get_16bit(Reg16::SP);
                self.temp_hi = bus.read(sp);
                self.register_file.inc16(Reg16::SP);
                MCycleResult::Continue
            }
            (0xC9, 3) => {
                self.assemble_temp_addr();
                self.register_file.set_16bit(Reg16::PC, self.temp_addr);
                done(op)
            }

            // ════════════════════════════════════════════════════════════
            // 4M — RETI
            // ════════════════════════════════════════════════════════════
            (0xD9, 1) => {
                let sp = self.register_file.get_16bit(Reg16::SP);
                self.temp_lo = bus.read(sp);
                self.register_file.inc16(Reg16::SP);
                MCycleResult::Continue
            }
            (0xD9, 2) => {
                let sp = self.register_file.get_16bit(Reg16::SP);
                self.temp_hi = bus.read(sp);
                self.register_file.inc16(Reg16::SP);
                MCycleResult::Continue
            }
            (0xD9, 3) => {
                self.assemble_temp_addr();
                self.register_file.set_16bit(Reg16::PC, self.temp_addr);
                self.ime = true;
                done(op)
            }

            // ════════════════════════════════════════════════════════════
            // 2-5M — RET cc (conditional)
            // ════════════════════════════════════════════════════════════
            (0xC0 | 0xC8 | 0xD0 | 0xD8, 1) => {
                // M2: internal — evaluate condition
                let cond = cond_from_bits(((op as u8) >> 3) & 0x03);
                let f = self.register_file.get_8bit(Reg8::F);
                if cond.eval(f) {
                    self.condition_taken = true;
                    MCycleResult::Continue
                } else {
                    self.condition_taken = false;
                    done(op) // 2M: not taken
                }
            }
            (0xC0 | 0xC8 | 0xD0 | 0xD8, 2) => {
                // M3: pop low
                let sp = self.register_file.get_16bit(Reg16::SP);
                self.temp_lo = bus.read(sp);
                self.register_file.inc16(Reg16::SP);
                MCycleResult::Continue
            }
            (0xC0 | 0xC8 | 0xD0 | 0xD8, 3) => {
                // M4: pop high
                let sp = self.register_file.get_16bit(Reg16::SP);
                self.temp_hi = bus.read(sp);
                self.register_file.inc16(Reg16::SP);
                MCycleResult::Continue
            }
            (0xC0 | 0xC8 | 0xD0 | 0xD8, 4) => {
                // M5: internal — set PC
                self.assemble_temp_addr();
                self.register_file.set_16bit(Reg16::PC, self.temp_addr);
                done(op)
            }

            // ════════════════════════════════════════════════════════════
            // 4M — PUSH rr
            // ════════════════════════════════════════════════════════════
            (0xC5 | 0xD5 | 0xE5 | 0xF5, 1) => {
                // M2: internal cycle
                MCycleResult::Continue
            }
            (0xC5 | 0xD5 | 0xE5 | 0xF5, 2) => {
                // M3: push high byte
                let rr = reg16_push_pop(((op as u8) >> 4) & 0x03);
                let val = self.register_file.get_16bit(rr);
                self.register_file.dec16(Reg16::SP);
                let sp = self.register_file.get_16bit(Reg16::SP);
                bus.write(sp, (val >> 8) as u8);
                MCycleResult::Continue
            }
            (0xC5 | 0xD5 | 0xE5 | 0xF5, 3) => {
                // M4: push low byte
                let rr = reg16_push_pop(((op as u8) >> 4) & 0x03);
                let val = self.register_file.get_16bit(rr);
                self.register_file.dec16(Reg16::SP);
                let sp = self.register_file.get_16bit(Reg16::SP);
                bus.write(sp, val as u8);
                done(op)
            }

            // ════════════════════════════════════════════════════════════
            // 4M — RST vec
            // ════════════════════════════════════════════════════════════
            (0xC7 | 0xCF | 0xD7 | 0xDF | 0xE7 | 0xEF | 0xF7 | 0xFF, 1) => {
                // M2: internal
                MCycleResult::Continue
            }
            (0xC7 | 0xCF | 0xD7 | 0xDF | 0xE7 | 0xEF | 0xF7 | 0xFF, 2) => {
                // M3: push PC high
                let pc = self.register_file.get_16bit(Reg16::PC);
                self.register_file.dec16(Reg16::SP);
                let sp = self.register_file.get_16bit(Reg16::SP);
                bus.write(sp, (pc >> 8) as u8);
                MCycleResult::Continue
            }
            (0xC7 | 0xCF | 0xD7 | 0xDF | 0xE7 | 0xEF | 0xF7 | 0xFF, 3) => {
                // M4: push PC low, jump to vector
                let pc = self.register_file.get_16bit(Reg16::PC);
                self.register_file.dec16(Reg16::SP);
                let sp = self.register_file.get_16bit(Reg16::SP);
                bus.write(sp, pc as u8);
                let vector = (op as u8) & 0x38;
                self.register_file.set_16bit(Reg16::PC, vector as u16);
                done(op)
            }

            // ════════════════════════════════════════════════════════════
            // 4M — ADD SP, e
            // ════════════════════════════════════════════════════════════
            (0xE8, 1) => { self.temp_lo = self.read_pc_inc(bus); MCycleResult::Continue }
            (0xE8, 2) => { MCycleResult::Continue } // internal
            (0xE8, 3) => {
                let sp = self.register_file.get_16bit(Reg16::SP);
                let e = self.temp_lo as i8;
                let result = sp.wrapping_add(e as i16 as u16);
                self.register_file.set_16bit(Reg16::SP, result);
                self.sp_offset_flags(sp, self.temp_lo);
                done(op)
            }

            // ════════════════════════════════════════════════════════════
            // 4M — LD A, (nn) / LD (nn), A
            // ════════════════════════════════════════════════════════════
            (0xFA | 0xEA, 1) => { self.temp_lo = self.read_pc_inc(bus); MCycleResult::Continue }
            (0xFA | 0xEA, 2) => {
                self.temp_hi = self.read_pc_inc(bus);
                self.assemble_temp_addr();
                MCycleResult::Continue
            }
            (0xFA, 3) => { // LD A,(nn) — read
                self.register_file
                    .set_8bit(Reg8::A, bus.read(self.temp_addr));
                done(op)
            }
            (0xEA, 3) => { // LD (nn),A — write
                bus.write(self.temp_addr, self.register_file.get_8bit(Reg8::A));
                done(op)
            }

            // ════════════════════════════════════════════════════════════
            // 5M — LD (nn), SP
            // ════════════════════════════════════════════════════════════
            (0x08, 1) => { self.temp_lo = self.read_pc_inc(bus); MCycleResult::Continue }
            (0x08, 2) => {
                self.temp_hi = self.read_pc_inc(bus);
                self.assemble_temp_addr();
                MCycleResult::Continue
            }
            (0x08, 3) => {
                let sp = self.register_file.get_16bit(Reg16::SP);
                bus.write(self.temp_addr, sp as u8);
                self.temp_addr = self.temp_addr.wrapping_add(1);
                MCycleResult::Continue
            }
            (0x08, 4) => {
                let sp = self.register_file.get_16bit(Reg16::SP);
                bus.write(self.temp_addr, (sp >> 8) as u8);
                done(op)
            }

            // ════════════════════════════════════════════════════════════
            // 6M — CALL nn
            // ════════════════════════════════════════════════════════════
            (0xCD, 1) => { self.temp_lo = self.read_pc_inc(bus); MCycleResult::Continue }
            (0xCD, 2) => {
                self.temp_hi = self.read_pc_inc(bus);
                self.assemble_temp_addr();
                MCycleResult::Continue
            }
            (0xCD, 3) => { MCycleResult::Continue } // internal
            (0xCD, 4) => {
                let pc = self.register_file.get_16bit(Reg16::PC);
                self.register_file.dec16(Reg16::SP);
                let sp = self.register_file.get_16bit(Reg16::SP);
                bus.write(sp, (pc >> 8) as u8);
                MCycleResult::Continue
            }
            (0xCD, 5) => {
                let pc = self.register_file.get_16bit(Reg16::PC);
                self.register_file.dec16(Reg16::SP);
                let sp = self.register_file.get_16bit(Reg16::SP);
                bus.write(sp, pc as u8);
                self.register_file.set_16bit(Reg16::PC, self.temp_addr);
                done(op)
            }

            // ════════════════════════════════════════════════════════════
            // 3-6M — CALL cc, nn (conditional)
            // ════════════════════════════════════════════════════════════
            (0xC4 | 0xCC | 0xD4 | 0xDC, 1) => {
                self.temp_lo = self.read_pc_inc(bus);
                MCycleResult::Continue
            }
            (0xC4 | 0xCC | 0xD4 | 0xDC, 2) => {
                self.temp_hi = self.read_pc_inc(bus);
                self.assemble_temp_addr();
                let cond = cond_from_bits(((op as u8) >> 3) & 0x03);
                let f = self.register_file.get_8bit(Reg8::F);
                if cond.eval(f) {
                    self.condition_taken = true;
                    MCycleResult::Continue
                } else {
                    self.condition_taken = false;
                    done(op) // 3M: not taken
                }
            }
            (0xC4 | 0xCC | 0xD4 | 0xDC, 3) => {
                MCycleResult::Continue // internal
            }
            (0xC4 | 0xCC | 0xD4 | 0xDC, 4) => {
                let pc = self.register_file.get_16bit(Reg16::PC);
                self.register_file.dec16(Reg16::SP);
                let sp = self.register_file.get_16bit(Reg16::SP);
                bus.write(sp, (pc >> 8) as u8);
                MCycleResult::Continue
            }
            (0xC4 | 0xCC | 0xD4 | 0xDC, 5) => {
                let pc = self.register_file.get_16bit(Reg16::PC);
                self.register_file.dec16(Reg16::SP);
                let sp = self.register_file.get_16bit(Reg16::SP);
                bus.write(sp, pc as u8);
                self.register_file.set_16bit(Reg16::PC, self.temp_addr);
                done(op)
            }

            // ════════════════════════════════════════════════════════════
            // Catch-all
            // ════════════════════════════════════════════════════════════
            _ => panic!(
                "execute_mcycle: unhandled opcode {:#06X}, mcycle {}",
                op, m
            ),
        }
    }
}

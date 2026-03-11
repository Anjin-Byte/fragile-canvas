use core::fmt;

use super::interrupts::Interrupt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reg8 { A, F, B, C, D, E, H, L, IR, IE, }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reg16 { PC, SP, AF, BC, DE, HL, }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flag {
    Zero = 7,
    Subtract = 6,
    HalfCarry = 5,
    Carry = 4,
}

#[derive(Clone)]
pub struct RegisterFile {
    pc: u16,
    sp: u16,
    a: u8,
    f: u8,
    b: u8,
    c: u8,
    d: u8,
    e: u8,
    h: u8,
    l: u8,
    ir: u8,
    ie: u8,
}

impl fmt::Display for RegisterFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[PC: {:#06X} | SP: {:#06X} | AF: {:#06X} | BC: {:#06X} | DE: {:#06X} | HL: {:#06X} | IR: {:#04X} | IE: {:#04X}]",
            self.pc, self.sp,
            u16::from_be_bytes([self.a, self.f]),
            u16::from_be_bytes([self.b, self.c]),
            u16::from_be_bytes([self.d, self.e]),
            u16::from_be_bytes([self.h, self.l]),
            self.ir, self.ie)
    }
}

impl RegisterFile {
    pub fn new() -> Self {
        Self {
            pc: 0,
            sp: 0,
            a: 0,
            f: 0,
            b: 0,
            c: 0,
            d: 0,
            e: 0,
            h: 0,
            l: 0,
            ir: 0,
            ie: 0,
        }
    }

    pub fn get_8bit(&self, register: Reg8) -> u8 {
        match register {
            Reg8::A => self.a,
            Reg8::F => self.f,
            Reg8::B => self.b,
            Reg8::C => self.c,
            Reg8::D => self.d,
            Reg8::E => self.e,
            Reg8::H => self.h,
            Reg8::L => self.l,
            Reg8::IR => self.ir,
            Reg8::IE => self.ie,
        }
    }

    pub fn set_8bit(&mut self, register: Reg8, value: u8) {
        match register {
            Reg8::A => self.a = value,
            Reg8::F => self.f = value,
            Reg8::B => self.b = value,
            Reg8::C => self.c = value,
            Reg8::D => self.d = value,
            Reg8::E => self.e = value,
            Reg8::H => self.h = value,
            Reg8::L => self.l = value,
            Reg8::IR => self.ir = value,
            Reg8::IE => self.ie = value,
        }
    }

    pub fn get_16bit(&self, register: Reg16) -> u16 {
        match register {
            Reg16::PC => self.pc,
            Reg16::SP => self.sp,
            Reg16::AF => u16::from_be_bytes([self.a, self.f]),
            Reg16::BC => u16::from_be_bytes([self.b, self.c]),
            Reg16::DE => u16::from_be_bytes([self.d, self.e]),
            Reg16::HL => u16::from_be_bytes([self.h, self.l]),
        }
    }

    pub fn set_16bit(&mut self, register: Reg16, value: u16) {
        match register {
            Reg16::PC => self.pc = value,
            Reg16::SP => self.sp = value,
            Reg16::AF => {
                let [high, low] = value.to_be_bytes();
                self.a = high;
                self.f = low;
            }
            Reg16::BC => {
                let [high, low] = value.to_be_bytes();
                self.b = high;
                self.c = low;
            }
            Reg16::DE => {
                let [high, low] = value.to_be_bytes();
                self.d = high;
                self.e = low;
            }
            Reg16::HL => {
                let [high, low] = value.to_be_bytes();
                self.h = high;
                self.l = low;
            }
        }
    }

    pub fn inc8(&mut self, reg: Reg8) {
        let value = self.get_8bit(reg).wrapping_add(1);
        self.set_8bit(reg, value);
    }

    pub fn dec8(&mut self, reg: Reg8) {
        let value = self.get_8bit(reg).wrapping_sub(1);
        self.set_8bit(reg, value);
    }

    pub fn inc16(&mut self, reg: Reg16) {
        let value = self.get_16bit(reg).wrapping_add(1);
        self.set_16bit(reg, value);
    }

    pub fn dec16(&mut self, reg: Reg16) {
        let value = self.get_16bit(reg).wrapping_sub(1);
        self.set_16bit(reg, value);
    }

    pub fn write_flag(&mut self, flag: Flag, set: bool) {
        if set {
            self.set_flag(flag);
        } else {
            self.reset_flag(flag);
        }
    }

    pub fn set_flag(&mut self, flag: Flag) {
        self.f |= 1 << flag as u8;
    }

    pub fn reset_flag(&mut self, flag: Flag) {
        self.f &= !(1 << flag as u8);
    }

    pub fn is_flag_set(&self, flag: Flag) -> bool {
        self.f & (1 << flag as u8) != 0
    }

    pub fn enable_interrupt(&mut self, interrupt: Interrupt) {
        self.ie |= 1 << interrupt as u8;
    }

    pub fn disable_interrupt(&mut self, interrupt: Interrupt) {
        self.ie &= !(1 << interrupt as u8);
    }

    pub fn is_interrupt_enabled(&self, interrupt: Interrupt) -> bool {
        self.ie & (1 << interrupt as u8) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::interrupts::Interrupt;

    // ── Initialization ──────────────────────────────────────────────

    #[test]
    fn new_register_file_is_all_zeros() {
        let rf = RegisterFile::new();
        for reg in [Reg8::A, Reg8::F, Reg8::B, Reg8::C,
                    Reg8::D, Reg8::E, Reg8::H, Reg8::L,
                    Reg8::IR, Reg8::IE] {
            assert_eq!(rf.get_8bit(reg), 0, "{:?} not zero at init", reg);
        }
        for reg in [Reg16::PC, Reg16::SP, Reg16::AF, Reg16::BC,
                    Reg16::DE, Reg16::HL] {
            assert_eq!(rf.get_16bit(reg), 0, "{:?} not zero at init", reg);
        }
    }

    // ── 8-bit round-trip ────────────────────────────────────────────

    #[test]
    fn set_get_8bit_round_trip_all_registers() {
        let mut rf = RegisterFile::new();
        let regs = [Reg8::A, Reg8::F, Reg8::B, Reg8::C,
                    Reg8::D, Reg8::E, Reg8::H, Reg8::L,
                    Reg8::IR, Reg8::IE];
        for (i, &reg) in regs.iter().enumerate() {
            let val = (0x10 + i) as u8;
            rf.set_8bit(reg, val);
            assert_eq!(rf.get_8bit(reg), val, "{:?} round-trip failed", reg);
        }
    }

    #[test]
    fn set_8bit_boundary_values() {
        let mut rf = RegisterFile::new();
        for &val in &[0x00, 0x01, 0x7F, 0x80, 0xFE, 0xFF] {
            rf.set_8bit(Reg8::A, val);
            assert_eq!(rf.get_8bit(Reg8::A), val);
        }
    }

    #[test]
    fn set_8bit_does_not_clobber_neighbors() {
        let mut rf = RegisterFile::new();
        rf.set_8bit(Reg8::B, 0xBB);
        rf.set_8bit(Reg8::C, 0xCC);
        assert_eq!(rf.get_8bit(Reg8::B), 0xBB);
        assert_eq!(rf.get_8bit(Reg8::C), 0xCC);
    }

    // ── 16-bit round-trip ───────────────────────────────────────────

    #[test]
    fn set_get_16bit_round_trip_pc_sp() {
        let mut rf = RegisterFile::new();
        rf.set_16bit(Reg16::PC, 0xBEEF);
        rf.set_16bit(Reg16::SP, 0xCAFE);
        assert_eq!(rf.get_16bit(Reg16::PC), 0xBEEF);
        assert_eq!(rf.get_16bit(Reg16::SP), 0xCAFE);
    }

    #[test]
    fn set_get_16bit_round_trip_pairs() {
        let mut rf = RegisterFile::new();
        let pairs = [(Reg16::AF, 0x1234), (Reg16::BC, 0x5678),
                     (Reg16::DE, 0x9ABC), (Reg16::HL, 0xDEF0)];
        for (reg, val) in pairs {
            rf.set_16bit(reg, val);
            assert_eq!(rf.get_16bit(reg), val, "{:?} round-trip failed", reg);
        }
    }

    #[test]
    fn pair_bc_decomposes_to_b_and_c() {
        let mut rf = RegisterFile::new();
        rf.set_16bit(Reg16::BC, 0xAABB);
        assert_eq!(rf.get_8bit(Reg8::B), 0xAA);
        assert_eq!(rf.get_8bit(Reg8::C), 0xBB);
    }

    #[test]
    fn pair_de_decomposes_to_d_and_e() {
        let mut rf = RegisterFile::new();
        rf.set_16bit(Reg16::DE, 0xCCDD);
        assert_eq!(rf.get_8bit(Reg8::D), 0xCC);
        assert_eq!(rf.get_8bit(Reg8::E), 0xDD);
    }

    #[test]
    fn pair_hl_decomposes_to_h_and_l() {
        let mut rf = RegisterFile::new();
        rf.set_16bit(Reg16::HL, 0xEEFF);
        assert_eq!(rf.get_8bit(Reg8::H), 0xEE);
        assert_eq!(rf.get_8bit(Reg8::L), 0xFF);
    }

    #[test]
    fn pair_af_decomposes_to_a_and_f() {
        let mut rf = RegisterFile::new();
        rf.set_16bit(Reg16::AF, 0x12B0);
        assert_eq!(rf.get_8bit(Reg8::A), 0x12);
        assert_eq!(rf.get_8bit(Reg8::F), 0xB0);
    }

    #[test]
    fn composing_8bit_halves_yields_16bit_pair() {
        let mut rf = RegisterFile::new();
        rf.set_8bit(Reg8::H, 0xDE);
        rf.set_8bit(Reg8::L, 0xAD);
        assert_eq!(rf.get_16bit(Reg16::HL), 0xDEAD);
    }

    // ── Increment / Decrement ───────────────────────────────────────

    #[test]
    fn inc8_normal() {
        let mut rf = RegisterFile::new();
        rf.set_8bit(Reg8::A, 0x41);
        rf.inc8(Reg8::A);
        assert_eq!(rf.get_8bit(Reg8::A), 0x42);
    }

    #[test]
    fn inc8_wraps_at_boundary() {
        let mut rf = RegisterFile::new();
        rf.set_8bit(Reg8::B, 0xFF);
        rf.inc8(Reg8::B);
        assert_eq!(rf.get_8bit(Reg8::B), 0x00);
    }

    #[test]
    fn dec8_normal() {
        let mut rf = RegisterFile::new();
        rf.set_8bit(Reg8::C, 0x42);
        rf.dec8(Reg8::C);
        assert_eq!(rf.get_8bit(Reg8::C), 0x41);
    }

    #[test]
    fn dec8_wraps_at_boundary() {
        let mut rf = RegisterFile::new();
        rf.set_8bit(Reg8::D, 0x00);
        rf.dec8(Reg8::D);
        assert_eq!(rf.get_8bit(Reg8::D), 0xFF);
    }

    #[test]
    fn inc16_normal() {
        let mut rf = RegisterFile::new();
        rf.set_16bit(Reg16::PC, 0x00FF);
        rf.inc16(Reg16::PC);
        assert_eq!(rf.get_16bit(Reg16::PC), 0x0100);
    }

    #[test]
    fn inc16_wraps_at_boundary() {
        let mut rf = RegisterFile::new();
        rf.set_16bit(Reg16::SP, 0xFFFF);
        rf.inc16(Reg16::SP);
        assert_eq!(rf.get_16bit(Reg16::SP), 0x0000);
    }

    #[test]
    fn dec16_normal() {
        let mut rf = RegisterFile::new();
        rf.set_16bit(Reg16::HL, 0x0100);
        rf.dec16(Reg16::HL);
        assert_eq!(rf.get_16bit(Reg16::HL), 0x00FF);
    }

    #[test]
    fn dec16_wraps_at_boundary() {
        let mut rf = RegisterFile::new();
        rf.set_16bit(Reg16::BC, 0x0000);
        rf.dec16(Reg16::BC);
        assert_eq!(rf.get_16bit(Reg16::BC), 0xFFFF);
    }

    #[test]
    fn inc16_on_pair_propagates_carry_from_low_to_high() {
        let mut rf = RegisterFile::new();
        rf.set_8bit(Reg8::H, 0x01);
        rf.set_8bit(Reg8::L, 0xFF);
        rf.inc16(Reg16::HL);
        assert_eq!(rf.get_8bit(Reg8::H), 0x02);
        assert_eq!(rf.get_8bit(Reg8::L), 0x00);
    }

    // ── Flags ───────────────────────────────────────────────────────

    #[test]
    fn flags_occupy_correct_bit_positions() {
        // Z=bit7, N=bit6, H=bit5, C=bit4 — the SM83 flag register layout
        let mut rf = RegisterFile::new();

        rf.set_flag(Flag::Zero);
        assert_eq!(rf.get_8bit(Reg8::F), 0b1000_0000);

        rf.set_8bit(Reg8::F, 0);
        rf.set_flag(Flag::Subtract);
        assert_eq!(rf.get_8bit(Reg8::F), 0b0100_0000);

        rf.set_8bit(Reg8::F, 0);
        rf.set_flag(Flag::HalfCarry);
        assert_eq!(rf.get_8bit(Reg8::F), 0b0010_0000);

        rf.set_8bit(Reg8::F, 0);
        rf.set_flag(Flag::Carry);
        assert_eq!(rf.get_8bit(Reg8::F), 0b0001_0000);
    }

    #[test]
    fn set_flag_is_idempotent() {
        let mut rf = RegisterFile::new();
        rf.set_flag(Flag::Zero);
        rf.set_flag(Flag::Zero);
        assert_eq!(rf.get_8bit(Reg8::F), 0b1000_0000);
    }

    #[test]
    fn reset_flag_clears_only_target_bit() {
        let mut rf = RegisterFile::new();
        rf.set_8bit(Reg8::F, 0xF0); // all four flags set
        rf.reset_flag(Flag::Subtract);
        assert!(rf.is_flag_set(Flag::Zero));
        assert!(!rf.is_flag_set(Flag::Subtract));
        assert!(rf.is_flag_set(Flag::HalfCarry));
        assert!(rf.is_flag_set(Flag::Carry));
    }

    #[test]
    fn write_flag_true_sets_and_false_clears() {
        let mut rf = RegisterFile::new();
        rf.write_flag(Flag::Carry, true);
        assert!(rf.is_flag_set(Flag::Carry));
        rf.write_flag(Flag::Carry, false);
        assert!(!rf.is_flag_set(Flag::Carry));
    }

    #[test]
    fn all_four_flags_set_simultaneously() {
        let mut rf = RegisterFile::new();
        rf.set_flag(Flag::Zero);
        rf.set_flag(Flag::Subtract);
        rf.set_flag(Flag::HalfCarry);
        rf.set_flag(Flag::Carry);
        assert_eq!(rf.get_8bit(Reg8::F), 0xF0);
    }

    #[test]
    fn flags_visible_through_af_pair() {
        let mut rf = RegisterFile::new();
        rf.set_8bit(Reg8::A, 0x01);
        rf.set_flag(Flag::Zero);
        rf.set_flag(Flag::Carry);
        // A=0x01, F=0x90 → AF=0x0190
        assert_eq!(rf.get_16bit(Reg16::AF), 0x0190);
    }

    // ── Interrupts ──────────────────────────────────────────────────

    #[test]
    fn interrupt_enable_sets_correct_bit() {
        let mut rf = RegisterFile::new();
        rf.enable_interrupt(Interrupt::VBlank);
        assert_eq!(rf.get_8bit(Reg8::IE), 0b0000_0001);

        rf.enable_interrupt(Interrupt::Timer);
        assert_eq!(rf.get_8bit(Reg8::IE), 0b0000_0101);
    }

    #[test]
    fn interrupt_disable_clears_only_target() {
        let mut rf = RegisterFile::new();
        rf.set_8bit(Reg8::IE, 0b0001_1111); // all five enabled
        rf.disable_interrupt(Interrupt::Serial);
        assert!(rf.is_interrupt_enabled(Interrupt::VBlank));
        assert!(rf.is_interrupt_enabled(Interrupt::LCDStat));
        assert!(rf.is_interrupt_enabled(Interrupt::Timer));
        assert!(!rf.is_interrupt_enabled(Interrupt::Serial));
        assert!(rf.is_interrupt_enabled(Interrupt::Joypad));
    }

    #[test]
    fn all_five_interrupts_independent() {
        let mut rf = RegisterFile::new();
        let all = [Interrupt::VBlank, Interrupt::LCDStat,
                   Interrupt::Timer, Interrupt::Serial, Interrupt::Joypad];
        for &int in &all {
            assert!(!rf.is_interrupt_enabled(int));
            rf.enable_interrupt(int);
            assert!(rf.is_interrupt_enabled(int));
        }
        assert_eq!(rf.get_8bit(Reg8::IE), 0b0001_1111);
        for &int in &all {
            rf.disable_interrupt(int);
            assert!(!rf.is_interrupt_enabled(int));
        }
        assert_eq!(rf.get_8bit(Reg8::IE), 0);
    }

    // ── Overwrite semantics ─────────────────────────────────────────

    #[test]
    fn successive_writes_to_same_register() {
        let mut rf = RegisterFile::new();
        for val in 0..=255u8 {
            rf.set_8bit(Reg8::A, val);
        }
        assert_eq!(rf.get_8bit(Reg8::A), 0xFF);
    }

    #[test]
    fn set_16bit_pair_overwrites_both_halves() {
        let mut rf = RegisterFile::new();
        rf.set_16bit(Reg16::DE, 0xFFFF);
        rf.set_16bit(Reg16::DE, 0x0000);
        assert_eq!(rf.get_8bit(Reg8::D), 0x00);
        assert_eq!(rf.get_8bit(Reg8::E), 0x00);
    }
}

use core::fmt;

use crate::utils::bit_twiddling::{
    set_bit,
    reset_bit,
    is_bit_set,
};

use super::interrupts::Interrupt;

#[derive(Debug, Clone)]
pub enum Reg8 { A, F, B, C, D, E, H, L, IR, IE, }

#[derive(Debug, Clone)]
pub enum Reg16 { PC, SP, AF, BC, DE, HL, }

#[derive(Debug)]
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
        f.write_str("[PC: 0x")?;
        write!(f, "{:#^04X}", self.get_16bit(&Reg16::PC))?;
        f.write_str(" | SP: 0x")?;
        write!(f, "{:#^04X}", self.get_16bit(&Reg16::SP))?;
        f.write_str(" | AF: 0x")?;
        write!(f, "{:#^04X}", self.get_16bit(&Reg16::AF))?;
        f.write_str(" | BC: 0x")?;
        write!(f, "{:#^04X}", self.get_16bit(&Reg16::BC))?;
        f.write_str(" | DE: 0x")?;
        write!(f, "{:#^04X}", self.get_16bit(&Reg16::DE))?;
        f.write_str(" | HL: 0x")?;
        write!(f, "{:#^04X}", self.get_16bit(&Reg16::HL))?;
        f.write_str(" | IR: 0x")?;
        write!(f, "{:#^02X}", self.get_8bit(&Reg8::IR))?;
        f.write_str(" | IE: 0x")?;
        write!(f, "{:#^02X}", self.get_8bit(&Reg8::IE))?;
        f.write_str("]")
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

    pub fn get_8bit(&self, register: &Reg8) -> u8 {
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

    pub fn set_8bit(&mut self, register: &Reg8, value: u8) {
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

    pub fn get_16bit(&self, register: &Reg16) -> u16 {
        match register {
            Reg16::PC => self.pc,
            Reg16::SP => self.sp,
            Reg16::AF => u16::from_be_bytes([self.a, self.f]),
            Reg16::BC => u16::from_be_bytes([self.b, self.c]),
            Reg16::DE => u16::from_be_bytes([self.d, self.e]),
            Reg16::HL => u16::from_be_bytes([self.h, self.l]),
        }
    }

    pub fn set_16bit(&mut self, register: &Reg16, value: u16) {
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

    pub fn inc8(&mut self, reg: &Reg8) {
        let value = self.get_8bit(reg).wrapping_add(1);
        self.set_8bit(reg, value);
    }

    pub fn dec8(&mut self, reg: &Reg8) {
        let value = self.get_8bit(reg).wrapping_sub(1);
        self.set_8bit(reg, value);
    }

    pub fn inc16(&mut self, reg: &Reg16) {
        let value = self.get_16bit(reg).wrapping_add(1);
        self.set_16bit(reg, value);
    }

    pub fn dec16(&mut self, reg: &Reg16) {
        let value = self.get_16bit(reg).wrapping_sub(1);
        self.set_16bit(reg, value);
    }
   
    fn set_register_bit(&mut self, register: Reg8, bit: u8) {
        match register {
            Reg8::A => self.a = set_bit(self.a, bit),
            Reg8::F => self.f = set_bit(self.f, bit),
            Reg8::IR => self.ir = set_bit(self.ir, bit),
            Reg8::IE => self.ie = set_bit(self.ie, bit),
            _ => panic!(
                "Bitwise operation 'set' not allowed on register: {:?}",
                register
            ),
        }
    }

    fn reset_register_bit(&mut self, register: Reg8, bit: u8) {
        match register {
            Reg8::A => self.a = reset_bit(self.a, bit),
            Reg8::F => self.f = reset_bit(self.f, bit),
            Reg8::IR => self.ir = reset_bit(self.ir, bit),
            Reg8::IE => self.ie = reset_bit(self.ie, bit),
            _ => panic!(
                "Bitwise operation 'reset' not allowed on register: {:?}",
                register
            ),
        }
    }

    fn is_register_bit_set(&self, register: Reg8, bit: u8) -> bool {
        match register {
            Reg8::A => is_bit_set(self.a, bit),
            Reg8::F => is_bit_set(self.f, bit),
            Reg8::IR => is_bit_set(self.ir, bit),
            Reg8::IE => is_bit_set(self.ie, bit),
            _ => panic!(
                "Bitwise check 'is_set' not allowed on register: {:?}",
                register
            ),
        }
    }

    pub fn set_flag(&mut self, flag: Flag) {
        self.set_register_bit(Reg8::F, flag as u8);
    }

    pub fn reset_flag(&mut self, flag: Flag) {
        self.reset_register_bit(Reg8::F, flag as u8);
    }

    pub fn is_flag_set(&self, flag: Flag) -> bool {
        self.is_register_bit_set(Reg8::F, flag as u8)
    }

    pub fn enable_interrupt(&mut self, interrupt: Interrupt) {
        self.set_register_bit(Reg8::IE, interrupt as u8);
    }

    pub fn disable_interrupt(&mut self, interrupt: Interrupt) {
        self.reset_register_bit(Reg8::IE, interrupt as u8);
    }

    pub fn is_interrupt_enabled(&self, interrupt: Interrupt) -> bool {
        self.is_register_bit_set(Reg8::IE, interrupt as u8)
    }
}
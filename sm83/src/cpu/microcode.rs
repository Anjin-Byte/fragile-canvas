use crate::cpu::CPU;
use crate::memory::bus::Bus;

use super::{
    alu::{
        alu,
        AluResult,
        AluOpKind
    },
    registers::{
        Flag,
        Reg16,
        Reg8
    }
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operand8 { Reg(Reg8), Imm(u8) }

pub const FLAG_Z: u8 = 0b1000_0000;
pub const FLAG_N: u8 = 0b0100_0000;
pub const FLAG_H: u8 = 0b0010_0000;
pub const FLAG_C: u8 = 0b0001_0000;

fn flags(z: bool, n: bool, h: bool, c: bool) -> u8 {
    (if z { FLAG_Z } else { 0 })
    | (if n { FLAG_N } else { 0 })
    | (if h { FLAG_H } else { 0 })
    | (if c { FLAG_C } else { 0 })
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Condition {
    NZ, // Not Zero  (Z == 0)
    Z,  // Zero      (Z == 1)
    NC, // Not Carry (C == 0)
    C,  // Carry     (C == 1)
}

impl Condition {
    pub fn eval(self, flags: u8) -> bool {
        match self {
            Condition::NZ => (flags & FLAG_Z) == 0,
            Condition::Z  => (flags & FLAG_Z) != 0,
            Condition::NC => (flags & FLAG_C) == 0,
            Condition::C  => (flags & FLAG_C) != 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MicroOp {
    // ----------------------------------------
    // Memory & Register Access
    // ----------------------------------------
    LoadReg8 { dst: Reg8, src: Reg8 },
    LoadReg16 { dst: Reg16, src: Reg16 },
    ReadImmediate8 { into: Reg8 },
    ReadImmediate16 { into: Reg16 },
    ReadMemReg8 { addr_reg: Reg16, into: Reg8 },
    ReadMemImm8 { addr: u16, into: Reg8 },
    /// Read from high-page (0xFF00 + offset) into an 8-bit register
    ReadHighPage { offset: Reg8, into: Reg8 },
    WriteMemReg8 { addr_reg: Reg16, src: Reg8 },
    WriteMemImm8 { addr: u16, src: Reg8 },
    /// Write an 8-bit register into high-page (0xFF00 + offset)
    WriteHighPage { offset: Reg8, src: Reg8 },
    Push { src: Reg16 },
    Pop { dst: Reg16 },

    // ----------------------------------------
    // Arithmetic & Logic (8-bit)
    // ----------------------------------------
    Alu8 { kind: AluOpKind, dest: Reg8, src: Operand8 },
    /// Decimal Adjust Accumulator (BCD correction)
    Daa,
    /// One's complement on A (flip bits)
    Cpl,
    /// Set Carry flag, clear N and H
    Scf,
    /// Complement Carry flag, clear N and H
    Ccf,
    Inc8 { target: Operand8 },
    Dec8 { target: Operand8 },

    // ----------------------------------------
    // 16-bit Arithmetic
    // ----------------------------------------
    Add16 { dest: Reg16, src: Reg16 },
    /// Add signed 8-bit immediate to SP, update H/C (SP = SP + e)
    AddSpE { e: i8 },
    Inc16 { reg: Reg16 },
    Dec16 { reg: Reg16 },

    // ----------------------------------------
    // Rotate, Shift & Bit Ops
    // ----------------------------------------
    Rlc { dst: Reg8, src: Reg8 },
    Rrc { dst: Reg8, src: Reg8 },
    Rl  { dst: Reg8, src: Reg8 },
    Rr  { dst: Reg8, src: Reg8 },
    /// Accumulator rotates (0x07/0x0F/0x17/0x1F) — always Z=0, N=0, H=0
    RlcA,
    RrcA,
    RlA,
    RrA,
    Sla { dst: Reg8, src: Reg8 },
    Sra { dst: Reg8, src: Reg8 },
    Srl { dst: Reg8, src: Reg8 },
    Swap { dst: Reg8, src: Reg8 },
    BitTest { bit: u8, reg: Reg8 },
    SetBit { bit: u8, reg: Reg8 },
    ResetBit { bit: u8, reg: Reg8 },

    // ----------------------------------------
    // Control Flow
    // ----------------------------------------
    /// Check a condition (NZ, Z, NC, C) against flags
    CheckCond { cond: Condition },
    /// Unconditional absolute jump
    JumpAbs { addr: u16 },
    /// Relative jump by signed offset
    JumpRel { offset: i8 },
    /// Call subroutine at absolute address
    Call { addr: u16 },
    /// Return from subroutine
    Ret,
    /// Return from interrupt (and enable IME)
    RetI,
    /// Restart (call) to fixed vector
    Rst { addr: u16 },
    /// Fetch next opcode from PC
    FetchOpcode,
    /// Decode a CB-prefixed opcode
    DecodeCb { prefix: u8 },

    // ----------------------------------------
    // Compound Ops (read operands from PC)
    // ----------------------------------------
    /// JR e: read signed byte from [PC++], PC += offset
    JumpRelImm,
    /// JP nn: read u16 from [PC], PC = addr
    JumpAbsImm,
    /// JP HL: PC = HL
    JumpHL,
    /// CALL nn: read u16 from [PC], push return addr, jump
    CallImm,
    /// ADD SP, e: read signed byte from [PC++], SP += e, set H/C
    AddSpImm,
    /// LD HL, SP+e: read signed byte from [PC++], HL = SP + e, set H/C
    LoadHlSpImm,
    /// LD r, (nn): read u16 addr from [PC], load [addr] into reg
    ReadMem16BitAddr { into: Reg8 },
    /// LD (nn), r: read u16 addr from [PC], write reg to [addr]
    WriteMem16BitAddr { src: Reg8 },
    /// LD (nn), SP: read u16 addr from [PC], write SP.lo to [addr], SP.hi to [addr+1]
    WriteSp16BitAddr,

    // ----------------------------------------
    // System & Interrupt State
    // ----------------------------------------
    /// Set or clear a specific flag (Z, N, H, C)
    SetFlag { flag: Flag, value: bool },
    /// Enable or disable IME (global interrupt enable)
    SetIme { value: bool },
    /// Schedule IME to be set after next instruction
    DeferImeEnable,
    /// Enter HALT state
    TriggerHalt,
    /// Enter STOP state
    TriggerStop,
    /// Check and service pending interrupts
    CheckInterrupts,
}

pub fn execute(cpu: &mut CPU, bus: &mut Bus, op: MicroOp) {
    match op {
        // Memory & Register Access ---------------------------------------------------
        MicroOp::LoadReg8 { dst, src } => {
            let v = cpu.register_file.get_8bit(src);
            cpu.register_file.set_8bit(dst, v);
        }
        MicroOp::LoadReg16 { dst, src } => {
            let value = cpu.register_file.get_16bit(src);
            cpu.register_file.set_16bit(dst, value);
        },
        MicroOp::ReadImmediate8 { into } => {
            let pc_addr = cpu.register_file.get_16bit(Reg16::PC);
            let imm = bus.read(pc_addr);
            cpu.register_file.inc16(Reg16::PC);
            cpu.register_file.set_8bit(into, imm);
        },
        MicroOp::ReadImmediate16 { into } => {
            let pc = cpu.register_file.get_16bit(Reg16::PC);
            let lo = bus.read(pc);
            cpu.register_file.inc16(Reg16::PC);
    
            let pc = cpu.register_file.get_16bit(Reg16::PC);
            let hi = bus.read(pc);
            cpu.register_file.inc16(Reg16::PC);

            let value = u16::from_le_bytes([lo, hi]);
            cpu.register_file.set_16bit(into, value);
        },
        MicroOp::ReadMemReg8 { addr_reg, into }  => {
            let address = cpu.register_file.get_16bit(addr_reg);
            let value = bus.read(address);
            cpu.register_file.set_8bit(into, value);
        },
        MicroOp::ReadMemImm8 { addr, into } => {
            let value = bus.read(addr);
            cpu.register_file.set_8bit(into, value);
        },
        MicroOp::ReadHighPage { offset, into } => {
            let off_val = cpu.register_file.get_8bit(offset) as u16;  
            let addr = 0xFF00_u16.wrapping_add(off_val);
            let value = bus.read(addr);                  
            cpu.register_file.set_8bit(into, value);                        
        },
        MicroOp::WriteMemReg8 { addr_reg, src } => {
            let addr = cpu.register_file.get_16bit(addr_reg);
            let value = cpu.register_file.get_8bit(src);
            bus.write(addr, value);
        },        
        MicroOp::WriteMemImm8 { addr, src } => {
            let value = cpu.register_file.get_8bit(src);
            bus.write(addr, value);
        },
        MicroOp::WriteHighPage { offset, src } => {
            let offset_val = cpu.register_file.get_8bit(offset);              
            let addr = 0xFF00u16.wrapping_add(offset_val as u16);              
            let value = cpu.register_file.get_8bit(src);                     
            bus.write(addr, value);                              
        },

        // Stack Operations -------------------------------------------------------------
        MicroOp::Push { src } => {
            let value = cpu.register_file.get_16bit(src);
            let [high, low] = value.to_be_bytes();
            cpu.register_file.dec16(Reg16::SP);

            let sp = cpu.register_file.get_16bit(Reg16::SP);
            bus.write(sp, high);
            cpu.register_file.dec16(Reg16::SP);

            let sp = cpu.register_file.get_16bit(Reg16::SP);
            bus.write(sp, low);
        },
        MicroOp::Pop { dst } => {
            let sp_addr = cpu.register_file.get_16bit(Reg16::SP);
            let low = bus.read(sp_addr);
            cpu.register_file.inc16(Reg16::SP);

            let sp_addr = cpu.register_file.get_16bit(Reg16::SP);
            let high = bus.read(sp_addr);
            cpu.register_file.inc16(Reg16::SP);

            let value = u16::from_le_bytes([low, high]);
            cpu.register_file.set_16bit(dst, value);
            // !!! Special case: POP AF must mask F’s low nibble
            // On real hardware, the lower 4 bits of F are always zero; if dst == AF,
            // clear any stray low bits from the popped value.
            // Could add logic in register_file to ground lower 4 bits
            if let Reg16::AF = dst {
                let f = cpu.register_file.get_8bit(Reg8::F) & 0xF0;
                cpu.register_file.set_8bit(Reg8::F, f);
            }
        },

        // Arithmetic & Logic (8-bit) --------------------------------------------------
        MicroOp::Alu8 { kind, dest, src } => {
            let a = cpu.register_file.get_8bit(dest);
            let b = match src {
                Operand8::Reg(r) => cpu.register_file.get_8bit(r),
                Operand8::Imm(v)   => v,
            };
            let carry_in = (cpu.register_file.get_8bit(Reg8::F) & FLAG_C) != 0;
            let is_cp = matches!(kind, AluOpKind::Cp);

            let AluResult { value, code: flag } = alu(kind, a, b, carry_in);
            if !is_cp {
                cpu.register_file.set_8bit(dest, value);
            }

            cpu.register_file.set_8bit(Reg8::F, flags(flag.z, flag.n, flag.h, flag.c));
        },
        MicroOp::Daa => {
            let mut a = cpu.register_file.get_8bit(Reg8::A);
            let old_f = cpu.register_file.get_8bit(Reg8::F);
            let n = old_f & FLAG_N != 0;
            let h = old_f & FLAG_H != 0;
            let c = old_f & FLAG_C != 0;
        
            let mut adj = 0;            // Adjustment value to be added/subtracted from A
            let mut new_c = c;          // Updated carry flag (may change if adjustment triggers carry)
        
            if !n {
                // If the previous operation was an addition
                // Adjust the low nibble if half-carry occurred or the value is too large for BCD
                if h || (a & 0x0F) > 9 {
                    adj |= 0x06; // Add 6 to fix invalid BCD in lower 4 bits
                }
                // Adjust the high nibble if carry occurred or A > 99 (invalid BCD)
                if c || a > 0x99 {
                    adj |= 0x60; // Add 0x60 (i.e., +6 in the high nibble)
                    new_c = true; // This adjustment triggers a carry flag
                }
                a = a.wrapping_add(adj); // Apply BCD correction
            } else {
                // If the previous operation was a subtraction
                // Subtract the same adjustments as needed
                if h {
                    adj |= 0x06;
                }
                if c {
                    adj |= 0x60;
                }
                a = a.wrapping_sub(adj); // Apply BCD correction
            }
            cpu.register_file.set_8bit(Reg8::A, a);
        
            cpu.register_file.set_8bit(Reg8::F, flags(a == 0, n, false, new_c));
        },    
        MicroOp::Cpl => {
            let a = cpu.register_file.get_8bit(Reg8::A);
            cpu.register_file.set_8bit(Reg8::A, !a);
            // N=1, H=1, preserve Z and C
            let prev = cpu.register_file.get_8bit(Reg8::F);
            let newf = (prev & (FLAG_Z | FLAG_C)) | FLAG_N | FLAG_H;
            cpu.register_file.set_8bit(Reg8::F, newf);
        },
        MicroOp::Scf => {
            // C=1, N=0, H=0, preserve Z
            let prev_z = cpu.register_file.get_8bit(Reg8::F) & FLAG_Z;
            cpu.register_file.set_8bit(Reg8::F, prev_z | FLAG_C);
        },
        MicroOp::Ccf => {
            // C ← ¬C, N=0, H=0, preserve Z
            let prev = cpu.register_file.get_8bit(Reg8::F);
            let prev_z = prev & FLAG_Z;
            let new_c = if prev & FLAG_C == 0 { FLAG_C } else { 0 };
            cpu.register_file.set_8bit(Reg8::F, prev_z | new_c);
        },        
        MicroOp::Inc8 { target } => {
            let (reg, old) = match target {
                Operand8::Reg(r) => (r, cpu.register_file.get_8bit(r)),
                _ => panic!("Inc8 only supports register targets"),
            };
            let res = old.wrapping_add(1);
            cpu.register_file.set_8bit(reg, res);
            let old_c = cpu.register_file.get_8bit(Reg8::F) & FLAG_C;
            cpu.register_file.set_8bit(Reg8::F,
                flags(res == 0, false, (old & 0x0F) + 1 > 0x0F, false) | old_c);
        },
        MicroOp::Dec8 { target } => {
            let (reg, old) = match target {
                Operand8::Reg(r) => (r, cpu.register_file.get_8bit(r)),
                _ => panic!("Dec8 only supports register targets"),
            };
            let res = old.wrapping_sub(1);
            cpu.register_file.set_8bit(reg, res);
            let old_c = cpu.register_file.get_8bit(Reg8::F) & FLAG_C;
            cpu.register_file.set_8bit(Reg8::F,
                flags(res == 0, true, (old & 0x0F) == 0, false) | old_c);
        },

        // 16-bit Arithmetic  --------------------------------------------------
        MicroOp::Add16 { dest, src } => {
            // HL (or other dest) <- dest + src
            let a = cpu.register_file.get_16bit(dest);
            let b = cpu.register_file.get_16bit(src);
            let result = a.wrapping_add(b);
            cpu.register_file.set_16bit(dest, result);
        
            let old_z = cpu.register_file.get_8bit(Reg8::F) & FLAG_Z;
            cpu.register_file.set_8bit(Reg8::F,
                flags(false, false,
                    (a & 0x0FFF) + (b & 0x0FFF) > 0x0FFF,
                    (a as u32 + b as u32) > 0xFFFF) | old_z);
        },
        MicroOp::AddSpE { e } => {
            // SP <- SP + signed immediate e
            let sp = cpu.register_file.get_16bit(Reg16::SP);
            let result = (sp as i16).wrapping_add(e as i16) as u16;
            cpu.register_file.set_16bit(Reg16::SP, result);
        
            let imm_u = e as u8 as u16;
            cpu.register_file.set_8bit(Reg8::F, flags(false, false,
                (sp & 0x000F) + (imm_u & 0x000F) > 0x000F,
                (sp & 0x00FF) + imm_u > 0x00FF));
        },
        MicroOp::Inc16 { reg } => {
            cpu.register_file.inc16(reg);
        },
        MicroOp::Dec16 { reg } => {
            cpu.register_file.dec16(reg);
        },

        // Rotate, Shift & Bit Ops  --------------------------------------------------
        MicroOp::Rlc { dst, src } => {
            let val = cpu.register_file.get_8bit(src);
            let carry = val >> 7;
            let result = (val << 1) | carry;
            cpu.register_file.set_8bit(dst, result);
            cpu.register_file.set_8bit(Reg8::F, flags(result == 0, false, false, carry != 0));
        },
        MicroOp::Rrc { dst, src } => {
            let val = cpu.register_file.get_8bit(src);
            let carry = val & 1;
            let result = (val >> 1) | (carry << 7);
            cpu.register_file.set_8bit(dst, result);
            cpu.register_file.set_8bit(Reg8::F, flags(result == 0, false, false, carry != 0));
        },
        MicroOp::Rl { dst, src } => {
            let val = cpu.register_file.get_8bit(src);
            let old_carry = (cpu.register_file.get_8bit(Reg8::F) & FLAG_C) != 0;
            let new_carry = val >> 7;
            let result = (val << 1) | (old_carry as u8);
            cpu.register_file.set_8bit(dst, result);
            cpu.register_file.set_8bit(Reg8::F, flags(result == 0, false, false, new_carry != 0));
        },
        MicroOp::Rr { dst, src } => {
            let val = cpu.register_file.get_8bit(src);
            let old_carry = (cpu.register_file.get_8bit(Reg8::F) & FLAG_C) != 0;
            let new_carry = val & 1;
            let result = (val >> 1) | ((old_carry as u8) << 7);
            cpu.register_file.set_8bit(dst, result);
            cpu.register_file.set_8bit(Reg8::F, flags(result == 0, false, false, new_carry != 0));
        },

        // Accumulator rotates — identical math but always Z=0
        MicroOp::RlcA => {
            let val = cpu.register_file.get_8bit(Reg8::A);
            let carry = val >> 7;
            let result = (val << 1) | carry;
            cpu.register_file.set_8bit(Reg8::A, result);
            cpu.register_file.set_8bit(Reg8::F, flags(false, false, false, carry != 0));
        },
        MicroOp::RrcA => {
            let val = cpu.register_file.get_8bit(Reg8::A);
            let carry = val & 1;
            let result = (val >> 1) | (carry << 7);
            cpu.register_file.set_8bit(Reg8::A, result);
            cpu.register_file.set_8bit(Reg8::F, flags(false, false, false, carry != 0));
        },
        MicroOp::RlA => {
            let val = cpu.register_file.get_8bit(Reg8::A);
            let old_carry = (cpu.register_file.get_8bit(Reg8::F) & FLAG_C) != 0;
            let new_carry = val >> 7;
            let result = (val << 1) | (old_carry as u8);
            cpu.register_file.set_8bit(Reg8::A, result);
            cpu.register_file.set_8bit(Reg8::F, flags(false, false, false, new_carry != 0));
        },
        MicroOp::RrA => {
            let val = cpu.register_file.get_8bit(Reg8::A);
            let old_carry = (cpu.register_file.get_8bit(Reg8::F) & FLAG_C) != 0;
            let new_carry = val & 1;
            let result = (val >> 1) | ((old_carry as u8) << 7);
            cpu.register_file.set_8bit(Reg8::A, result);
            cpu.register_file.set_8bit(Reg8::F, flags(false, false, false, new_carry != 0));
        },

        MicroOp::Sla { dst, src } => {
            let val = cpu.register_file.get_8bit(src);
            let carry = val >> 7;
            let result = val << 1;
            cpu.register_file.set_8bit(dst, result);
            cpu.register_file.set_8bit(Reg8::F, flags(result == 0, false, false, carry != 0));
        },
        MicroOp::Sra { dst, src } => {
            let val = cpu.register_file.get_8bit(src);
            let carry = val & 1;
            let result = (val >> 1) | (val & 0x80);
            cpu.register_file.set_8bit(dst, result);
            cpu.register_file.set_8bit(Reg8::F, flags(result == 0, false, false, carry != 0));
        },
        MicroOp::Srl { dst, src } => {
            let val = cpu.register_file.get_8bit(src);
            let carry = val & 1;
            let result = val >> 1;
            cpu.register_file.set_8bit(dst, result);
            cpu.register_file.set_8bit(Reg8::F, flags(result == 0, false, false, carry != 0));
        },
        MicroOp::Swap { dst, src } => {
            let val = cpu.register_file.get_8bit(src);
            let result = (val >> 4) | (val << 4);
            cpu.register_file.set_8bit(dst, result);
            cpu.register_file.set_8bit(Reg8::F, flags(result == 0, false, false, false));
        },
        MicroOp::BitTest { bit, reg } => {
            let val = cpu.register_file.get_8bit(reg);
            let is_zero = (val & (1 << bit)) == 0;
            let old_c = cpu.register_file.get_8bit(Reg8::F) & FLAG_C;
            cpu.register_file.set_8bit(Reg8::F, flags(is_zero, false, true, false) | old_c);
        },
        MicroOp::SetBit { bit, reg } => {
            let val = cpu.register_file.get_8bit(reg);
            cpu.register_file.set_8bit(reg, val | (1 << bit));
        },
        MicroOp::ResetBit { bit, reg } => {
            let val = cpu.register_file.get_8bit(reg);
            cpu.register_file.set_8bit(reg, val & !(1 << bit));
        },

        // Control Flow  --------------------------------------------------
        MicroOp::CheckCond { cond } => {
            let flags = cpu.register_file.get_8bit(Reg8::F);
            if !cond.eval(flags) {
                // Signal to the tick loop that the condition failed.
                // The tick loop will stop executing remaining micro-ops
                // and advance PC past any unread immediates.
                cpu.condition_taken = false;
            }
        },
        MicroOp::JumpAbs { addr } => {
            cpu.register_file.set_16bit(Reg16::PC, addr);
        },
        MicroOp::JumpRel { offset } => {
            let pc = cpu.register_file.get_16bit(Reg16::PC);
            let new_pc = (pc as i16).wrapping_add(offset as i16) as u16;
            cpu.register_file.set_16bit(Reg16::PC, new_pc);
        },
        MicroOp::Call { addr } => {
            // Push current PC, then jump
            let pc = cpu.register_file.get_16bit(Reg16::PC);
            let [hi, lo] = pc.to_be_bytes();
            cpu.register_file.dec16(Reg16::SP);
            let sp = cpu.register_file.get_16bit(Reg16::SP);
            bus.write(sp, hi);
            cpu.register_file.dec16(Reg16::SP);
            let sp = cpu.register_file.get_16bit(Reg16::SP);
            bus.write(sp, lo);

            cpu.register_file.set_16bit(Reg16::PC, addr);
        },
        MicroOp::Ret => {
            // Pop address from stack into PC
            let sp = cpu.register_file.get_16bit(Reg16::SP);
            let lo = bus.read(sp);
            cpu.register_file.inc16(Reg16::SP);
            let sp = cpu.register_file.get_16bit(Reg16::SP);
            let hi = bus.read(sp);
            cpu.register_file.inc16(Reg16::SP);

            let addr = u16::from_le_bytes([lo, hi]);
            cpu.register_file.set_16bit(Reg16::PC, addr);
        },
        MicroOp::RetI => {
            // Pop address from stack into PC, then enable IME
            let sp = cpu.register_file.get_16bit(Reg16::SP);
            let lo = bus.read(sp);
            cpu.register_file.inc16(Reg16::SP);
            let sp = cpu.register_file.get_16bit(Reg16::SP);
            let hi = bus.read(sp);
            cpu.register_file.inc16(Reg16::SP);

            let addr = u16::from_le_bytes([lo, hi]);
            cpu.register_file.set_16bit(Reg16::PC, addr);
            cpu.ime = true;
        },
        MicroOp::Rst { addr } => {
            // Push PC, jump to fixed vector (same mechanics as Call)
            let pc = cpu.register_file.get_16bit(Reg16::PC);
            let [hi, lo] = pc.to_be_bytes();
            cpu.register_file.dec16(Reg16::SP);
            let sp = cpu.register_file.get_16bit(Reg16::SP);
            bus.write(sp, hi);
            cpu.register_file.dec16(Reg16::SP);
            let sp = cpu.register_file.get_16bit(Reg16::SP);
            bus.write(sp, lo);

            cpu.register_file.set_16bit(Reg16::PC, addr);
        },
        MicroOp::FetchOpcode => {
            // FetchOpcode is no longer needed — tick() handles fetch directly.
        },
        MicroOp::DecodeCb { prefix: _ } => {
            // DecodeCb is no longer needed — tick() handles CB prefix directly.
        },

        // Compound Ops (read operands from PC) ----------------------------------------
        MicroOp::JumpRelImm => {
            let pc = cpu.register_file.get_16bit(Reg16::PC);
            let offset = bus.read(pc) as i8;
            cpu.register_file.inc16(Reg16::PC);
            let new_pc = cpu.register_file.get_16bit(Reg16::PC);
            let new_pc = (new_pc as i16).wrapping_add(offset as i16) as u16;
            cpu.register_file.set_16bit(Reg16::PC, new_pc);
        },
        MicroOp::JumpAbsImm => {
            let pc = cpu.register_file.get_16bit(Reg16::PC);
            let lo = bus.read(pc);
            let hi = bus.read(pc.wrapping_add(1));
            cpu.register_file.set_16bit(Reg16::PC, u16::from_le_bytes([lo, hi]));
        },
        MicroOp::JumpHL => {
            let hl = cpu.register_file.get_16bit(Reg16::HL);
            cpu.register_file.set_16bit(Reg16::PC, hl);
        },
        MicroOp::CallImm => {
            let pc = cpu.register_file.get_16bit(Reg16::PC);
            let lo = bus.read(pc);
            let hi = bus.read(pc.wrapping_add(1));
            let target = u16::from_le_bytes([lo, hi]);
            // PC now points past the two address bytes
            let ret_addr = pc.wrapping_add(2);
            // Push return address
            let [ret_hi, ret_lo] = ret_addr.to_be_bytes();
            cpu.register_file.dec16(Reg16::SP);
            let sp = cpu.register_file.get_16bit(Reg16::SP);
            bus.write(sp, ret_hi);
            cpu.register_file.dec16(Reg16::SP);
            let sp = cpu.register_file.get_16bit(Reg16::SP);
            bus.write(sp, ret_lo);
            cpu.register_file.set_16bit(Reg16::PC, target);
        },
        MicroOp::AddSpImm => {
            let pc = cpu.register_file.get_16bit(Reg16::PC);
            let e = bus.read(pc) as i8;
            cpu.register_file.inc16(Reg16::PC);
            let sp = cpu.register_file.get_16bit(Reg16::SP);
            let result = sp.wrapping_add(e as i16 as u16);
            // Flags based on lower byte addition
            let half = ((sp & 0xF) + (e as i16 as u16 & 0xF)) > 0xF;
            let carry = ((sp & 0xFF) + (e as i16 as u16 & 0xFF)) > 0xFF;
            cpu.register_file.set_16bit(Reg16::SP, result);
            cpu.register_file.set_8bit(Reg8::F, flags(false, false, half, carry));
        },
        MicroOp::LoadHlSpImm => {
            let pc = cpu.register_file.get_16bit(Reg16::PC);
            let e = bus.read(pc) as i8;
            cpu.register_file.inc16(Reg16::PC);
            let sp = cpu.register_file.get_16bit(Reg16::SP);
            let result = sp.wrapping_add(e as i16 as u16);
            let half = ((sp & 0xF) + (e as i16 as u16 & 0xF)) > 0xF;
            let carry = ((sp & 0xFF) + (e as i16 as u16 & 0xFF)) > 0xFF;
            cpu.register_file.set_16bit(Reg16::HL, result);
            cpu.register_file.set_8bit(Reg8::F, flags(false, false, half, carry));
        },
        MicroOp::ReadMem16BitAddr { into } => {
            let pc = cpu.register_file.get_16bit(Reg16::PC);
            let lo = bus.read(pc);
            let hi = bus.read(pc.wrapping_add(1));
            let addr = u16::from_le_bytes([lo, hi]);
            let value = bus.read(addr);
            cpu.register_file.set_8bit(into, value);
            cpu.register_file.set_16bit(Reg16::PC, pc.wrapping_add(2));
        },
        MicroOp::WriteMem16BitAddr { src } => {
            let pc = cpu.register_file.get_16bit(Reg16::PC);
            let lo = bus.read(pc);
            let hi = bus.read(pc.wrapping_add(1));
            let addr = u16::from_le_bytes([lo, hi]);
            let value = cpu.register_file.get_8bit(src);
            bus.write(addr, value);
            cpu.register_file.set_16bit(Reg16::PC, pc.wrapping_add(2));
        },
        MicroOp::WriteSp16BitAddr => {
            let pc = cpu.register_file.get_16bit(Reg16::PC);
            let lo = bus.read(pc);
            let hi = bus.read(pc.wrapping_add(1));
            let addr = u16::from_le_bytes([lo, hi]);
            let sp = cpu.register_file.get_16bit(Reg16::SP);
            let [sp_lo, sp_hi] = sp.to_le_bytes();
            bus.write(addr, sp_lo);
            bus.write(addr.wrapping_add(1), sp_hi);
            cpu.register_file.set_16bit(Reg16::PC, pc.wrapping_add(2));
        },

        // System & Interrupt State  --------------------------------------------------
        MicroOp::SetFlag { flag, value } => {
            cpu.register_file.write_flag(flag, value);
        },
        MicroOp::SetIme { value } => {
            cpu.ime = value;
        },
        MicroOp::DeferImeEnable => {
            // EI enables IME after the *next* instruction completes
            cpu.ime_defer = true;
        },
        MicroOp::TriggerHalt => {
            cpu.halted = true;
        },
        MicroOp::TriggerStop => {
            // STOP: for now, treat same as HALT
            // On real hardware this also affects clock/LCD
            cpu.halted = true;
        },
        MicroOp::CheckInterrupts => {
            // Interrupt dispatch is now handled directly in cpu.tick().
            // This micro-op is retained only for backward compatibility.
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::CPU;
    use crate::memory::bus::Bus;

    fn make_cpu() -> (CPU, Bus) {
        (CPU::new(crate::trace::Tracer::off()), Bus::new())
    }

    fn set_flags(cpu: &mut CPU, z: bool, n: bool, h: bool, c: bool) {
        cpu.register_file.set_8bit(Reg8::F, flags(z, n, h, c));
    }

    fn read_flags(cpu: &CPU) -> (bool, bool, bool, bool) {
        let f = cpu.register_file.get_8bit(Reg8::F);
        (
            f & FLAG_Z != 0,
            f & FLAG_N != 0,
            f & FLAG_H != 0,
            f & FLAG_C != 0,
        )
    }

    // =====================================================================
    //  RLC
    // =====================================================================

    #[test]
    fn rlc_zero() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::B, 0x00);
        execute(&mut cpu, &mut bus, MicroOp::Rlc { dst: Reg8::B, src: Reg8::B });
        assert_eq!(cpu.register_file.get_8bit(Reg8::B), 0x00);
        assert_eq!(read_flags(&cpu), (true, false, false, false));
    }

    #[test]
    fn rlc_bit7_rotates_to_bit0_and_carry() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x80);
        execute(&mut cpu, &mut bus, MicroOp::Rlc { dst: Reg8::A, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x01);
        assert_eq!(read_flags(&cpu), (false, false, false, true));
    }

    #[test]
    fn rlc_0xff_stays_0xff_carry_set() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0xFF);
        execute(&mut cpu, &mut bus, MicroOp::Rlc { dst: Reg8::A, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0xFF);
        assert_eq!(read_flags(&cpu), (false, false, false, true));
    }

    #[test]
    fn rlc_no_carry_when_bit7_clear() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x01);
        execute(&mut cpu, &mut bus, MicroOp::Rlc { dst: Reg8::A, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x02);
        assert_eq!(read_flags(&cpu), (false, false, false, false));
    }

    #[test]
    fn rlc_different_dst_src() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::B, 0x85); // 1000_0101
        cpu.register_file.set_8bit(Reg8::A, 0x00);
        execute(&mut cpu, &mut bus, MicroOp::Rlc { dst: Reg8::A, src: Reg8::B });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x0B); // 0000_1011
        assert_eq!(cpu.register_file.get_8bit(Reg8::B), 0x85); // src unchanged
        assert_eq!(read_flags(&cpu), (false, false, false, true));
    }

    #[test]
    fn rlc_clears_n_and_h_flags() {
        let (mut cpu, mut bus) = make_cpu();
        set_flags(&mut cpu, true, true, true, true);
        cpu.register_file.set_8bit(Reg8::A, 0x01);
        execute(&mut cpu, &mut bus, MicroOp::Rlc { dst: Reg8::A, src: Reg8::A });
        let (_, n, h, _) = read_flags(&cpu);
        assert!(!n);
        assert!(!h);
    }

    #[test]
    fn rlc_0x55() {
        // 0101_0101 → 1010_1010, C=0
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x55);
        execute(&mut cpu, &mut bus, MicroOp::Rlc { dst: Reg8::A, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0xAA);
        assert_eq!(read_flags(&cpu), (false, false, false, false));
    }

    // =====================================================================
    //  RRC
    // =====================================================================

    #[test]
    fn rrc_zero() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x00);
        execute(&mut cpu, &mut bus, MicroOp::Rrc { dst: Reg8::A, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x00);
        assert_eq!(read_flags(&cpu), (true, false, false, false));
    }

    #[test]
    fn rrc_bit0_rotates_to_bit7_and_carry() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x01);
        execute(&mut cpu, &mut bus, MicroOp::Rrc { dst: Reg8::A, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x80);
        assert_eq!(read_flags(&cpu), (false, false, false, true));
    }

    #[test]
    fn rrc_0xff() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0xFF);
        execute(&mut cpu, &mut bus, MicroOp::Rrc { dst: Reg8::A, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0xFF);
        assert_eq!(read_flags(&cpu), (false, false, false, true));
    }

    #[test]
    fn rrc_even_number_no_carry() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x02);
        execute(&mut cpu, &mut bus, MicroOp::Rrc { dst: Reg8::A, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x01);
        assert_eq!(read_flags(&cpu), (false, false, false, false));
    }

    #[test]
    fn rrc_0xaa() {
        // 1010_1010 → 0101_0101, C=0
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0xAA);
        execute(&mut cpu, &mut bus, MicroOp::Rrc { dst: Reg8::A, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x55);
        assert_eq!(read_flags(&cpu), (false, false, false, false));
    }

    // =====================================================================
    //  RL (rotate left through carry)
    // =====================================================================

    #[test]
    fn rl_carry_in_zero_carry_out_zero() {
        let (mut cpu, mut bus) = make_cpu();
        set_flags(&mut cpu, false, false, false, false);
        cpu.register_file.set_8bit(Reg8::A, 0x00);
        execute(&mut cpu, &mut bus, MicroOp::Rl { dst: Reg8::A, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x00);
        assert_eq!(read_flags(&cpu), (true, false, false, false));
    }

    #[test]
    fn rl_carry_in_feeds_bit0() {
        let (mut cpu, mut bus) = make_cpu();
        set_flags(&mut cpu, false, false, false, true);
        cpu.register_file.set_8bit(Reg8::A, 0x00);
        execute(&mut cpu, &mut bus, MicroOp::Rl { dst: Reg8::A, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x01);
        assert_eq!(read_flags(&cpu), (false, false, false, false));
    }

    #[test]
    fn rl_bit7_goes_to_carry() {
        let (mut cpu, mut bus) = make_cpu();
        set_flags(&mut cpu, false, false, false, false);
        cpu.register_file.set_8bit(Reg8::A, 0x80);
        execute(&mut cpu, &mut bus, MicroOp::Rl { dst: Reg8::A, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x00);
        assert_eq!(read_flags(&cpu), (true, false, false, true));
    }

    #[test]
    fn rl_full_chain_carry_in_and_out() {
        let (mut cpu, mut bus) = make_cpu();
        set_flags(&mut cpu, false, false, false, true);
        cpu.register_file.set_8bit(Reg8::A, 0x80);
        execute(&mut cpu, &mut bus, MicroOp::Rl { dst: Reg8::A, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x01);
        assert_eq!(read_flags(&cpu), (false, false, false, true));
    }

    #[test]
    fn rl_double_rotate() {
        let (mut cpu, mut bus) = make_cpu();
        set_flags(&mut cpu, false, false, false, false);
        cpu.register_file.set_8bit(Reg8::A, 0x01);
        execute(&mut cpu, &mut bus, MicroOp::Rl { dst: Reg8::A, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x02);
        execute(&mut cpu, &mut bus, MicroOp::Rl { dst: Reg8::A, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x04);
    }

    // =====================================================================
    //  RR (rotate right through carry)
    // =====================================================================

    #[test]
    fn rr_carry_in_feeds_bit7() {
        let (mut cpu, mut bus) = make_cpu();
        set_flags(&mut cpu, false, false, false, true);
        cpu.register_file.set_8bit(Reg8::A, 0x00);
        execute(&mut cpu, &mut bus, MicroOp::Rr { dst: Reg8::A, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x80);
        assert_eq!(read_flags(&cpu), (false, false, false, false));
    }

    #[test]
    fn rr_bit0_goes_to_carry() {
        let (mut cpu, mut bus) = make_cpu();
        set_flags(&mut cpu, false, false, false, false);
        cpu.register_file.set_8bit(Reg8::A, 0x01);
        execute(&mut cpu, &mut bus, MicroOp::Rr { dst: Reg8::A, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x00);
        assert_eq!(read_flags(&cpu), (true, false, false, true));
    }

    #[test]
    fn rr_full_chain() {
        let (mut cpu, mut bus) = make_cpu();
        set_flags(&mut cpu, false, false, false, true);
        cpu.register_file.set_8bit(Reg8::A, 0x01);
        execute(&mut cpu, &mut bus, MicroOp::Rr { dst: Reg8::A, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x80);
        assert_eq!(read_flags(&cpu), (false, false, false, true));
    }

    #[test]
    fn rr_carry_propagation() {
        // A=0x00, C=1 → A=0x80, C=0 → A=0x40, C=0
        let (mut cpu, mut bus) = make_cpu();
        set_flags(&mut cpu, false, false, false, true);
        cpu.register_file.set_8bit(Reg8::A, 0x00);
        execute(&mut cpu, &mut bus, MicroOp::Rr { dst: Reg8::A, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x80);
        assert!(!read_flags(&cpu).3);
        execute(&mut cpu, &mut bus, MicroOp::Rr { dst: Reg8::A, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x40);
    }

    // =====================================================================
    //  Accumulator rotates (RlcA / RrcA / RlA / RrA)
    // =====================================================================

    #[test]
    fn rlca_basic() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x85); // 1000_0101
        execute(&mut cpu, &mut bus, MicroOp::RlcA);
        // rotate left: bit7 -> carry and bit0
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x0B); // 0000_1011
        let f = cpu.register_file.get_8bit(Reg8::F);
        assert_ne!(f & FLAG_C, 0, "C should be set (old bit7 was 1)");
        assert_eq!(f & FLAG_Z, 0, "Z must always be 0");
        assert_eq!(f & FLAG_N, 0);
        assert_eq!(f & FLAG_H, 0);
    }

    #[test]
    fn rlca_zero_input_still_clears_z() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x00);
        execute(&mut cpu, &mut bus, MicroOp::RlcA);
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x00);
        assert_eq!(cpu.register_file.get_8bit(Reg8::F), 0, "all flags 0 including Z");
    }

    #[test]
    fn rrca_basic() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x01);
        execute(&mut cpu, &mut bus, MicroOp::RrcA);
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x80);
        let f = cpu.register_file.get_8bit(Reg8::F);
        assert_ne!(f & FLAG_C, 0, "C should be set (old bit0 was 1)");
        assert_eq!(f & FLAG_Z, 0, "Z must always be 0");
    }

    #[test]
    fn rrca_zero_input_still_clears_z() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x00);
        execute(&mut cpu, &mut bus, MicroOp::RrcA);
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x00);
        assert_eq!(cpu.register_file.get_8bit(Reg8::F), 0);
    }

    #[test]
    fn rla_through_carry() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x80); // 1000_0000
        cpu.register_file.set_8bit(Reg8::F, FLAG_C); // carry in = 1
        execute(&mut cpu, &mut bus, MicroOp::RlA);
        // shift left, old carry into bit0, bit7 into carry
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x01);
        let f = cpu.register_file.get_8bit(Reg8::F);
        assert_ne!(f & FLAG_C, 0, "C should be set (old bit7 was 1)");
        assert_eq!(f & FLAG_Z, 0, "Z must always be 0");
    }

    #[test]
    fn rla_zero_input_carry_clear() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x00);
        cpu.register_file.set_8bit(Reg8::F, 0);
        execute(&mut cpu, &mut bus, MicroOp::RlA);
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x00);
        assert_eq!(cpu.register_file.get_8bit(Reg8::F), 0, "all flags 0 including Z");
    }

    #[test]
    fn rra_through_carry() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x01); // 0000_0001
        cpu.register_file.set_8bit(Reg8::F, FLAG_C); // carry in = 1
        execute(&mut cpu, &mut bus, MicroOp::RrA);
        // shift right, old carry into bit7, bit0 into carry
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x80);
        let f = cpu.register_file.get_8bit(Reg8::F);
        assert_ne!(f & FLAG_C, 0, "C should be set (old bit0 was 1)");
        assert_eq!(f & FLAG_Z, 0, "Z must always be 0");
    }

    #[test]
    fn rra_zero_input_carry_clear() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x00);
        cpu.register_file.set_8bit(Reg8::F, 0);
        execute(&mut cpu, &mut bus, MicroOp::RrA);
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x00);
        assert_eq!(cpu.register_file.get_8bit(Reg8::F), 0);
    }

    // =====================================================================
    //  SLA
    // =====================================================================

    #[test]
    fn sla_zero() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x00);
        execute(&mut cpu, &mut bus, MicroOp::Sla { dst: Reg8::A, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x00);
        assert_eq!(read_flags(&cpu), (true, false, false, false));
    }

    #[test]
    fn sla_bit7_to_carry_result_zero() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x80);
        execute(&mut cpu, &mut bus, MicroOp::Sla { dst: Reg8::A, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x00);
        assert_eq!(read_flags(&cpu), (true, false, false, true));
    }

    #[test]
    fn sla_0xff() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0xFF);
        execute(&mut cpu, &mut bus, MicroOp::Sla { dst: Reg8::A, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0xFE);
        assert_eq!(read_flags(&cpu), (false, false, false, true));
    }

    #[test]
    fn sla_no_carry() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x01);
        execute(&mut cpu, &mut bus, MicroOp::Sla { dst: Reg8::A, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x02);
        assert_eq!(read_flags(&cpu), (false, false, false, false));
    }

    // =====================================================================
    //  SRA (arithmetic shift right — preserves sign bit)
    // =====================================================================

    #[test]
    fn sra_preserves_bit7_when_set() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x80);
        execute(&mut cpu, &mut bus, MicroOp::Sra { dst: Reg8::A, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0xC0);
        assert_eq!(read_flags(&cpu), (false, false, false, false));
    }

    #[test]
    fn sra_preserves_bit7_when_clear() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x7F);
        execute(&mut cpu, &mut bus, MicroOp::Sra { dst: Reg8::A, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x3F);
        assert_eq!(read_flags(&cpu), (false, false, false, true));
    }

    #[test]
    fn sra_0xff_stays_0xff() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0xFF);
        execute(&mut cpu, &mut bus, MicroOp::Sra { dst: Reg8::A, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0xFF);
        assert_eq!(read_flags(&cpu), (false, false, false, true));
    }

    #[test]
    fn sra_zero() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x00);
        execute(&mut cpu, &mut bus, MicroOp::Sra { dst: Reg8::A, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x00);
        assert_eq!(read_flags(&cpu), (true, false, false, false));
    }

    #[test]
    fn sra_negative_odd() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x81); // -127
        execute(&mut cpu, &mut bus, MicroOp::Sra { dst: Reg8::A, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0xC0); // -64
        assert_eq!(read_flags(&cpu), (false, false, false, true));
    }

    // =====================================================================
    //  SRL (logical shift right — bit7 always 0)
    // =====================================================================

    #[test]
    fn srl_clears_bit7() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x80);
        execute(&mut cpu, &mut bus, MicroOp::Srl { dst: Reg8::A, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x40);
        assert_eq!(read_flags(&cpu), (false, false, false, false));
    }

    #[test]
    fn srl_0x01_becomes_zero_carry_set() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x01);
        execute(&mut cpu, &mut bus, MicroOp::Srl { dst: Reg8::A, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x00);
        assert_eq!(read_flags(&cpu), (true, false, false, true));
    }

    #[test]
    fn srl_0xff() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0xFF);
        execute(&mut cpu, &mut bus, MicroOp::Srl { dst: Reg8::A, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x7F);
        assert_eq!(read_flags(&cpu), (false, false, false, true));
    }

    // =====================================================================
    //  SWAP
    // =====================================================================

    #[test]
    fn swap_zero() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x00);
        execute(&mut cpu, &mut bus, MicroOp::Swap { dst: Reg8::A, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x00);
        assert_eq!(read_flags(&cpu), (true, false, false, false));
    }

    #[test]
    fn swap_0xf0() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0xF0);
        execute(&mut cpu, &mut bus, MicroOp::Swap { dst: Reg8::A, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x0F);
        assert_eq!(read_flags(&cpu), (false, false, false, false));
    }

    #[test]
    fn swap_0xa5() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0xA5);
        execute(&mut cpu, &mut bus, MicroOp::Swap { dst: Reg8::A, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x5A);
    }

    #[test]
    fn swap_0xff_not_zero() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0xFF);
        execute(&mut cpu, &mut bus, MicroOp::Swap { dst: Reg8::A, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0xFF);
        assert_eq!(read_flags(&cpu), (false, false, false, false));
    }

    #[test]
    fn swap_clears_all_other_flags() {
        let (mut cpu, mut bus) = make_cpu();
        set_flags(&mut cpu, true, true, true, true);
        cpu.register_file.set_8bit(Reg8::A, 0x12);
        execute(&mut cpu, &mut bus, MicroOp::Swap { dst: Reg8::A, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x21);
        assert_eq!(read_flags(&cpu), (false, false, false, false));
    }

    #[test]
    fn swap_is_own_inverse() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x3C);
        execute(&mut cpu, &mut bus, MicroOp::Swap { dst: Reg8::A, src: Reg8::A });
        execute(&mut cpu, &mut bus, MicroOp::Swap { dst: Reg8::A, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x3C);
    }

    // =====================================================================
    //  BIT test
    // =====================================================================

    #[test]
    fn bit_test_set_bit_clears_z() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x80);
        execute(&mut cpu, &mut bus, MicroOp::BitTest { bit: 7, reg: Reg8::A });
        let (z, n, h, _) = read_flags(&cpu);
        assert!(!z);
        assert!(!n);
        assert!(h);
    }

    #[test]
    fn bit_test_clear_bit_sets_z() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x00);
        execute(&mut cpu, &mut bus, MicroOp::BitTest { bit: 3, reg: Reg8::A });
        let (z, _, h, _) = read_flags(&cpu);
        assert!(z);
        assert!(h);
    }

    #[test]
    fn bit_test_preserves_carry_when_set() {
        let (mut cpu, mut bus) = make_cpu();
        set_flags(&mut cpu, false, false, false, true);
        cpu.register_file.set_8bit(Reg8::A, 0x00);
        execute(&mut cpu, &mut bus, MicroOp::BitTest { bit: 0, reg: Reg8::A });
        assert_eq!(read_flags(&cpu), (true, false, true, true));
    }

    #[test]
    fn bit_test_preserves_carry_when_clear() {
        let (mut cpu, mut bus) = make_cpu();
        set_flags(&mut cpu, false, false, false, false);
        cpu.register_file.set_8bit(Reg8::A, 0xFF);
        execute(&mut cpu, &mut bus, MicroOp::BitTest { bit: 4, reg: Reg8::A });
        assert_eq!(read_flags(&cpu), (false, false, true, false));
    }

    #[test]
    fn bit_test_every_position() {
        for bit in 0..8u8 {
            let (mut cpu, mut bus) = make_cpu();
            cpu.register_file.set_8bit(Reg8::B, 1 << bit);
            execute(&mut cpu, &mut bus, MicroOp::BitTest { bit, reg: Reg8::B });
            assert!(!read_flags(&cpu).0, "bit {} should be set", bit);

            for other in 0..8u8 {
                if other == bit { continue; }
                let (mut cpu2, mut bus2) = make_cpu();
                cpu2.register_file.set_8bit(Reg8::B, 1 << bit);
                execute(&mut cpu2, &mut bus2, MicroOp::BitTest { bit: other, reg: Reg8::B });
                assert!(read_flags(&cpu2).0, "bit {} should be clear when only bit {} set", other, bit);
            }
        }
    }

    // =====================================================================
    //  SET / RES bit
    // =====================================================================

    #[test]
    fn set_bit_on_zero() {
        for bit in 0..8u8 {
            let (mut cpu, mut bus) = make_cpu();
            cpu.register_file.set_8bit(Reg8::A, 0x00);
            execute(&mut cpu, &mut bus, MicroOp::SetBit { bit, reg: Reg8::A });
            assert_eq!(cpu.register_file.get_8bit(Reg8::A), 1 << bit);
        }
    }

    #[test]
    fn set_bit_idempotent() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0xFF);
        execute(&mut cpu, &mut bus, MicroOp::SetBit { bit: 3, reg: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0xFF);
    }

    #[test]
    fn set_bit_does_not_change_flags() {
        let (mut cpu, mut bus) = make_cpu();
        set_flags(&mut cpu, true, true, true, true);
        cpu.register_file.set_8bit(Reg8::B, 0x00);
        execute(&mut cpu, &mut bus, MicroOp::SetBit { bit: 5, reg: Reg8::B });
        assert_eq!(read_flags(&cpu), (true, true, true, true));
    }

    #[test]
    fn reset_bit_clears_each() {
        for bit in 0..8u8 {
            let (mut cpu, mut bus) = make_cpu();
            cpu.register_file.set_8bit(Reg8::A, 0xFF);
            execute(&mut cpu, &mut bus, MicroOp::ResetBit { bit, reg: Reg8::A });
            assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0xFF & !(1 << bit));
        }
    }

    #[test]
    fn reset_bit_idempotent() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x00);
        execute(&mut cpu, &mut bus, MicroOp::ResetBit { bit: 7, reg: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x00);
    }

    #[test]
    fn reset_bit_does_not_change_flags() {
        let (mut cpu, mut bus) = make_cpu();
        set_flags(&mut cpu, false, true, false, true);
        cpu.register_file.set_8bit(Reg8::B, 0xFF);
        execute(&mut cpu, &mut bus, MicroOp::ResetBit { bit: 2, reg: Reg8::B });
        assert_eq!(read_flags(&cpu), (false, true, false, true));
    }

    // =====================================================================
    //  Rotate/shift: N and H always cleared
    // =====================================================================

    #[test]
    fn all_rotates_and_shifts_clear_n_and_h() {
        let ops: Vec<(&str, MicroOp)> = vec![
            ("RLC",  MicroOp::Rlc  { dst: Reg8::A, src: Reg8::A }),
            ("RRC",  MicroOp::Rrc  { dst: Reg8::A, src: Reg8::A }),
            ("RL",   MicroOp::Rl   { dst: Reg8::A, src: Reg8::A }),
            ("RR",   MicroOp::Rr   { dst: Reg8::A, src: Reg8::A }),
            ("SLA",  MicroOp::Sla  { dst: Reg8::A, src: Reg8::A }),
            ("SRA",  MicroOp::Sra  { dst: Reg8::A, src: Reg8::A }),
            ("SRL",  MicroOp::Srl  { dst: Reg8::A, src: Reg8::A }),
        ];
        for (name, op) in ops {
            let (mut cpu, mut bus) = make_cpu();
            set_flags(&mut cpu, true, true, true, true);
            cpu.register_file.set_8bit(Reg8::A, 0x42);
            execute(&mut cpu, &mut bus, op);
            let (_, n, h, _) = read_flags(&cpu);
            assert!(!n, "{} must clear N", name);
            assert!(!h, "{} must clear H", name);
        }
    }

    // =====================================================================
    //  CheckCond — sets condition_taken flag
    // =====================================================================

    #[test]
    fn checkcond_nz_passes_when_z_clear() {
        let (mut cpu, mut bus) = make_cpu();
        set_flags(&mut cpu, false, false, false, false);
        cpu.condition_taken = true;
        execute(&mut cpu, &mut bus, MicroOp::CheckCond { cond: Condition::NZ });
        assert!(cpu.condition_taken, "NZ should pass when Z is clear");
    }

    #[test]
    fn checkcond_nz_fails_when_z_set() {
        let (mut cpu, mut bus) = make_cpu();
        set_flags(&mut cpu, true, false, false, false);
        cpu.condition_taken = true;
        execute(&mut cpu, &mut bus, MicroOp::CheckCond { cond: Condition::NZ });
        assert!(!cpu.condition_taken, "NZ should fail when Z is set");
    }

    #[test]
    fn checkcond_z_passes_when_z_set() {
        let (mut cpu, mut bus) = make_cpu();
        set_flags(&mut cpu, true, false, false, false);
        cpu.condition_taken = true;
        execute(&mut cpu, &mut bus, MicroOp::CheckCond { cond: Condition::Z });
        assert!(cpu.condition_taken, "Z should pass when Z is set");
    }

    #[test]
    fn checkcond_z_fails_when_z_clear() {
        let (mut cpu, mut bus) = make_cpu();
        set_flags(&mut cpu, false, false, false, false);
        cpu.condition_taken = true;
        execute(&mut cpu, &mut bus, MicroOp::CheckCond { cond: Condition::Z });
        assert!(!cpu.condition_taken, "Z should fail when Z is clear");
    }

    #[test]
    fn checkcond_nc_passes_when_c_clear() {
        let (mut cpu, mut bus) = make_cpu();
        set_flags(&mut cpu, false, false, false, false);
        cpu.condition_taken = true;
        execute(&mut cpu, &mut bus, MicroOp::CheckCond { cond: Condition::NC });
        assert!(cpu.condition_taken);
    }

    #[test]
    fn checkcond_nc_fails_when_c_set() {
        let (mut cpu, mut bus) = make_cpu();
        set_flags(&mut cpu, false, false, false, true);
        cpu.condition_taken = true;
        execute(&mut cpu, &mut bus, MicroOp::CheckCond { cond: Condition::NC });
        assert!(!cpu.condition_taken);
    }

    #[test]
    fn checkcond_c_passes_when_c_set() {
        let (mut cpu, mut bus) = make_cpu();
        set_flags(&mut cpu, false, false, false, true);
        cpu.condition_taken = true;
        execute(&mut cpu, &mut bus, MicroOp::CheckCond { cond: Condition::C });
        assert!(cpu.condition_taken);
    }

    #[test]
    fn checkcond_c_fails_when_c_clear() {
        let (mut cpu, mut bus) = make_cpu();
        set_flags(&mut cpu, false, false, false, false);
        cpu.condition_taken = true;
        execute(&mut cpu, &mut bus, MicroOp::CheckCond { cond: Condition::C });
        assert!(!cpu.condition_taken);
    }

    #[test]
    fn checkcond_ignores_irrelevant_flags() {
        // NZ should not care about C
        let (mut cpu, mut bus) = make_cpu();
        set_flags(&mut cpu, false, false, false, true); // Z=0, C=1
        cpu.condition_taken = true;
        execute(&mut cpu, &mut bus, MicroOp::CheckCond { cond: Condition::NZ });
        assert!(cpu.condition_taken, "NZ should pass regardless of C");

        // C should not care about Z
        let (mut cpu2, mut bus2) = make_cpu();
        set_flags(&mut cpu2, true, false, false, true); // Z=1, C=1
        cpu2.condition_taken = true;
        execute(&mut cpu2, &mut bus2, MicroOp::CheckCond { cond: Condition::C });
        assert!(cpu2.condition_taken, "C should pass regardless of Z");
    }

    #[test]
    fn checkcond_pass_does_not_alter_pc() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::PC, 0x5000);
        set_flags(&mut cpu, false, false, false, false); // Z=0 → NZ passes
        cpu.condition_taken = true;
        execute(&mut cpu, &mut bus, MicroOp::CheckCond { cond: Condition::NZ });
        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0x5000,
            "passed condition must not touch PC");
        assert!(cpu.condition_taken);
    }

    // =====================================================================
    //  JumpAbs
    // =====================================================================

    #[test]
    fn jump_abs_sets_pc() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::PC, 0x1234);
        execute(&mut cpu, &mut bus, MicroOp::JumpAbs { addr: 0xABCD });
        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0xABCD);
    }

    #[test]
    fn jump_abs_to_zero() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::PC, 0xFFFF);
        execute(&mut cpu, &mut bus, MicroOp::JumpAbs { addr: 0x0000 });
        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0x0000);
    }

    #[test]
    fn jump_abs_to_max() {
        let (mut cpu, mut bus) = make_cpu();
        execute(&mut cpu, &mut bus, MicroOp::JumpAbs { addr: 0xFFFF });
        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0xFFFF);
    }

    // =====================================================================
    //  JumpRel
    // =====================================================================

    #[test]
    fn jump_rel_positive() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::PC, 0x0100);
        execute(&mut cpu, &mut bus, MicroOp::JumpRel { offset: 10 });
        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0x010A);
    }

    #[test]
    fn jump_rel_negative() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::PC, 0x0100);
        execute(&mut cpu, &mut bus, MicroOp::JumpRel { offset: -5 });
        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0x00FB);
    }

    #[test]
    fn jump_rel_zero() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::PC, 0x0200);
        execute(&mut cpu, &mut bus, MicroOp::JumpRel { offset: 0 });
        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0x0200);
    }

    #[test]
    fn jump_rel_wraps_forward() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::PC, 0xFFFF);
        execute(&mut cpu, &mut bus, MicroOp::JumpRel { offset: 1 });
        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0x0000);
    }

    #[test]
    fn jump_rel_wraps_backward() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::PC, 0x0000);
        execute(&mut cpu, &mut bus, MicroOp::JumpRel { offset: -1 });
        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0xFFFF);
    }

    #[test]
    fn jump_rel_max_positive() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::PC, 0x0000);
        execute(&mut cpu, &mut bus, MicroOp::JumpRel { offset: 127 });
        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0x007F);
    }

    #[test]
    fn jump_rel_max_negative() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::PC, 0x0100);
        execute(&mut cpu, &mut bus, MicroOp::JumpRel { offset: -128 });
        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0x0080);
    }

    // =====================================================================
    //  Call / Ret
    // =====================================================================

    #[test]
    fn call_pushes_pc_and_jumps() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::SP, 0xFFFE);
        cpu.register_file.set_16bit(Reg16::PC, 0x1234);
        execute(&mut cpu, &mut bus, MicroOp::Call { addr: 0xABCD });

        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0xABCD);
        assert_eq!(cpu.register_file.get_16bit(Reg16::SP), 0xFFFC);
        let lo = bus.read(0xFFFC);
        let hi = bus.read(0xFFFD);
        assert_eq!(u16::from_le_bytes([lo, hi]), 0x1234);
    }

    #[test]
    fn ret_pops_pc() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::SP, 0xFFFC);
        bus.write(0xFFFC, 0xEF);
        bus.write(0xFFFD, 0xBE);
        execute(&mut cpu, &mut bus, MicroOp::Ret);
        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0xBEEF);
        assert_eq!(cpu.register_file.get_16bit(Reg16::SP), 0xFFFE);
    }

    #[test]
    fn call_then_ret_round_trip() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::SP, 0xFFFE);
        cpu.register_file.set_16bit(Reg16::PC, 0x4567);

        execute(&mut cpu, &mut bus, MicroOp::Call { addr: 0x1000 });
        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0x1000);

        execute(&mut cpu, &mut bus, MicroOp::Ret);
        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0x4567);
        assert_eq!(cpu.register_file.get_16bit(Reg16::SP), 0xFFFE);
    }

    #[test]
    fn nested_calls_and_rets() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::SP, 0xFFFE);
        cpu.register_file.set_16bit(Reg16::PC, 0x0100);

        execute(&mut cpu, &mut bus, MicroOp::Call { addr: 0x0200 });
        assert_eq!(cpu.register_file.get_16bit(Reg16::SP), 0xFFFC);

        execute(&mut cpu, &mut bus, MicroOp::Call { addr: 0x0300 });
        assert_eq!(cpu.register_file.get_16bit(Reg16::SP), 0xFFFA);

        execute(&mut cpu, &mut bus, MicroOp::Ret);
        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0x0200);

        execute(&mut cpu, &mut bus, MicroOp::Ret);
        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0x0100);
        assert_eq!(cpu.register_file.get_16bit(Reg16::SP), 0xFFFE);
    }

    #[test]
    fn call_with_pc_at_zero() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::SP, 0xFFFE);
        cpu.register_file.set_16bit(Reg16::PC, 0x0000);
        execute(&mut cpu, &mut bus, MicroOp::Call { addr: 0x0150 });

        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0x0150);
        let sp = cpu.register_file.get_16bit(Reg16::SP);
        let lo = bus.read(sp);
        let hi = bus.read(sp + 1);
        assert_eq!(u16::from_le_bytes([lo, hi]), 0x0000);
    }

    #[test]
    fn call_with_pc_at_ffff() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::SP, 0xFFFE);
        cpu.register_file.set_16bit(Reg16::PC, 0xFFFF);
        execute(&mut cpu, &mut bus, MicroOp::Call { addr: 0x0040 });

        let sp = cpu.register_file.get_16bit(Reg16::SP);
        let lo = bus.read(sp);
        let hi = bus.read(sp + 1);
        assert_eq!(u16::from_le_bytes([lo, hi]), 0xFFFF);
    }

    // =====================================================================
    //  RetI
    // =====================================================================

    #[test]
    fn reti_pops_pc_and_enables_ime() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.ime = false;
        cpu.register_file.set_16bit(Reg16::SP, 0xFFFC);
        bus.write(0xFFFC, 0xCD);
        bus.write(0xFFFD, 0xAB);

        execute(&mut cpu, &mut bus, MicroOp::RetI);
        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0xABCD);
        assert_eq!(cpu.register_file.get_16bit(Reg16::SP), 0xFFFE);
        assert!(cpu.ime);
    }

    #[test]
    fn reti_when_ime_already_set() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.ime = true;
        cpu.register_file.set_16bit(Reg16::SP, 0xFFFC);
        bus.write(0xFFFC, 0x00);
        bus.write(0xFFFD, 0x01);

        execute(&mut cpu, &mut bus, MicroOp::RetI);
        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0x0100);
        assert!(cpu.ime);
    }

    // =====================================================================
    //  Rst
    // =====================================================================

    #[test]
    fn rst_all_vectors() {
        let vectors: [u16; 8] = [0x00, 0x08, 0x10, 0x18, 0x20, 0x28, 0x30, 0x38];
        for &vec in &vectors {
            let (mut cpu, mut bus) = make_cpu();
            cpu.register_file.set_16bit(Reg16::SP, 0xFFFE);
            cpu.register_file.set_16bit(Reg16::PC, 0x5678);

            execute(&mut cpu, &mut bus, MicroOp::Rst { addr: vec });

            assert_eq!(cpu.register_file.get_16bit(Reg16::PC), vec,
                "RST {:#04X} failed", vec);
            let sp = cpu.register_file.get_16bit(Reg16::SP);
            let lo = bus.read(sp);
            let hi = bus.read(sp + 1);
            assert_eq!(u16::from_le_bytes([lo, hi]), 0x5678);
        }
    }

    // =====================================================================
    //  SetFlag
    // =====================================================================

    #[test]
    fn set_flag_each_individually() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::F, 0x00);
        execute(&mut cpu, &mut bus, MicroOp::SetFlag { flag: Flag::Zero, value: true });
        execute(&mut cpu, &mut bus, MicroOp::SetFlag { flag: Flag::Subtract, value: true });
        execute(&mut cpu, &mut bus, MicroOp::SetFlag { flag: Flag::HalfCarry, value: true });
        execute(&mut cpu, &mut bus, MicroOp::SetFlag { flag: Flag::Carry, value: true });
        assert_eq!(cpu.register_file.get_8bit(Reg8::F), FLAG_Z | FLAG_N | FLAG_H | FLAG_C);
    }

    #[test]
    fn set_flag_clear_preserves_others() {
        let (mut cpu, mut bus) = make_cpu();
        set_flags(&mut cpu, true, true, true, true);
        execute(&mut cpu, &mut bus, MicroOp::SetFlag { flag: Flag::Carry, value: false });
        let f = cpu.register_file.get_8bit(Reg8::F);
        assert!(f & FLAG_Z != 0);
        assert!(f & FLAG_N != 0);
        assert!(f & FLAG_H != 0);
        assert!(f & FLAG_C == 0);
    }

    // =====================================================================
    //  SetIme / DeferImeEnable
    // =====================================================================

    #[test]
    fn set_ime_on_off() {
        let (mut cpu, mut bus) = make_cpu();
        assert!(!cpu.ime);
        execute(&mut cpu, &mut bus, MicroOp::SetIme { value: true });
        assert!(cpu.ime);
        execute(&mut cpu, &mut bus, MicroOp::SetIme { value: false });
        assert!(!cpu.ime);
    }

    #[test]
    fn defer_ime_sets_flag_not_ime() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.ime = false;
        cpu.ime_defer = false;
        execute(&mut cpu, &mut bus, MicroOp::DeferImeEnable);
        assert!(cpu.ime_defer);
        assert!(!cpu.ime);
    }

    // =====================================================================
    //  TriggerHalt / TriggerStop
    // =====================================================================

    #[test]
    fn trigger_halt() {
        let (mut cpu, mut bus) = make_cpu();
        execute(&mut cpu, &mut bus, MicroOp::TriggerHalt);
        assert!(cpu.halted);
    }

    #[test]
    fn trigger_stop() {
        let (mut cpu, mut bus) = make_cpu();
        execute(&mut cpu, &mut bus, MicroOp::TriggerStop);
        assert!(cpu.halted);
    }

    // =====================================================================
    //  Interrupts (tested via cpu.tick())
    // =====================================================================

    #[test]
    fn interrupts_noop_when_ime_off() {
        use crate::cpu::CPU;
        let mut cpu = CPU::new(crate::trace::Tracer::off());
        let mut bus = Bus::new();
        cpu.ime = false;
        bus.write(0xFFFF, 0x01); // IE=1 (memory-mapped)
        bus.write(0xFF0F, 0x01);
        // Put a NOP at PC=0 so tick has something to execute
        bus.write(0x0000, 0x00);
        let old_pc = cpu.register_file.get_16bit(Reg16::PC);
        cpu.tick(&mut bus);
        // Should have executed NOP, not dispatched interrupt
        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), old_pc + 1);
    }

    #[test]
    fn interrupts_noop_when_none_pending() {
        use crate::cpu::CPU;
        let mut cpu = CPU::new(crate::trace::Tracer::off());
        let mut bus = Bus::new();
        cpu.ime = true;
        bus.write(0xFFFF, 0x1F); // IE=all (memory-mapped)
        bus.write(0xFF0F, 0x00);
        cpu.register_file.set_16bit(Reg16::SP, 0xFFFE);
        bus.write(0x0000, 0x00); // NOP
        cpu.tick(&mut bus);
        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0x0001);
        assert!(cpu.ime);
    }

    #[test]
    fn interrupts_services_vblank() {
        use crate::cpu::CPU;
        let mut cpu = CPU::new(crate::trace::Tracer::off());
        let mut bus = Bus::new();
        cpu.ime = true;
        bus.write(0xFFFF, 0x01); // IE=VBlank (memory-mapped)
        bus.write(0xFF0F, 0x01);
        cpu.register_file.set_16bit(Reg16::SP, 0xFFFE);
        cpu.register_file.set_16bit(Reg16::PC, 0x1234);

        let t = cpu.tick(&mut bus);

        assert_eq!(t, 20, "interrupt dispatch should take 20 T-cycles");
        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0x0040);
        assert!(!cpu.ime);
        assert_eq!(bus.read(0xFF0F), 0x00);
        let sp = cpu.register_file.get_16bit(Reg16::SP);
        let lo = bus.read(sp);
        let hi = bus.read(sp + 1);
        assert_eq!(u16::from_le_bytes([lo, hi]), 0x1234);
    }

    #[test]
    fn interrupts_services_timer() {
        use crate::cpu::CPU;
        let mut cpu = CPU::new(crate::trace::Tracer::off());
        let mut bus = Bus::new();
        cpu.ime = true;
        bus.write(0xFFFF, 0x04); // IE=Timer (memory-mapped)
        bus.write(0xFF0F, 0x04);
        cpu.register_file.set_16bit(Reg16::SP, 0xFFFE);

        cpu.tick(&mut bus);
        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0x0050);
        assert_eq!(bus.read(0xFF0F), 0x00);
    }

    #[test]
    fn interrupts_priority_lowest_bit_wins() {
        use crate::cpu::CPU;
        let mut cpu = CPU::new(crate::trace::Tracer::off());
        let mut bus = Bus::new();
        cpu.ime = true;
        bus.write(0xFFFF, 0x05); // IE=VBlank+Timer (memory-mapped)
        bus.write(0xFF0F, 0x05);
        cpu.register_file.set_16bit(Reg16::SP, 0xFFFE);

        cpu.tick(&mut bus);
        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0x0040); // VBlank wins
        assert_eq!(bus.read(0xFF0F), 0x04); // Timer still pending
    }

    #[test]
    fn interrupts_all_five_vectors() {
        use crate::cpu::CPU;
        let cases: [(u8, u16); 5] = [
            (0x01, 0x0040), // VBlank
            (0x02, 0x0048), // LCDStat
            (0x04, 0x0050), // Timer
            (0x08, 0x0058), // Serial
            (0x10, 0x0060), // Joypad
        ];
        for (mask, vector) in cases {
            let mut cpu = CPU::new(crate::trace::Tracer::off());
            let mut bus = Bus::new();
            cpu.ime = true;
            bus.write(0xFFFF, mask); // IE (memory-mapped)
            bus.write(0xFF0F, mask);
            cpu.register_file.set_16bit(Reg16::SP, 0xFFFE);

            cpu.tick(&mut bus);
            assert_eq!(
                cpu.register_file.get_16bit(Reg16::PC), vector,
                "Interrupt {:#04X} → {:#06X}", mask, vector
            );
        }
    }

    #[test]
    fn interrupts_clears_only_serviced_bit() {
        use crate::cpu::CPU;
        let mut cpu = CPU::new(crate::trace::Tracer::off());
        let mut bus = Bus::new();
        cpu.ime = true;
        bus.write(0xFFFF, 0x1F); // IE=all (memory-mapped)
        bus.write(0xFF0F, 0x1F);
        cpu.register_file.set_16bit(Reg16::SP, 0xFFFE);

        cpu.tick(&mut bus);
        assert_eq!(bus.read(0xFF0F), 0x1E); // only bit 0 cleared
    }

    #[test]
    fn interrupts_ignores_upper_bits_of_if() {
        use crate::cpu::CPU;
        let mut cpu = CPU::new(crate::trace::Tracer::off());
        let mut bus = Bus::new();
        cpu.ime = true;
        bus.write(0xFFFF, 0x00); // IE=none (memory-mapped, also the default)
        bus.write(0xFF0F, 0xE0);     // only upper bits set
        cpu.register_file.set_16bit(Reg16::SP, 0xFFFE);
        bus.write(0x0000, 0x00); // NOP
        cpu.tick(&mut bus);
        // No interrupt dispatched, NOP executed
        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0x0001);
    }

    // =====================================================================
    //  LoadReg8
    // =====================================================================

    #[test]
    fn load_reg8_basic() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x42);
        execute(&mut cpu, &mut bus, MicroOp::LoadReg8 { dst: Reg8::B, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::B), 0x42);
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x42); // src unchanged
    }

    #[test]
    fn load_reg8_self_copy() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0xAB);
        execute(&mut cpu, &mut bus, MicroOp::LoadReg8 { dst: Reg8::A, src: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0xAB);
    }

    #[test]
    fn load_reg8_zero() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::B, 0xFF);
        cpu.register_file.set_8bit(Reg8::C, 0x00);
        execute(&mut cpu, &mut bus, MicroOp::LoadReg8 { dst: Reg8::B, src: Reg8::C });
        assert_eq!(cpu.register_file.get_8bit(Reg8::B), 0x00);
    }

    #[test]
    fn load_reg8_all_registers() {
        let regs = [Reg8::A, Reg8::B, Reg8::C, Reg8::D, Reg8::E, Reg8::H, Reg8::L];
        for (i, &src) in regs.iter().enumerate() {
            for &dst in &regs {
                let (mut cpu, mut bus) = make_cpu();
                let val = (i as u8).wrapping_mul(0x11).wrapping_add(0x10);
                cpu.register_file.set_8bit(src, val);
                execute(&mut cpu, &mut bus, MicroOp::LoadReg8 { dst, src });
                assert_eq!(cpu.register_file.get_8bit(dst), val);
            }
        }
    }

    // =====================================================================
    //  LoadReg16
    // =====================================================================

    #[test]
    fn load_reg16_basic() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::HL, 0xBEEF);
        execute(&mut cpu, &mut bus, MicroOp::LoadReg16 { dst: Reg16::SP, src: Reg16::HL });
        assert_eq!(cpu.register_file.get_16bit(Reg16::SP), 0xBEEF);
    }

    #[test]
    fn load_reg16_zero() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::SP, 0xFFFE);
        cpu.register_file.set_16bit(Reg16::BC, 0x0000);
        execute(&mut cpu, &mut bus, MicroOp::LoadReg16 { dst: Reg16::SP, src: Reg16::BC });
        assert_eq!(cpu.register_file.get_16bit(Reg16::SP), 0x0000);
    }

    #[test]
    fn load_reg16_ffff() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::DE, 0xFFFF);
        execute(&mut cpu, &mut bus, MicroOp::LoadReg16 { dst: Reg16::HL, src: Reg16::DE });
        assert_eq!(cpu.register_file.get_16bit(Reg16::HL), 0xFFFF);
    }

    // =====================================================================
    //  ReadImmediate8
    // =====================================================================

    #[test]
    fn read_immediate8_basic() {
        let (mut cpu, mut bus) = make_cpu();
        // Place 0x42 at address 0x0100 in ROM
        bus.write(0x8000, 0x42); // use VRAM since ROM is read-only
        cpu.register_file.set_16bit(Reg16::PC, 0x8000);
        execute(&mut cpu, &mut bus, MicroOp::ReadImmediate8 { into: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x42);
        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0x8001); // PC advanced
    }

    #[test]
    fn read_immediate8_zero() {
        let (mut cpu, mut bus) = make_cpu();
        bus.write(0x8000, 0x00);
        cpu.register_file.set_16bit(Reg16::PC, 0x8000);
        cpu.register_file.set_8bit(Reg8::B, 0xFF);
        execute(&mut cpu, &mut bus, MicroOp::ReadImmediate8 { into: Reg8::B });
        assert_eq!(cpu.register_file.get_8bit(Reg8::B), 0x00);
    }

    #[test]
    fn read_immediate8_ff() {
        let (mut cpu, mut bus) = make_cpu();
        bus.write(0x8000, 0xFF);
        cpu.register_file.set_16bit(Reg16::PC, 0x8000);
        execute(&mut cpu, &mut bus, MicroOp::ReadImmediate8 { into: Reg8::C });
        assert_eq!(cpu.register_file.get_8bit(Reg8::C), 0xFF);
    }

    #[test]
    fn read_immediate8_consecutive() {
        let (mut cpu, mut bus) = make_cpu();
        bus.write(0x8000, 0xAA);
        bus.write(0x8001, 0xBB);
        cpu.register_file.set_16bit(Reg16::PC, 0x8000);
        execute(&mut cpu, &mut bus, MicroOp::ReadImmediate8 { into: Reg8::D });
        execute(&mut cpu, &mut bus, MicroOp::ReadImmediate8 { into: Reg8::E });
        assert_eq!(cpu.register_file.get_8bit(Reg8::D), 0xAA);
        assert_eq!(cpu.register_file.get_8bit(Reg8::E), 0xBB);
        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0x8002);
    }

    // =====================================================================
    //  ReadImmediate16
    // =====================================================================

    #[test]
    fn read_immediate16_little_endian() {
        let (mut cpu, mut bus) = make_cpu();
        bus.write(0x8000, 0xEF); // lo
        bus.write(0x8001, 0xBE); // hi
        cpu.register_file.set_16bit(Reg16::PC, 0x8000);
        execute(&mut cpu, &mut bus, MicroOp::ReadImmediate16 { into: Reg16::HL });
        assert_eq!(cpu.register_file.get_16bit(Reg16::HL), 0xBEEF);
        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0x8002);
    }

    #[test]
    fn read_immediate16_zero() {
        let (mut cpu, mut bus) = make_cpu();
        bus.write(0x8000, 0x00);
        bus.write(0x8001, 0x00);
        cpu.register_file.set_16bit(Reg16::PC, 0x8000);
        cpu.register_file.set_16bit(Reg16::SP, 0xFFFF);
        execute(&mut cpu, &mut bus, MicroOp::ReadImmediate16 { into: Reg16::SP });
        assert_eq!(cpu.register_file.get_16bit(Reg16::SP), 0x0000);
    }

    #[test]
    fn read_immediate16_ffff() {
        let (mut cpu, mut bus) = make_cpu();
        bus.write(0x8000, 0xFF);
        bus.write(0x8001, 0xFF);
        cpu.register_file.set_16bit(Reg16::PC, 0x8000);
        execute(&mut cpu, &mut bus, MicroOp::ReadImmediate16 { into: Reg16::BC });
        assert_eq!(cpu.register_file.get_16bit(Reg16::BC), 0xFFFF);
    }

    // =====================================================================
    //  ReadMemReg8 / WriteMemReg8
    // =====================================================================

    #[test]
    fn write_then_read_mem_reg8() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::HL, 0xC000); // WRAM
        cpu.register_file.set_8bit(Reg8::A, 0x55);
        execute(&mut cpu, &mut bus, MicroOp::WriteMemReg8 { addr_reg: Reg16::HL, src: Reg8::A });

        cpu.register_file.set_8bit(Reg8::B, 0x00);
        execute(&mut cpu, &mut bus, MicroOp::ReadMemReg8 { addr_reg: Reg16::HL, into: Reg8::B });
        assert_eq!(cpu.register_file.get_8bit(Reg8::B), 0x55);
    }

    #[test]
    fn read_mem_reg8_zero() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::BC, 0xC000);
        // WRAM is initialized to 0
        execute(&mut cpu, &mut bus, MicroOp::ReadMemReg8 { addr_reg: Reg16::BC, into: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x00);
    }

    #[test]
    fn write_mem_reg8_ff() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::DE, 0xC100);
        cpu.register_file.set_8bit(Reg8::A, 0xFF);
        execute(&mut cpu, &mut bus, MicroOp::WriteMemReg8 { addr_reg: Reg16::DE, src: Reg8::A });
        assert_eq!(bus.read(0xC100), 0xFF);
    }

    #[test]
    fn write_mem_multiple_locations() {
        let (mut cpu, mut bus) = make_cpu();
        for i in 0..8u16 {
            cpu.register_file.set_16bit(Reg16::HL, 0xC000 + i);
            cpu.register_file.set_8bit(Reg8::A, i as u8 * 0x11);
            execute(&mut cpu, &mut bus, MicroOp::WriteMemReg8 { addr_reg: Reg16::HL, src: Reg8::A });
        }
        for i in 0..8u16 {
            assert_eq!(bus.read(0xC000 + i), i as u8 * 0x11);
        }
    }

    // =====================================================================
    //  ReadMemImm8 / WriteMemImm8
    // =====================================================================

    #[test]
    fn write_then_read_mem_imm8() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0xAB);
        execute(&mut cpu, &mut bus, MicroOp::WriteMemImm8 { addr: 0xC050, src: Reg8::A });

        cpu.register_file.set_8bit(Reg8::B, 0x00);
        execute(&mut cpu, &mut bus, MicroOp::ReadMemImm8 { addr: 0xC050, into: Reg8::B });
        assert_eq!(cpu.register_file.get_8bit(Reg8::B), 0xAB);
    }

    // =====================================================================
    //  ReadHighPage / WriteHighPage
    // =====================================================================

    #[test]
    fn high_page_write_then_read() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::C, 0x44); // offset → 0xFF44
        cpu.register_file.set_8bit(Reg8::A, 0x77);
        execute(&mut cpu, &mut bus, MicroOp::WriteHighPage { offset: Reg8::C, src: Reg8::A });

        cpu.register_file.set_8bit(Reg8::B, 0x00);
        execute(&mut cpu, &mut bus, MicroOp::ReadHighPage { offset: Reg8::C, into: Reg8::B });
        assert_eq!(cpu.register_file.get_8bit(Reg8::B), 0x77);
    }

    #[test]
    fn high_page_offset_zero() {
        // 0xFF00 + 0x00 = 0xFF00
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::C, 0x00);
        cpu.register_file.set_8bit(Reg8::A, 0xDD);
        execute(&mut cpu, &mut bus, MicroOp::WriteHighPage { offset: Reg8::C, src: Reg8::A });
        assert_eq!(bus.read(0xFF00), 0xDD);
    }

    #[test]
    fn high_page_offset_7f_is_io() {
        // 0xFF00 + 0x7F = 0xFF7F (last IO register)
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::C, 0x7F);
        cpu.register_file.set_8bit(Reg8::A, 0xEE);
        execute(&mut cpu, &mut bus, MicroOp::WriteHighPage { offset: Reg8::C, src: Reg8::A });
        assert_eq!(bus.read(0xFF7F), 0xEE);
    }

    // =====================================================================
    //  Push / Pop
    // =====================================================================

    #[test]
    fn push_pop_round_trip() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::SP, 0xFFFE);
        cpu.register_file.set_16bit(Reg16::BC, 0x1234);
        execute(&mut cpu, &mut bus, MicroOp::Push { src: Reg16::BC });
        assert_eq!(cpu.register_file.get_16bit(Reg16::SP), 0xFFFC);

        cpu.register_file.set_16bit(Reg16::DE, 0x0000);
        execute(&mut cpu, &mut bus, MicroOp::Pop { dst: Reg16::DE });
        assert_eq!(cpu.register_file.get_16bit(Reg16::DE), 0x1234);
        assert_eq!(cpu.register_file.get_16bit(Reg16::SP), 0xFFFE);
    }

    #[test]
    fn push_pop_zero() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::SP, 0xFFFE);
        cpu.register_file.set_16bit(Reg16::HL, 0x0000);
        execute(&mut cpu, &mut bus, MicroOp::Push { src: Reg16::HL });

        cpu.register_file.set_16bit(Reg16::BC, 0xFFFF);
        execute(&mut cpu, &mut bus, MicroOp::Pop { dst: Reg16::BC });
        assert_eq!(cpu.register_file.get_16bit(Reg16::BC), 0x0000);
    }

    #[test]
    fn push_pop_ffff() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::SP, 0xFFFE);
        cpu.register_file.set_16bit(Reg16::DE, 0xFFFF);
        execute(&mut cpu, &mut bus, MicroOp::Push { src: Reg16::DE });

        cpu.register_file.set_16bit(Reg16::HL, 0x0000);
        execute(&mut cpu, &mut bus, MicroOp::Pop { dst: Reg16::HL });
        assert_eq!(cpu.register_file.get_16bit(Reg16::HL), 0xFFFF);
    }

    #[test]
    fn push_pop_multiple() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::SP, 0xFFFE);
        cpu.register_file.set_16bit(Reg16::BC, 0x1111);
        cpu.register_file.set_16bit(Reg16::DE, 0x2222);
        cpu.register_file.set_16bit(Reg16::HL, 0x3333);

        execute(&mut cpu, &mut bus, MicroOp::Push { src: Reg16::BC });
        execute(&mut cpu, &mut bus, MicroOp::Push { src: Reg16::DE });
        execute(&mut cpu, &mut bus, MicroOp::Push { src: Reg16::HL });
        assert_eq!(cpu.register_file.get_16bit(Reg16::SP), 0xFFF8);

        // Pop in reverse order
        let mut r1 = Reg16::BC;
        execute(&mut cpu, &mut bus, MicroOp::Pop { dst: r1 });
        assert_eq!(cpu.register_file.get_16bit(r1), 0x3333);

        r1 = Reg16::DE;
        execute(&mut cpu, &mut bus, MicroOp::Pop { dst: r1 });
        assert_eq!(cpu.register_file.get_16bit(r1), 0x2222);

        r1 = Reg16::HL;
        execute(&mut cpu, &mut bus, MicroOp::Pop { dst: r1 });
        assert_eq!(cpu.register_file.get_16bit(r1), 0x1111);
    }

    #[test]
    fn pop_af_masks_lower_nibble() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::SP, 0xFFFC);
        // Push 0xFF onto stack where F byte would be (low nibble has junk bits)
        bus.write(0xFFFC, 0xFF); // F = 0xFF (lower 4 bits should be masked)
        bus.write(0xFFFD, 0x12); // A = 0x12
        execute(&mut cpu, &mut bus, MicroOp::Pop { dst: Reg16::AF });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x12);
        assert_eq!(cpu.register_file.get_8bit(Reg8::F), 0xF0); // lower nibble cleared
    }

    #[test]
    fn pop_af_preserves_only_upper_nibble() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::SP, 0xFFFC);
        bus.write(0xFFFC, 0x5A); // F = 0x5A → should become 0x50
        bus.write(0xFFFD, 0x00);
        execute(&mut cpu, &mut bus, MicroOp::Pop { dst: Reg16::AF });
        assert_eq!(cpu.register_file.get_8bit(Reg8::F), 0x50);
    }

    // =====================================================================
    //  Alu8 — ADD
    // =====================================================================

    #[test]
    fn alu8_add_basic() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x10);
        execute(&mut cpu, &mut bus, MicroOp::Alu8 {
            kind: AluOpKind::Add, dest: Reg8::A, src: Operand8::Imm(0x20),
        });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x30);
        assert_eq!(read_flags(&cpu), (false, false, false, false));
    }

    #[test]
    fn alu8_add_overflow_carry() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0xFF);
        execute(&mut cpu, &mut bus, MicroOp::Alu8 {
            kind: AluOpKind::Add, dest: Reg8::A, src: Operand8::Imm(0x01),
        });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x00);
        assert_eq!(read_flags(&cpu), (true, false, true, true)); // Z=1, H=1, C=1
    }

    #[test]
    fn alu8_add_half_carry() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x0F);
        execute(&mut cpu, &mut bus, MicroOp::Alu8 {
            kind: AluOpKind::Add, dest: Reg8::A, src: Operand8::Imm(0x01),
        });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x10);
        assert_eq!(read_flags(&cpu), (false, false, true, false)); // H=1
    }

    #[test]
    fn alu8_add_zero_plus_zero() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x00);
        execute(&mut cpu, &mut bus, MicroOp::Alu8 {
            kind: AluOpKind::Add, dest: Reg8::A, src: Operand8::Imm(0x00),
        });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x00);
        assert_eq!(read_flags(&cpu), (true, false, false, false));
    }

    #[test]
    fn alu8_add_from_register() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x30);
        cpu.register_file.set_8bit(Reg8::B, 0x40);
        execute(&mut cpu, &mut bus, MicroOp::Alu8 {
            kind: AluOpKind::Add, dest: Reg8::A, src: Operand8::Reg(Reg8::B),
        });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x70);
    }

    // =====================================================================
    //  Alu8 — SUB
    // =====================================================================

    #[test]
    fn alu8_sub_basic() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x50);
        execute(&mut cpu, &mut bus, MicroOp::Alu8 {
            kind: AluOpKind::Sub, dest: Reg8::A, src: Operand8::Imm(0x10),
        });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x40);
        let (z, n, _, _) = read_flags(&cpu);
        assert!(!z);
        assert!(n); // N always set for sub
    }

    #[test]
    fn alu8_sub_to_zero() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x42);
        execute(&mut cpu, &mut bus, MicroOp::Alu8 {
            kind: AluOpKind::Sub, dest: Reg8::A, src: Operand8::Imm(0x42),
        });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x00);
        assert_eq!(read_flags(&cpu), (true, true, false, false));
    }

    #[test]
    fn alu8_sub_borrow_carry() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x00);
        execute(&mut cpu, &mut bus, MicroOp::Alu8 {
            kind: AluOpKind::Sub, dest: Reg8::A, src: Operand8::Imm(0x01),
        });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0xFF);
        let (_, n, h, c) = read_flags(&cpu);
        assert!(n);
        assert!(h); // borrow from bit 4
        assert!(c); // borrow
    }

    #[test]
    fn alu8_sub_half_borrow() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x10);
        execute(&mut cpu, &mut bus, MicroOp::Alu8 {
            kind: AluOpKind::Sub, dest: Reg8::A, src: Operand8::Imm(0x01),
        });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x0F);
        let (_, _, h, c) = read_flags(&cpu);
        assert!(h); // borrow from bit 4
        assert!(!c);
    }

    // =====================================================================
    //  Alu8 — AND
    // =====================================================================

    #[test]
    fn alu8_and_basic() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0xFF);
        execute(&mut cpu, &mut bus, MicroOp::Alu8 {
            kind: AluOpKind::And, dest: Reg8::A, src: Operand8::Imm(0x0F),
        });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x0F);
        assert_eq!(read_flags(&cpu), (false, false, true, false)); // H always set for AND
    }

    #[test]
    fn alu8_and_zero_result() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0xF0);
        execute(&mut cpu, &mut bus, MicroOp::Alu8 {
            kind: AluOpKind::And, dest: Reg8::A, src: Operand8::Imm(0x0F),
        });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x00);
        assert_eq!(read_flags(&cpu), (true, false, true, false));
    }

    #[test]
    fn alu8_and_ff() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0xA5);
        execute(&mut cpu, &mut bus, MicroOp::Alu8 {
            kind: AluOpKind::And, dest: Reg8::A, src: Operand8::Imm(0xFF),
        });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0xA5);
    }

    // =====================================================================
    //  Alu8 — OR
    // =====================================================================

    #[test]
    fn alu8_or_basic() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0xF0);
        execute(&mut cpu, &mut bus, MicroOp::Alu8 {
            kind: AluOpKind::Or, dest: Reg8::A, src: Operand8::Imm(0x0F),
        });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0xFF);
        assert_eq!(read_flags(&cpu), (false, false, false, false));
    }

    #[test]
    fn alu8_or_zero() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x00);
        execute(&mut cpu, &mut bus, MicroOp::Alu8 {
            kind: AluOpKind::Or, dest: Reg8::A, src: Operand8::Imm(0x00),
        });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x00);
        assert_eq!(read_flags(&cpu), (true, false, false, false));
    }

    // =====================================================================
    //  Alu8 — XOR
    // =====================================================================

    #[test]
    fn alu8_xor_self_is_zero() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0xAB);
        execute(&mut cpu, &mut bus, MicroOp::Alu8 {
            kind: AluOpKind::Xor, dest: Reg8::A, src: Operand8::Reg(Reg8::A),
        });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x00);
        assert_eq!(read_flags(&cpu), (true, false, false, false));
    }

    #[test]
    fn alu8_xor_ff() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0xA5);
        execute(&mut cpu, &mut bus, MicroOp::Alu8 {
            kind: AluOpKind::Xor, dest: Reg8::A, src: Operand8::Imm(0xFF),
        });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x5A);
        assert_eq!(read_flags(&cpu), (false, false, false, false));
    }

    #[test]
    fn alu8_xor_double_is_identity() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x42);
        execute(&mut cpu, &mut bus, MicroOp::Alu8 {
            kind: AluOpKind::Xor, dest: Reg8::A, src: Operand8::Imm(0xBB),
        });
        execute(&mut cpu, &mut bus, MicroOp::Alu8 {
            kind: AluOpKind::Xor, dest: Reg8::A, src: Operand8::Imm(0xBB),
        });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x42);
    }

    // =====================================================================
    //  DAA
    // =====================================================================

    #[test]
    fn daa_after_add_09_plus_01() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x09);
        execute(&mut cpu, &mut bus, MicroOp::Alu8 {
            kind: AluOpKind::Add, dest: Reg8::A, src: Operand8::Imm(0x01),
        });
        // A = 0x0A, which is invalid BCD
        execute(&mut cpu, &mut bus, MicroOp::Daa);
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x10); // 09 + 01 = 10 BCD
    }

    #[test]
    fn daa_after_add_99_plus_01() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x99);
        execute(&mut cpu, &mut bus, MicroOp::Alu8 {
            kind: AluOpKind::Add, dest: Reg8::A, src: Operand8::Imm(0x01),
        });
        // A = 0x9A
        execute(&mut cpu, &mut bus, MicroOp::Daa);
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x00); // 99 + 01 = 100, wraps
        let (z, _, _, c) = read_flags(&cpu);
        assert!(z); // result is zero
        assert!(c); // BCD carry
    }

    #[test]
    fn daa_no_adjustment_needed() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x15);
        execute(&mut cpu, &mut bus, MicroOp::Alu8 {
            kind: AluOpKind::Add, dest: Reg8::A, src: Operand8::Imm(0x22),
        });
        // A = 0x37, already valid BCD
        execute(&mut cpu, &mut bus, MicroOp::Daa);
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x37);
    }

    #[test]
    fn daa_after_sub() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x10);
        execute(&mut cpu, &mut bus, MicroOp::Alu8 {
            kind: AluOpKind::Sub, dest: Reg8::A, src: Operand8::Imm(0x01),
        });
        // A = 0x0F, N=1, H=1
        execute(&mut cpu, &mut bus, MicroOp::Daa);
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x09); // 10 - 01 = 09 BCD
    }

    #[test]
    fn daa_clears_h_flag() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x0F);
        execute(&mut cpu, &mut bus, MicroOp::Alu8 {
            kind: AluOpKind::Add, dest: Reg8::A, src: Operand8::Imm(0x01),
        });
        execute(&mut cpu, &mut bus, MicroOp::Daa);
        let (_, _, h, _) = read_flags(&cpu);
        assert!(!h); // DAA always clears H
    }

    #[test]
    fn daa_preserves_n_flag() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x20);
        execute(&mut cpu, &mut bus, MicroOp::Alu8 {
            kind: AluOpKind::Sub, dest: Reg8::A, src: Operand8::Imm(0x01),
        });
        execute(&mut cpu, &mut bus, MicroOp::Daa);
        let (_, n, _, _) = read_flags(&cpu);
        assert!(n); // N preserved from sub
    }

    // =====================================================================
    //  CPL
    // =====================================================================

    #[test]
    fn cpl_basic() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x00);
        execute(&mut cpu, &mut bus, MicroOp::Cpl);
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0xFF);
    }

    #[test]
    fn cpl_ff() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0xFF);
        execute(&mut cpu, &mut bus, MicroOp::Cpl);
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x00);
    }

    #[test]
    fn cpl_sets_n_and_h() {
        let (mut cpu, mut bus) = make_cpu();
        set_flags(&mut cpu, false, false, false, false);
        cpu.register_file.set_8bit(Reg8::A, 0x42);
        execute(&mut cpu, &mut bus, MicroOp::Cpl);
        let (_, n, h, _) = read_flags(&cpu);
        assert!(n);
        assert!(h);
    }

    #[test]
    fn cpl_preserves_z_and_c() {
        let (mut cpu, mut bus) = make_cpu();
        set_flags(&mut cpu, true, false, false, true); // Z=1, C=1
        cpu.register_file.set_8bit(Reg8::A, 0xA5);
        execute(&mut cpu, &mut bus, MicroOp::Cpl);
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x5A);
        let (z, _, _, c) = read_flags(&cpu);
        assert!(z); // preserved
        assert!(c); // preserved
    }

    #[test]
    fn cpl_is_own_inverse() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x3C);
        execute(&mut cpu, &mut bus, MicroOp::Cpl);
        execute(&mut cpu, &mut bus, MicroOp::Cpl);
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x3C);
    }

    // =====================================================================
    //  SCF / CCF
    // =====================================================================

    #[test]
    fn scf_sets_carry_clears_n_h() {
        let (mut cpu, mut bus) = make_cpu();
        set_flags(&mut cpu, false, true, true, false);
        execute(&mut cpu, &mut bus, MicroOp::Scf);
        assert_eq!(read_flags(&cpu), (false, false, false, true));
    }

    #[test]
    fn scf_preserves_z() {
        let (mut cpu, mut bus) = make_cpu();
        set_flags(&mut cpu, true, true, true, false);
        execute(&mut cpu, &mut bus, MicroOp::Scf);
        assert_eq!(read_flags(&cpu), (true, false, false, true));
    }

    #[test]
    fn ccf_flips_carry_from_0_to_1() {
        let (mut cpu, mut bus) = make_cpu();
        set_flags(&mut cpu, false, true, true, false); // C=0
        execute(&mut cpu, &mut bus, MicroOp::Ccf);
        assert_eq!(read_flags(&cpu), (false, false, false, true)); // C=1
    }

    #[test]
    fn ccf_flips_carry_from_1_to_0() {
        let (mut cpu, mut bus) = make_cpu();
        set_flags(&mut cpu, false, true, true, true); // C=1
        execute(&mut cpu, &mut bus, MicroOp::Ccf);
        assert_eq!(read_flags(&cpu), (false, false, false, false)); // C=0
    }

    #[test]
    fn ccf_preserves_z() {
        let (mut cpu, mut bus) = make_cpu();
        set_flags(&mut cpu, true, false, false, true);
        execute(&mut cpu, &mut bus, MicroOp::Ccf);
        let (z, n, h, c) = read_flags(&cpu);
        assert!(z); // preserved
        assert!(!n);
        assert!(!h);
        assert!(!c); // flipped
    }

    // =====================================================================
    //  Inc8 / Dec8
    // =====================================================================

    #[test]
    fn inc8_basic() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x00);
        execute(&mut cpu, &mut bus, MicroOp::Inc8 { target: Operand8::Reg(Reg8::A) });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x01);
        assert_eq!(read_flags(&cpu), (false, false, false, false));
    }

    #[test]
    fn inc8_half_carry() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x0F);
        execute(&mut cpu, &mut bus, MicroOp::Inc8 { target: Operand8::Reg(Reg8::A) });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x10);
        let (_, _, h, _) = read_flags(&cpu);
        assert!(h);
    }

    #[test]
    fn inc8_wraps_to_zero() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0xFF);
        execute(&mut cpu, &mut bus, MicroOp::Inc8 { target: Operand8::Reg(Reg8::A) });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x00);
        let (z, n, h, _) = read_flags(&cpu);
        assert!(z);
        assert!(!n);
        assert!(h); // 0xF + 1 > 0xF
    }

    #[test]
    fn inc8_preserves_carry() {
        let (mut cpu, mut bus) = make_cpu();
        set_flags(&mut cpu, false, false, false, true); // C=1
        cpu.register_file.set_8bit(Reg8::B, 0x05);
        execute(&mut cpu, &mut bus, MicroOp::Inc8 { target: Operand8::Reg(Reg8::B) });
        let (_, _, _, c) = read_flags(&cpu);
        assert!(c); // carry must be preserved
    }

    #[test]
    fn inc8_n_always_clear() {
        let (mut cpu, mut bus) = make_cpu();
        set_flags(&mut cpu, false, true, false, false); // N was set
        cpu.register_file.set_8bit(Reg8::A, 0x10);
        execute(&mut cpu, &mut bus, MicroOp::Inc8 { target: Operand8::Reg(Reg8::A) });
        let (_, n, _, _) = read_flags(&cpu);
        assert!(!n);
    }

    #[test]
    fn dec8_basic() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x02);
        execute(&mut cpu, &mut bus, MicroOp::Dec8 { target: Operand8::Reg(Reg8::A) });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x01);
        let (_, n, _, _) = read_flags(&cpu);
        assert!(n); // N always set for dec
    }

    #[test]
    fn dec8_to_zero() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x01);
        execute(&mut cpu, &mut bus, MicroOp::Dec8 { target: Operand8::Reg(Reg8::A) });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x00);
        let (z, n, _, _) = read_flags(&cpu);
        assert!(z);
        assert!(n);
    }

    #[test]
    fn dec8_wraps_to_ff() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x00);
        execute(&mut cpu, &mut bus, MicroOp::Dec8 { target: Operand8::Reg(Reg8::A) });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0xFF);
        let (_, _, h, _) = read_flags(&cpu);
        assert!(h); // borrow from bit 4
    }

    #[test]
    fn dec8_half_borrow() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x10);
        execute(&mut cpu, &mut bus, MicroOp::Dec8 { target: Operand8::Reg(Reg8::A) });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x0F);
        let (_, _, h, _) = read_flags(&cpu);
        assert!(h); // (0x10 & 0xF) == 0 → H set
    }

    #[test]
    fn dec8_preserves_carry() {
        let (mut cpu, mut bus) = make_cpu();
        set_flags(&mut cpu, false, false, false, true);
        cpu.register_file.set_8bit(Reg8::B, 0x05);
        execute(&mut cpu, &mut bus, MicroOp::Dec8 { target: Operand8::Reg(Reg8::B) });
        let (_, _, _, c) = read_flags(&cpu);
        assert!(c);
    }

    // =====================================================================
    //  Add16
    // =====================================================================

    #[test]
    fn add16_basic() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::HL, 0x1000);
        cpu.register_file.set_16bit(Reg16::BC, 0x2000);
        execute(&mut cpu, &mut bus, MicroOp::Add16 { dest: Reg16::HL, src: Reg16::BC });
        assert_eq!(cpu.register_file.get_16bit(Reg16::HL), 0x3000);
    }

    #[test]
    fn add16_carry() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::HL, 0xFFFF);
        cpu.register_file.set_16bit(Reg16::BC, 0x0001);
        execute(&mut cpu, &mut bus, MicroOp::Add16 { dest: Reg16::HL, src: Reg16::BC });
        assert_eq!(cpu.register_file.get_16bit(Reg16::HL), 0x0000);
        let (_, n, _, c) = read_flags(&cpu);
        assert!(!n);
        assert!(c);
    }

    #[test]
    fn add16_half_carry() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::HL, 0x0FFF);
        cpu.register_file.set_16bit(Reg16::BC, 0x0001);
        execute(&mut cpu, &mut bus, MicroOp::Add16 { dest: Reg16::HL, src: Reg16::BC });
        assert_eq!(cpu.register_file.get_16bit(Reg16::HL), 0x1000);
        let (_, _, h, _) = read_flags(&cpu);
        assert!(h); // carry from bit 11
    }

    #[test]
    fn add16_preserves_z() {
        let (mut cpu, mut bus) = make_cpu();
        set_flags(&mut cpu, true, false, false, false); // Z=1
        cpu.register_file.set_16bit(Reg16::HL, 0x1000);
        cpu.register_file.set_16bit(Reg16::BC, 0x1000);
        execute(&mut cpu, &mut bus, MicroOp::Add16 { dest: Reg16::HL, src: Reg16::BC });
        let (z, _, _, _) = read_flags(&cpu);
        assert!(z); // Z preserved
    }

    #[test]
    fn add16_clears_n() {
        let (mut cpu, mut bus) = make_cpu();
        set_flags(&mut cpu, false, true, false, false); // N=1
        cpu.register_file.set_16bit(Reg16::HL, 0x0000);
        cpu.register_file.set_16bit(Reg16::BC, 0x0000);
        execute(&mut cpu, &mut bus, MicroOp::Add16 { dest: Reg16::HL, src: Reg16::BC });
        let (_, n, _, _) = read_flags(&cpu);
        assert!(!n);
    }

    // =====================================================================
    //  AddSpE (SP + signed 8-bit)
    // =====================================================================

    #[test]
    fn add_sp_e_positive() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::SP, 0xFFF0);
        execute(&mut cpu, &mut bus, MicroOp::AddSpE { e: 8 });
        assert_eq!(cpu.register_file.get_16bit(Reg16::SP), 0xFFF8);
    }

    #[test]
    fn add_sp_e_negative() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::SP, 0xFFF8);
        execute(&mut cpu, &mut bus, MicroOp::AddSpE { e: -8 });
        assert_eq!(cpu.register_file.get_16bit(Reg16::SP), 0xFFF0);
    }

    #[test]
    fn add_sp_e_z_always_clear() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::SP, 0x0000);
        execute(&mut cpu, &mut bus, MicroOp::AddSpE { e: 0 });
        let (z, n, _, _) = read_flags(&cpu);
        assert!(!z); // Z always 0 for ADD SP,e
        assert!(!n); // N always 0
    }

    #[test]
    fn add_sp_e_half_carry() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::SP, 0x000F);
        execute(&mut cpu, &mut bus, MicroOp::AddSpE { e: 1 });
        assert_eq!(cpu.register_file.get_16bit(Reg16::SP), 0x0010);
        let (_, _, h, _) = read_flags(&cpu);
        assert!(h); // carry from bit 3
    }

    #[test]
    fn add_sp_e_carry_from_byte() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::SP, 0x00FF);
        execute(&mut cpu, &mut bus, MicroOp::AddSpE { e: 1 });
        assert_eq!(cpu.register_file.get_16bit(Reg16::SP), 0x0100);
        let (_, _, _, c) = read_flags(&cpu);
        assert!(c); // carry from bit 7
    }

    // =====================================================================
    //  Inc16 / Dec16
    // =====================================================================

    #[test]
    fn inc16_basic() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::BC, 0x1234);
        execute(&mut cpu, &mut bus, MicroOp::Inc16 { reg: Reg16::BC });
        assert_eq!(cpu.register_file.get_16bit(Reg16::BC), 0x1235);
    }

    #[test]
    fn inc16_wraps() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::HL, 0xFFFF);
        execute(&mut cpu, &mut bus, MicroOp::Inc16 { reg: Reg16::HL });
        assert_eq!(cpu.register_file.get_16bit(Reg16::HL), 0x0000);
    }

    #[test]
    fn dec16_basic() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::DE, 0x1000);
        execute(&mut cpu, &mut bus, MicroOp::Dec16 { reg: Reg16::DE });
        assert_eq!(cpu.register_file.get_16bit(Reg16::DE), 0x0FFF);
    }

    #[test]
    fn dec16_wraps() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::SP, 0x0000);
        execute(&mut cpu, &mut bus, MicroOp::Dec16 { reg: Reg16::SP });
        assert_eq!(cpu.register_file.get_16bit(Reg16::SP), 0xFFFF);
    }

    #[test]
    fn inc16_dec16_round_trip() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::HL, 0x8000);
        execute(&mut cpu, &mut bus, MicroOp::Inc16 { reg: Reg16::HL });
        execute(&mut cpu, &mut bus, MicroOp::Dec16 { reg: Reg16::HL });
        assert_eq!(cpu.register_file.get_16bit(Reg16::HL), 0x8000);
    }
}
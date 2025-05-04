use crate::cpu::CPU;

use super::{alu::{alu, AluResult}, registers::{Flag, Reg16, Reg8}};

#[derive(Debug)]
pub enum AluOpKind { Add, Sub, Xor, And, Or, }

#[derive(Debug)]
pub enum Operand8 { Reg(Reg8), Imm(u8) }

pub const FLAG_Z: u8 = 0b1000_0000;
pub const FLAG_N: u8 = 0b0100_0000;
pub const FLAG_H: u8 = 0b0010_0000;
pub const FLAG_C: u8 = 0b0001_0000;

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

#[derive(Debug)]
pub enum MicroOp {
    // ----------------------------------------
    // Memory & Register Access
    // ----------------------------------------
    /// Copy an 8-bit register into another
    LoadReg8 { dst: Reg8, src: Reg8 },
    /// Copy a 16-bit register into another
    LoadReg16 { dst: Reg16, src: Reg16 },
    /// Read next byte from PC into an 8-bit register
    ReadImmediate8 { into: Reg8 },
    /// Read next two bytes (little-endian) from PC into a 16-bit register
    ReadImmediate16 { into: Reg16 },
    /// Read from memory at address in `addr_reg` into an 8-bit register
    ReadMemReg8 { addr_reg: Reg16, into: Reg8 },
    /// Read from absolute 16-bit immediate address into an 8-bit register
    ReadMemImm8 { addr: u16, into: Reg8 },
    /// Read from high-page (0xFF00 + offset) into an 8-bit register
    ReadHighPage { offset: Reg8, into: Reg8 },
    /// Write an 8-bit register into memory at address in `addr_reg`
    WriteMemReg8 { addr_reg: Reg16, src: Reg8 },
    /// Write an 8-bit register into memory at absolute 16-bit immediate
    WriteMemImm8 { addr: u16, src: Reg8 },
    /// Write an 8-bit register into high-page (0xFF00 + offset)
    WriteHighPage { offset: Reg8, src: Reg8 },
    /// Push a 16-bit register onto the stack
    Push { src: Reg16 },
    /// Pop a 16-bit value from the stack into a register
    Pop { dst: Reg16 },

    // ----------------------------------------
    // Arithmetic & Logic (8-bit)
    // ----------------------------------------
    /// Perform an 8-bit ALU operation, reading from either a register or immediate
    Alu8 { kind: AluOpKind, dest: Reg8, src: Operand8 },
    /// Decimal Adjust Accumulator (BCD correction)
    Daa,
    /// One's complement on A (flip bits)
    Cpl,
    /// Set Carry flag, clear N and H
    Scf,
    /// Complement Carry flag, clear N and H
    Ccf,
    /// Increment an 8-bit register or (HL)
    Inc8 { target: Operand8 },
    /// Decrement an 8-bit register or (HL)
    Dec8 { target: Operand8 },

    // ----------------------------------------
    // 16-bit Arithmetic
    // ----------------------------------------
    /// Add one 16-bit register into another (e.g. HL += rr)
    Add16 { dest: Reg16, src: Reg16 },
    /// Add signed 8-bit immediate to SP, update H/C (SP = SP + e)
    AddSpE { e: i8 },
    /// Increment a 16-bit register (BC, DE, HL, SP)
    Inc16 { reg: Reg16 },
    /// Decrement a 16-bit register (BC, DE, HL, SP)
    Dec16 { reg: Reg16 },

    // ----------------------------------------
    // Rotate, Shift & Bit Ops
    // ----------------------------------------
    Rlc { dst: Reg8, src: Reg8 },
    Rrc { dst: Reg8, src: Reg8 },
    Rl  { dst: Reg8, src: Reg8 },
    Rr  { dst: Reg8, src: Reg8 },
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
    // System & Interrupt State
    // ----------------------------------------
    /// Set or clear a specific flag (Z, N, H, C)
    SetFlag { flag: Flag, value: bool },
    /// Read a flag into internal state (rarely used)
    GetFlag { flag: Flag },
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

pub fn execute(cpu: &mut CPU, op: MicroOp) {
    match op {
        // Memory & Register Access ---------------------------------------------------
        MicroOp::LoadReg8 { dst, src } => {
            let v = cpu.register_file.get_8bit(&src);
            cpu.register_file.set_8bit(&dst, v);
        }
        MicroOp::LoadReg16 { dst, src } => {
            let value: u16 = cpu.register_file.get_16bit(&src);
            cpu.register_file.set_16bit(&dst, value);
        },
        MicroOp::ReadImmediate8 { into } => {
            let pc_addr = cpu.register_file.get_16bit(&Reg16::PC);
            let imm = cpu.bus.borrow_mut().read(pc_addr);
            cpu.register_file.inc16(&Reg16::PC);
            cpu.register_file.set_8bit(&into, imm);
        },
        MicroOp::ReadImmediate16 { into } => {
            let pc = cpu.register_file.get_16bit(&Reg16::PC);
            let lo = cpu.bus.borrow_mut().read(pc);
            cpu.register_file.inc16(&Reg16::PC);
    
            let pc = cpu.register_file.get_16bit(&Reg16::PC);
            let hi = cpu.bus.borrow_mut().read(pc);
            cpu.register_file.inc16(&Reg16::PC);

            let value = u16::from_le_bytes([lo, hi]);
            cpu.register_file.set_16bit(&into, value);
        },
        MicroOp::ReadMemReg8 { addr_reg, into }  => {
            let address: u16 = cpu.register_file.get_16bit(&addr_reg);
            let value: u8 = cpu.bus.borrow_mut().read(address);
            cpu.register_file.set_8bit(&into, value);
        },
        MicroOp::ReadMemImm8 { addr, into } => {
            let value: u8 = cpu.bus
                .borrow_mut()
                .read(addr);
            cpu.register_file
                .set_8bit(&into, value);
        },
        MicroOp::ReadHighPage { offset, into } => {
            let off_val: u16 = cpu.register_file.get_8bit(&offset) as u16;  
            let addr: u16 = 0xFF00_u16.wrapping_add(off_val);
            let value: u8 = cpu.bus.borrow_mut().read(addr);                  
            cpu.register_file.set_8bit(&into, value);                        
        },
        MicroOp::WriteMemReg8 { addr_reg, src } => {
            let addr: u16 = cpu.register_file.get_16bit(&addr_reg);
            let value: u8 = cpu.register_file.get_8bit(&src);
            cpu.bus.borrow_mut().write(addr, value);
        },        
        MicroOp::WriteMemImm8 { addr, src } => {
            // Read the 8-bit value from the source register `src`.
            // `get_8bit` returns the current contents of that register.
            let value: u8 = cpu.register_file.get_8bit(&src);
            // Write that value into memory at the absolute 16-bit address `addr`.
            // 'bus` is an Rc<RefCell<dyn Bus>>; we borrow it mutably to call `write`.
            cpu.bus.borrow_mut().write(addr, value);
        },
        MicroOp::WriteHighPage { offset, src } => {
            // Read the 8-bit offset value from the specified register.
            // This comes from, e.g., the operand in an instruction like LD (0xFF00+R), R′.
            let offset_val: u8 = cpu.register_file.get_8bit(&offset);              
            // Compute the target address in the “high page” (0xFF00–0xFFFF).
            // The Game Boy maps 0xFF00–0xFF7F to I/O registers and 0xFF80–0xFFFE to high RAM.
            // Wrapping_add is safe here because offset_val is 0–0xFF.
            let addr: u16 = 0xFF00u16.wrapping_add(offset_val as u16);              
            // Read the byte to store from the source register.
            let value: u8 = cpu.register_file.get_8bit(&src);                     
            // Perform the memory write via the shared Bus (MMU).
            // The bus implements address dispatching (ROM, VRAM, I/O, etc.).
            cpu.bus.borrow_mut().write(addr, value);                              
        },

        // Stack Operations -------------------------------------------------------------
        MicroOp::Push { src } => {
            let value: u16 = cpu.register_file.get_16bit(&src);
            let [high, low] = value.to_be_bytes();
            cpu.register_file.dec16(&Reg16::SP);

            let sp = cpu.register_file.get_16bit(&Reg16::SP);
            cpu.bus.borrow_mut().write(sp, high);
            cpu.register_file.dec16(&Reg16::SP);

            let sp = cpu.register_file.get_16bit(&Reg16::SP);
            cpu.bus.borrow_mut().write(sp, low);
        },
        MicroOp::Pop { dst } => {
            let sp_addr = cpu.register_file.get_16bit(&Reg16::SP);
            let low = cpu.bus.borrow_mut().read(sp_addr);
            cpu.register_file.inc16(&Reg16::SP);

            let sp_addr = cpu.register_file.get_16bit(&Reg16::SP);
            let high = cpu.bus.borrow_mut().read(sp_addr);
            cpu.register_file.inc16(&Reg16::SP);

            let value = u16::from_le_bytes([low, high]);
            cpu.register_file.set_16bit(&dst, value);
            // !!! Special case: POP AF must mask F’s low nibble
            // On real hardware, the lower 4 bits of F are always zero; if dst == AF,
            // clear any stray low bits from the popped value.
            if let Reg16::AF = dst {
                let f = cpu.register_file.get_8bit(&Reg8::F) & 0xF0;
                cpu.register_file.set_8bit(&Reg8::F, f);
            }
        },

        // Arithmetic & Logic (8-bit) --------------------------------------------------
        MicroOp::Alu8 { kind, dest, src } => {
            let a = cpu.register_file.get_8bit(&dest);
            let b = match src {
                Operand8::Reg(r) => cpu.register_file.get_8bit(&r),
                Operand8::Imm(v)   => v,
            };
            let AluResult { value, z, n, h, c } = alu(kind, a, b);
            cpu.register_file.set_8bit(&dest, value);
            let mut f = 0;
            if z { f |= FLAG_Z; }
            if n { f |= FLAG_N; }
            if h { f |= FLAG_H; }
            if c { f |= FLAG_C; }
            cpu.register_file.set_8bit(&Reg8::F, f);
        },
        MicroOp::Daa => {
            let mut a = cpu.register_file.get_8bit(&Reg8::A);
            let flags = cpu.register_file.get_8bit(&Reg8::F);
            let n = flags & FLAG_N != 0; // Was the previous operation a subtraction?
            let h = flags & FLAG_H != 0; // Did we have a half-carry?
            let c = flags & FLAG_C != 0; // Did we have a full carry?
        
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
            cpu.register_file.set_8bit(&Reg8::A, a);
        
            // Z: set if result is zero
            // N: preserved from previous operation
            // H: always cleared after DAA
            // C: set if BCD adjustment triggered carry
            let mut f = 0;
            if a == 0      { f |= FLAG_Z; }
            if n           { f |= FLAG_N; }
            if new_c       { f |= FLAG_C; }
            cpu.register_file.set_8bit(&Reg8::F, f);
        },    
        MicroOp::Cpl => {
            let a = cpu.register_file.get_8bit(&Reg8::A);
            cpu.register_file.set_8bit(&Reg8::A, !a);
            // N=1, H=1, preserve Z and C
            let prev = cpu.register_file.get_8bit(&Reg8::F);
            let newf = (prev & (FLAG_Z | FLAG_C)) | FLAG_N | FLAG_H;
            cpu.register_file.set_8bit(&Reg8::F, newf);
        },
        MicroOp::Scf => {
            // C=1, N=0, H=0, preserve Z
            let prev_z = cpu.register_file.get_8bit(&Reg8::F) & FLAG_Z;
            cpu.register_file.set_8bit(&Reg8::F, prev_z | FLAG_C);
        },
        MicroOp::Ccf => {
            // C ← ¬C, N=0, H=0, preserve Z
            let prev = cpu.register_file.get_8bit(&Reg8::F);
            let prev_z = prev & FLAG_Z;
            let new_c = if prev & FLAG_C == 0 { FLAG_C } else { 0 };
            cpu.register_file.set_8bit(&Reg8::F, prev_z | new_c);
        },        
        MicroOp::Inc8 { target } => {
            let (reg, old) = match target {
                Operand8::Reg(r) => (r, cpu.register_file.get_8bit(&r)),
                _ => panic!("Inc8 only supports register targets"),
            };
            let res = old.wrapping_add(1);
            cpu.register_file.set_8bit(&reg, res);
            // Flags: Z, H half-carry, N=0, keep old C
            let oldf = cpu.register_file.get_8bit(&Reg8::F) & FLAG_C;
            let mut f = oldf;
            if res == 0            { f |= FLAG_Z; }
            if ((old & 0x0F) + 1) > 0x0F { f |= FLAG_H; }
            cpu.register_file.set_8bit(&Reg8::F, f);
        },
        MicroOp::Dec8 { target } => {
            let (reg, old) = match target {
                Operand8::Reg(r) => (r, cpu.register_file.get_8bit(&r)),
                _ => panic!("Dec8 only supports register targets"),
            };
            let res = old.wrapping_sub(1);
            cpu.register_file.set_8bit(&reg, res);
            // Flags: Z, H (borrow from bit4), N=1, keep old C
            let oldf = cpu.register_file.get_8bit(&Reg8::F) & FLAG_C;
            let mut f = oldf | FLAG_N;
            if res == 0             { f |= FLAG_Z; }
            if (old & 0x0F) == 0    { f |= FLAG_H; }
            cpu.register_file.set_8bit(&Reg8::F, f);
        },

        // 16-bit Arithmetic  --------------------------------------------------
        MicroOp::Add16 { dest, src } => {
            // HL (or other dest) <- dest + src
            let a = cpu.register_file.get_16bit(&dest);
            let b = cpu.register_file.get_16bit(&src);
            let result = a.wrapping_add(b);
            cpu.register_file.set_16bit(&dest, result);
        
            // Flags: Z unchanged, N = 0, H = carry from bit 11, C = carry from bit 15
            let mut f = cpu.register_file.get_8bit(&Reg8::F) & FLAG_Z;
            // half-carry if lower 12 bits overflow
            if ((a & 0x0FFF) + (b & 0x0FFF)) > 0x0FFF {
                f |= FLAG_H;
            }
            // full carry if 16-bit sum overflows
            if (a as u32 + b as u32) > 0xFFFF {
                f |= FLAG_C;
            }
            cpu.register_file.set_8bit(&Reg8::F, f);
        },
        
        MicroOp::AddSpE { e } => {
            // SP <- SP + signed immediate e
            let sp = cpu.register_file.get_16bit(&Reg16::SP);
            let result = (sp as i16).wrapping_add(e as i16) as u16;
            cpu.register_file.set_16bit(&Reg16::SP, result);
        
            // Flags: Z = 0, N = 0, H = carry from bit 3, C = carry from bit 7
            // Note: use the raw 8-bit two’s-complement immediate for H/C detection
            let imm_u = e as u8 as u16;
            let mut f = 0;
            if ((sp & 0x000F) + (imm_u & 0x000F)) > 0x000F {
                f |= FLAG_H;
            }
            if ((sp & 0x00FF) + imm_u) > 0x00FF {
                f |= FLAG_C;
            }
            cpu.register_file.set_8bit(&Reg8::F, f);
        },
        
        MicroOp::Inc16 { reg } => {
            cpu.register_file.inc16(&reg);
        },
        
        MicroOp::Dec16 { reg } => {
            cpu.register_file.dec16(&reg);
        },

        // Rotate, Shift & Bit Ops  --------------------------------------------------
        MicroOp::Rlc        { dst, src } => todo!(),
        MicroOp::Rrc        { dst, src } => todo!(),
        MicroOp::Rl         { dst, src } => todo!(),
        MicroOp::Rr         { dst, src } => todo!(),
        MicroOp::Sla        { dst, src } => todo!(),
        MicroOp::Sra        { dst, src } => todo!(),
        MicroOp::Srl        { dst, src } => todo!(),
        MicroOp::Swap       { dst, src } => todo!(),
        MicroOp::BitTest    { bit, reg } => todo!(),
        MicroOp::SetBit     { bit, reg } => todo!(),
        MicroOp::ResetBit   { bit, reg } => todo!(),

        // Control Flow  --------------------------------------------------
        MicroOp::CheckCond  { cond }    => todo!(),
        MicroOp::JumpAbs    { addr }    => todo!(),
        MicroOp::JumpRel    { offset }  => todo!(),
        MicroOp::Call       { addr }    => todo!(),
        MicroOp::Ret                    => todo!(),
        MicroOp::RetI                   => todo!(),
        MicroOp::Rst        { addr }    => todo!(),
        MicroOp::FetchOpcode            => todo!(),
        MicroOp::DecodeCb   { prefix }  => todo!(),

        // System & Interrupt State  --------------------------------------------------
        MicroOp::SetFlag        { flag, value } => todo!(),
        MicroOp::GetFlag        { flag }        => todo!(),
        MicroOp::SetIme         { value }       => todo!(),
        MicroOp::DeferImeEnable                 => todo!(),
        MicroOp::TriggerHalt                    => todo!(),
        MicroOp::TriggerStop                    => todo!(),
        MicroOp::CheckInterrupts                => todo!(),
    }
}
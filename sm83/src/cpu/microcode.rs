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

/// High-level micro-operations for the Sharp SM83 CPU core
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
            // Read the full 16-bit value out of the `src` register pair.
            // This uses RegisterFile::get_16bit, which concatenates the two 8-bit halves.
            let value: u16 = cpu.register_file.get_16bit(&src);
            // Write that 16-bit value into the `dst` register pair.
            // RegisterFile::set_16bit splits it back into high/low bytes.
            cpu.register_file.set_16bit(&dst, value);
        },
        MicroOp::ReadImmediate8 { into } => {
            // Fetch the byte at the address in the PC register.
            // This is the “immediate” operand encoded in the instruction stream.
            let pc_addr = cpu.register_file.get_16bit(&Reg16::PC);
            let imm = cpu.bus.borrow_mut().read(pc_addr);
            // Advance PC by one so it now points to the next instruction byte.
            cpu.register_file.inc16(&Reg16::PC);
            // Write the fetched immediate value into the specified 8-bit register.
            cpu.register_file.set_8bit(&into, imm);
        },
        MicroOp::ReadImmediate16 { into } => {
            // Fetch the current PC value
            let pc = cpu.register_file.get_16bit(&Reg16::PC);
            // Read the low byte from memory at [PC]
            let lo = cpu.bus.borrow_mut().read(pc); // Memory read via Bus :contentReference[oaicite:2]{index=2}:contentReference[oaicite:3]{index=3}
            // Increment PC to point to the high byte
            cpu.register_file.inc16(&Reg16::PC);
            // Read the high byte from memory at the new PC
            let pc = cpu.register_file.get_16bit(&Reg16::PC);
            let hi = cpu.bus.borrow_mut().read(pc);
            // Advance PC past the immediate operand
            cpu.register_file.inc16(&Reg16::PC);
            // Combine the two bytes into a little-endian u16
            let value = u16::from_le_bytes([lo, hi]);
            // Write the resulting 16-bit value into the destination register
            cpu.register_file.set_16bit(&into, value);
        },
        MicroOp::ReadMemReg8 { addr_reg, into }  => {
            // Fetch the 16-bit address from the given register pair (e.g., HL, BC, DE)
            // `get_16bit` reads two 8-bit registers and combines them big-endian → u16
            let address: u16 = cpu.register_file.get_16bit(&addr_reg);
            // Perform a memory read on the shared bus at that address
            // `bus.read` consults the MMU to route the read to ROM, RAM, I/O, etc.
            let value: u8 = cpu.bus.borrow_mut().read(address);
            // Write the fetched byte into the target 8-bit register
            // `set_8bit` updates the specified Reg8 in the RegisterFile
            cpu.register_file.set_8bit(&into, value);
        },
        MicroOp::ReadMemImm8 { addr, into } => {
            // Perform the memory read:
            // The `Bus` trait abstracts ROM, RAM, MMIO, boot-ROM mapping, etc.  
            // Here we borrow the bus and read one byte at the given 16-bit address.
            let value: u8 = cpu.bus
                .borrow_mut() // borrow the shared Bus
                .read(addr);  // read at absolute address `addr` :contentReference[oaicite:0]{index=0}:contentReference[oaicite:1]{index=1}
            // Write the result into the target CPU register:
            // The RegisterFile handles writing into one of A, B, C, D, E, H, L, IR, IE, or F.
            cpu.register_file
                .set_8bit(&into, value);  // set register `into` = `value` :contentReference[oaicite:2]{index=2}:contentReference[oaicite:3]{index=3}
        },
        MicroOp::ReadHighPage { offset, into } => {
            // Read the 8-bit offset from the specified register.
            // This value n comes from the instruction operand (immediate or register C).
            let off_val: u16 = cpu.register_file.get_8bit(&offset) as u16;  // :contentReference[oaicite:2]{index=2}:contentReference[oaicite:3]{index=3}
            // Form the full 16-bit address by adding the high-page base (0xFF00).
            // All I/O registers and High RAM live at 0xFF00 + n.
            let addr: u16 = 0xFF00_u16.wrapping_add(off_val);
            // Perform the memory read via the shared bus.
            // `read` takes an absolute Game Boy address and returns the byte stored there.
            let value: u8 = cpu.bus.borrow_mut().read(addr);                  // :contentReference[oaicite:4]{index=4}:contentReference[oaicite:5]{index=5}
            // Write the fetched byte back into the target 8-bit CPU register.
            cpu.register_file.set_8bit(&into, value);                        // :contentReference[oaicite:6]{index=6}:contentReference[oaicite:7]{index=7}
        },
        MicroOp::WriteMemReg8 { addr_reg, src } => {
            // Read the target 16-bit address from the given register pair (e.g. BC, DE, or HL)
            let addr: u16 = cpu.register_file.get_16bit(&addr_reg);
            // Read the 8-bit value from the specified source register (e.g. A, B, C…)
            let value: u8 = cpu.register_file.get_8bit(&src);
            // Write that byte into memory via the shared bus abstraction
            // The Bus implementation (MMU) will route this to VRAM, external RAM, etc.
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
            let offset_val: u8 = cpu.register_file.get_8bit(&offset);              // :contentReference[oaicite:0]{index=0}:contentReference[oaicite:1]{index=1}
            // Compute the target address in the “high page” (0xFF00–0xFFFF).
            // The Game Boy maps 0xFF00–0xFF7F to I/O registers and 0xFF80–0xFFFE to high RAM.
            // Wrapping_add is safe here because offset_val is 0–0xFF.
            let addr: u16 = 0xFF00u16.wrapping_add(offset_val as u16);            // :contentReference[oaicite:2]{index=2}:contentReference[oaicite:3]{index=3}  
            // Read the byte to store from the source register.
            let value: u8 = cpu.register_file.get_8bit(&src);                     // :contentReference[oaicite:4]{index=4}:contentReference[oaicite:5]{index=5}
            // Perform the memory write via the shared Bus (MMU).
            // The bus implements address dispatching (ROM, VRAM, I/O, etc.).
            cpu.bus.borrow_mut().write(addr, value);                              // :contentReference[oaicite:6]{index=6}:contentReference[oaicite:7]{index=7}
        },

        // Stack Operations -------------------------------------------------------------
        MicroOp::Push { src } => {
            // Read the full 16-bit value from the source register (BC, DE, HL, or AF)
            let value: u16 = cpu.register_file.get_16bit(&src);
            // Split into two 8-bit halves: [high, low]
            let [high, low] = value.to_be_bytes();
            // Decrement SP by 1 to make room for the high byte
            cpu.register_file.dec16(&Reg16::SP);
            let sp = cpu.register_file.get_16bit(&Reg16::SP);
            // Write the high byte to memory at [SP]
            cpu.bus.borrow_mut().write(sp, high);
            // Decrement SP by 1 again to make room for the low byte
            cpu.register_file.dec16(&Reg16::SP);
            let sp = cpu.register_file.get_16bit(&Reg16::SP);
            // Write the low byte to memory at [SP]
            cpu.bus.borrow_mut().write(sp, low);
            // Next cycle overlaps with fetching the following opcode.
        },
        MicroOp::Pop { dst } => {
            // ─── M2: Read low byte from stack
            // 1. Get current SP
            let sp_addr = cpu.register_file.get_16bit(&Reg16::SP);
            // 2. Read the low byte from memory[SP]
            let low = cpu.bus.borrow_mut().read(sp_addr);
            // 3. Increment SP by 1
            cpu.register_file.inc16(&Reg16::SP);
            // ─── M3: Read high byte from stack
            // 4. Get updated SP
            let sp_addr = cpu.register_file.get_16bit(&Reg16::SP);
            // 5. Read the high byte from memory[SP]
            let high = cpu.bus.borrow_mut().read(sp_addr);
            // 6. Increment SP by 1 again
            cpu.register_file.inc16(&Reg16::SP);
            // ─── Combine into a 16-bit value (little endian)
            let value = u16::from_le_bytes([low, high]);
            // ─── Write result into the destination register
            cpu.register_file.set_16bit(&dst, value);
            // ─── Special case: POP AF must mask F’s low nibble
            // On real hardware, the lower 4 bits of F are always zero; if dst == AF,
            // clear any stray low bits from the popped value.
            if let Reg16::AF = dst {
                let f = cpu.register_file.get_8bit(&Reg8::F) & 0xF0;
                cpu.register_file.set_8bit(&Reg8::F, f);
            }
        },

        // Arithmetic & Logic (8-bit) --------------------------------------------------
        MicroOp::Alu8 { kind, dest, src } => {
            // 1) Read operands
            let a = cpu.register_file.get_8bit(&dest);
            let b = match src {
                Operand8::Reg(r) => cpu.register_file.get_8bit(&r),
                Operand8::Imm(v)   => v,
            };
            // 2) Compute ALU result + flags
            let AluResult { value, z, n, h, c } = alu(kind, a, b);
            // 3) Write back result
            cpu.register_file.set_8bit(&dest, value);
            // 4) Pack flags into F
            let mut f = 0;
            if z { f |= FLAG_Z; }
            if n { f |= FLAG_N; }
            if h { f |= FLAG_H; }
            if c { f |= FLAG_C; }
            cpu.register_file.set_8bit(&Reg8::F, f);
        },
        MicroOp::Daa                         => todo!(),
        MicroOp::Cpl                         => todo!(),
        MicroOp::Scf                         => todo!(),
        MicroOp::Ccf                         => todo!(),
        MicroOp::Inc8 { target }             => todo!(),
        MicroOp::Dec8 { target }             => todo!(),

        // 16-bit Arithmetic
        MicroOp::Add16 { dest, src }         => todo!(),
        MicroOp::AddSpE { e }                => todo!(),
        MicroOp::Inc16 { reg }               => todo!(),
        MicroOp::Dec16 { reg }               => todo!(),

        // Rotate, Shift & Bit Ops
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

        // Control Flow
        MicroOp::CheckCond  { cond }    => todo!(),
        MicroOp::JumpAbs    { addr }    => todo!(),
        MicroOp::JumpRel    { offset }  => todo!(),
        MicroOp::Call       { addr }    => todo!(),
        MicroOp::Ret                    => todo!(),
        MicroOp::RetI                   => todo!(),
        MicroOp::Rst        { addr }    => todo!(),
        MicroOp::FetchOpcode            => todo!(),
        MicroOp::DecodeCb   { prefix }  => todo!(),

        // System & Interrupt State
        MicroOp::SetFlag        { flag, value } => todo!(),
        MicroOp::GetFlag        { flag }        => todo!(),
        MicroOp::SetIme         { value }       => todo!(),
        MicroOp::DeferImeEnable                 => todo!(),
        MicroOp::TriggerHalt                    => todo!(),
        MicroOp::TriggerStop                    => todo!(),
        MicroOp::CheckInterrupts                => todo!(),
    }
}
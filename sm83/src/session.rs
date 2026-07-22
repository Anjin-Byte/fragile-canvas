/// High-level emulator session.
///
/// Owns the `GameBoy` system and a `ClockGovernor`, providing
/// the full lifecycle (load → tick → read state → reset) as a
/// single testable API.  Frontend crates (Tauri, WASM) wrap this
/// with their serialization layer and expose it over their FFI boundary.

use crate::clock::ClockGovernor;
use crate::cpu::registers::{Reg8, Reg16};
use crate::memory::bus::Bus;
use crate::memory::cartridge::Cartridge;
use crate::system::GameBoy;
use crate::trace::Tracer;

/// Why a bounded snippet run (`run_code`) stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// The CPU executed `HALT`.
    Halt,
    /// PC reached an enforced breakpoint address.
    Breakpoint,
    /// PC left the loaded code range `[origin, end)`.
    LeftRange,
    /// The instruction jumped to itself (`jr $` / `jp $`) — a terminal loop.
    SelfLoop,
    /// The instruction budget was exhausted.
    Budget,
}

/// Result of a bounded snippet run.
pub struct RunResult {
    pub snapshot: CpuSnapshot,
    pub reason: StopReason,
    pub steps: u32,
}

/// Result of a real-time slice (`tick_frame_until`) that may stop early at a
/// breakpoint.
pub struct TickResult {
    pub snapshot: CpuSnapshot,
    /// True iff the slice stopped BEFORE executing an instruction whose PC is
    /// an enforced breakpoint (the machine is now paused at that PC).
    pub hit: bool,
    /// Instructions actually executed this slice.
    pub steps: u32,
}

/// Snapshot of CPU register state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuSnapshot {
    pub pc: u16,
    pub sp: u16,
    pub af: u16,
    pub bc: u16,
    pub de: u16,
    pub hl: u16,
    pub ir: u8,
    pub ie: u8,
    pub halted: bool,
}

fn snapshot(gb: &GameBoy) -> CpuSnapshot {
    let regs = &gb.cpu.register_file;
    CpuSnapshot {
        pc: regs.get_16bit(Reg16::PC),
        sp: regs.get_16bit(Reg16::SP),
        af: regs.get_16bit(Reg16::AF),
        bc: regs.get_16bit(Reg16::BC),
        de: regs.get_16bit(Reg16::DE),
        hl: regs.get_16bit(Reg16::HL),
        ir: regs.get_8bit(Reg8::IR),
        // IE is memory-mapped at 0xFFFF — read from the bus so the displayed
        // value reflects what the game actually wrote, not the stale register
        // file field.
        ie: gb.bus.read(0xFFFF),
        halted: gb.cpu.halted,
    }
}

pub struct Session {
    gb: Option<GameBoy>,
    gov: ClockGovernor,
}

impl Session {
    pub fn new() -> Self {
        Self {
            gb: None,
            gov: ClockGovernor::new(),
        }
    }

    /// Load a cartridge ROM (with the built-in boot ROM).
    pub fn load_rom(&mut self, cart_rom: &[u8]) -> CpuSnapshot {
        let mut bus = Bus::new();
        bus.load_boot_rom(crate::BOOT_ROM).unwrap();
        bus.load_cartridge(cart_rom);
        let gb = GameBoy::new(bus, Tracer::off());
        let snap = snapshot(&gb);
        self.gb = Some(gb);
        self.gov = ClockGovernor::new();
        snap
    }

    /// Load the bundled default ROM.
    pub fn load_default_rom(&mut self) -> CpuSnapshot {
        self.load_rom(crate::DEFAULT_ROM)
    }

    /// List all bundled ROMs (id, title, author).
    pub fn list_bundled_roms() -> Vec<(&'static str, &'static str, &'static str)> {
        crate::BUNDLED_ROMS
            .iter()
            .map(|r| (r.id, r.title, r.author))
            .collect()
    }

    /// Load a bundled ROM by id. Returns Err if the id is not found.
    pub fn load_bundled_rom(&mut self, id: &str) -> Result<CpuSnapshot, &'static str> {
        let rom = crate::BUNDLED_ROMS
            .iter()
            .find(|r| r.id == id)
            .ok_or("unknown bundled ROM id")?;
        Ok(self.load_rom(rom.data))
    }

    /// Load a bundled ROM by id, skipping the boot ROM (starts at PC=0x0100).
    pub fn load_bundled_rom_no_boot(&mut self, id: &str) -> Result<CpuSnapshot, &'static str> {
        let rom = crate::BUNDLED_ROMS
            .iter()
            .find(|r| r.id == id)
            .ok_or("unknown bundled ROM id")?;
        Ok(self.load_rom_no_boot(rom.data))
    }

    /// Step by N M-cycles (manual stepping, ignores governor).
    pub fn step(&mut self, m_cycles: u32) -> Result<CpuSnapshot, &'static str> {
        let gb = self.gb.as_mut().ok_or("no ROM loaded")?;
        gb.tick_n(m_cycles);
        Ok(snapshot(gb))
    }

    /// Execute exactly one instruction and return a detailed trace record.
    ///
    /// Captures the full CPU + I/O state before the instruction, the opcode
    /// bytes, and the state after.  Designed for instruction-level debugging.
    pub fn step_traced(&mut self) -> Result<InstrTrace, &'static str> {
        let gb = self.gb.as_mut().ok_or("no ROM loaded")?;

        // Capture state BEFORE the instruction
        let pc_before = gb.cpu.register_file.get_16bit(Reg16::PC);
        let snap_before = snapshot(gb);
        let if_before = gb.bus.if_reg;
        let ie_before = gb.bus.read(0xFFFF);
        let ime_before = gb.cpu.ime;
        let halted_before = gb.cpu.halted;

        // Read up to 3 bytes at PC for disassembly context
        let op0 = gb.bus.read(pc_before);
        let op1 = gb.bus.read(pc_before.wrapping_add(1));
        let op2 = gb.bus.read(pc_before.wrapping_add(2));

        // Execute one instruction
        let t_cycles = gb.tick();

        // Capture state AFTER
        let snap_after = snapshot(gb);
        let if_after = gb.bus.if_reg;
        let ime_after = gb.cpu.ime;
        let halted_after = gb.cpu.halted;

        Ok(InstrTrace {
            pc: pc_before,
            opcode: [op0, op1, op2],
            t_cycles,
            af_before: snap_before.af,
            af_after: snap_after.af,
            bc: snap_after.bc,
            de: snap_after.de,
            hl: snap_after.hl,
            sp: snap_after.sp,
            pc_after: snap_after.pc,
            if_before,
            if_after,
            ie: ie_before,
            ime_before,
            ime_after,
            halted_before,
            halted_after,
        })
    }

    /// Governed tick: convert wall-clock elapsed nanoseconds into the
    /// correct number of T-cycles and execute them.
    pub fn tick_frame(&mut self, elapsed_ns: u64) -> Result<CpuSnapshot, &'static str> {
        let gb = self.gb.as_mut().ok_or("no ROM loaded")?;
        let cycles = self.gov.cycles_due(elapsed_ns);
        if cycles > 0 {
            gb.tick_t(cycles);
        }
        Ok(snapshot(gb))
    }

    /// Read current CPU state without advancing.
    pub fn cpu_snapshot(&self) -> Result<CpuSnapshot, &'static str> {
        let gb = self.gb.as_ref().ok_or("no ROM loaded")?;
        Ok(snapshot(gb))
    }

    /// Read a range of memory bytes.
    pub fn read_memory(&self, addr: u16, length: u16) -> Result<Vec<u8>, &'static str> {
        let gb = self.gb.as_ref().ok_or("no ROM loaded")?;
        let end = addr.saturating_add(length);
        Ok((addr..end).map(|a| gb.bus.read(a)).collect())
    }

    /// Set joypad button state from the frontend.
    ///
    /// `action`: bitmask of pressed action buttons (A=1, B=2, Select=4, Start=8).
    /// `direction`: bitmask of pressed direction buttons (Right=1, Left=2, Up=4, Down=8).
    ///
    /// Call this whenever keyboard/controller state changes. The emulator
    /// reads the state when the game polls 0xFF00.
    pub fn set_buttons(&mut self, action: u8, direction: u8) {
        if let Some(gb) = self.gb.as_mut() {
            gb.bus.joypad.set_state(action, direction);
        }
    }

    /// Reset the governor accumulator (call after pause/resume).
    pub fn reset_governor(&mut self) {
        self.gov.reset();
    }

    /// Drop the loaded ROM and reset everything.
    pub fn reset(&mut self) {
        self.gb = None;
        self.gov = ClockGovernor::new();
    }

    /// Access the inner `GameBoy` (for tracer toggle, etc.).
    pub fn gameboy(&self) -> Option<&GameBoy> {
        self.gb.as_ref()
    }

    /// Mutable access to the inner `GameBoy`.
    pub fn gameboy_mut(&mut self) -> Option<&mut GameBoy> {
        self.gb.as_mut()
    }

    /// Drain the latest completed frame from the PPU framebuffer.
    ///
    /// Returns `Some(Vec<u8>)` containing 160×144 shade indices (0-3, one byte
    /// per pixel, row-major) when a new frame is ready, then clears the flag.
    /// Returns `None` if no ROM is loaded or the PPU has not completed a frame
    /// since the last call.
    pub fn drain_frame(&mut self) -> Option<Vec<u8>> {
        let gb = self.gb.as_mut()?;
        if !gb.bus.ppu.frame_ready {
            return None;
        }
        gb.bus.ppu.frame_ready = false;
        Some(gb.bus.ppu.completed_frame.to_vec())
    }

    /// Drain all buffered audio samples (interleaved L,R,L,R... f32).
    /// Returns an empty vec if no ROM is loaded.
    pub fn drain_audio_samples(&mut self) -> Vec<f32> {
        let mut out = Vec::new();
        if let Some(gb) = self.gb.as_mut() {
            gb.bus.apu.drain_audio_samples(&mut out);
        }
        out
    }

    /// Return the serial port output buffer as a UTF-8 string (lossy).
    ///
    /// Blargg test ROMs write their pass/fail results to the serial port
    /// as ASCII text, so this is the primary way to check test outcomes.
    pub fn serial_output_as_string(&self) -> String {
        match self.gb.as_ref() {
            Some(gb) => String::from_utf8_lossy(&gb.bus.serial.output_buffer).into_owned(),
            None => String::new(),
        }
    }

    /// Load a cartridge ROM, skipping the boot ROM entirely.
    ///
    /// Sets CPU registers and bus state to the exact DMG post-boot values
    /// so execution starts at PC=0x0100.  This saves ~2.2M M-cycles of
    /// boot ROM logo animation per load.
    pub fn load_rom_no_boot(&mut self, cart_rom: &[u8]) -> CpuSnapshot {
        self.boot_skipped_with_cart(Cartridge::new(cart_rom))
    }

    /// Bring the machine up in the exact DMG post-boot state with `cart`
    /// installed and the boot ROM skipped (PC=0x0100). Shared by
    /// `load_rom_no_boot` and `load_code`.
    fn boot_skipped_with_cart(&mut self, cart: Cartridge) -> CpuSnapshot {
        let mut bus = Bus::new();
        bus.install_cartridge(cart);
        // Unmap the boot ROM (write bit 0 to 0xFF50)
        bus.write(0xFF50, 0x01);

        // DMG/MGB post-boot I/O register state (from Pandocs Power Up Sequence)
        bus.if_reg = 0x01;              // IF  = VBlank (upper bits read as 1 via bus mask)
        bus.timer.set_system_counter(0xABCC); // DIV = 0xAB after boot ROM
        bus.write(0xFF00, 0xCF);        // P1  (joypad)
        bus.write(0xFF02, 0x7E);        // SC  (serial control)
        bus.write(0xFF07, 0xF8);        // TAC (timer control — disabled, upper bits set)
        bus.write(0xFF40, 0x91);        // LCDC (LCD on, BG on, window off, OBJ off)
        bus.write(0xFF41, 0x85);        // STAT
        bus.write(0xFF47, 0xFC);        // BGP (background palette)

        let mut gb = GameBoy::new(bus, Tracer::off());
        // DMG post-boot CPU register state (verified against hardware)
        gb.cpu.register_file.set_8bit(Reg8::A, 0x01);
        gb.cpu.register_file.set_8bit(Reg8::F, 0xB0);
        gb.cpu.register_file.set_8bit(Reg8::B, 0x00);
        gb.cpu.register_file.set_8bit(Reg8::C, 0x13);
        gb.cpu.register_file.set_8bit(Reg8::D, 0x00);
        gb.cpu.register_file.set_8bit(Reg8::E, 0xD8);
        gb.cpu.register_file.set_8bit(Reg8::H, 0x01);
        gb.cpu.register_file.set_8bit(Reg8::L, 0x4D);
        gb.cpu.register_file.set_16bit(Reg16::SP, 0xFFFE);
        gb.cpu.register_file.set_16bit(Reg16::PC, 0x0100);
        let snap = snapshot(&gb);
        self.gb = Some(gb);
        self.gov = ClockGovernor::new();
        snap
    }

    /// Assemble output → a fresh boot-skipped ROM-only machine with the flattened
    /// `bytes` placed at `origin` and PC set to `entry`. `origin + bytes.len()`
    /// must fit the 32 KiB ROM window. Header bytes are not parsed, so code may
    /// occupy 0x0147/0x0148 without changing the MBC or ROM size.
    pub fn load_code(
        &mut self,
        origin: u16,
        bytes: &[u8],
        entry: u16,
    ) -> Result<CpuSnapshot, &'static str> {
        if origin as usize + bytes.len() > 0x8000 {
            return Err("code exceeds the 32 KiB ROM window");
        }
        let mut image = vec![0xFFu8; 0x8000];
        let start = origin as usize;
        image[start..start + bytes.len()].copy_from_slice(bytes);
        self.boot_skipped_with_cart(Cartridge::rom_only(&image));
        if let Some(gb) = self.gb.as_mut() {
            gb.cpu.register_file.set_16bit(Reg16::PC, entry);
        }
        self.cpu_snapshot()
    }

    /// Run from the current PC until a stop condition, one full instruction per
    /// step. `[lo, hi)` is the loaded code range; leaving it stops. Enforces the
    /// given breakpoint addresses (checked before each instruction; the entry
    /// instruction is exempt so a resume makes progress). Returns why it stopped.
    pub fn run_code(
        &mut self,
        budget: u32,
        breakpoints: &[u16],
        lo: u16,
        hi: u16,
    ) -> Result<RunResult, &'static str> {
        let gb = self.gb.as_mut().ok_or("no ROM loaded")?;
        let mut steps: u32 = 0;
        let reason = loop {
            let pc = gb.cpu.register_file.get_16bit(Reg16::PC);
            if gb.cpu.halted {
                break StopReason::Halt;
            }
            if Self::is_breakpoint(pc, breakpoints, steps == 0) {
                break StopReason::Breakpoint;
            }
            if pc < lo || pc >= hi {
                break StopReason::LeftRange;
            }
            if steps >= budget {
                break StopReason::Budget;
            }
            gb.tick();
            steps += 1;
            if gb.cpu.register_file.get_16bit(Reg16::PC) == pc {
                break StopReason::SelfLoop; // jr $ / jp $ — terminal self-loop
            }
        };
        Ok(RunResult {
            snapshot: snapshot(gb),
            reason,
            steps,
        })
    }

    /// The single breakpoint rule both engines share: PC is an enforced
    /// breakpoint, unless it's the exempt first instruction of a run (so a
    /// resume makes progress instead of instantly re-tripping the breakpoint it
    /// paused on).
    #[inline]
    fn is_breakpoint(pc: u16, breakpoints: &[u16], is_first: bool) -> bool {
        !is_first && breakpoints.contains(&pc)
    }

    /// Governed real-time tick that honors breakpoints (the free-run break
    /// engine). Converts `elapsed_ns` to a T-cycle budget, then executes whole
    /// instructions until the budget is spent (`hit=false`) or PC reaches an
    /// enforced breakpoint (`hit=true`, paused *before* executing it).
    ///
    /// `exempt_first` skips the breakpoint check for the first executed
    /// instruction (used on resume). Empty `breakpoints` takes a zero-overhead
    /// fast path identical to [`tick_frame`](Self::tick_frame). Unlike
    /// `run_code`, HALT / self-loops / leaving any range are NORMAL here — only
    /// a breakpoint pauses (every real ROM HALTs each frame and sits in
    /// wait-loops).
    pub fn tick_frame_until(
        &mut self,
        elapsed_ns: u64,
        breakpoints: &[u16],
        exempt_first: bool,
    ) -> Result<TickResult, &'static str> {
        let gb = self.gb.as_mut().ok_or("no ROM loaded")?;
        let cycles = self.gov.cycles_due(elapsed_ns);

        // Zero-breakpoint fast path: one blast, identical to `tick_frame`.
        if breakpoints.is_empty() {
            if cycles > 0 {
                gb.tick_t(cycles);
            }
            return Ok(TickResult { snapshot: snapshot(gb), hit: false, steps: 0 });
        }

        let mut spent: u32 = 0;
        let mut executed: u32 = 0;
        let hit = loop {
            if spent >= cycles {
                break false;
            }
            let pc = gb.cpu.register_file.get_16bit(Reg16::PC);
            let is_first = exempt_first && executed == 0;
            // Don't stop while parked in HALT (PC is frozen post-HALT): let
            // cycles pass so the waking interrupt can fire. We break only when
            // execution truly arrives at that PC.
            if !gb.cpu.halted && Self::is_breakpoint(pc, breakpoints, is_first) {
                break true;
            }
            spent += gb.tick() as u32;
            executed += 1;
        };
        Ok(TickResult { snapshot: snapshot(gb), hit, steps: executed })
    }

    /// Start logging all bus accesses to `addr` (reads/writes/internal sets).
    pub fn watch_bus(&mut self, addr: u16) {
        if let Some(gb) = self.gb.as_mut() {
            gb.bus.watch(addr);
        }
    }

    /// Stop bus watch logging.
    pub fn unwatch_bus(&mut self) {
        if let Some(gb) = self.gb.as_mut() {
            gb.bus.unwatch();
        }
    }

    /// Drain accumulated bus access log entries.
    pub fn drain_bus_log(&mut self) -> Vec<crate::memory::bus::BusAccessLog> {
        match self.gb.as_mut() {
            Some(gb) => gb.bus.drain_access_log(),
            None => Vec::new(),
        }
    }

    /// Snapshot all relevant I/O register state in one call.
    pub fn io_snapshot(&self) -> Result<IoSnapshot, &'static str> {
        let gb = self.gb.as_ref().ok_or("no ROM loaded")?;
        Ok(IoSnapshot {
            if_reg: gb.bus.if_reg,
            ie: gb.bus.read(0xFFFF),
            lcdc: gb.bus.read(0xFF40),
            stat: gb.bus.read(0xFF41),
            ly: gb.bus.read(0xFF44),
            lyc: gb.bus.read(0xFF45),
            div: gb.bus.read(0xFF04),
            tima: gb.bus.read(0xFF05),
            tma: gb.bus.read(0xFF06),
            tac: gb.bus.read(0xFF07),
            sb: gb.bus.read(0xFF01),
            sc: gb.bus.read(0xFF02),
            joyp: gb.bus.read(0xFF00),
            ime: gb.cpu.ime,
        })
    }
}

/// Trace record for a single instruction execution.
#[derive(Debug, Clone)]
pub struct InstrTrace {
    /// PC at instruction start.
    pub pc: u16,
    /// Up to 3 opcode bytes at PC (for disassembly context).
    pub opcode: [u8; 3],
    /// T-cycles consumed by this instruction.
    pub t_cycles: u8,
    /// AF before and after.
    pub af_before: u16,
    pub af_after: u16,
    /// Registers after execution.
    pub bc: u16,
    pub de: u16,
    pub hl: u16,
    pub sp: u16,
    /// PC after execution (next instruction).
    pub pc_after: u16,
    /// IF register before and after.
    pub if_before: u8,
    pub if_after: u8,
    /// IE register.
    pub ie: u8,
    /// IME before and after.
    pub ime_before: bool,
    pub ime_after: bool,
    /// Halted state before and after.
    pub halted_before: bool,
    pub halted_after: bool,
}

impl std::fmt::Display for InstrTrace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Format: [PC] OP OP OP  Tcyc  AF→AF  BC   DE   HL   SP   IF→IF IE IME  flags
        let halt_marker = if self.halted_after && !self.halted_before {
            " →HALT"
        } else if self.halted_before && !self.halted_after {
            " WAKE←"
        } else if self.halted_before {
            " (halt)"
        } else {
            ""
        };

        let ime_change = if self.ime_before != self.ime_after {
            format!(" IME:{}→{}", self.ime_before as u8, self.ime_after as u8)
        } else {
            format!(" IME:{}", self.ime_before as u8)
        };

        let if_change = if self.if_before != self.if_after {
            format!("IF:{:02X}→{:02X}", self.if_before, self.if_after)
        } else {
            format!("IF:{:02X}", self.if_before)
        };

        write!(
            f,
            "[{:04X}] {:02X} {:02X} {:02X}  {:>2}T  AF:{:04X}→{:04X} BC:{:04X} DE:{:04X} HL:{:04X} SP:{:04X}  {} IE:{:02X}{}{}",
            self.pc,
            self.opcode[0], self.opcode[1], self.opcode[2],
            self.t_cycles,
            self.af_before, self.af_after,
            self.bc, self.de, self.hl, self.sp,
            if_change, self.ie,
            ime_change,
            halt_marker,
        )
    }
}

/// Snapshot of I/O register state for debugging.
#[derive(Debug, Clone)]
pub struct IoSnapshot {
    pub if_reg: u8,
    pub ie: u8,
    pub lcdc: u8,
    pub stat: u8,
    pub ly: u8,
    pub lyc: u8,
    pub div: u8,
    pub tima: u8,
    pub tma: u8,
    pub tac: u8,
    pub sb: u8,
    pub sc: u8,
    pub joyp: u8,
    pub ime: bool,
}

impl std::fmt::Display for IoSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "IF={:02X} IE={:02X} IME={} | LCDC={:02X} STAT={:02X} LY={:02X} LYC={:02X} | \
             DIV={:02X} TIMA={:02X} TMA={:02X} TAC={:02X} | SB={:02X} SC={:02X} | P1={:02X}",
            self.if_reg, self.ie, self.ime as u8,
            self.lcdc, self.stat, self.ly, self.lyc,
            self.div, self.tima, self.tma, self.tac,
            self.sb, self.sc,
            self.joyp,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_session_has_no_rom() {
        let s = Session::new();
        assert!(s.cpu_snapshot().is_err());
        assert!(s.read_memory(0, 1).is_err());
    }

    #[test]
    fn load_code_runs_snippet_to_halt() {
        let mut s = Session::new();
        // LD A,$05 ; INC A ; LD B,A ; HALT   placed at 0x0150.
        let code = [0x3E, 0x05, 0x3C, 0x47, 0x76];
        s.load_code(0x0150, &code, 0x0150).unwrap();
        let r = s
            .run_code(1000, &[], 0x0150, 0x0150 + code.len() as u16)
            .unwrap();
        assert_eq!(r.reason, StopReason::Halt);
        assert_eq!(r.snapshot.af >> 8, 0x06); // A = 5 + 1
        assert_eq!(r.snapshot.bc >> 8, 0x06); // B = A
        assert_eq!(r.steps, 4);
    }

    #[test]
    fn run_code_self_loop_returns_immediately() {
        let mut s = Session::new();
        // jr $ (18 FE) jumps to itself — must stop at once, not spin the budget.
        s.load_code(0x0150, &[0x18, 0xFE], 0x0150).unwrap();
        let r = s.run_code(1_000_000, &[], 0x0150, 0x0152).unwrap();
        assert_eq!(r.reason, StopReason::SelfLoop);
        assert!(r.steps <= 2, "steps = {}", r.steps);
    }

    #[test]
    fn run_code_stops_at_breakpoint() {
        let mut s = Session::new();
        // NOP ; NOP ; NOP ; HALT at 0x0150 — breakpoint at 0x0152.
        s.load_code(0x0150, &[0x00, 0x00, 0x00, 0x76], 0x0150).unwrap();
        let r = s.run_code(1000, &[0x0152], 0x0150, 0x0154).unwrap();
        assert_eq!(r.reason, StopReason::Breakpoint);
        assert_eq!(r.snapshot.pc, 0x0152); // stops before executing the bp instruction
    }

    #[test]
    fn load_code_rejects_oversized() {
        let mut s = Session::new();
        assert!(s.load_code(0x7FFF, &[0x00, 0x00, 0x00], 0x7FFF).is_err());
    }

    #[test]
    fn load_code_ignores_header_bytes_at_origin_zero() {
        // A program long enough to cover 0x0147/0x0148 must still be ROM-only
        // and read back verbatim (the rom_only path never parses the header).
        let mut s = Session::new();
        let mut code = vec![0x00u8; 0x0200]; // 512 NOPs, covers header region
        code[0x0147] = 0x19; // would be MBC5 if the header were parsed
        code[0x0148] = 0x08; // would demand 512 ROM banks if parsed
        s.load_code(0x0000, &code, 0x0000).unwrap();
        // Byte at 0x0147 reads back as written (proves rom_only, not MBC5 aliasing).
        assert_eq!(s.read_memory(0x0147, 1).unwrap(), vec![0x19]);
    }

    #[test]
    fn load_rom_returns_initial_state() {
        let mut s = Session::new();
        let snap = s.load_default_rom();
        // Boot ROM starts execution at PC=0x0000
        assert_eq!(snap.pc, 0x0000);
        assert!(!snap.halted);
    }

    #[test]
    fn step_advances_cpu() {
        let mut s = Session::new();
        s.load_default_rom();
        let before = s.cpu_snapshot().unwrap();
        let after = s.step(10).unwrap();
        // PC should have moved after 10 M-cycles
        assert_ne!(before.pc, after.pc);
    }

    #[test]
    fn step_without_rom_errors() {
        let mut s = Session::new();
        assert_eq!(s.step(1).unwrap_err(), "no ROM loaded");
    }

    #[test]
    fn tick_frame_without_rom_errors() {
        let mut s = Session::new();
        assert_eq!(s.tick_frame(16_000_000).unwrap_err(), "no ROM loaded");
    }

    #[test]
    fn tick_frame_zero_elapsed_does_not_advance() {
        let mut s = Session::new();
        s.load_default_rom();
        let before = s.cpu_snapshot().unwrap();
        let after = s.tick_frame(0).unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn tick_frame_advances_cpu() {
        let mut s = Session::new();
        s.load_default_rom();
        let before = s.cpu_snapshot().unwrap();
        // ~16ms frame = 16_000_000 ns → ~67,108 T-cycles
        let after = s.tick_frame(16_000_000).unwrap();
        assert_ne!(before.pc, after.pc);
    }

    #[test]
    fn read_memory_returns_correct_length() {
        let mut s = Session::new();
        s.load_default_rom();
        let data = s.read_memory(0x0000, 16).unwrap();
        assert_eq!(data.len(), 16);
    }

    #[test]
    fn read_memory_saturates_at_boundary() {
        let mut s = Session::new();
        s.load_default_rom();
        // Start near end of address space
        let data = s.read_memory(0xFFF0, 0x20).unwrap();
        // saturating_add(0xFFF0, 0x20) = 0xFFFF, so 0xFFFF - 0xFFF0 = 15 bytes
        assert_eq!(data.len(), 0xFFFF_u16.wrapping_sub(0xFFF0) as usize);
    }

    #[test]
    fn reset_clears_session() {
        let mut s = Session::new();
        s.load_default_rom();
        assert!(s.cpu_snapshot().is_ok());
        s.reset();
        assert!(s.cpu_snapshot().is_err());
    }

    #[test]
    fn load_rom_resets_governor() {
        let mut s = Session::new();
        s.load_default_rom();
        // Accumulate some governor state
        s.tick_frame(16_000_000).unwrap();
        // Loading a new ROM should reset the governor
        s.load_default_rom();
        // Zero elapsed should yield zero cycles (no leftover accumulator)
        let before = s.cpu_snapshot().unwrap();
        let after = s.tick_frame(0).unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn reset_governor_clears_accumulator() {
        let mut s = Session::new();
        s.load_default_rom();
        // Feed a sub-cycle amount to build up fractional accumulator
        s.tick_frame(100).unwrap();
        s.reset_governor();
        // After reset, zero elapsed should not produce any cycles
        let before = s.cpu_snapshot().unwrap();
        let after = s.tick_frame(0).unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn gameboy_accessors() {
        let mut s = Session::new();
        assert!(s.gameboy().is_none());
        assert!(s.gameboy_mut().is_none());
        s.load_default_rom();
        assert!(s.gameboy().is_some());
        assert!(s.gameboy_mut().is_some());
    }

    #[test]
    fn snapshot_reads_all_registers() {
        let mut s = Session::new();
        s.load_default_rom();
        let snap = s.cpu_snapshot().unwrap();
        // Just verify all fields are accessible and the struct is well-formed.
        // At boot, SP starts at 0x0000 (before boot ROM sets it to 0xFFFE).
        assert_eq!(snap.sp, 0x0000);
        let _ = (snap.af, snap.bc, snap.de, snap.hl, snap.ir, snap.ie, snap.halted);
    }

    #[test]
    fn multiple_tick_frames_accumulate_correctly() {
        let mut s = Session::new();
        s.load_default_rom();
        // Simulate 60 frames at ~16.6ms each
        for _ in 0..60 {
            s.tick_frame(16_666_667).unwrap();
        }
        // After ~1 second of emulated time, PC should be deep into execution
        let snap = s.cpu_snapshot().unwrap();
        assert_ne!(snap.pc, 0x0000);
    }

    // ─── Real-time break engine (tick_frame_until) ──────────────────────────

    #[test]
    fn tick_frame_until_stops_before_breakpoint() {
        let mut s = Session::new();
        s.load_code(0x0150, &[0x00, 0x00, 0x00, 0x76], 0x0150).unwrap(); // NOP;NOP;NOP;HALT
        let r = s.tick_frame_until(50_000, &[0x0152], false).unwrap();
        assert!(r.hit);
        assert_eq!(r.snapshot.pc, 0x0152); // paused BEFORE executing the bp instruction
        assert_eq!(r.steps, 2);
    }

    #[test]
    fn tick_frame_until_exempt_first_runs_through_entry() {
        // Breakpoint AT the entry PC, but `exempt_first` → the entry instruction
        // executes and we do NOT stop on it (resume semantics).
        let mut s = Session::new();
        s.load_code(0x0150, &[0x3C, 0x18, 0xFE], 0x0150).unwrap(); // INC A ; JR $
        let r = s.tick_frame_until(50_000, &[0x0150], true).unwrap();
        assert!(!r.hit, "the exempted entry breakpoint must not stop the run");
        assert_eq!(r.snapshot.af >> 8, 0x02, "INC A ran (post-boot A $01 + 1)");
    }

    #[test]
    fn tick_frame_until_no_breakpoints_matches_tick_frame() {
        // The empty-breakpoint fast path is byte-identical to tick_frame.
        let code = [0x06, 0x00, 0x04, 0x18, 0xFD]; // LD B,0 ; loop: INC B ; JR loop
        let mut a = Session::new();
        a.load_code(0x0150, &code, 0x0150).unwrap();
        let mut b = Session::new();
        b.load_code(0x0150, &code, 0x0150).unwrap();
        let ra = a.tick_frame(50_000).unwrap();
        let rb = b.tick_frame_until(50_000, &[], false).unwrap();
        assert_eq!(ra, rb.snapshot);
        assert!(!rb.hit);
    }

    #[test]
    fn tick_frame_until_does_not_stop_on_self_loop_or_halt() {
        // A self-loop is normal in real time (not a stop condition).
        let mut s = Session::new();
        s.load_code(0x0150, &[0x18, 0xFE], 0x0150).unwrap(); // JR $
        assert!(!s.tick_frame_until(50_000, &[0x0999], false).unwrap().hit);
        // DI;HALT on the stepped path (non-matching bp) must terminate, not spin.
        let mut s2 = Session::new();
        s2.load_code(0x0150, &[0xF3, 0x76], 0x0150).unwrap(); // DI ; HALT
        let r = s2.tick_frame_until(50_000, &[0x0999], false).unwrap();
        assert!(!r.hit);
        assert!(r.snapshot.halted);
    }

    #[test]
    fn tick_frame_until_zero_budget_is_noop() {
        let mut s = Session::new();
        s.load_code(0x0150, &[0x3C, 0x76], 0x0150).unwrap();
        let before = s.cpu_snapshot().unwrap();
        let r = s.tick_frame_until(0, &[0x0150], false).unwrap(); // PC is the bp, but budget 0
        assert_eq!(r.steps, 0);
        assert!(!r.hit);
        assert_eq!(r.snapshot, before);
    }

    #[test]
    fn tick_frame_until_resume_makes_progress() {
        let mut s = Session::new();
        s.load_code(0x0150, &[0x00, 0x3C, 0x76], 0x0150).unwrap(); // NOP ; INC A ; HALT
        // First slice stops before the bp at 0x0151.
        let r1 = s.tick_frame_until(50_000, &[0x0151], false).unwrap();
        assert!(r1.hit);
        assert_eq!(r1.snapshot.pc, 0x0151);
        // Resume with exemption → INC A runs, no immediate re-trip, reaches HALT.
        let r2 = s.tick_frame_until(50_000, &[0x0151], true).unwrap();
        assert!(!r2.hit);
        assert_eq!(r2.snapshot.af >> 8, 0x02);
        assert!(r2.snapshot.halted);
    }
}

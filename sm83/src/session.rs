/// High-level emulator session.
///
/// Owns the `GameBoy` system and a `ClockGovernor`, providing
/// the full lifecycle (load → tick → read state → reset) as a
/// single testable API.  Frontend crates (Tauri, WASM) wrap this
/// with their serialization layer and expose it over their FFI boundary.

use crate::clock::ClockGovernor;
use crate::cpu::pipeline::PipelineState;
use crate::cpu::registers::{Reg8, Reg16};
use crate::memory::bus::Bus;
use crate::system::GameBoy;
use crate::trace::Tracer;

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
        ie: regs.get_8bit(Reg8::IE),
        halted: matches!(gb.cpu.state, PipelineState::Halted),
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

    /// Step by N M-cycles (manual stepping, ignores governor).
    pub fn step(&mut self, m_cycles: u32) -> Result<CpuSnapshot, &'static str> {
        let gb = self.gb.as_mut().ok_or("no ROM loaded")?;
        gb.tick_n(m_cycles);
        Ok(snapshot(gb))
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
}

/// Top-level Game Boy system.
///
/// Owns the CPU and Bus as sibling fields and orchestrates lock-step
/// ticking of all subsystems.
///
/// `cpu.tick()` executes one full instruction and returns the number of
/// T-cycles consumed.  We then advance the timer, APU, and scanline
/// counter by that many T-cycles.

use crate::cpu::CPU;
use crate::memory::bus::Bus;
use crate::trace::Tracer;

/// T-cycles per scanline.
const DOTS_PER_LINE: u16 = 456;
/// Total scanlines per frame (0-153, V-blank at 144-153).
const LINES_PER_FRAME: u8 = 154;
/// First V-blank scanline.
const VBLANK_LINE: u8 = 144;
/// LY register address.
const LY_ADDR: u16 = 0xFF44;

pub struct GameBoy {
    pub cpu: CPU,
    pub bus: Bus,
    /// Total T-cycles (dots) executed since boot.
    total_dots: u64,
    /// Dot counter within current scanline (0..455).
    scanline_dots: u16,
    /// Current scanline (0..153).
    scanline: u8,
}

impl GameBoy {
    pub fn new(bus: Bus, tracer: Tracer) -> Self {
        Self {
            cpu: CPU::new(tracer),
            bus,
            total_dots: 0,
            scanline_dots: 0,
            scanline: 0,
        }
    }

    /// Execute one CPU instruction and advance all subsystems by the
    /// resulting T-cycle count.  Returns the number of T-cycles consumed.
    pub fn tick(&mut self) -> u8 {
        let t_cycles = self.cpu.tick(&mut self.bus);
        self.advance_subsystems(t_cycles);
        t_cycles
    }

    /// Advance timer, APU, and scanline counter by `t` T-cycles.
    fn advance_subsystems(&mut self, t: u8) {
        for _ in 0..t {
            // Timer ticks every T-cycle.
            self.bus.timer.tick();

            // If the timer raised an interrupt, set the IF bit.
            if self.bus.timer.interrupt_pending {
                self.bus.timer.interrupt_pending = false;
                let if_val = self.bus.read(0xFF0F);
                self.bus.write(0xFF0F, if_val | (1 << 2));
            }

            // APU frame sequencer (clocked by DIV-APU falling edge).
            let div_fell = self.bus.timer.div_apu_fell();
            self.bus.apu.tick(div_fell);

            // Minimal scanline counter (PPU stub).
            self.scanline_dots += 1;
            if self.scanline_dots >= DOTS_PER_LINE {
                self.scanline_dots = 0;
                self.scanline += 1;
                if self.scanline >= LINES_PER_FRAME {
                    self.scanline = 0;
                }
                self.bus.write(LY_ADDR, self.scanline);
                if self.scanline == VBLANK_LINE {
                    let if_val = self.bus.read(0xFF0F);
                    self.bus.write(0xFF0F, if_val | 0x01);
                }
            }
        }
        self.total_dots += t as u64;
    }

    /// Advance by the given number of T-cycles.
    pub fn tick_t(&mut self, dots: u32) {
        let mut remaining = dots;
        while remaining > 0 {
            let t = self.tick();
            remaining = remaining.saturating_sub(t as u32);
        }
    }

    /// Advance by one M-cycle (4 T-cycles).
    /// May overshoot slightly since cpu.tick() returns variable T-cycles.
    pub fn tick_m(&mut self) {
        self.tick();
    }

    /// Run for a given number of M-cycles.
    pub fn tick_n(&mut self, n: u32) {
        let target_t = n * 4;
        self.tick_t(target_t);
    }

    /// Total T-cycles (dots) executed since boot.
    pub fn total_dots(&self) -> u64 {
        self.total_dots
    }

    /// Elapsed emulated time in nanoseconds.
    pub fn elapsed_ns(&self) -> u64 {
        self.total_dots * 1_000_000_000 / crate::timer::MASTER_CLOCK_HZ as u64
    }
}

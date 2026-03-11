/// Top-level Game Boy system.
///
/// Owns the CPU (which owns the MMU/bus) and orchestrates lock-step
/// ticking of all subsystems at T-cycle (dot) granularity.
///
/// Timing hierarchy:
///   `tick()`   = 1 T-cycle (dot) at 2^22 Hz (4,194,304 Hz)
///   `tick_m()` = 1 M-cycle = 4 T-cycles
///
/// The system counter inside the Timer advances every T-cycle.
/// The CPU executes one pipeline step every 4 T-cycles (1 M-cycle).
/// Future subsystems (PPU, APU) will be ticked at their native rates here.

use crate::cpu::CPU;
use crate::memory::bus::Bus;
use crate::timer::T_CYCLES_PER_M;
use crate::trace::Tracer;

pub struct GameBoy {
    pub cpu: CPU,
    pub bus: Bus,
    /// Tracks position within the current M-cycle (0..3).
    dot_phase: u8,
    /// Total T-cycles (dots) executed since boot.
    total_dots: u64,
}

impl GameBoy {
    pub fn new(bus: Bus, tracer: Tracer) -> Self {
        Self {
            cpu: CPU::new(tracer),
            bus,
            dot_phase: 0,
            total_dots: 0,
        }
    }

    /// Advance the entire system by one T-cycle (dot).
    ///
    /// This is the fundamental tick of the emulator.
    /// Every subsystem derives its timing from this.
    pub fn tick(&mut self) {
        // 1. Advance the system counter and timer.
        self.bus.timer.tick();

        // 2. If the timer raised an interrupt, set the IF bit.
        if self.bus.timer.interrupt_pending {
            self.bus.timer.interrupt_pending = false;
            let if_val = self.bus.read(0xFF0F);
            self.bus.write(0xFF0F, if_val | (1 << 2));
        }

        // 3. CPU ticks once per M-cycle (every 4th dot).
        self.dot_phase += 1;
        if self.dot_phase >= T_CYCLES_PER_M as u8 {
            self.dot_phase = 0;
            self.cpu.tick(&mut self.bus);
        }

        // Future: PPU ticks every dot
        // Future: APU checks div_apu_fell() every dot

        self.total_dots += 1;
    }

    /// Advance by the given number of T-cycles (dots).
    pub fn tick_t(&mut self, dots: u32) {
        for _ in 0..dots {
            self.tick();
        }
    }

    /// Advance by one M-cycle (4 T-cycles).
    pub fn tick_m(&mut self) {
        for _ in 0..T_CYCLES_PER_M {
            self.tick();
        }
    }

    /// Run for a given number of M-cycles.
    pub fn tick_n(&mut self, n: u32) {
        for _ in 0..n {
            self.tick_m();
        }
    }

    /// Total T-cycles (dots) executed since boot.
    pub fn total_dots(&self) -> u64 {
        self.total_dots
    }

    /// Elapsed emulated time in nanoseconds.
    pub fn elapsed_ns(&self) -> u64 {
        // 1 T-cycle = 10^9 / 2^22 ns ≈ 238.42 ns
        self.total_dots * 1_000_000_000 / crate::timer::MASTER_CLOCK_HZ as u64
    }
}

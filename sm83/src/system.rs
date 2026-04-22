/// Top-level Game Boy system.
///
/// Owns the CPU and Bus as sibling fields and orchestrates lock-step
/// ticking of all subsystems.
///
/// `cpu.step_m()` executes one M-cycle (4 T-cycles) at a time.  After
/// each M-cycle, all subsystems (timer, PPU, APU, serial, DMA) advance
/// by 4 T-cycles.  This gives cycle-accurate interleaving — subsystems
/// see intermediate CPU state between M-cycles of multi-cycle instructions.

use crate::cpu::pipeline::MCycleResult;
use crate::cpu::CPU;
use crate::memory::bus::Bus;
use crate::trace::Tracer;

pub struct GameBoy {
    pub cpu: CPU,
    pub bus: Bus,
    /// Total T-cycles (dots) executed since boot.
    total_dots: u64,
}

impl GameBoy {
    pub fn new(bus: Bus, tracer: Tracer) -> Self {
        Self {
            cpu: CPU::new(tracer),
            bus,
            total_dots: 0,
        }
    }

    /// Execute one CPU instruction with per-M-cycle subsystem ticking.
    /// Returns the total number of T-cycles consumed.
    ///
    /// Each M-cycle: subsystems advance by 4 T-cycles, then
    /// `cpu.step_m()` does one bus operation.  This matches real DMG
    /// hardware where the CPU sees post-tick subsystem state within
    /// the same M-cycle.
    pub fn tick(&mut self) -> u8 {
        let mut total_t: u8 = 0;
        loop {
            self.advance_subsystems(4);
            let result = self.cpu.step_m(&mut self.bus);
            total_t += 4;
            match result {
                MCycleResult::Continue => continue,
                MCycleResult::InstructionComplete { .. }
                | MCycleResult::HaltBurn => break,
            }
        }
        total_t
    }

    /// Advance timer, APU, PPU, and DMA by `t` T-cycles.
    ///
    /// Subsystem interrupt signals write directly to `bus.if_reg` — the
    /// hardware IF register — rather than going through `bus.read/write()`.
    /// On real hardware these are direct internal SoC connections, not CPU
    /// bus transactions, so they are unaffected by DMA bus conflicts.
    fn advance_subsystems(&mut self, t: u8) {
        for i in 0..t {
            // Timer ticks every T-cycle.
            self.bus.timer.tick();
            if self.bus.timer.interrupt_pending {
                self.bus.timer.interrupt_pending = false;
                self.bus.if_reg |= 1 << 2; // Timer interrupt: IF bit 2
                self.bus.log_if_write(self.bus.if_reg, "timer");
            }

            // Serial port ticks every T-cycle (internal clock only).
            self.bus.serial.tick();
            if self.bus.serial.interrupt_pending {
                self.bus.serial.interrupt_pending = false;
                self.bus.if_reg |= 1 << 3; // Serial interrupt: IF bit 3
                self.bus.log_if_write(self.bus.if_reg, "serial");
            }

            // Joypad interrupt (button press transition).
            if self.bus.joypad.interrupt_pending {
                self.bus.joypad.interrupt_pending = false;
                self.bus.if_reg |= 1 << 4; // Joypad interrupt: IF bit 4
            }

            // APU frame sequencer (clocked by DIV-APU falling edge).
            let div_fell = self.bus.timer.div_apu_fell();
            self.bus.apu.tick(div_fell);

            // PPU — one dot per T-cycle.  ppu_tick() handles IRQ wiring and
            // invokes render_scanline() at the Mode3→HBlank transition.
            self.bus.ppu_tick();

            // DMA ticks once per M-cycle (every 4 T-cycles).
            if i % 4 == 3 {
                self.bus.dma_tick();
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
    pub fn tick_m(&mut self) {
        self.tick();
    }

    /// Run for a given number of M-cycles.
    pub fn tick_n(&mut self, n: u32) {
        self.tick_t(n * 4);
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

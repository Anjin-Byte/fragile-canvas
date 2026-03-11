/// Top-level Game Boy system.
///
/// Owns the CPU and Bus as sibling fields and orchestrates lock-step
/// ticking of all subsystems.
///
/// `cpu.tick()` executes one full instruction and returns the number of
/// T-cycles consumed.  We then advance the timer, APU, and PPU by that
/// many T-cycles.

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

    /// Execute one CPU instruction and advance all subsystems by the
    /// resulting T-cycle count.  Returns the number of T-cycles consumed.
    pub fn tick(&mut self) -> u8 {
        let t_cycles = self.cpu.tick(&mut self.bus);
        self.advance_subsystems(t_cycles);
        t_cycles
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
            }

            // APU frame sequencer (clocked by DIV-APU falling edge).
            let div_fell = self.bus.timer.div_apu_fell();
            self.bus.apu.tick(div_fell);

            // PPU — one dot per T-cycle.
            self.bus.ppu.tick();
            if self.bus.ppu.vblank_irq {
                self.bus.ppu.vblank_irq = false;
                self.bus.if_reg |= 0x01; // VBlank: IF bit 0
            }
            if self.bus.ppu.stat_irq {
                self.bus.ppu.stat_irq = false;
                self.bus.if_reg |= 0x02; // STAT: IF bit 1
            }

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

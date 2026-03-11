/// System clock and timer hardware.
///
/// The Game Boy's timing derives from a single 2^22 Hz (4,194,304 Hz)
/// crystal. A 16-bit **system counter** increments every T-cycle (dot),
/// which is the fundamental timing unit of the entire SoC.
///
/// Timing hierarchy:
///   1 T-cycle  = 1 dot  = 1 master clock tick  (2^22 Hz)
///   4 T-cycles = 1 M-cycle                      (2^20 Hz)
///
/// - **DIV** (FF04): exposes bits [15:8] of the system counter.
///   Writing *any* value resets the entire counter to 0.
/// - **TIMA** (FF05): timer counter, clocked by a falling edge of
///   `TAC_enable AND system_counter[selected_bit]`.
/// - **TMA** (FF06): reload value copied into TIMA on overflow
///   (delayed by 1 M-cycle = 4 T-cycles).
/// - **TAC** (FF07): timer control — enable bit + clock select.
///
/// Clock-select bits map to system-counter bit positions:
///   00 → bit  9  (every 1024 T-cycles = 256 M-cycles,   4 096 Hz)
///   01 → bit  3  (every   16 T-cycles =   4 M-cycles, 262 144 Hz)
///   10 → bit  5  (every   64 T-cycles =  16 M-cycles,  65 536 Hz)
///   11 → bit  7  (every  256 T-cycles =  64 M-cycles,  16 384 Hz)
///
/// DIV-APU: bit 12 of the system counter (= DIV bit 4).
///   Falls every 8192 T-cycles = 2048 M-cycles → 512 Hz.

/// Addresses of the timer registers in I/O space.
pub const DIV_ADDR: u16 = 0xFF04;
pub const TIMA_ADDR: u16 = 0xFF05;
pub const TMA_ADDR: u16 = 0xFF06;
pub const TAC_ADDR: u16 = 0xFF07;

/// Bit positions within the system counter selected by TAC clock-select.
/// These are T-cycle–based positions (master clock resolution).
const CLOCK_SELECT_BITS: [u8; 4] = [9, 3, 5, 7];

/// Master clock frequency: 2^22 Hz (4,194,304 Hz).
/// Every timing divider in the DMG is a power-of-two bit shift from this
/// single crystal. Nothing in the system is an arbitrary frequency.
pub const MASTER_CLOCK_HZ: u32 = 1 << 22;

/// Number of T-cycles in one M-cycle.
pub const T_CYCLES_PER_M: u32 = 4;

/// M-cycle frequency: 2^20 Hz (1,048,576 Hz).
pub const M_CYCLE_HZ: u32 = MASTER_CLOCK_HZ / T_CYCLES_PER_M;

/// The TIMA overflow reload is delayed by 1 M-cycle (4 T-cycles).
const OVERFLOW_DELAY_TICKS: u8 = 4;

#[derive(Clone)]
pub struct Timer {
    /// 16-bit free-running counter, incremented once per T-cycle (dot).
    system_counter: u16,

    /// TIMA — the timer counter register (FF05).
    tima: u8,

    /// TMA — the modulo / reload value (FF06).
    tma: u8,

    /// TAC — timer control (FF07). Only bits [2:0] are meaningful.
    tac: u8,

    /// The previous value of the composite signal
    /// `TAC_enable AND system_counter[selected_bit]`.
    /// Used for falling-edge detection.
    prev_and_result: bool,

    /// Countdown (in T-cycles) for the delayed TIMA reload.
    /// Nonzero = we're in "cycle A" (TIMA reads 0x00, reload pending).
    overflow_countdown: u8,

    /// Countdown (in T-cycles) for the reload-active window ("cycle B").
    /// While nonzero, TIMA writes are ignored (TMA overwrites them)
    /// and TMA writes also propagate to TIMA.
    reload_cycle: u8,

    /// Interrupt flag: set when the TIMA overflow + delay fires.
    /// The owner (System) should read and clear this.
    pub interrupt_pending: bool,

    /// Whether bit 12 of the system counter experienced a falling edge
    /// this tick (from natural increment or DIV write). Used by the APU
    /// frame sequencer (512 Hz).
    div_apu_event: bool,

    /// Previous state of system counter bit 12, for falling-edge detection.
    prev_div_apu_bit: bool,

    /// True when the CPU is in STOP mode; the system counter halts.
    stopped: bool,
}

impl Timer {
    pub fn new() -> Self {
        Self {
            system_counter: 0,
            tima: 0,
            tma: 0,
            tac: 0,
            prev_and_result: false,
            overflow_countdown: 0,
            reload_cycle: 0,
            interrupt_pending: false,
            div_apu_event: false,
            prev_div_apu_bit: false,
            stopped: false,
        }
    }

    // ── Public interface ──────────────────────────────────────────────

    /// Advance the system counter by one T-cycle (dot) and update TIMA.
    pub fn tick(&mut self) {
        // 0. Reset per-tick event flags.
        self.div_apu_event = false;

        // 1. Wind down the cycle-B (reload-active) window.
        if self.reload_cycle > 0 {
            self.reload_cycle -= 1;
        }

        // 2. Handle the delayed TIMA reload countdown (cycle A → B transition).
        if self.overflow_countdown > 0 {
            self.overflow_countdown -= 1;
            if self.overflow_countdown == 0 {
                // Entering cycle B: reload TMA into TIMA, request interrupt.
                self.tima = self.tma;
                self.interrupt_pending = true;
                self.reload_cycle = T_CYCLES_PER_M as u8; // cycle B lasts 1 M-cycle
            }
        }

        // 3. In STOP mode the system counter is frozen.
        if self.stopped {
            return;
        }

        // 4. Increment the system counter.
        self.system_counter = self.system_counter.wrapping_add(1);

        // 5. Detect falling edge on the composite signal → TIMA tick.
        self.check_falling_edge();

        // 6. Track DIV-APU (bit 12) falling edge.
        let current_bit12 = self.system_counter & (1 << 12) != 0;
        if self.prev_div_apu_bit && !current_bit12 {
            self.div_apu_event = true;
        }
        self.prev_div_apu_bit = current_bit12;
    }

    /// Read a timer register.
    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            DIV_ADDR => self.div(),
            TIMA_ADDR => self.tima,
            TMA_ADDR => self.tma,
            TAC_ADDR => self.tac | 0xF8, // unused upper bits read as 1
            _ => 0xFF,
        }
    }

    /// Write a timer register.
    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            DIV_ADDR => self.write_div(),
            TIMA_ADDR => self.write_tima(value),
            TMA_ADDR => self.write_tma(value),
            TAC_ADDR => self.write_tac(value),
            _ => {}
        }
    }

    // ── Accessors for other subsystems ────────────────────────────────

    /// Raw system counter value (for subsystems that derive timing from it).
    pub fn system_counter(&self) -> u16 {
        self.system_counter
    }

    /// DIV register value: bits [15:8] of the system counter.
    pub fn div(&self) -> u8 {
        (self.system_counter >> 8) as u8
    }

    /// Whether the DIV-APU bit (system counter bit 12 = DIV bit 4)
    /// experienced a falling edge this tick — from either a natural
    /// counter increment or a DIV register write resetting the counter.
    pub fn div_apu_fell(&self) -> bool {
        self.div_apu_event
    }

    /// Enter STOP mode — resets the system counter to 0 and freezes it.
    /// Like a DIV write, the reset can trigger falling edges on the
    /// composite signal (spurious TIMA tick) and on bit 12 (DIV-APU event).
    pub fn stop(&mut self) {
        // Reset counter (same side-effects as write_div).
        if self.prev_div_apu_bit {
            self.div_apu_event = true;
        }
        self.prev_div_apu_bit = false;
        self.system_counter = 0;
        self.check_falling_edge();

        self.stopped = true;
    }

    /// Resume from STOP mode — the system counter starts incrementing again.
    pub fn resume(&mut self) {
        self.stopped = false;
    }

    /// Whether the timer is in STOP mode.
    pub fn is_stopped(&self) -> bool {
        self.stopped
    }

    // ── Internals ─────────────────────────────────────────────────────

    /// Compute the current composite signal: TAC enable AND counter[selected_bit].
    fn and_result(&self) -> bool {
        let enabled = self.tac & 0x04 != 0;
        let bit_pos = CLOCK_SELECT_BITS[(self.tac & 0x03) as usize];
        let bit_val = (self.system_counter >> bit_pos) & 1 != 0;
        enabled && bit_val
    }

    /// If the composite signal fell (1 → 0), increment TIMA.
    fn check_falling_edge(&mut self) {
        let current = self.and_result();
        if self.prev_and_result && !current {
            self.increment_tima();
        }
        self.prev_and_result = current;
    }

    /// Increment TIMA; on overflow, schedule the delayed reload.
    fn increment_tima(&mut self) {
        let (new_val, overflow) = self.tima.overflowing_add(1);
        self.tima = new_val;
        if overflow {
            // TIMA sits at 0x00 for 4 T-cycles (1 M-cycle) before reload.
            self.overflow_countdown = OVERFLOW_DELAY_TICKS;
        }
    }

    /// Writing to DIV resets the entire system counter.
    /// If the selected bit was high, this creates a falling edge → TIMA tick.
    /// If bit 12 was high, this also fires a DIV-APU event.
    fn write_div(&mut self) {
        // Check bit 12 before reset — DIV-APU falling edge.
        if self.prev_div_apu_bit {
            self.div_apu_event = true;
        }
        self.prev_div_apu_bit = false;

        self.system_counter = 0;
        self.check_falling_edge();
    }

    /// Writing to TIMA:
    /// - During cycle A (overflow pending): cancels the reload and interrupt.
    /// - During cycle B (reload just fired): write is ignored — TMA overwrites.
    fn write_tima(&mut self, value: u8) {
        if self.reload_cycle > 0 {
            return; // Cycle B: TIMA writes are ignored.
        }
        self.tima = value;
        if self.overflow_countdown > 0 {
            // Cycle A: cancel the pending reload.
            self.overflow_countdown = 0;
        }
    }

    /// Writing to TMA. During cycle B (reload active), the new value also
    /// goes to TIMA (Pan Docs point 3: "the same value [is] copied to
    /// TIMA as well"). During cycle A (overflow pending), TMA is updated
    /// but TIMA stays at 0x00 — the Load signal is not yet active.
    fn write_tma(&mut self, value: u8) {
        self.tma = value;
        if self.reload_cycle > 0 {
            self.tima = value;
        }
    }

    /// Writing to TAC can change the selected bit and/or the enable flag,
    /// which may create a falling edge on the composite signal.
    fn write_tac(&mut self, value: u8) {
        self.tac = value & 0x07; // only bits [2:0] are writable
        self.check_falling_edge();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Helpers ───────────────────────────────────────────────────────

    fn tick_n(timer: &mut Timer, n: u32) {
        for _ in 0..n {
            timer.tick();
        }
    }

    /// Tick for a given number of M-cycles (convenience).
    fn tick_m(timer: &mut Timer, m: u32) {
        tick_n(timer, m * T_CYCLES_PER_M);
    }

    // ── System counter ────────────────────────────────────────────────

    #[test]
    fn system_counter_increments_every_tcycle() {
        let mut timer = Timer::new();
        assert_eq!(timer.system_counter(), 0);
        timer.tick();
        assert_eq!(timer.system_counter(), 1);
        tick_n(&mut timer, 99);
        assert_eq!(timer.system_counter(), 100);
    }

    #[test]
    fn system_counter_wraps_at_16bit() {
        let mut timer = Timer::new();
        tick_n(&mut timer, 0x10000);
        assert_eq!(timer.system_counter(), 0);
    }

    #[test]
    fn four_tcycles_equal_one_mcycle() {
        let mut timer = Timer::new();
        tick_n(&mut timer, 4);
        assert_eq!(timer.system_counter(), 4);
        // 4 T-cycles = 1 M-cycle
    }

    // ── DIV ───────────────────────────────────────────────────────────

    #[test]
    fn div_starts_at_zero() {
        let timer = Timer::new();
        assert_eq!(timer.div(), 0);
    }

    #[test]
    fn div_increments_every_256_tcycles() {
        let mut timer = Timer::new();
        tick_n(&mut timer, 255);
        assert_eq!(timer.div(), 0);
        timer.tick(); // 256th T-cycle
        assert_eq!(timer.div(), 1);
    }

    #[test]
    fn div_increments_every_64_mcycles() {
        let mut timer = Timer::new();
        tick_m(&mut timer, 63);
        assert_eq!(timer.div(), 0);
        tick_m(&mut timer, 1); // 64th M-cycle = 256th T-cycle
        assert_eq!(timer.div(), 1);
    }

    #[test]
    fn div_wraps_at_256() {
        let mut timer = Timer::new();
        tick_n(&mut timer, 256 * 256); // 65536 T-cycles
        assert_eq!(timer.div(), 0);
    }

    #[test]
    fn div_write_resets_entire_counter() {
        let mut timer = Timer::new();
        tick_n(&mut timer, 500);
        assert_ne!(timer.system_counter(), 0);
        timer.write(DIV_ADDR, 0x42); // value is ignored
        assert_eq!(timer.system_counter(), 0);
        assert_eq!(timer.div(), 0);
    }

    #[test]
    fn div_read_returns_upper_byte() {
        let mut timer = Timer::new();
        tick_n(&mut timer, 256 * 5 + 100); // DIV should be 5
        assert_eq!(timer.div(), 5);
        assert_eq!(timer.read(DIV_ADDR), 5);
    }

    // ── TIMA / TAC basic clocking ─────────────────────────────────────
    //
    // All TIMA rates expressed in both T-cycles and M-cycles:
    //   clock 01 → bit 3 → every  16 T =   4 M
    //   clock 10 → bit 5 → every  64 T =  16 M
    //   clock 11 → bit 7 → every 256 T =  64 M
    //   clock 00 → bit 9 → every 1024 T = 256 M

    #[test]
    fn tima_does_not_tick_when_disabled() {
        let mut timer = Timer::new();
        timer.write(TAC_ADDR, 0x00); // disabled
        tick_n(&mut timer, 4000);
        assert_eq!(timer.read(TIMA_ADDR), 0);
    }

    #[test]
    fn tima_clocks_every_16_tcycles_clock_01() {
        let mut timer = Timer::new();
        timer.write(TAC_ADDR, 0x05); // enabled, clock 01 (bit 3)
        tick_n(&mut timer, 16); // 16 T-cycles = 4 M-cycles
        assert_eq!(timer.read(TIMA_ADDR), 1);
        tick_n(&mut timer, 16);
        assert_eq!(timer.read(TIMA_ADDR), 2);
    }

    #[test]
    fn tima_clocks_every_64_tcycles_clock_10() {
        let mut timer = Timer::new();
        timer.write(TAC_ADDR, 0x06); // enabled, clock 10 (bit 5)
        tick_n(&mut timer, 63);
        assert_eq!(timer.read(TIMA_ADDR), 0);
        timer.tick(); // 64th T-cycle
        assert_eq!(timer.read(TIMA_ADDR), 1);
    }

    #[test]
    fn tima_clocks_every_256_tcycles_clock_11() {
        let mut timer = Timer::new();
        timer.write(TAC_ADDR, 0x07); // enabled, clock 11 (bit 7)
        tick_n(&mut timer, 255);
        assert_eq!(timer.read(TIMA_ADDR), 0);
        timer.tick(); // 256th T-cycle
        assert_eq!(timer.read(TIMA_ADDR), 1);
    }

    #[test]
    fn tima_clocks_every_1024_tcycles_clock_00() {
        let mut timer = Timer::new();
        timer.write(TAC_ADDR, 0x04); // enabled, clock 00 (bit 9)
        tick_n(&mut timer, 1023);
        assert_eq!(timer.read(TIMA_ADDR), 0);
        timer.tick(); // 1024th T-cycle
        assert_eq!(timer.read(TIMA_ADDR), 1);
    }

    // Verify equivalence in M-cycle terms
    #[test]
    fn tima_clock_01_equals_4_mcycles() {
        let mut timer = Timer::new();
        timer.write(TAC_ADDR, 0x05);
        tick_m(&mut timer, 4);
        assert_eq!(timer.read(TIMA_ADDR), 1);
    }

    #[test]
    fn tima_clock_10_equals_16_mcycles() {
        let mut timer = Timer::new();
        timer.write(TAC_ADDR, 0x06);
        tick_m(&mut timer, 16);
        assert_eq!(timer.read(TIMA_ADDR), 1);
    }

    #[test]
    fn tima_clock_11_equals_64_mcycles() {
        let mut timer = Timer::new();
        timer.write(TAC_ADDR, 0x07);
        tick_m(&mut timer, 64);
        assert_eq!(timer.read(TIMA_ADDR), 1);
    }

    #[test]
    fn tima_clock_00_equals_256_mcycles() {
        let mut timer = Timer::new();
        timer.write(TAC_ADDR, 0x04);
        tick_m(&mut timer, 256);
        assert_eq!(timer.read(TIMA_ADDR), 1);
    }

    // ── TIMA overflow and TMA reload ──────────────────────────────────

    #[test]
    fn tima_overflow_reloads_tma_after_4_tcycle_delay() {
        let mut timer = Timer::new();
        timer.write(TMA_ADDR, 0x40);
        timer.write(TIMA_ADDR, 0xFF);
        timer.write(TAC_ADDR, 0x05); // clock 01 (every 16 T-cycles)

        // Tick to the overflow point
        tick_n(&mut timer, 16);
        assert_eq!(timer.read(TIMA_ADDR), 0x00); // in delay window
        assert!(!timer.interrupt_pending);

        // 3 more T-cycles: still in delay window
        tick_n(&mut timer, 3);
        assert_eq!(timer.read(TIMA_ADDR), 0x00);
        assert!(!timer.interrupt_pending);

        // 4th T-cycle: reload fires
        timer.tick();
        assert_eq!(timer.read(TIMA_ADDR), 0x40);
        assert!(timer.interrupt_pending);
    }

    #[test]
    fn tima_overflow_delay_is_exactly_one_mcycle() {
        let mut timer = Timer::new();
        timer.write(TMA_ADDR, 0x00);
        timer.write(TIMA_ADDR, 0xFF);
        timer.write(TAC_ADDR, 0x05); // clock 01

        tick_n(&mut timer, 16); // overflow
        assert!(!timer.interrupt_pending);

        // 1 M-cycle later (4 T-cycles)
        tick_n(&mut timer, 4);
        assert!(timer.interrupt_pending);
    }

    // ── TIMA write during overflow window ─────────────────────────────

    #[test]
    fn tima_write_during_delay_cancels_reload() {
        let mut timer = Timer::new();
        timer.write(TMA_ADDR, 0x40);
        timer.write(TIMA_ADDR, 0xFF);
        timer.write(TAC_ADDR, 0x05);

        tick_n(&mut timer, 16); // overflow
        assert_eq!(timer.read(TIMA_ADDR), 0x00);

        timer.write(TIMA_ADDR, 0xBB); // cancel
        tick_n(&mut timer, 4); // would have been the reload tick
        assert_eq!(timer.read(TIMA_ADDR), 0xBB);
        assert!(!timer.interrupt_pending);
    }

    // ── TMA write during overflow window ──────────────────────────────

    #[test]
    fn tma_write_during_cycle_a_does_not_update_tima() {
        // During cycle A the Load signal is not active — TMA writes
        // update TMA but TIMA stays at 0x00.
        let mut timer = Timer::new();
        timer.write(TMA_ADDR, 0x40);
        timer.write(TIMA_ADDR, 0xFF);
        timer.write(TAC_ADDR, 0x05);

        tick_n(&mut timer, 16); // overflow → cycle A, TIMA = 0x00
        timer.write(TMA_ADDR, 0x77);
        assert_eq!(timer.read(TMA_ADDR), 0x77);
        assert_eq!(timer.read(TIMA_ADDR), 0x00); // TIMA unchanged

        // But when cycle B fires, the *new* TMA is loaded
        tick_n(&mut timer, 4);
        assert_eq!(timer.read(TIMA_ADDR), 0x77);
    }

    // ── DIV write spurious TIMA tick ──────────────────────────────────

    #[test]
    fn div_write_causes_spurious_tima_tick() {
        let mut timer = Timer::new();
        timer.write(TAC_ADDR, 0x05); // enabled, clock 01 (bit 3)

        // Advance to a point where bit 3 of the system counter is set.
        // Counter = 8 → bit 3 is set → composite = true.
        tick_n(&mut timer, 8);
        assert_eq!(timer.system_counter(), 8);

        let tima_before = timer.read(TIMA_ADDR);
        // Writing DIV resets counter to 0 → bit 3 goes 1→0 → falling edge
        timer.write(DIV_ADDR, 0);
        assert_eq!(timer.read(TIMA_ADDR), tima_before + 1);
    }

    #[test]
    fn div_write_no_spurious_tick_when_bit_not_set() {
        let mut timer = Timer::new();
        timer.write(TAC_ADDR, 0x05); // enabled, clock 01 (bit 3)

        // Counter = 4 → bit 3 = 0 → no falling edge
        tick_n(&mut timer, 4);
        let tima_before = timer.read(TIMA_ADDR);
        timer.write(DIV_ADDR, 0);
        assert_eq!(timer.read(TIMA_ADDR), tima_before);
    }

    // ── TAC write edge cases ──────────────────────────────────────────

    #[test]
    fn tac_change_causes_spurious_tick_when_switching_from_set_to_unset_bit() {
        let mut timer = Timer::new();
        // enabled, clock 01 (bit 3). Advance to counter=8 → bit 3 set.
        timer.write(TAC_ADDR, 0x05);
        tick_n(&mut timer, 8);
        assert_eq!(timer.system_counter(), 8); // bit 3 set, bit 5 not set

        let tima_before = timer.read(TIMA_ADDR);
        // Switch to clock 10 (bit 5). Bit 5 of counter 8 is 0.
        // Composite goes 1→0 → falling edge → TIMA++
        timer.write(TAC_ADDR, 0x06);
        assert_eq!(timer.read(TIMA_ADDR), tima_before + 1);
    }

    #[test]
    fn tac_change_no_tick_when_both_bits_set() {
        let mut timer = Timer::new();
        // counter = 0b101000 (40): bit 3 set, bit 5 set
        timer.write(TAC_ADDR, 0x05); // clock 01 → bit 3
        tick_n(&mut timer, 40);
        assert_eq!(timer.system_counter(), 40); // 0b101000

        let tima_before = timer.read(TIMA_ADDR);
        // Switch to clock 10 → bit 5. Both bits set → no falling edge.
        timer.write(TAC_ADDR, 0x06);
        assert_eq!(timer.read(TIMA_ADDR), tima_before);
    }

    #[test]
    fn disabling_timer_when_selected_bit_set_causes_tick() {
        let mut timer = Timer::new();
        timer.write(TAC_ADDR, 0x05); // enabled, clock 01 (bit 3)
        tick_n(&mut timer, 8); // counter=8, bit 3 set

        let tima_before = timer.read(TIMA_ADDR);
        timer.write(TAC_ADDR, 0x01); // disable
        assert_eq!(timer.read(TIMA_ADDR), tima_before + 1);
    }

    // ── DIV-APU falling edge ──────────────────────────────────────────

    #[test]
    fn div_apu_falls_at_bit_12() {
        let mut timer = Timer::new();
        // Bit 12 is set at counter 4096, falls at 8192.
        tick_n(&mut timer, 8191);
        assert!(!timer.div_apu_fell());
        timer.tick(); // counter = 8192: bit 12 goes 1→0
        assert!(timer.div_apu_fell());
    }

    #[test]
    fn div_apu_frequency_is_512hz() {
        // 512 Hz = bit 12 falls every 8192 T-cycles.
        // Full 16-bit wrap = 65536 T-cycles → 65536/8192 = 8 falling edges.
        // At 4,194,304 Hz: 4,194,304 / 8192 = 512 Hz.
        let mut timer = Timer::new();
        let mut fall_count = 0u32;

        for _ in 0..65536u32 {
            timer.tick();
            if timer.div_apu_fell() {
                fall_count += 1;
            }
        }
        assert_eq!(fall_count, 8);
    }

    #[test]
    fn div_apu_falls_every_2048_mcycles() {
        let mut timer = Timer::new();
        let mut fall_count = 0u32;

        // Run for 2048 * 4 = 8192 T-cycles (one full DIV-APU period)
        for _ in 0..(2048 * T_CYCLES_PER_M) {
            timer.tick();
            if timer.div_apu_fell() {
                fall_count += 1;
            }
        }
        assert_eq!(fall_count, 1);
    }

    // ── TAC read ──────────────────────────────────────────────────────

    #[test]
    fn tac_unused_bits_read_as_ones() {
        let timer = Timer::new();
        assert_eq!(timer.read(TAC_ADDR), 0xF8);
    }

    #[test]
    fn tac_round_trip() {
        let mut timer = Timer::new();
        timer.write(TAC_ADDR, 0xFF);
        assert_eq!(timer.read(TAC_ADDR), 0xFF); // 0x07 | 0xF8
        assert_eq!(timer.tac, 0x07);
    }

    // ── TMA / TIMA read/write ─────────────────────────────────────────

    #[test]
    fn tma_round_trip() {
        let mut timer = Timer::new();
        timer.write(TMA_ADDR, 0xAB);
        assert_eq!(timer.read(TMA_ADDR), 0xAB);
    }

    #[test]
    fn tima_round_trip() {
        let mut timer = Timer::new();
        timer.write(TIMA_ADDR, 0xCD);
        assert_eq!(timer.read(TIMA_ADDR), 0xCD);
    }

    // ── Multiple overflows ────────────────────────────────────────────

    #[test]
    fn tima_overflows_repeatedly() {
        let mut timer = Timer::new();
        timer.write(TMA_ADDR, 0xFE);
        timer.write(TIMA_ADDR, 0xFE);
        timer.write(TAC_ADDR, 0x05); // clock 01, every 16 T-cycles

        // First tick to 0xFF
        tick_n(&mut timer, 16);
        assert_eq!(timer.read(TIMA_ADDR), 0xFF);

        // Second tick overflows
        tick_n(&mut timer, 16);
        assert_eq!(timer.read(TIMA_ADDR), 0x00); // in delay window

        // 4 T-cycles later: reload
        tick_n(&mut timer, 4);
        assert_eq!(timer.read(TIMA_ADDR), 0xFE);
        assert!(timer.interrupt_pending);

        // Clear and go again
        timer.interrupt_pending = false;
        tick_n(&mut timer, 16); // 0xFE → 0xFF
        tick_n(&mut timer, 16); // overflow
        tick_n(&mut timer, 4);  // reload
        assert_eq!(timer.read(TIMA_ADDR), 0xFE);
        assert!(timer.interrupt_pending);
    }

    // ── Cross-verification: T-cycle and M-cycle consistency ───────────

    #[test]
    fn div_tcycle_mcycle_consistency() {
        // DIV should read the same whether we tick 256 T-cycles
        // or 64 M-cycles.
        let mut t_timer = Timer::new();
        let mut m_timer = Timer::new();

        tick_n(&mut t_timer, 256 * 3);
        tick_m(&mut m_timer, 64 * 3);

        assert_eq!(t_timer.div(), m_timer.div());
        assert_eq!(t_timer.div(), 3);
    }

    #[test]
    fn tima_tcycle_mcycle_consistency() {
        // TIMA with clock 01 should increment once per 4 M-cycles
        // = once per 16 T-cycles.
        let mut t_timer = Timer::new();
        let mut m_timer = Timer::new();

        t_timer.write(TAC_ADDR, 0x05);
        m_timer.write(TAC_ADDR, 0x05);

        tick_n(&mut t_timer, 16 * 10); // 10 TIMA ticks via T-cycles
        tick_m(&mut m_timer, 4 * 10);  // 10 TIMA ticks via M-cycles

        assert_eq!(t_timer.read(TIMA_ADDR), m_timer.read(TIMA_ADDR));
        assert_eq!(t_timer.read(TIMA_ADDR), 10);
    }

    // ── Cycle A vs Cycle B overflow semantics ─────────────────────────

    #[test]
    fn tima_write_during_cycle_a_cancels_reload() {
        // Pan Docs: writing TIMA during cycle A acts as if overflow didn't happen.
        let mut timer = Timer::new();
        timer.write(TMA_ADDR, 0x40);
        timer.write(TIMA_ADDR, 0xFF);
        timer.write(TAC_ADDR, 0x05); // clock 01

        tick_n(&mut timer, 16); // overflow → cycle A begins
        assert_eq!(timer.read(TIMA_ADDR), 0x00);

        // Write during cycle A (1 T-cycle into the delay window)
        timer.write(TIMA_ADDR, 0xBB);
        assert_eq!(timer.read(TIMA_ADDR), 0xBB);

        // Advance past where reload would have fired
        tick_n(&mut timer, 4);
        // Reload was cancelled — TIMA keeps the written value
        assert_eq!(timer.read(TIMA_ADDR), 0xBB);
        assert!(!timer.interrupt_pending);
    }

    #[test]
    fn tima_write_during_cycle_b_is_ignored() {
        // Pan Docs: writing TIMA during cycle B is ignored — TMA overwrites.
        let mut timer = Timer::new();
        timer.write(TMA_ADDR, 0x40);
        timer.write(TIMA_ADDR, 0xFF);
        timer.write(TAC_ADDR, 0x05); // clock 01

        tick_n(&mut timer, 16); // overflow → cycle A
        tick_n(&mut timer, 4);  // cycle A ends → cycle B begins, TIMA = TMA = 0x40
        assert_eq!(timer.read(TIMA_ADDR), 0x40);

        // Write during cycle B — should be ignored
        timer.write(TIMA_ADDR, 0xBB);
        assert_eq!(timer.read(TIMA_ADDR), 0x40);
    }

    #[test]
    fn tma_write_during_cycle_b_propagates_to_tima() {
        // Pan Docs: writing TMA during cycle B also copies to TIMA.
        let mut timer = Timer::new();
        timer.write(TMA_ADDR, 0x40);
        timer.write(TIMA_ADDR, 0xFF);
        timer.write(TAC_ADDR, 0x05);

        tick_n(&mut timer, 16); // overflow
        tick_n(&mut timer, 4);  // reload → cycle B, TIMA = 0x40
        assert_eq!(timer.read(TIMA_ADDR), 0x40);

        // Write TMA during cycle B — new value goes to both TMA and TIMA
        timer.write(TMA_ADDR, 0x99);
        assert_eq!(timer.read(TMA_ADDR), 0x99);
        assert_eq!(timer.read(TIMA_ADDR), 0x99);
    }

    #[test]
    fn cycle_b_ends_after_one_mcycle() {
        // After cycle B's 4 T-cycles, TIMA writes should work normally again.
        let mut timer = Timer::new();
        timer.write(TMA_ADDR, 0x40);
        timer.write(TIMA_ADDR, 0xFF);
        timer.write(TAC_ADDR, 0x05);

        tick_n(&mut timer, 16); // overflow
        tick_n(&mut timer, 4);  // reload → cycle B
        tick_n(&mut timer, 4);  // cycle B ends

        // Now TIMA writes should work
        timer.write(TIMA_ADDR, 0xDD);
        assert_eq!(timer.read(TIMA_ADDR), 0xDD);
    }

    // ── DIV-APU falling edge on DIV write ─────────────────────────────

    #[test]
    fn div_write_fires_div_apu_when_bit12_set() {
        let mut timer = Timer::new();
        // Advance until bit 12 is set (counter ≥ 4096)
        tick_n(&mut timer, 4096);
        assert!(timer.system_counter() & (1 << 12) != 0);

        // Writing DIV resets counter → bit 12 goes 1→0 → DIV-APU event
        timer.write(DIV_ADDR, 0);
        assert!(timer.div_apu_fell());
    }

    #[test]
    fn div_write_no_div_apu_when_bit12_clear() {
        let mut timer = Timer::new();
        // Counter = 100 → bit 12 = 0
        tick_n(&mut timer, 100);
        assert!(timer.system_counter() & (1 << 12) == 0);

        timer.write(DIV_ADDR, 0);
        assert!(!timer.div_apu_fell());
    }

    // ── STOP mode ─────────────────────────────────────────────────────

    #[test]
    fn stop_resets_and_freezes_system_counter() {
        let mut timer = Timer::new();
        tick_n(&mut timer, 100);
        assert_ne!(timer.system_counter(), 0);

        timer.stop();
        // Counter is reset to 0 on STOP entry.
        assert_eq!(timer.system_counter(), 0);
        // And stays frozen.
        tick_n(&mut timer, 1000);
        assert_eq!(timer.system_counter(), 0);
    }

    #[test]
    fn resume_unfreezes_system_counter() {
        let mut timer = Timer::new();
        tick_n(&mut timer, 100);
        timer.stop();
        // Counter reset to 0 on stop.
        assert_eq!(timer.system_counter(), 0);
        tick_n(&mut timer, 50);

        timer.resume();
        tick_n(&mut timer, 10);
        assert_eq!(timer.system_counter(), 10);
    }

    #[test]
    fn stop_triggers_spurious_tima_tick_if_selected_bit_set() {
        // Like a DIV write, STOP resets the counter — if the selected
        // bit was high, the falling edge increments TIMA.
        let mut timer = Timer::new();
        timer.write(TAC_ADDR, 0x05); // enabled, clock 01 (bit 3)
        tick_n(&mut timer, 8); // counter=8, bit 3 set

        let tima_before = timer.read(TIMA_ADDR);
        timer.stop();
        assert_eq!(timer.read(TIMA_ADDR), tima_before + 1);
    }

    #[test]
    fn stop_triggers_div_apu_if_bit12_set() {
        let mut timer = Timer::new();
        tick_n(&mut timer, 4096); // bit 12 set
        assert!(timer.system_counter() & (1 << 12) != 0);

        timer.stop();
        assert!(timer.div_apu_fell());
    }

    #[test]
    fn stop_does_not_affect_overflow_countdown() {
        // The overflow delay should still count down during STOP
        // (the counter freezes, but the reload mechanism is separate).
        let mut timer = Timer::new();
        timer.write(TMA_ADDR, 0x40);
        timer.write(TIMA_ADDR, 0xFF);
        timer.write(TAC_ADDR, 0x05);

        tick_n(&mut timer, 16); // overflow → cycle A
        timer.stop();
        tick_n(&mut timer, 4); // reload should still fire
        assert_eq!(timer.read(TIMA_ADDR), 0x40);
        assert!(timer.interrupt_pending);
    }

    #[test]
    fn tima_does_not_tick_during_stop() {
        let mut timer = Timer::new();
        timer.write(TAC_ADDR, 0x05); // enabled, clock 01
        timer.stop();
        tick_n(&mut timer, 1000);
        // Counter frozen → no falling edges → TIMA stays 0
        assert_eq!(timer.read(TIMA_ADDR), 0);
    }
}

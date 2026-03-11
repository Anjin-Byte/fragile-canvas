/// Clock governor — regulates emulation speed to match the DMG master clock.
///
/// Platform-agnostic: the caller provides elapsed wall-clock time in
/// nanoseconds. The governor tracks fractional T-cycle debt and returns
/// whole T-cycles to execute each update.
///
/// The DMG master clock is exactly 2^22 Hz (4,194,304 Hz). One T-cycle
/// is ~238.42 ns. The governor accumulates sub-cycle fractions so drift
/// is bounded to < 1 T-cycle regardless of how often `cycles_due` is called.

use crate::timer::MASTER_CLOCK_HZ;

pub struct ClockGovernor {
    /// Accumulated fractional T-cycles (sub-cycle debt).
    /// Stored as a count of nanosecond-scaled units to defer the division.
    ///
    /// Invariant: `accum < NS_PER_SEC` — always less than one second's
    /// worth of nanoseconds, so overflow is impossible with u64.
    accum: u64,

    /// Total T-cycles dispensed since creation.
    total_dispensed: u64,
}

const NS_PER_SEC: u64 = 1_000_000_000;

impl ClockGovernor {
    pub fn new() -> Self {
        Self {
            accum: 0,
            total_dispensed: 0,
        }
    }

    /// Given elapsed wall-clock nanoseconds, returns the number of
    /// T-cycles the emulator should execute to stay in sync.
    ///
    /// Fractional cycles are carried forward to the next call.
    /// For a 60 Hz frame loop calling every ~16.7 ms, this returns
    /// ~69,905 T-cycles per call (≈ one PPU frame).
    pub fn cycles_due(&mut self, elapsed_ns: u64) -> u32 {
        // cycles = elapsed_ns * MASTER_CLOCK_HZ / 1_000_000_000
        //
        // We accumulate the numerator to preserve the fractional part.
        // Max single-call input before u64 overflow:
        //   (2^64 - accum) / 2^22 ≈ 4.4 × 10^12 ns ≈ 73 minutes.
        // Well beyond any reasonable frame interval.
        self.accum += elapsed_ns * MASTER_CLOCK_HZ as u64;
        let cycles = self.accum / NS_PER_SEC;
        self.accum %= NS_PER_SEC;
        self.total_dispensed += cycles;
        cycles as u32
    }

    /// Total T-cycles dispensed since this governor was created.
    pub fn total_dispensed(&self) -> u64 {
        self.total_dispensed
    }

    /// Reset the accumulator (e.g. after a pause or debugger break)
    /// to prevent a burst of catch-up cycles.
    pub fn reset(&mut self) {
        self.accum = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timer::MASTER_CLOCK_HZ;

    #[test]
    fn one_second_yields_master_clock_cycles() {
        let mut gov = ClockGovernor::new();
        let cycles = gov.cycles_due(NS_PER_SEC);
        assert_eq!(cycles, MASTER_CLOCK_HZ);
    }

    #[test]
    fn fractional_cycles_accumulate() {
        let mut gov = ClockGovernor::new();
        // 1 T-cycle ≈ 238.42 ns. Feed 200 ns — not enough for a full cycle.
        let c1 = gov.cycles_due(200);
        assert_eq!(c1, 0);
        // Feed another 200 ns — now we should have enough for 1 cycle.
        // 400 ns * 2^22 / 10^9 ≈ 1.677 → 1 cycle, remainder carried.
        let c2 = gov.cycles_due(200);
        assert_eq!(c2, 1);
    }

    #[test]
    fn no_drift_over_many_frames() {
        let mut gov = ClockGovernor::new();
        // Simulate 60 Hz for 10 seconds (600 frames).
        let frame_ns = NS_PER_SEC / 60;
        let mut total = 0u64;
        for _ in 0..600 {
            total += gov.cycles_due(frame_ns) as u64;
        }
        // 10 seconds at 2^22 Hz = 41,943,040 T-cycles.
        // With integer frame_ns (16,666,666 ns), total elapsed =
        // 600 * 16,666,666 = 9,999,999,600 ns, so expected cycles =
        // 9,999,999,600 * 2^22 / 10^9 = 41,943,038.3...
        // We should get 41,943,038 with sub-cycle remainder carried.
        let expected_ns = frame_ns * 600;
        let expected_cycles = expected_ns * MASTER_CLOCK_HZ as u64 / NS_PER_SEC;
        assert_eq!(total, expected_cycles);
    }

    #[test]
    fn reset_clears_accumulator() {
        let mut gov = ClockGovernor::new();
        gov.cycles_due(200); // build up some fractional debt
        gov.reset();
        // After reset, a small input shouldn't yield cycles from old debt.
        let c = gov.cycles_due(100);
        assert_eq!(c, 0);
    }

    #[test]
    fn total_dispensed_tracks_all_cycles() {
        let mut gov = ClockGovernor::new();
        gov.cycles_due(NS_PER_SEC);     // 2^22
        gov.cycles_due(NS_PER_SEC / 2); // 2^21
        assert_eq!(
            gov.total_dispensed(),
            MASTER_CLOCK_HZ as u64 + MASTER_CLOCK_HZ as u64 / 2
        );
    }
}

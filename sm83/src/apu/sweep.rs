/// Frequency sweep for CH1.
///
/// Clocked at 128 Hz (frame sequencer steps 2, 6).
/// Shifts the frequency up or down, disabling the channel on overflow (>2047).

#[derive(Clone)]
pub struct Sweep {
    /// Shadow copy of the frequency, used for calculations.
    pub shadow_freq: u16,
    /// Countdown timer, reloaded from period.
    timer: u8,
    /// Sweep period from NR10 (0 treated as 8).
    period: u8,
    /// Right-shift amount from NR10.
    shift: u8,
    /// True = subtract (negate), false = add.
    negate: bool,
    /// Whether sweep is active (period or shift nonzero).
    enabled: bool,
    /// Whether negate mode was used since last trigger.
    /// Writing NR10 without negate after using negate disables the channel.
    negate_used: bool,
}

impl Sweep {
    pub fn new() -> Self {
        Self {
            shadow_freq: 0,
            timer: 0,
            period: 0,
            shift: 0,
            negate: false,
            enabled: false,
            negate_used: false,
        }
    }

    /// Clock the sweep. Returns `Some(new_freq)` if the frequency should be
    /// updated, or `None` if no change. Sets `overflow` flag if the channel
    /// should be disabled.
    ///
    /// Returns `(new_freq_or_none, should_disable)`.
    pub fn tick(&mut self) -> (Option<u16>, bool) {
        if !self.enabled {
            return (None, false);
        }

        if self.timer > 0 {
            self.timer -= 1;
        }

        if self.timer == 0 {
            // Reload timer (period 0 treated as 8)
            self.timer = if self.period == 0 { 8 } else { self.period };

            if self.period != 0 {
                let (new_freq, overflow) = self.calculate();
                if overflow {
                    return (None, true);
                }
                if self.shift != 0 {
                    self.shadow_freq = new_freq;
                    // Overflow check again with the new frequency
                    let (_, overflow2) = self.calculate();
                    if overflow2 {
                        return (Some(new_freq), true);
                    }
                    return (Some(new_freq), false);
                }
            }
        }

        (None, false)
    }

    /// Calculate the new frequency. Returns `(new_freq, overflowed)`.
    fn calculate(&mut self) -> (u16, bool) {
        let delta = self.shadow_freq >> self.shift;
        let new_freq = if self.negate {
            self.negate_used = true;
            self.shadow_freq.wrapping_sub(delta)
        } else {
            self.shadow_freq.wrapping_add(delta)
        };

        (new_freq, new_freq > 2047)
    }

    /// Called on channel trigger. Initializes sweep from NR10 and current frequency.
    pub fn trigger(&mut self, freq: u16, reg: u8) {
        self.shadow_freq = freq;
        self.period = (reg >> 4) & 0x07;
        self.negate = reg & 0x08 != 0;
        self.shift = reg & 0x07;
        // Reload timer (period 0 treated as 8)
        self.timer = if self.period == 0 { 8 } else { self.period };
        self.enabled = self.period != 0 || self.shift != 0;
        self.negate_used = false;

        // If shift is nonzero, perform overflow check immediately
        if self.shift != 0 {
            let (_, overflow) = self.calculate();
            if overflow {
                // Caller should disable channel — we signal via a subsequent tick
                // Actually, set enabled false so tick returns disable on next call
                // But per hardware behavior, the channel is disabled immediately.
                // We'll handle this by returning from trigger and letting caller check.
            }
        }
    }

    /// Check if trigger caused an overflow (caller should disable channel).
    pub fn trigger_overflows(&mut self, freq: u16, reg: u8) -> bool {
        self.trigger(freq, reg);
        if self.shift != 0 {
            let (_, overflow) = self.calculate();
            return overflow;
        }
        false
    }

    /// Check if switching from negate to non-negate mode should disable channel.
    /// This happens when negate was used and a new NR10 write uses add mode.
    pub fn check_negate_disable(&self, new_reg: u8) -> bool {
        self.negate_used && (new_reg & 0x08 == 0)
    }

    /// Reset all state (power off).
    pub fn power_off(&mut self) {
        self.shadow_freq = 0;
        self.timer = 0;
        self.period = 0;
        self.shift = 0;
        self.negate = false;
        self.enabled = false;
        self.negate_used = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frequency_increases() {
        let mut sweep = Sweep::new();
        // freq=0x100, period=1, shift=1, add mode
        let overflow = sweep.trigger_overflows(0x100, 0x11); // period=1, shift=1
        assert!(!overflow);

        let (new_freq, disable) = sweep.tick();
        assert!(!disable);
        // 0x100 + (0x100 >> 1) = 0x100 + 0x80 = 0x180
        assert_eq!(new_freq, Some(0x180));
    }

    #[test]
    fn frequency_decreases() {
        let mut sweep = Sweep::new();
        // freq=0x200, period=1, shift=2, negate mode
        let overflow = sweep.trigger_overflows(0x200, 0x1A); // period=1, negate, shift=2
        assert!(!overflow);

        let (new_freq, disable) = sweep.tick();
        assert!(!disable);
        // 0x200 - (0x200 >> 2) = 0x200 - 0x80 = 0x180
        assert_eq!(new_freq, Some(0x180));
    }

    #[test]
    fn overflow_disables() {
        let mut sweep = Sweep::new();
        // freq=0x700, period=1, shift=1, add mode
        // 0x700 + 0x380 = 0xA80 > 2047
        let overflow = sweep.trigger_overflows(0x700, 0x11);
        assert!(overflow);
    }

    #[test]
    fn period_zero_treated_as_8() {
        let mut sweep = Sweep::new();
        // period=0, shift=1 → enabled (shift nonzero), timer=8
        sweep.trigger_overflows(0x100, 0x01); // period=0, shift=1

        // Should take 8 ticks before firing
        for _ in 0..7 {
            let (freq, _) = sweep.tick();
            assert_eq!(freq, None);
        }
        let (freq, _) = sweep.tick();
        // period=0 means no update even though timer fires
        assert_eq!(freq, None);
    }

    #[test]
    fn shift_zero_no_change() {
        let mut sweep = Sweep::new();
        // period=1, shift=0 → enabled (period nonzero), but shift=0 means no freq change
        sweep.trigger_overflows(0x100, 0x10); // period=1, shift=0

        let (freq, disable) = sweep.tick();
        assert_eq!(freq, None);
        assert!(!disable);
    }

    #[test]
    fn negate_then_add_disables() {
        let mut sweep = Sweep::new();
        // Use negate mode
        sweep.trigger_overflows(0x100, 0x19); // period=1, negate, shift=1
        sweep.tick(); // uses negate

        // Now check if switching to add mode should disable
        assert!(sweep.check_negate_disable(0x11)); // add mode
        assert!(!sweep.check_negate_disable(0x19)); // still negate, fine
    }

    #[test]
    fn power_off_resets() {
        let mut sweep = Sweep::new();
        sweep.trigger_overflows(0x100, 0x11);
        sweep.power_off();
        assert_eq!(sweep.shadow_freq, 0);
        assert!(!sweep.enabled);
        assert!(!sweep.negate_used);
    }
}

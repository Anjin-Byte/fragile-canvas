/// Volume envelope shared by CH1, CH2, and CH4.
///
/// Clocked at 64 Hz (frame sequencer step 7).
/// Ramps volume up or down by 1 each period tick, clamped to 0-15.

#[derive(Clone)]
pub struct VolumeEnvelope {
    /// Current volume (0-15).
    pub volume: u8,
    /// True = increase, false = decrease.
    direction: bool,
    /// Envelope period (0 = disabled).
    period: u8,
    /// Countdown timer, reloaded from period.
    timer: u8,
}

impl VolumeEnvelope {
    pub fn new() -> Self {
        Self {
            volume: 0,
            direction: false,
            period: 0,
            timer: 0,
        }
    }

    /// Clock the envelope. Called at 64 Hz (step 7).
    pub fn tick(&mut self) {
        if self.timer > 0 {
            self.timer -= 1;
        }

        if self.timer == 0 {
            // Period 0 is treated as 8 for timer reload
            self.timer = if self.period == 0 { 8 } else { self.period };

            // Only change volume if period is nonzero
            if self.period != 0 {
                if self.direction && self.volume < 15 {
                    self.volume += 1;
                } else if !self.direction && self.volume > 0 {
                    self.volume -= 1;
                }
            }
        }
    }

    /// Called on channel trigger. Loads initial state from NRx2 register value.
    pub fn trigger(&mut self, reg: u8) {
        self.volume = reg >> 4;
        self.direction = reg & 0x08 != 0;
        self.period = reg & 0x07;
        // Period 0 treated as 8 for timer reload
        self.timer = if self.period == 0 { 8 } else { self.period };
    }

    /// Reset all state (power off).
    pub fn power_off(&mut self) {
        self.volume = 0;
        self.direction = false;
        self.period = 0;
        self.timer = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ramps_down_to_zero() {
        let mut env = VolumeEnvelope::new();
        // period=1
        env.trigger(0x31); // volume=3, down, period=1
        env.tick(); // timer 1→0, reload, vol 3→2
        assert_eq!(env.volume, 2);
        env.tick();
        assert_eq!(env.volume, 1);
        env.tick();
        assert_eq!(env.volume, 0);
        // Clamps at 0
        env.tick();
        assert_eq!(env.volume, 0);
    }

    #[test]
    fn ramps_up_to_fifteen() {
        let mut env = VolumeEnvelope::new();
        env.trigger(0xD9); // volume=13, up, period=1
        assert_eq!(env.volume, 13);

        env.tick();
        assert_eq!(env.volume, 14);
        env.tick();
        assert_eq!(env.volume, 15);
        // Clamps at 15
        env.tick();
        assert_eq!(env.volume, 15);
    }

    #[test]
    fn period_zero_no_volume_change() {
        let mut env = VolumeEnvelope::new();
        env.trigger(0xF0); // volume=15, down, period=0
        // Period 0 means timer reloads as 8, but no volume changes
        for _ in 0..16 {
            env.tick();
        }
        assert_eq!(env.volume, 15); // no change
    }

    #[test]
    fn period_controls_rate() {
        let mut env = VolumeEnvelope::new();
        env.trigger(0xA3); // volume=10, down, period=3

        // First 2 ticks just decrement timer, no volume change
        env.tick(); // timer: 3→2
        assert_eq!(env.volume, 10);
        env.tick(); // timer: 2→1
        assert_eq!(env.volume, 10);
        env.tick(); // timer: 1→0, reload to 3, vol 10→9
        assert_eq!(env.volume, 9);
    }

    #[test]
    fn trigger_reloads_state() {
        let mut env = VolumeEnvelope::new();
        env.trigger(0x51); // volume=5, down, period=1
        env.tick(); // vol→4
        env.tick(); // vol→3

        // Re-trigger with different values
        env.trigger(0xF2); // volume=15, down, period=2
        assert_eq!(env.volume, 15);

        env.tick(); // timer: 2→1
        assert_eq!(env.volume, 15);
        env.tick(); // timer: 1→0, reload, vol→14
        assert_eq!(env.volume, 14);
    }

    #[test]
    fn power_off_resets() {
        let mut env = VolumeEnvelope::new();
        env.trigger(0xF3);
        env.power_off();
        assert_eq!(env.volume, 0);
        assert_eq!(env.period, 0);
    }
}

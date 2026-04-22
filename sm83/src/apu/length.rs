/// Length counter shared by all four channels.
///
/// Counts down at 256 Hz (frame sequencer steps 0,2,4,6).
/// When the counter reaches 0 the channel is disabled.
/// CH1/CH2/CH4 use max_length = 64, CH3 uses max_length = 256.

#[derive(Clone)]
pub struct LengthCounter {
    /// Current countdown value.
    pub counter: u16,
    /// Maximum length (64 or 256).
    pub max_length: u16,
    /// Whether length expiry disables the channel (NRx4 bit 6).
    pub enabled: bool,
}

impl LengthCounter {
    pub fn new(max_length: u16) -> Self {
        Self {
            counter: 0,
            max_length,
            enabled: false,
        }
    }

    /// Clock the length counter. Returns true if the counter just expired
    /// (hit 0), meaning the channel should be disabled.
    pub fn tick(&mut self) -> bool {
        if self.enabled && self.counter > 0 {
            self.counter -= 1;
            return self.counter == 0;
        }
        false
    }

    /// Called on trigger: if counter is 0, reload to max_length.
    pub fn trigger(&mut self) {
        if self.counter == 0 {
            self.counter = self.max_length;
        }
    }

    /// Write a new length value from the length register.
    /// The counter is loaded with (max_length - value).
    pub fn write_length(&mut self, value: u16) {
        self.counter = self.max_length - value;
    }

    /// Reset all state (power off).
    pub fn power_off(&mut self) {
        self.counter = 0;
        self.enabled = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn countdown_to_zero_returns_expired() {
        let mut lc = LengthCounter::new(64);
        lc.write_length(62); // counter = 64 - 62 = 2
        lc.enabled = true;

        assert!(!lc.tick()); // 2 → 1
        assert!(lc.tick());  // 1 → 0, expired
    }

    #[test]
    fn expired_counter_stays_at_zero() {
        let mut lc = LengthCounter::new(64);
        lc.write_length(63); // counter = 1
        lc.enabled = true;

        assert!(lc.tick()); // 1 → 0
        assert!(!lc.tick()); // already 0, no further tick
    }

    #[test]
    fn disabled_counter_does_not_tick() {
        let mut lc = LengthCounter::new(64);
        lc.write_length(62); // counter = 2
        lc.enabled = false;

        assert!(!lc.tick());
        assert_eq!(lc.counter, 2); // unchanged
    }

    #[test]
    fn trigger_reloads_when_zero() {
        let mut lc = LengthCounter::new(64);
        assert_eq!(lc.counter, 0);
        lc.trigger();
        assert_eq!(lc.counter, 64);
    }

    #[test]
    fn trigger_does_not_reload_when_nonzero() {
        let mut lc = LengthCounter::new(64);
        lc.write_length(60); // counter = 4
        lc.trigger();
        assert_eq!(lc.counter, 4); // unchanged
    }

    #[test]
    fn write_length_sets_counter() {
        let mut lc = LengthCounter::new(64);
        lc.write_length(0);
        assert_eq!(lc.counter, 64);
        lc.write_length(63);
        assert_eq!(lc.counter, 1);
    }

    #[test]
    fn ch3_uses_256_step() {
        let mut lc = LengthCounter::new(256);
        lc.write_length(0);
        assert_eq!(lc.counter, 256);
        lc.write_length(255);
        assert_eq!(lc.counter, 1);
    }

    #[test]
    fn full_countdown_64() {
        let mut lc = LengthCounter::new(64);
        lc.write_length(0); // counter = 64
        lc.enabled = true;

        for _ in 0..63 {
            assert!(!lc.tick());
        }
        assert!(lc.tick()); // 64th tick → expired
        assert_eq!(lc.counter, 0);
    }

    #[test]
    fn power_off_resets() {
        let mut lc = LengthCounter::new(64);
        lc.write_length(32);
        lc.enabled = true;
        lc.power_off();
        assert_eq!(lc.counter, 0);
        assert!(!lc.enabled);
    }
}

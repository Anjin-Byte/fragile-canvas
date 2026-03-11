/// Pulse channel (CH1 and CH2).
///
/// CH1 has sweep (NR10), CH2 does not.
/// Both share: duty/length (NRx1), envelope (NRx2), period (NRx3/NRx4).

use super::envelope::VolumeEnvelope;
use super::length::LengthCounter;
use super::sweep::Sweep;

#[derive(Clone)]
pub struct PulseChannel {
    /// NR10 (CH1 only): sweep register. Always 0 for CH2.
    pub sweep_reg: u8,
    /// NRx1: duty (bits 7:6) + length timer load (bits 5:0).
    pub duty_length: u8,
    /// NRx2: initial volume (7:4), direction (3), period (2:0).
    pub envelope_reg: u8,
    /// NRx3: period low byte (write-only).
    pub period_low: u8,
    /// NRx4: trigger (7, write-only), length enable (6), period high (2:0).
    pub period_high_ctrl: u8,

    /// Whether this channel has sweep hardware (CH1 = true, CH2 = false).
    pub has_sweep: bool,
    /// Whether the channel is currently producing output.
    pub enabled: bool,
    /// Whether the DAC is powered (NRx2 bits 7:3 != 0).
    pub dac_enabled: bool,
    /// Length counter (64-step).
    pub length: LengthCounter,
    /// Volume envelope.
    pub envelope: VolumeEnvelope,
    /// Frequency sweep (CH1 only).
    pub sweep: Option<Sweep>,
    /// Period timer countdown (T-cycles).
    pub period_timer: u16,
    /// Duty step position (0-7).
    pub duty_step: u8,
}

/// Duty cycle waveform patterns.
/// Index by duty (0-3), then step (0-7). 1 = high, 0 = low.
const DUTY_TABLE: [[u8; 8]; 4] = [
    [0, 0, 0, 0, 0, 0, 0, 1], // 12.5%
    [0, 0, 0, 0, 0, 0, 1, 1], // 25%
    [0, 0, 0, 0, 1, 1, 1, 1], // 50%
    [1, 1, 1, 1, 1, 1, 0, 0], // 75%
];

impl PulseChannel {
    pub fn new(has_sweep: bool) -> Self {
        Self {
            sweep_reg: 0,
            duty_length: 0,
            envelope_reg: 0,
            period_low: 0,
            period_high_ctrl: 0,
            has_sweep,
            enabled: false,
            dac_enabled: false,
            length: LengthCounter::new(64),
            envelope: VolumeEnvelope::new(),
            sweep: if has_sweep { Some(Sweep::new()) } else { None },
            period_timer: 0,
            duty_step: 0,
        }
    }

    /// Read a register by index within the channel's 5-register block.
    /// reg_index: 0 = NRx0 (sweep), 1 = NRx1, 2 = NRx2, 3 = NRx3, 4 = NRx4
    pub fn read(&self, reg_index: u8) -> u8 {
        match reg_index {
            0 => if self.has_sweep { self.sweep_reg | 0x80 } else { 0xFF },
            1 => self.duty_length | 0x3F, // length bits write-only
            2 => self.envelope_reg,        // fully readable
            3 => 0xFF,                     // period low: write-only
            4 => self.period_high_ctrl | 0xBF, // trigger wo, bits 5:3 unused
            _ => 0xFF,
        }
    }

    /// Write a register by index.
    pub fn write(&mut self, reg_index: u8, value: u8) {
        match reg_index {
            0 => {
                if self.has_sweep {
                    // Check negate-to-add disable before storing new value
                    if let Some(ref sweep) = self.sweep {
                        if sweep.check_negate_disable(value) {
                            self.enabled = false;
                        }
                    }
                    self.sweep_reg = value;
                }
            }
            1 => {
                self.duty_length = value;
                self.length.write_length((value & 0x3F) as u16);
            }
            2 => {
                self.envelope_reg = value;
                self.dac_enabled = value & 0xF8 != 0;
                if !self.dac_enabled {
                    self.enabled = false;
                }
            }
            3 => self.period_low = value,
            4 => {
                self.period_high_ctrl = value;
                self.length.enabled = value & 0x40 != 0;
                if value & 0x80 != 0 {
                    self.trigger();
                }
            }
            _ => {}
        }
    }

    /// Handle trigger event (NRx4 bit 7 written).
    fn trigger(&mut self) {
        if self.dac_enabled {
            self.enabled = true;
        }
        self.length.trigger();
        self.envelope.trigger(self.envelope_reg);

        self.period_timer = (2048 - self.period()) * 4;
        // Note: duty_step is NOT reset on trigger (only on APU power-off)

        // Initialize sweep on trigger
        let freq = self.period();
        let sweep_reg = self.sweep_reg;
        if let Some(ref mut sweep) = self.sweep {
            if sweep.trigger_overflows(freq, sweep_reg) {
                self.enabled = false;
            }
        }
    }

    /// 11-bit period value from NRx3 (low) + NRx4 bits 2:0 (high).
    pub fn period(&self) -> u16 {
        let hi = (self.period_high_ctrl & 0x07) as u16;
        let lo = self.period_low as u16;
        (hi << 8) | lo
    }

    /// Clock the period timer (called every T-cycle).
    /// Advances the duty step when the timer expires.
    pub fn tick_period(&mut self) {
        if self.period_timer > 0 {
            self.period_timer -= 1;
        }
        if self.period_timer == 0 {
            self.period_timer = (2048 - self.period()) * 4;
            self.duty_step = (self.duty_step + 1) & 7;
        }
    }

    /// Current output of the duty waveform (0 or 1).
    pub fn duty_output(&self) -> u8 {
        let duty = (self.duty_length >> 6) as usize;
        DUTY_TABLE[duty][self.duty_step as usize]
    }

    /// DAC output as analog value in -1.0..+1.0.
    /// DAC off → 0.0. Channel disabled → DAC zero-point (-1.0).
    pub fn dac_output(&self) -> f32 {
        if !self.dac_enabled {
            return 0.0;
        }
        let digital = if self.enabled {
            self.duty_output() as f32 * self.envelope.volume as f32
        } else {
            0.0
        };
        (digital / 7.5) - 1.0
    }

    /// Clock the frequency sweep. Updates period registers if frequency changes.
    pub fn tick_sweep(&mut self) {
        if let Some(ref mut sweep) = self.sweep {
            let (new_freq, disable) = sweep.tick();
            if disable {
                self.enabled = false;
            }
            if let Some(freq) = new_freq {
                // Write new frequency back to period registers
                self.period_low = (freq & 0xFF) as u8;
                self.period_high_ctrl = (self.period_high_ctrl & 0xF8) | ((freq >> 8) & 0x07) as u8;
            }
        }
    }

    /// Clock the volume envelope.
    pub fn tick_envelope(&mut self) {
        self.envelope.tick();
    }

    /// Clock the length counter. Disables channel if it expires.
    pub fn tick_length(&mut self) {
        if self.length.tick() {
            self.enabled = false;
        }
    }

    /// Reset all state (power off).
    pub fn power_off(&mut self) {
        self.sweep_reg = 0;
        self.duty_length = 0;
        self.envelope_reg = 0;
        self.period_low = 0;
        self.period_high_ctrl = 0;
        self.enabled = false;
        self.dac_enabled = false;
        self.length.power_off();
        self.envelope.power_off();
        if let Some(ref mut sweep) = self.sweep {
            sweep.power_off();
        }
        self.period_timer = 0;
        self.duty_step = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ch1_sweep_reg_readable() {
        let mut ch = PulseChannel::new(true);
        ch.write(0, 0x37);
        assert_eq!(ch.read(0), 0x37 | 0x80);
    }

    #[test]
    fn ch2_sweep_reg_reads_ff() {
        let ch = PulseChannel::new(false);
        assert_eq!(ch.read(0), 0xFF);
    }

    #[test]
    fn ch2_sweep_write_ignored() {
        let mut ch = PulseChannel::new(false);
        ch.write(0, 0x37);
        assert_eq!(ch.sweep_reg, 0); // not stored
    }

    #[test]
    fn duty_readable_length_write_only() {
        let mut ch = PulseChannel::new(true);
        ch.write(1, 0xC0); // duty = 11, length = 0
        assert_eq!(ch.read(1), 0xFF); // 0xC0 | 0x3F
        ch.write(1, 0x80); // duty = 10
        assert_eq!(ch.read(1), 0xBF); // 0x80 | 0x3F
    }

    #[test]
    fn envelope_fully_readable() {
        let mut ch = PulseChannel::new(true);
        ch.write(2, 0xA5);
        assert_eq!(ch.read(2), 0xA5);
    }

    #[test]
    fn period_low_write_only() {
        let mut ch = PulseChannel::new(true);
        ch.write(3, 0x42);
        assert_eq!(ch.read(3), 0xFF);
        assert_eq!(ch.period_low, 0x42); // internally stored
    }

    #[test]
    fn period_high_trigger_write_only() {
        let mut ch = PulseChannel::new(true);
        ch.write(2, 0xF0); // DAC on
        ch.write(4, 0xC3); // trigger + length enable + period high = 3
        assert_eq!(ch.read(4), 0xC3 | 0xBF); // 0xFF
    }

    #[test]
    fn trigger_enables_channel_when_dac_on() {
        let mut ch = PulseChannel::new(true);
        ch.write(2, 0xF0); // DAC on (bits 7:4 = 0xF)
        assert!(!ch.enabled);
        ch.write(4, 0x80); // trigger
        assert!(ch.enabled);
    }

    #[test]
    fn trigger_does_not_enable_when_dac_off() {
        let mut ch = PulseChannel::new(true);
        ch.write(2, 0x00); // DAC off
        ch.write(4, 0x80); // trigger
        assert!(!ch.enabled);
    }

    #[test]
    fn dac_off_disables_channel() {
        let mut ch = PulseChannel::new(true);
        ch.write(2, 0xF0); // DAC on
        ch.write(4, 0x80); // trigger → enabled
        assert!(ch.enabled);
        ch.write(2, 0x00); // DAC off → disabled
        assert!(!ch.enabled);
        assert!(!ch.dac_enabled);
    }

    #[test]
    fn dac_enabled_when_upper_bits_nonzero() {
        let mut ch = PulseChannel::new(true);
        ch.write(2, 0x08); // only direction bit set → bits 7:3 = 0b00001
        assert!(ch.dac_enabled);
        ch.write(2, 0x07); // only period bits → bits 7:3 = 0
        assert!(!ch.dac_enabled);
    }

    #[test]
    fn period_combines_low_and_high() {
        let mut ch = PulseChannel::new(true);
        ch.write(3, 0xAB); // low
        ch.write(4, 0x05); // high bits 2:0 = 5
        assert_eq!(ch.period(), 0x5AB);
    }

    #[test]
    fn power_off_resets_all_state() {
        let mut ch = PulseChannel::new(true);
        ch.write(0, 0x37);
        ch.write(2, 0xF0);
        ch.write(4, 0x80); // trigger
        assert!(ch.enabled);

        ch.power_off();
        assert!(!ch.enabled);
        assert!(!ch.dac_enabled);
        assert_eq!(ch.sweep_reg, 0);
        assert_eq!(ch.envelope_reg, 0);
    }

    #[test]
    fn duty_patterns_correct() {
        let mut ch = PulseChannel::new(true);
        // Duty 0 (12.5%): only steps 6,7 are high
        ch.write(1, 0x00); // duty = 00
        let expected_0 = [0, 0, 0, 0, 0, 0, 0, 1];
        for i in 0..8 {
            ch.duty_step = i;
            assert_eq!(ch.duty_output(), expected_0[i as usize], "duty 0, step {}", i);
        }

        // Duty 1 (25%): steps 6,7 are high
        ch.write(1, 0x40); // duty = 01
        let expected_1 = [0, 0, 0, 0, 0, 0, 1, 1];
        for i in 0..8 {
            ch.duty_step = i;
            assert_eq!(ch.duty_output(), expected_1[i as usize], "duty 1, step {}", i);
        }

        // Duty 2 (50%): steps 4-7 are high
        ch.write(1, 0x80); // duty = 10
        let expected_2 = [0, 0, 0, 0, 1, 1, 1, 1];
        for i in 0..8 {
            ch.duty_step = i;
            assert_eq!(ch.duty_output(), expected_2[i as usize], "duty 2, step {}", i);
        }

        // Duty 3 (75%): steps 0-5 are high
        ch.write(1, 0xC0); // duty = 11
        let expected_3 = [1, 1, 1, 1, 1, 1, 0, 0];
        for i in 0..8 {
            ch.duty_step = i;
            assert_eq!(ch.duty_output(), expected_3[i as usize], "duty 3, step {}", i);
        }
    }

    #[test]
    fn trigger_does_not_reset_duty_step() {
        let mut ch = PulseChannel::new(true);
        ch.write(2, 0xF0); // DAC on
        ch.duty_step = 5;
        ch.write(4, 0x80); // trigger
        assert_eq!(ch.duty_step, 5); // duty step preserved
    }
}

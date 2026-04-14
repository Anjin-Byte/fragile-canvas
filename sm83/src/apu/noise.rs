/// Noise channel (CH4).
///
/// Generates pseudo-random noise via a linear feedback shift register (LFSR).
/// Registers: NR41 (FF20) - NR44 (FF23).

use super::envelope::VolumeEnvelope;
use super::length::LengthCounter;

#[derive(Clone)]
pub struct NoiseChannel {
    /// NR41: length timer load (bits 5:0, write-only).
    pub length_reg: u8,
    /// NR42: initial volume (7:4), direction (3), period (2:0).
    pub envelope_reg: u8,
    /// NR43: clock shift (7:4), LFSR width (3), clock divider (2:0).
    pub poly_reg: u8,
    /// NR44: trigger (7, wo), length enable (6).
    pub control: u8,
    /// Whether the channel is currently producing output.
    pub enabled: bool,
    /// Whether the DAC is powered (NR42 bits 7:3 != 0).
    pub dac_enabled: bool,
    /// Length counter (64-step).
    pub length: LengthCounter,
    /// Volume envelope.
    pub envelope: VolumeEnvelope,
    /// Period timer countdown (T-cycles).
    pub period_timer: u16,
    /// 15-bit linear feedback shift register.
    pub lfsr: u16,
}

impl NoiseChannel {
    pub fn new() -> Self {
        Self {
            length_reg: 0,
            envelope_reg: 0,
            poly_reg: 0,
            control: 0,
            enabled: false,
            dac_enabled: false,
            length: LengthCounter::new(64),
            envelope: VolumeEnvelope::new(),
            period_timer: 0,
            lfsr: 0x7FFF, // all bits set on startup
        }
    }

    /// Read a register by index (0-3 within the channel's block).
    pub fn read(&self, reg_index: u8) -> u8 {
        match reg_index {
            0 => 0xFF,                 // length: write-only + bits 7:6 unused
            1 => self.envelope_reg,    // fully readable
            2 => self.poly_reg,        // fully readable
            3 => self.control | 0xBF,  // trigger wo, bits 5:0 unused
            _ => 0xFF,
        }
    }

    /// Write a register by index.
    pub fn write(&mut self, reg_index: u8, value: u8, frame_step: u8) {
        match reg_index {
            0 => {
                self.length_reg = value;
                self.length.write_length((value & 0x3F) as u16);
            }
            1 => {
                self.envelope_reg = value;
                self.dac_enabled = value & 0xF8 != 0;
                if !self.dac_enabled {
                    self.enabled = false;
                }
            }
            2 => self.poly_reg = value,
            3 => {
                self.control = value;
                let was_enabled = self.length.enabled;
                let now_enabled = value & 0x40 != 0;
                self.length.enabled = now_enabled;

                let trigger = value & 0x80 != 0;
                if !was_enabled && now_enabled && frame_step & 1 == 1 && !trigger {
                    if self.length.tick() {
                        self.enabled = false;
                    }
                }

                if trigger {
                    self.trigger(frame_step);
                }
            }
            _ => {}
        }
    }

    /// Handle trigger event.
    fn trigger(&mut self, frame_step: u8) {
        if self.dac_enabled {
            self.enabled = true;
        }
        self.length.trigger();

        if self.length.enabled && frame_step & 1 == 1 {
            if self.length.tick() {
                self.enabled = false;
            }
        }

        self.envelope.trigger(self.envelope_reg);
        self.lfsr = 0x7FFF;
        self.period_timer = self.timer_reload();
    }

    /// Compute the period timer reload value from NR43.
    fn timer_reload(&self) -> u16 {
        let shift = (self.poly_reg >> 4) as u16;
        let divisor_code = self.poly_reg & 0x07;
        let base_divisor: u16 = if divisor_code == 0 { 8 } else { divisor_code as u16 * 16 };
        base_divisor << shift
    }

    /// Clock the period timer (called every T-cycle).
    /// Advances the LFSR when the timer expires.
    pub fn tick_period(&mut self) {
        if self.period_timer > 0 {
            self.period_timer -= 1;
        }
        if self.period_timer == 0 {
            self.period_timer = self.timer_reload();
            let xor_bit = (self.lfsr & 1) ^ ((self.lfsr >> 1) & 1);
            self.lfsr >>= 1;
            self.lfsr |= xor_bit << 14; // set bit 14
            // 7-bit mode: also set bit 6
            if self.poly_reg & 0x08 != 0 {
                self.lfsr = (self.lfsr & !0x40) | (xor_bit << 6);
            }
        }
    }

    /// Current output of the LFSR (0 or 1). Output is inverted bit 0.
    pub fn lfsr_output(&self) -> u8 {
        (!self.lfsr & 1) as u8
    }

    /// DAC output as analog value in -1.0..+1.0.
    /// DAC off → 0.0. Channel disabled → DAC zero-point (-1.0).
    pub fn dac_output(&self) -> f32 {
        if !self.dac_enabled {
            return 0.0;
        }
        let digital = if self.enabled {
            self.lfsr_output() as f32 * self.envelope.volume as f32
        } else {
            0.0
        };
        (digital / 7.5) - 1.0
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
        self.length_reg = 0;
        self.envelope_reg = 0;
        self.poly_reg = 0;
        self.control = 0;
        self.enabled = false;
        self.dac_enabled = false;
        self.length.power_off();
        self.envelope.power_off();
        self.period_timer = 0;
        self.lfsr = 0x7FFF;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_write_only() {
        let mut ch = NoiseChannel::new();
        ch.write(0, 0x2A, 0);
        assert_eq!(ch.read(0), 0xFF);
        assert_eq!(ch.length_reg, 0x2A);
    }

    #[test]
    fn envelope_fully_readable() {
        let mut ch = NoiseChannel::new();
        ch.write(1, 0xF3, 0);
        assert_eq!(ch.read(1), 0xF3);
    }

    #[test]
    fn poly_reg_fully_readable() {
        let mut ch = NoiseChannel::new();
        ch.write(2, 0x5A, 0);
        assert_eq!(ch.read(2), 0x5A);
    }

    #[test]
    fn control_trigger_reads_zero() {
        let mut ch = NoiseChannel::new();
        ch.write(1, 0xF0, 0); // DAC on
        ch.write(3, 0xC0, 0); // trigger + length enable
        assert_eq!(ch.read(3), 0xFF); // 0xC0 | 0xBF
    }

    #[test]
    fn trigger_enables_when_dac_on() {
        let mut ch = NoiseChannel::new();
        ch.write(1, 0xF0, 0); // DAC on
        ch.write(3, 0x80, 0); // trigger
        assert!(ch.enabled);
    }

    #[test]
    fn trigger_does_not_enable_when_dac_off() {
        let mut ch = NoiseChannel::new();
        ch.write(1, 0x00, 0); // DAC off
        ch.write(3, 0x80, 0); // trigger
        assert!(!ch.enabled);
    }

    #[test]
    fn dac_off_disables_channel() {
        let mut ch = NoiseChannel::new();
        ch.write(1, 0xF0, 0);
        ch.write(3, 0x80, 0);
        assert!(ch.enabled);
        ch.write(1, 0x07, 0); // bits 7:3 = 0 → DAC off
        assert!(!ch.enabled);
    }

    #[test]
    fn power_off_resets_all() {
        let mut ch = NoiseChannel::new();
        ch.write(1, 0xF0, 0);
        ch.write(2, 0x5A, 0);
        ch.write(3, 0x80, 0);
        assert!(ch.enabled);

        ch.power_off();
        assert!(!ch.enabled);
        assert!(!ch.dac_enabled);
        assert_eq!(ch.envelope_reg, 0);
        assert_eq!(ch.poly_reg, 0);
    }

    #[test]
    fn trigger_resets_lfsr() {
        let mut ch = NoiseChannel::new();
        ch.lfsr = 0x1234;
        ch.write(1, 0xF0, 0); // DAC on
        ch.write(3, 0x80, 0); // trigger
        assert_eq!(ch.lfsr, 0x7FFF);
    }

    #[test]
    fn lfsr_15bit_mode() {
        let mut ch = NoiseChannel::new();
        ch.write(2, 0x00, 0); // shift=0, 15-bit, divisor=0
        ch.write(1, 0xF0, 0);
        ch.write(3, 0x80, 0); // trigger → LFSR = 0x7FFF

        // Output is inverted bit 0
        assert_eq!(ch.lfsr_output(), 0); // bit 0 of 0x7FFF is 1 → inverted = 0

        // Clock: XOR bits 0,1 of 0x7FFF = 1^1 = 0
        // shift right: 0x3FFF, set bit 14 with 0: 0x3FFF
        ch.period_timer = 0;
        ch.tick_period();
        assert_eq!(ch.lfsr, 0x3FFF);
        assert_eq!(ch.lfsr_output(), 0); // bit 0 is still 1
    }

    #[test]
    fn lfsr_7bit_mode() {
        let mut ch = NoiseChannel::new();
        ch.write(2, 0x08, 0); // shift=0, 7-bit mode, divisor=0
        ch.write(1, 0xF0, 0);
        ch.write(3, 0x80, 0); // trigger → LFSR = 0x7FFF

        ch.period_timer = 0;
        ch.tick_period();
        // XOR bits 0,1 of 0x7FFF = 0, shift right = 0x3FFF
        // 7-bit mode: also set bit 6 with XOR result (0)
        assert_eq!(ch.lfsr & 0x40, 0x00); // bit 6 cleared
    }
}

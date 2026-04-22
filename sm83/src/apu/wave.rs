/// Wave channel (CH3).
///
/// Plays back 32 4-bit samples from wave RAM (FF30-FF3F).
/// Registers: NR30 (FF1A) - NR34 (FF1E).

use super::length::LengthCounter;

#[derive(Clone)]
pub struct WaveChannel {
    /// NR30: DAC enable (bit 7).
    pub dac_enable: u8,
    /// NR31: length timer load (8-bit, write-only).
    pub length_reg: u8,
    /// NR32: output level (bits 6:5).
    pub output_level: u8,
    /// NR33: period low byte (write-only).
    pub period_low: u8,
    /// NR34: trigger (7, wo), length enable (6), period high (2:0, wo).
    pub period_high_ctrl: u8,
    /// Wave pattern RAM: 16 bytes = 32 4-bit samples.
    pub wave_ram: [u8; 16],
    /// Whether the channel is currently producing output.
    pub enabled: bool,
    /// Length counter (256-step for wave channel).
    pub length: LengthCounter,
    /// Period timer countdown (T-cycles).
    pub period_timer: u16,
    /// Wave position (0-31), indexes into 32 4-bit samples.
    pub wave_position: u8,
    /// Current sample value (4-bit).
    pub sample_buffer: u8,
}

impl WaveChannel {
    pub fn new() -> Self {
        Self {
            dac_enable: 0,
            length_reg: 0,
            output_level: 0,
            period_low: 0,
            period_high_ctrl: 0,
            wave_ram: [0; 16],
            enabled: false,
            length: LengthCounter::new(256),
            period_timer: 0,
            wave_position: 0,
            sample_buffer: 0,
        }
    }

    /// Read a register by index (0-4 within the channel's block).
    pub fn read(&self, reg_index: u8) -> u8 {
        match reg_index {
            0 => self.dac_enable | 0x7F,  // bits 6:0 unused
            1 => 0xFF,                     // length: write-only
            2 => self.output_level | 0x9F, // bits 7,4:0 unused
            3 => 0xFF,                     // period low: write-only
            4 => self.period_high_ctrl | 0xBF, // trigger wo, bits 5:3 unused
            _ => 0xFF,
        }
    }

    /// Write a register by index.
    pub fn write(&mut self, reg_index: u8, value: u8, frame_step: u8) {
        match reg_index {
            0 => {
                self.dac_enable = value;
                if value & 0x80 == 0 {
                    self.enabled = false;
                }
            }
            1 => {
                self.length_reg = value;
                self.length.write_length(value as u16);
            }
            2 => self.output_level = value,
            3 => self.period_low = value,
            4 => {
                self.period_high_ctrl = value;
                let was_enabled = self.length.enabled;
                let now_enabled = value & 0x40 != 0;
                self.length.enabled = now_enabled;

                let trigger = value & 0x80 != 0;

                if !was_enabled && now_enabled && frame_step & 1 == 1
                    && self.length.counter > 0
                {
                    if self.length.tick() {
                        if !trigger {
                            self.enabled = false;
                        }
                    }
                }

                if trigger {
                    self.trigger(frame_step);
                }
            }
            _ => {}
        }
    }

    /// Read wave RAM byte.
    ///
    /// DMG quirk: while CH3 is actively playing, all reads return the byte
    /// at the current playback position, ignoring the address.
    pub fn read_wave_ram(&self, offset: u8) -> u8 {
        if self.enabled {
            self.wave_ram[(self.wave_position / 2) as usize]
        } else {
            self.wave_ram[offset as usize & 0x0F]
        }
    }

    /// Write wave RAM byte.
    ///
    /// DMG quirk: while CH3 is actively playing, all writes go to the byte
    /// at the current playback position, ignoring the address.
    pub fn write_wave_ram(&mut self, offset: u8, value: u8) {
        if self.enabled {
            let idx = (self.wave_position / 2) as usize;
            self.wave_ram[idx] = value;
        } else {
            self.wave_ram[offset as usize & 0x0F] = value;
        }
    }

    /// Whether the DAC is enabled (NR30 bit 7).
    pub fn dac_enabled(&self) -> bool {
        self.dac_enable & 0x80 != 0
    }

    /// Handle trigger event.
    fn trigger(&mut self, frame_step: u8) {
        if self.dac_enabled() {
            self.enabled = true;
        }
        self.length.trigger();

        if self.length.enabled && frame_step & 1 == 1
            && self.length.counter == self.length.max_length
        {
            self.length.tick();
        }

        self.period_timer = (2048 - self.period()) * 2;
        self.wave_position = 0;
    }

    /// Clock the period timer (called every T-cycle).
    /// Advances wave position and reads next sample when timer expires.
    pub fn tick_period(&mut self) {
        if self.period_timer > 0 {
            self.period_timer -= 1;
        }
        if self.period_timer == 0 {
            self.period_timer = (2048 - self.period()) * 2;
            self.wave_position = (self.wave_position + 1) & 31;
            // Each byte holds two 4-bit samples: high nibble first
            let byte = self.wave_ram[(self.wave_position / 2) as usize];
            self.sample_buffer = if self.wave_position & 1 == 0 {
                byte >> 4
            } else {
                byte & 0x0F
            };
        }
    }

    /// Current output sample, shifted by the output level.
    pub fn wave_output(&self) -> u8 {
        let shift = match (self.output_level >> 5) & 0x03 {
            0 => 4, // mute
            1 => 0, // 100%
            2 => 1, // 50%
            3 => 2, // 25%
            _ => unreachable!(),
        };
        self.sample_buffer >> shift
    }

    /// DAC output as analog value in -1.0..+1.0.
    /// DAC off → 0.0. Channel disabled → DAC zero-point (-1.0).
    pub fn dac_output(&self) -> f32 {
        if !self.dac_enabled() {
            return 0.0;
        }
        let digital = if self.enabled {
            self.wave_output() as f32
        } else {
            0.0
        };
        (digital / 7.5) - 1.0
    }

    /// Clock the length counter. Disables channel if it expires.
    pub fn tick_length(&mut self) {
        if self.length.tick() {
            self.enabled = false;
        }
    }

    /// 11-bit period from NR33 + NR34 bits 2:0.
    pub fn period(&self) -> u16 {
        let hi = (self.period_high_ctrl & 0x07) as u16;
        let lo = self.period_low as u16;
        (hi << 8) | lo
    }

    /// Reset all state (power off).
    pub fn power_off(&mut self) {
        self.dac_enable = 0;
        self.length_reg = 0;
        self.output_level = 0;
        self.period_low = 0;
        self.period_high_ctrl = 0;
        self.enabled = false;
        self.length.power_off();
        self.period_timer = 0;
        self.wave_position = 0;
        self.sample_buffer = 0;
        // Wave RAM is NOT cleared on power off
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dac_enable_readable() {
        let mut ch = WaveChannel::new();
        ch.write(0, 0x80, 0);
        assert_eq!(ch.read(0), 0xFF); // 0x80 | 0x7F
        ch.write(0, 0x00, 0);
        assert_eq!(ch.read(0), 0x7F); // 0x00 | 0x7F
    }

    #[test]
    fn length_write_only() {
        let mut ch = WaveChannel::new();
        ch.write(1, 0x42, 0);
        assert_eq!(ch.read(1), 0xFF);
        assert_eq!(ch.length_reg, 0x42);
    }

    #[test]
    fn output_level_readable() {
        let mut ch = WaveChannel::new();
        ch.write(2, 0x60, 0);
        assert_eq!(ch.read(2), 0xFF); // 0x60 | 0x9F
        ch.write(2, 0x20, 0);
        assert_eq!(ch.read(2), 0xBF); // 0x20 | 0x9F
    }

    #[test]
    fn trigger_enables_when_dac_on() {
        let mut ch = WaveChannel::new();
        ch.write(0, 0x80, 0); // DAC on
        ch.write(4, 0x80, 0); // trigger
        assert!(ch.enabled);
    }

    #[test]
    fn trigger_does_not_enable_when_dac_off() {
        let mut ch = WaveChannel::new();
        ch.write(0, 0x00, 0); // DAC off
        ch.write(4, 0x80, 0); // trigger
        assert!(!ch.enabled);
    }

    #[test]
    fn dac_off_disables_channel() {
        let mut ch = WaveChannel::new();
        ch.write(0, 0x80, 0); // DAC on
        ch.write(4, 0x80, 0); // trigger
        assert!(ch.enabled);
        ch.write(0, 0x00, 0); // DAC off
        assert!(!ch.enabled);
    }

    #[test]
    fn wave_ram_round_trip() {
        let mut ch = WaveChannel::new();
        for i in 0..16u8 {
            ch.write_wave_ram(i, i * 0x11);
        }
        for i in 0..16u8 {
            assert_eq!(ch.read_wave_ram(i), i * 0x11);
        }
    }

    #[test]
    fn power_off_preserves_wave_ram() {
        let mut ch = WaveChannel::new();
        for i in 0..16u8 {
            ch.write_wave_ram(i, 0xAB);
        }
        ch.power_off();
        for i in 0..16u8 {
            assert_eq!(ch.read_wave_ram(i), 0xAB);
        }
    }

    #[test]
    fn period_combines_low_and_high() {
        let mut ch = WaveChannel::new();
        ch.write(3, 0xFF, 0);
        ch.write(4, 0x07, 0);
        assert_eq!(ch.period(), 0x7FF);
    }

    #[test]
    fn trigger_resets_wave_position() {
        let mut ch = WaveChannel::new();
        ch.wave_position = 15;
        ch.write(0, 0x80, 0); // DAC on
        ch.write(4, 0x80, 0); // trigger
        assert_eq!(ch.wave_position, 0);
    }

    #[test]
    fn wave_reads_correct_samples() {
        let mut ch = WaveChannel::new();
        // Wave RAM byte 0 = 0xAB → sample 0 = 0xA, sample 1 = 0xB
        ch.write_wave_ram(0, 0xAB);
        ch.write(0, 0x80, 0); // DAC on
        ch.write(2, 0x20, 0); // output level = 01 (100%, no shift)
        ch.write(4, 0x80, 0); // trigger → position = 0

        // Position 0: high nibble of byte 0 = 0xA
        // Need to advance once since trigger sets position to 0
        // and tick_period advances on timer expiry
        ch.period_timer = 0;
        ch.tick_period(); // position → 1
        assert_eq!(ch.sample_buffer, 0xB); // low nibble of byte 0

        ch.period_timer = 0;
        ch.tick_period(); // position → 2
        // byte 1 = 0x00 by default, high nibble
        assert_eq!(ch.sample_buffer, 0x00);
    }
}

/// Game Boy APU (Audio Processing Unit).
///
/// 4 sound channels mapped to I/O registers FF10-FF3F:
///   CH1 (FF10-FF14): Pulse with sweep
///   CH2 (FF16-FF19): Pulse
///   CH3 (FF1A-FF1E): Wave
///   CH4 (FF20-FF23): Noise
///   Master (FF24-FF26): Volume, panning, power
///   Wave RAM (FF30-FF3F): 16 bytes / 32 4-bit samples

pub mod audio_buffer;
pub mod envelope;
pub mod length;
pub mod noise;
pub mod pulse;
pub mod sweep;
pub mod wave;

use audio_buffer::{AudioBuffer, Downsampler, SAMPLE_RATE};
use noise::NoiseChannel;
use pulse::PulseChannel;
use wave::WaveChannel;

// Register addresses
pub const NR10: u16 = 0xFF10;
pub const NR11: u16 = 0xFF11;
pub const NR12: u16 = 0xFF12;
pub const NR13: u16 = 0xFF13;
pub const NR14: u16 = 0xFF14;

pub const NR21: u16 = 0xFF16;
pub const NR22: u16 = 0xFF17;
pub const NR23: u16 = 0xFF18;
pub const NR24: u16 = 0xFF19;

pub const NR30: u16 = 0xFF1A;
pub const NR31: u16 = 0xFF1B;
pub const NR32: u16 = 0xFF1C;
pub const NR33: u16 = 0xFF1D;
pub const NR34: u16 = 0xFF1E;

pub const NR41: u16 = 0xFF20;
pub const NR42: u16 = 0xFF21;
pub const NR43: u16 = 0xFF22;
pub const NR44: u16 = 0xFF23;

pub const NR50: u16 = 0xFF24;
pub const NR51: u16 = 0xFF25;
pub const NR52: u16 = 0xFF26;

const WAVE_RAM_START: u16 = 0xFF30;
const WAVE_RAM_END: u16 = 0xFF3F;

#[derive(Clone)]
pub struct Apu {
    /// CH1: pulse with sweep.
    pub ch1: PulseChannel,
    /// CH2: pulse without sweep.
    pub ch2: PulseChannel,
    /// CH3: wave.
    pub ch3: WaveChannel,
    /// CH4: noise.
    pub ch4: NoiseChannel,
    /// NR50: master volume / VIN panning.
    pub nr50: u8,
    /// NR51: sound panning (which channels to left/right).
    pub nr51: u8,
    /// NR52 bit 7: master power.
    power: bool,
    /// Frame sequencer step (0-7), advanced by DIV-APU falling edge.
    pub frame_step: u8,
    /// Latest mixed+filtered stereo output.
    output_left: f32,
    output_right: f32,
    /// High-pass filter capacitor state (left/right).
    hp_cap_left: f32,
    hp_cap_right: f32,
    /// Downsampler: converts T-cycle rate to output sample rate.
    downsampler: Downsampler,
    /// Ring buffer of downsampled stereo audio.
    audio_buf: AudioBuffer,
}

impl Apu {
    pub fn new() -> Self {
        Self {
            ch1: PulseChannel::new(true),
            ch2: PulseChannel::new(false),
            ch3: WaveChannel::new(),
            ch4: NoiseChannel::new(),
            nr50: 0,
            nr51: 0,
            power: false,
            frame_step: 0,
            output_left: 0.0,
            output_right: 0.0,
            hp_cap_left: 0.0,
            hp_cap_right: 0.0,
            downsampler: Downsampler::new(
                crate::timer::MASTER_CLOCK_HZ,
                SAMPLE_RATE,
            ),
            audio_buf: AudioBuffer::new(),
        }
    }

    pub fn read(&self, addr: u16) -> u8 {
        // Wave RAM is always accessible
        if addr >= WAVE_RAM_START && addr <= WAVE_RAM_END {
            return self.ch3.read_wave_ram((addr - WAVE_RAM_START) as u8);
        }

        // NR52 is special: power bit + read-only channel status bits
        if addr == NR52 {
            let power_bit = if self.power { 0x80 } else { 0x00 };
            let status = (self.ch1.enabled as u8)
                | ((self.ch2.enabled as u8) << 1)
                | ((self.ch3.enabled as u8) << 2)
                | ((self.ch4.enabled as u8) << 3);
            return power_bit | 0x70 | status;
        }

        // When powered off, all registers read their mask
        if !self.power {
            return read_mask(addr);
        }

        match addr {
            // CH1: FF10-FF14
            0xFF10..=0xFF14 => self.ch1.read((addr - 0xFF10) as u8),
            // CH2: FF16-FF19 (no sweep register at FF15)
            0xFF16..=0xFF19 => self.ch2.read((addr - 0xFF15) as u8),
            // CH3: FF1A-FF1E
            0xFF1A..=0xFF1E => self.ch3.read((addr - 0xFF1A) as u8),
            // CH4: FF20-FF23
            0xFF20..=0xFF23 => self.ch4.read((addr - 0xFF20) as u8),
            // Master registers
            NR50 => self.nr50,
            NR51 => self.nr51,
            // Unused addresses: FF15, FF1F, FF27-FF2F
            _ => 0xFF,
        }
    }

    pub fn write(&mut self, addr: u16, value: u8) {
        // Wave RAM is always writable
        if addr >= WAVE_RAM_START && addr <= WAVE_RAM_END {
            self.ch3.write_wave_ram((addr - WAVE_RAM_START) as u8, value);
            return;
        }

        // NR52: only bit 7 is writable
        if addr == NR52 {
            let was_on = self.power;
            self.power = value & 0x80 != 0;
            if was_on && !self.power {
                self.power_off_channels();
            }
            return;
        }

        // When powered off, all writes except NR52 and wave RAM are ignored
        if !self.power {
            return;
        }

        match addr {
            // CH1: FF10-FF14
            0xFF10..=0xFF14 => self.ch1.write((addr - 0xFF10) as u8, value),
            // CH2: FF16-FF19
            0xFF16..=0xFF19 => self.ch2.write((addr - 0xFF15) as u8, value),
            // CH3: FF1A-FF1E
            0xFF1A..=0xFF1E => self.ch3.write((addr - 0xFF1A) as u8, value),
            // CH4: FF20-FF23
            0xFF20..=0xFF23 => self.ch4.write((addr - 0xFF20) as u8, value),
            // Master registers
            NR50 => self.nr50 = value,
            NR51 => self.nr51 = value,
            // Unused addresses: writes ignored
            _ => {}
        }
    }

    /// Called every T-cycle from the system loop.
    /// `div_apu_fell` is true when the DIV-APU bit (bit 12 of system counter)
    /// transitions from 1→0, producing a 512 Hz clock.
    pub fn tick(&mut self, div_apu_fell: bool) {
        if !self.power {
            self.update_output();
            return;
        }

        // Period timers tick every T-cycle
        self.ch1.tick_period();
        self.ch2.tick_period();
        self.ch3.tick_period();
        self.ch4.tick_period();

        // Update mixed output every T-cycle (before frame sequencer,
        // so the sample reflects the state after period timer advancement)
        self.update_output();

        if !div_apu_fell {
            return;
        }

        match self.frame_step {
            0 | 4 => {
                // Clock length counters (256 Hz)
                self.ch1.tick_length();
                self.ch2.tick_length();
                self.ch3.tick_length();
                self.ch4.tick_length();
            }
            2 | 6 => {
                // Clock length counters + sweep (128 Hz)
                self.ch1.tick_length();
                self.ch2.tick_length();
                self.ch3.tick_length();
                self.ch4.tick_length();
                // Clock sweep (CH1 only)
                self.ch1.tick_sweep();
            }
            7 => {
                // Clock volume envelopes (64 Hz)
                self.ch1.tick_envelope();
                self.ch2.tick_envelope();
                self.ch4.tick_envelope();
                // CH3 has no envelope
            }
            _ => {}
        }

        self.frame_step = (self.frame_step + 1) & 7;
    }

    /// Latest stereo sample after mixing and high-pass filtering.
    /// Returns (left, right) in approximately -1.0..+1.0.
    pub fn sample(&self) -> (f32, f32) {
        (self.output_left, self.output_right)
    }

    /// Mix all 4 channels according to NR51 panning and NR50 master volume.
    fn mix_raw(&self) -> (f32, f32) {
        if !self.power {
            return (0.0, 0.0);
        }

        let ch_out = [
            self.ch1.dac_output(),
            self.ch2.dac_output(),
            self.ch3.dac_output(),
            self.ch4.dac_output(),
        ];

        let mut left = 0.0f32;
        let mut right = 0.0f32;

        for i in 0..4 {
            if self.nr51 & (1 << (i + 4)) != 0 {
                left += ch_out[i];
            }
            if self.nr51 & (1 << i) != 0 {
                right += ch_out[i];
            }
        }

        // Normalize: 4 channels summed
        left /= 4.0;
        right /= 4.0;

        // Apply NR50 master volume (0-7 → scale 1/8..1)
        let left_vol = ((self.nr50 >> 4) & 0x07) as f32;
        let right_vol = (self.nr50 & 0x07) as f32;
        left *= (left_vol + 1.0) / 8.0;
        right *= (right_vol + 1.0) / 8.0;

        (left, right)
    }

    /// Update the mixed output with high-pass filtering.
    /// Called every T-cycle from tick(). Pushes to the audio buffer
    /// when the downsampler indicates a sample is due.
    fn update_output(&mut self) {
        let (left, right) = self.mix_raw();
        // High-pass filter: capacitor-coupled AC output.
        // Charge factor ~0.999958 per T-cycle at 4.194304 MHz.
        const CHARGE_FACTOR: f32 = 0.999958;
        self.output_left = left - self.hp_cap_left;
        self.hp_cap_left = left - self.output_left * CHARGE_FACTOR;
        self.output_right = right - self.hp_cap_right;
        self.hp_cap_right = right - self.output_right * CHARGE_FACTOR;

        if self.downsampler.tick() {
            self.audio_buf.push(self.output_left, self.output_right);
        }
    }

    /// Drain all buffered audio samples into `out` (interleaved L,R,L,R...).
    /// Call this from the frontend after each frame / audio callback.
    pub fn drain_audio_samples(&mut self, out: &mut Vec<f32>) {
        self.audio_buf.drain(out);
    }

    /// Number of stereo sample pairs buffered and ready to drain.
    pub fn audio_samples_available(&self) -> usize {
        self.audio_buf.available()
    }

    /// Zero all channel registers and master regs on power off.
    fn power_off_channels(&mut self) {
        self.ch1.power_off();
        self.ch2.power_off();
        self.ch3.power_off();
        self.ch4.power_off();
        self.nr50 = 0;
        self.nr51 = 0;
        self.frame_step = 0;
        self.hp_cap_left = 0.0;
        self.hp_cap_right = 0.0;
        self.downsampler.reset();
        self.audio_buf.clear();
    }
}

/// Read masks: bits that always read as 1 (unused/write-only bits).
/// Used when APU is powered off to return correct values.
fn read_mask(addr: u16) -> u8 {
    match addr {
        NR10 => 0x80,
        NR11 => 0x3F,
        NR12 => 0x00,
        NR13 => 0xFF,
        NR14 => 0xBF,
        NR21 => 0x3F,
        NR22 => 0x00,
        NR23 => 0xFF,
        NR24 => 0xBF,
        NR30 => 0x7F,
        NR31 => 0xFF,
        NR32 => 0x9F,
        NR33 => 0xFF,
        NR34 => 0xBF,
        NR41 => 0xFF,
        NR42 => 0x00,
        NR43 => 0x00,
        NR44 => 0xBF,
        NR50 => 0x00,
        NR51 => 0x00,
        NR52 => 0x70,
        _ => 0xFF,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Read mask semantics ──────────────────────────────────────────

    #[test]
    fn nr10_bit7_reads_as_one() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80); // power on
        apu.write(NR10, 0x00);
        assert_eq!(apu.read(NR10), 0x80);
    }

    #[test]
    fn nr11_length_bits_read_as_ones() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);
        apu.write(NR11, 0xC0); // duty = 11, length = 0
        assert_eq!(apu.read(NR11), 0xFF); // 0xC0 | 0x3F
    }

    #[test]
    fn nr12_fully_readable() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);
        apu.write(NR12, 0xA5);
        assert_eq!(apu.read(NR12), 0xA5);
    }

    #[test]
    fn nr13_fully_write_only() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);
        apu.write(NR13, 0x42);
        assert_eq!(apu.read(NR13), 0xFF);
    }

    #[test]
    fn nr14_trigger_reads_zero_unused_bits_read_one() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);
        apu.write(NR14, 0xFF);
        // Trigger (bit 7) reads 0, bits 5:3 read 1, bits 2:0 (period high) write-only → read with mask
        assert_eq!(apu.read(NR14), 0xFF); // 0xFF | 0xBF = 0xFF
        apu.write(NR14, 0x40); // only length enable set
        assert_eq!(apu.read(NR14), 0xFF); // 0x40 | 0xBF = 0xFF
    }

    #[test]
    fn nr30_bits_6_to_0_read_as_ones() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);
        apu.write(NR30, 0x80); // DAC enable only
        assert_eq!(apu.read(NR30), 0xFF); // 0x80 | 0x7F
        apu.write(NR30, 0x00);
        assert_eq!(apu.read(NR30), 0x7F); // 0x00 | 0x7F
    }

    #[test]
    fn nr32_output_level_readable() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);
        apu.write(NR32, 0x60); // output level = 11
        assert_eq!(apu.read(NR32), 0xFF); // 0x60 | 0x9F
        apu.write(NR32, 0x00);
        assert_eq!(apu.read(NR32), 0x9F); // 0x00 | 0x9F
    }

    #[test]
    fn nr41_fully_write_only() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);
        apu.write(NR41, 0x2A);
        assert_eq!(apu.read(NR41), 0xFF);
    }

    #[test]
    fn nr42_nr43_fully_readable() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);
        apu.write(NR42, 0xF3);
        apu.write(NR43, 0x5A);
        assert_eq!(apu.read(NR42), 0xF3);
        assert_eq!(apu.read(NR43), 0x5A);
    }

    #[test]
    fn nr44_trigger_reads_zero() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);
        apu.write(NR44, 0xC0); // trigger + length enable
        assert_eq!(apu.read(NR44), 0xFF); // 0xC0 | 0xBF
    }

    #[test]
    fn nr50_nr51_fully_readable() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);
        apu.write(NR50, 0x77);
        apu.write(NR51, 0xFF);
        assert_eq!(apu.read(NR50), 0x77);
        assert_eq!(apu.read(NR51), 0xFF);
    }

    #[test]
    fn ch2_read_masks() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);
        apu.write(NR21, 0x80); // duty = 10, length = 0
        assert_eq!(apu.read(NR21), 0xBF); // 0x80 | 0x3F
        apu.write(NR23, 0xAB);
        assert_eq!(apu.read(NR23), 0xFF); // write-only
        apu.write(NR24, 0x40);
        assert_eq!(apu.read(NR24), 0xFF); // 0x40 | 0xBF
    }

    #[test]
    fn ch3_read_masks() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);
        apu.write(NR31, 0x42);
        assert_eq!(apu.read(NR31), 0xFF); // write-only
        apu.write(NR33, 0x42);
        assert_eq!(apu.read(NR33), 0xFF); // write-only
        apu.write(NR34, 0x40);
        assert_eq!(apu.read(NR34), 0xFF); // 0x40 | 0xBF
    }

    // ── NR52 power ──────────────────────────────────────────────────

    #[test]
    fn nr52_reads_power_bit_and_unused_bits() {
        let mut apu = Apu::new();
        // Power off: bit 7 = 0, bits 6:4 = 1, bits 3:0 = 0
        assert_eq!(apu.read(NR52), 0x70);
        apu.write(NR52, 0x80);
        // Power on: bit 7 = 1, bits 6:4 = 1, bits 3:0 = 0 (no channels active)
        assert_eq!(apu.read(NR52), 0xF0);
    }

    #[test]
    fn nr52_only_bit7_writable() {
        let mut apu = Apu::new();
        apu.write(NR52, 0xFF); // try to write all bits
        // Only bit 7 should take effect
        assert_eq!(apu.read(NR52), 0xF0); // 0x80 | 0x70 | 0x00 status
    }

    // ── Power on/off ────────────────────────────────────────────────

    #[test]
    fn power_off_zeroes_registers() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80); // power on
        apu.write(NR10, 0x37);
        apu.write(NR12, 0xF3);
        apu.write(NR50, 0x77);
        apu.write(NR51, 0xFF);

        apu.write(NR52, 0x00); // power off

        // All registers should read their mask (zeroed value | mask)
        assert_eq!(apu.read(NR10), 0x80); // mask only
        assert_eq!(apu.read(NR12), 0x00); // mask = 0x00
        assert_eq!(apu.read(NR50), 0x00);
        assert_eq!(apu.read(NR51), 0x00);
    }

    #[test]
    fn power_off_ignores_writes() {
        let mut apu = Apu::new();
        // Don't power on
        apu.write(NR10, 0x37);
        apu.write(NR12, 0xF3);

        // Power on and check — values should not have been stored
        apu.write(NR52, 0x80);
        assert_eq!(apu.read(NR10), 0x80); // mask only, not 0x37 | 0x80
        assert_eq!(apu.read(NR12), 0x00); // not 0xF3
    }

    #[test]
    fn power_off_preserves_wave_ram() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);
        for i in 0..16u8 {
            apu.write(WAVE_RAM_START + i as u16, i * 0x11);
        }

        apu.write(NR52, 0x00); // power off

        // Wave RAM should be preserved
        for i in 0..16u8 {
            assert_eq!(
                apu.read(WAVE_RAM_START + i as u16),
                i * 0x11,
                "wave RAM byte {} not preserved", i
            );
        }
    }

    // ── Wave RAM ────────────────────────────────────────────────────

    #[test]
    fn wave_ram_round_trip() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);
        for i in 0..16u8 {
            apu.write(WAVE_RAM_START + i as u16, 0xA0 + i);
        }
        for i in 0..16u8 {
            assert_eq!(apu.read(WAVE_RAM_START + i as u16), 0xA0 + i);
        }
    }

    #[test]
    fn wave_ram_writable_when_powered_off() {
        let mut apu = Apu::new();
        // Power off (default)
        apu.write(WAVE_RAM_START, 0xAB);
        assert_eq!(apu.read(WAVE_RAM_START), 0xAB);
    }

    // ── Unused addresses ────────────────────────────────────────────

    #[test]
    fn unused_addresses_read_0xff() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);
        // FF15
        assert_eq!(apu.read(0xFF15), 0xFF);
        // FF1F
        assert_eq!(apu.read(0xFF1F), 0xFF);
        // FF27-FF2F
        for addr in 0xFF27..=0xFF2Fu16 {
            assert_eq!(apu.read(addr), 0xFF, "addr {:#06X} should read 0xFF", addr);
        }
    }

    // ── Power cycle ─────────────────────────────────────────────────

    #[test]
    fn power_cycle_resets_registers() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);
        apu.write(NR12, 0xF3);
        assert_eq!(apu.read(NR12), 0xF3);

        // Power off then on
        apu.write(NR52, 0x00);
        apu.write(NR52, 0x80);

        // Register should have been cleared
        assert_eq!(apu.read(NR12), 0x00);
    }

    #[test]
    fn writes_work_after_power_on() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);
        apu.write(NR50, 0x77);
        assert_eq!(apu.read(NR50), 0x77);
        apu.write(NR51, 0xAB);
        assert_eq!(apu.read(NR51), 0xAB);
    }

    // ── All register read masks exhaustive ──────────────────────────

    #[test]
    fn all_readable_registers_apply_correct_mask() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);

        // Write 0x00 to each register, read back should equal the mask
        let test_cases: &[(u16, u8)] = &[
            (NR10, 0x80), (NR11, 0x3F), (NR12, 0x00), (NR13, 0xFF), (NR14, 0xBF),
            (NR21, 0x3F), (NR22, 0x00), (NR23, 0xFF), (NR24, 0xBF),
            (NR30, 0x7F), (NR31, 0xFF), (NR32, 0x9F), (NR33, 0xFF), (NR34, 0xBF),
            (NR41, 0xFF), (NR42, 0x00), (NR43, 0x00), (NR44, 0xBF),
            (NR50, 0x00), (NR51, 0x00),
        ];

        for &(addr, expected_mask) in test_cases {
            apu.write(addr, 0x00);
            assert_eq!(
                apu.read(addr), expected_mask,
                "register {:#06X}: expected mask {:#04X}, got {:#04X}",
                addr, expected_mask, apu.read(addr)
            );
        }
    }

    #[test]
    fn all_readable_registers_preserve_readable_bits() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);

        // Write 0xFF, read back should be 0xFF for all registers
        // (writable bits set + mask bits set = all 1s)
        let addrs: &[u16] = &[
            NR10, NR11, NR12, NR13, NR14,
            NR21, NR22, NR23, NR24,
            NR30, NR31, NR32, NR33, NR34,
            NR41, NR42, NR43, NR44,
            NR50, NR51,
        ];

        for &addr in addrs {
            apu.write(addr, 0xFF);
            assert_eq!(
                apu.read(addr), 0xFF,
                "register {:#06X}: writing 0xFF should read back 0xFF",
                addr
            );
        }
    }

    // ── NR52 status bits reflect channel state ──────────────────────

    #[test]
    fn nr52_status_reflects_channel_enabled() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);

        // No channels enabled
        assert_eq!(apu.read(NR52) & 0x0F, 0x00);

        // Enable CH1 via trigger (need DAC on first)
        apu.write(NR12, 0xF0); // CH1 DAC on
        apu.write(NR14, 0x80); // CH1 trigger
        assert_eq!(apu.read(NR52) & 0x01, 0x01);

        // Enable CH2
        apu.write(NR22, 0xF0); // CH2 DAC on
        apu.write(NR24, 0x80); // CH2 trigger
        assert_eq!(apu.read(NR52) & 0x02, 0x02);

        // Enable CH3
        apu.write(NR30, 0x80); // CH3 DAC on
        apu.write(NR34, 0x80); // CH3 trigger
        assert_eq!(apu.read(NR52) & 0x04, 0x04);

        // Enable CH4
        apu.write(NR42, 0xF0); // CH4 DAC on
        apu.write(NR44, 0x80); // CH4 trigger
        assert_eq!(apu.read(NR52) & 0x08, 0x08);

        // All enabled
        assert_eq!(apu.read(NR52) & 0x0F, 0x0F);
    }

    #[test]
    fn nr52_status_clears_when_dac_off() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);

        // Enable CH1
        apu.write(NR12, 0xF0);
        apu.write(NR14, 0x80);
        assert_eq!(apu.read(NR52) & 0x01, 0x01);

        // Turn DAC off → channel disabled
        apu.write(NR12, 0x00);
        assert_eq!(apu.read(NR52) & 0x01, 0x00);
    }

    // ── Length counter integration ───────────────────────────────────

    #[test]
    fn length_counter_disables_channel_via_tick() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);

        // CH1: DAC on, short length, trigger with length enable
        apu.write(NR11, 0xC0 | 62); // duty=11, length=62 → counter=2
        apu.write(NR12, 0xF0);      // DAC on
        apu.write(NR14, 0xC0);      // trigger + length enable
        assert!(apu.ch1.enabled);

        apu.ch1.tick_length();
        assert!(apu.ch1.enabled); // counter: 2→1

        apu.ch1.tick_length();
        assert!(!apu.ch1.enabled); // counter: 1→0, disabled
        assert_eq!(apu.read(NR52) & 0x01, 0x00);
    }

    #[test]
    fn trigger_reloads_length_when_zero() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);

        // CH1: don't write length (counter stays 0), trigger
        apu.write(NR12, 0xF0);  // DAC on
        apu.write(NR14, 0xC0);  // trigger + length enable
        assert!(apu.ch1.enabled);
        assert_eq!(apu.ch1.length.counter, 64); // reloaded to max
    }

    #[test]
    fn ch3_length_counter_uses_256_steps() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);

        apu.write(NR30, 0x80);  // DAC on
        apu.write(NR31, 0x00);  // length = 0 → counter = 256
        apu.write(NR34, 0xC0);  // trigger + length enable
        assert!(apu.ch3.enabled);
        assert_eq!(apu.ch3.length.counter, 256);
    }

    // ── Frame sequencer ──────────────────────────────────────────────

    #[test]
    fn frame_step_advances_on_div_apu_fell() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);

        assert_eq!(apu.frame_step, 0);
        apu.tick(true);
        assert_eq!(apu.frame_step, 1);
        apu.tick(true);
        assert_eq!(apu.frame_step, 2);
    }

    #[test]
    fn frame_step_does_not_advance_without_div_fell() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);

        apu.tick(false);
        assert_eq!(apu.frame_step, 0);
        apu.tick(false);
        assert_eq!(apu.frame_step, 0);
    }

    #[test]
    fn frame_step_does_not_advance_when_off() {
        let mut apu = Apu::new();
        // power off (default)
        apu.tick(true);
        assert_eq!(apu.frame_step, 0);
    }

    #[test]
    fn frame_step_wraps_7_to_0() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);

        for _ in 0..7 {
            apu.tick(true);
        }
        assert_eq!(apu.frame_step, 7);
        apu.tick(true);
        assert_eq!(apu.frame_step, 0);
    }

    #[test]
    fn frame_sequencer_clocks_length_at_even_steps() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);

        // Set up CH1 with short length
        apu.write(NR11, 0xC0 | 60); // length=60 → counter=4
        apu.write(NR12, 0xF0);
        apu.write(NR14, 0xC0); // trigger + length enable

        let initial = apu.ch1.length.counter;

        // Step 0 clocks length
        apu.tick(true); // step 0→1
        assert_eq!(apu.ch1.length.counter, initial - 1);

        // Step 1 does NOT clock length
        apu.tick(true); // step 1→2
        assert_eq!(apu.ch1.length.counter, initial - 1);

        // Step 2 clocks length
        apu.tick(true); // step 2→3
        assert_eq!(apu.ch1.length.counter, initial - 2);
    }

    #[test]
    fn length_disabled_does_not_tick() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);

        apu.write(NR11, 0xC0 | 63); // length=63 → counter=1
        apu.write(NR12, 0xF0);
        apu.write(NR14, 0x80);       // trigger WITHOUT length enable
        assert!(apu.ch1.enabled);

        apu.ch1.tick_length();
        assert!(apu.ch1.enabled); // should still be enabled — length not active
    }

    // ── Power off completeness ──────────────────────────────────────

    #[test]
    fn power_off_disables_all_channels() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);

        // Enable all channels
        apu.write(NR12, 0xF0);
        apu.write(NR14, 0x80);
        apu.write(NR22, 0xF0);
        apu.write(NR24, 0x80);
        apu.write(NR30, 0x80);
        apu.write(NR34, 0x80);
        apu.write(NR42, 0xF0);
        apu.write(NR44, 0x80);
        assert_eq!(apu.read(NR52) & 0x0F, 0x0F);

        apu.write(NR52, 0x00); // power off
        assert_eq!(apu.read(NR52), 0x70); // all disabled + power off
    }

    #[test]
    fn power_off_resets_internal_state() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);

        // Set up CH1 with envelope, length, sweep
        apu.write(NR10, 0x37);
        apu.write(NR11, 0xC0 | 32);
        apu.write(NR12, 0xF3);
        apu.write(NR13, 0xAB);
        apu.write(NR14, 0xC0); // trigger + length enable

        // Advance some state
        apu.tick(true); // frame step 0
        apu.tick(true); // frame step 1

        apu.write(NR52, 0x00); // power off

        // Verify internal state is reset
        assert_eq!(apu.ch1.envelope.volume, 0);
        assert_eq!(apu.ch1.length.counter, 0);
        assert_eq!(apu.ch1.duty_step, 0);
        assert_eq!(apu.frame_step, 0);
    }

    #[test]
    fn power_off_resets_frame_step() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);

        for _ in 0..5 {
            apu.tick(true);
        }
        assert_eq!(apu.frame_step, 5);

        apu.write(NR52, 0x00);
        assert_eq!(apu.frame_step, 0);
    }

    // ── Integration: simulated sound start ──────────────────────────

    #[test]
    fn ch1_sound_start_sequence() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);

        // Typical game sound init for CH1:
        // Sweep: period=2, shift=1, subtract
        apu.write(NR10, 0x29); // period=2, negate, shift=1
        // Duty 50%, length=20
        apu.write(NR11, 0x80 | 20); // duty=10, length=20
        // Volume 15, down, period=3
        apu.write(NR12, 0xF3);
        // Frequency ~440 Hz → period value
        apu.write(NR13, 0xD6);
        // Trigger + length enable, period high
        apu.write(NR14, 0xC6);

        // Channel should be active
        assert!(apu.ch1.enabled);
        assert_eq!(apu.read(NR52) & 0x01, 0x01);

        // Envelope should be initialized
        assert_eq!(apu.ch1.envelope.volume, 15);

        // Length counter should be loaded: 64 - 20 = 44
        assert_eq!(apu.ch1.length.counter, 44);

        // Duty step not reset by trigger (only by power-off)
        assert_eq!(apu.ch1.duty_step, 0); // was already 0
    }

    #[test]
    fn ch4_noise_start_sequence() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);

        // Typical noise SFX:
        apu.write(NR41, 0x10);  // length = 16
        apu.write(NR42, 0xA1);  // volume=10, down, period=1
        apu.write(NR43, 0x31);  // shift=3, 15-bit, divisor=1
        apu.write(NR44, 0xC0);  // trigger + length enable

        assert!(apu.ch4.enabled);
        assert_eq!(apu.ch4.envelope.volume, 10);
        assert_eq!(apu.ch4.length.counter, 48); // 64 - 16
        assert_eq!(apu.ch4.lfsr, 0x7FFF);
    }

    #[test]
    fn ch3_wave_start_sequence() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);

        // Load wave pattern
        for i in 0..16u8 {
            apu.write(0xFF30 + i as u16, (i << 4) | i);
        }

        apu.write(NR30, 0x80);  // DAC on
        apu.write(NR31, 0x00);  // max length
        apu.write(NR32, 0x20);  // 100% volume
        apu.write(NR33, 0x00);
        apu.write(NR34, 0x80);  // trigger, no length enable

        assert!(apu.ch3.enabled);
        assert_eq!(apu.ch3.wave_position, 0);
        assert_eq!(apu.ch3.length.counter, 256);
    }

    // ── Audio output: DAC, mixing, high-pass filter ──────────────────

    #[test]
    fn silence_when_powered_off() {
        let apu = Apu::new();
        let (l, r) = apu.sample();
        assert_eq!(l, 0.0);
        assert_eq!(r, 0.0);
    }

    #[test]
    fn silence_after_power_off() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);
        apu.write(NR12, 0xF0);
        apu.write(NR14, 0x80);
        apu.write(NR50, 0x77);
        apu.write(NR51, 0xFF);

        apu.write(NR52, 0x00); // power off

        // Tick a few times to let filter settle
        for _ in 0..1000 {
            apu.tick(false);
        }

        let (l, r) = apu.sample();
        assert!(l.abs() < 0.01, "left should be ~0, got {}", l);
        assert!(r.abs() < 0.01, "right should be ~0, got {}", r);
    }

    #[test]
    fn dac_output_range_pulse() {
        let mut ch = PulseChannel::new(true);
        ch.write(2, 0xF0); // DAC on, volume=15
        ch.write(4, 0x80); // trigger

        // duty_output=1, volume=15 → digital=15 → (15/7.5)-1 = +1.0
        ch.duty_step = 7; // step 7 is high for duty 0 (12.5%)
        let out = ch.dac_output();
        assert!((out - 1.0).abs() < 0.001, "expected +1.0, got {}", out);

        // duty_output=0 → digital=0 → (0/7.5)-1 = -1.0
        ch.duty_step = 0;
        let out = ch.dac_output();
        assert!((out - (-1.0)).abs() < 0.001, "expected -1.0, got {}", out);
    }

    #[test]
    fn dac_off_returns_zero() {
        let ch = PulseChannel::new(true);
        // DAC off by default
        assert_eq!(ch.dac_output(), 0.0);
    }

    #[test]
    fn nr51_panning_routes_correctly() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);
        apu.write(NR50, 0x77); // max volume both sides
        apu.write(NR51, 0x10); // CH1 to left only (bit 4)

        // Enable CH1
        apu.write(NR12, 0xF0);
        apu.write(NR14, 0x80);

        let (left, right) = apu.mix_raw();
        // CH1 should contribute to left but not right
        assert!(left.abs() > 0.01, "left should be nonzero");
        assert!(right.abs() < 0.001, "right should be ~0, got {}", right);
    }

    #[test]
    fn nr51_panning_both_sides() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);
        apu.write(NR50, 0x77); // max volume both sides
        apu.write(NR51, 0x11); // CH1 to both left and right

        apu.write(NR12, 0xF0);
        apu.write(NR14, 0x80);

        let (left, right) = apu.mix_raw();
        assert!((left - right).abs() < 0.001, "both sides should be equal");
    }

    #[test]
    fn nr50_volume_scales_output() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);
        apu.write(NR51, 0x11); // CH1 to both
        apu.write(NR12, 0xF0);
        apu.write(NR14, 0x80);

        // Max volume (7)
        apu.write(NR50, 0x77);
        let (l_max, _) = apu.mix_raw();

        // Min volume (0) → 1/8 of max
        apu.write(NR50, 0x00);
        let (l_min, _) = apu.mix_raw();

        if l_max.abs() > 0.001 {
            let ratio = l_min / l_max;
            assert!(
                (ratio - 0.125).abs() < 0.01,
                "vol 0 should be 1/8 of vol 7, ratio = {}", ratio
            );
        }
    }

    #[test]
    fn high_pass_filter_removes_dc() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);
        apu.write(NR50, 0x77);
        apu.write(NR51, 0x11); // CH1 both sides

        // Constant tone: CH1 enabled, duty 50% step stuck at a high position
        apu.write(NR12, 0xF0);
        apu.write(NR14, 0x80);
        // Force a constant output by not ticking period (just tick the filter)
        apu.ch1.duty_step = 4; // high for 50% duty
        apu.ch1.period_timer = u16::MAX; // prevent period timer from firing

        // Run many ticks with constant input
        for _ in 0..100_000 {
            apu.update_output();
        }

        let (l, _) = apu.sample();
        // With constant DC input, high-pass should drive output toward 0
        assert!(l.abs() < 0.1, "HP filter should remove DC, got {}", l);
    }

    // ── Hardware accuracy: brutal edge-case tests ────────────────────

    /// LFSR 15-bit mode must produce exactly 32767 unique states before cycling.
    #[test]
    fn lfsr_15bit_sequence_length() {
        let mut ch = NoiseChannel::new();
        ch.write(2, 0x00); // shift=0, 15-bit, divisor=0
        ch.write(1, 0xF0); // DAC on
        ch.write(3, 0x80); // trigger → LFSR = 0x7FFF

        let initial = ch.lfsr;
        let mut count = 0u32;
        loop {
            ch.period_timer = 0;
            ch.tick_period();
            count += 1;
            if ch.lfsr == initial {
                break;
            }
            assert!(count <= 32767, "LFSR 15-bit did not cycle within 32767 steps");
        }
        assert_eq!(count, 32767, "LFSR 15-bit should cycle after exactly 32767 steps");
    }

    /// LFSR 7-bit mode output must cycle with period 127.
    /// In 7-bit mode, the effective register is 7 bits (2^7 - 1 = 127 states).
    #[test]
    fn lfsr_7bit_sequence_length() {
        let mut ch = NoiseChannel::new();
        ch.write(2, 0x08); // shift=0, 7-bit mode, divisor=0
        ch.write(1, 0xF0); // DAC on
        ch.write(3, 0x80); // trigger → LFSR = 0x7FFF

        // In 7-bit mode, the lower 7 bits form the effective LFSR.
        // After settling, the lower 7 bits should cycle with period 127.
        // First, let the upper bits settle by clocking a few times.
        for _ in 0..15 {
            ch.period_timer = 0;
            ch.tick_period();
        }

        let initial_low7 = ch.lfsr & 0x7F;
        let mut count = 0u32;
        loop {
            ch.period_timer = 0;
            ch.tick_period();
            count += 1;
            if (ch.lfsr & 0x7F) == initial_low7 {
                break;
            }
            assert!(count <= 256, "LFSR 7-bit lower 7 bits did not cycle within 256 steps");
        }
        assert_eq!(count, 127, "LFSR 7-bit should cycle after exactly 127 steps");
    }

    /// Full frame sequencer cycle: verify every step clocks exactly the right components.
    /// Steps 0,2,4,6 → length. Steps 2,6 → also sweep. Step 7 → envelope.
    /// Steps 1,3,5 → nothing.
    #[test]
    fn frame_sequencer_full_cycle_accuracy() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);

        // CH1: length=62→counter=2, envelope vol=15 period=1, sweep period=1 shift=1
        apu.write(NR10, 0x11); // sweep period=1, shift=1, add
        apu.write(NR11, 0x80 | 62); // duty=10, length=62 → counter=2
        apu.write(NR12, 0xF1); // vol=15, down, period=1
        apu.write(NR13, 0x80); // period low = 0x80
        apu.write(NR14, 0xC0); // trigger + length enable, period high = 0 → period = 0x80

        assert!(apu.ch1.enabled);
        let len_before = apu.ch1.length.counter;
        let vol_before = apu.ch1.envelope.volume;
        let sweep_shadow = apu.ch1.sweep.as_ref().unwrap().shadow_freq;

        // Step 0: length clocked
        apu.tick(true);
        assert_eq!(apu.ch1.length.counter, len_before - 1, "step 0 should clock length");
        assert_eq!(apu.ch1.envelope.volume, vol_before, "step 0 should NOT clock envelope");

        // Step 1: nothing clocked
        let len_after_0 = apu.ch1.length.counter;
        apu.tick(true);
        assert_eq!(apu.ch1.length.counter, len_after_0, "step 1 should NOT clock length");
        assert_eq!(apu.ch1.envelope.volume, vol_before, "step 1 should NOT clock envelope");

        // Step 2: length + sweep clocked
        apu.tick(true);
        assert_eq!(apu.ch1.length.counter, len_after_0 - 1, "step 2 should clock length");
        // Sweep should have fired (period=1, shift=1)
        let new_shadow = apu.ch1.sweep.as_ref().unwrap().shadow_freq;
        assert_ne!(new_shadow, sweep_shadow, "step 2 should clock sweep");

        // Step 3: nothing
        let len_after_2 = apu.ch1.length.counter;
        let vol_after_2 = apu.ch1.envelope.volume;
        apu.tick(true);
        assert_eq!(apu.ch1.length.counter, len_after_2, "step 3 should NOT clock length");
        assert_eq!(apu.ch1.envelope.volume, vol_after_2, "step 3 should NOT clock envelope");

        // Step 4: length only
        apu.tick(true);
        // Length counter hit 0 at step 2 (was 1 after step 0, decremented at step 2 → 0)
        // Channel should have been disabled at step 2
        // Actually: counter was 2 initially, step 0 → 1, step 2 → 0 → disabled
        assert!(!apu.ch1.enabled, "channel should be disabled after length expired");

        // Step 5: nothing
        apu.tick(true);

        // Step 6: length + sweep (but channel already disabled)
        apu.tick(true);

        // Step 7: envelope
        // Re-enable a channel to test envelope
        apu.write(NR22, 0xF1); // CH2: vol=15, down, period=1
        apu.write(NR24, 0x80); // trigger
        let vol_ch2 = apu.ch2.envelope.volume;
        apu.tick(true); // step 7 → envelope clocked
        assert_eq!(apu.ch2.envelope.volume, vol_ch2 - 1, "step 7 should clock envelope");

        // Verify wrap: next step should be 0
        assert_eq!(apu.frame_step, 0, "frame step should wrap to 0 after step 7");
    }

    /// Pulse period timer fires after exactly (2048 - period) * 4 T-cycles.
    #[test]
    fn pulse_period_timer_exact_tcycle_count() {
        let mut ch = PulseChannel::new(true);
        ch.write(2, 0xF0); // DAC on
        ch.write(3, 0x00); // period low = 0
        ch.write(4, 0x84); // trigger, period high = 4 → period = 0x400 = 1024

        // Timer should be (2048 - 1024) * 4 = 4096
        let initial_step = ch.duty_step;

        // Tick 4095 times — duty step should NOT advance
        for _ in 0..4095 {
            ch.tick_period();
        }
        assert_eq!(ch.duty_step, initial_step, "duty step should not advance before timer expires");

        // Tick once more — duty step SHOULD advance
        ch.tick_period();
        assert_eq!(ch.duty_step, (initial_step + 1) & 7, "duty step should advance on timer expiry");
    }

    /// Wave period timer fires after exactly (2048 - period) * 2 T-cycles.
    #[test]
    fn wave_period_timer_exact_tcycle_count() {
        let mut ch = WaveChannel::new();
        ch.write_wave_ram(0, 0xAB);
        ch.write_wave_ram(1, 0xCD);
        ch.write(0, 0x80); // DAC on
        ch.write(3, 0x00); // period low = 0
        ch.write(4, 0x84); // trigger, period high = 4 → period = 0x400

        // Timer = (2048 - 1024) * 2 = 2048
        let initial_pos = ch.wave_position;

        for _ in 0..2047 {
            ch.tick_period();
        }
        assert_eq!(ch.wave_position, initial_pos, "wave pos should not advance before timer");

        ch.tick_period();
        assert_eq!(ch.wave_position, (initial_pos + 1) & 31, "wave pos should advance on expiry");
    }

    /// Wave channel reads all 32 4-bit samples in correct order (high nibble first).
    #[test]
    fn wave_channel_reads_all_32_samples() {
        let mut ch = WaveChannel::new();
        // Fill wave RAM so each 4-bit sample has a unique value (0-15 twice).
        // Byte i: high nibble = sample 2i, low nibble = sample 2i+1.
        // Use sample values: position mod 16.
        for i in 0..16u8 {
            let high = (i * 2) & 0x0F;
            let low = (i * 2 + 1) & 0x0F;
            ch.write_wave_ram(i, (high << 4) | low);
        }
        ch.write(0, 0x80); // DAC on
        ch.write(2, 0x20); // output level 100%
        ch.write(3, 0xFF); // period low = 0xFF
        ch.write(4, 0x87); // trigger, period high = 7 → period = 0x7FF

        // Read all 32 samples by forcing timer expiry.
        // tick_period advances position THEN reads the sample at that position.
        let mut samples = Vec::new();
        for _ in 0..32 {
            ch.period_timer = 0;
            ch.tick_period();
            samples.push(ch.sample_buffer);
        }

        // Verify each sample: position advances 1→2→...→31→0
        for i in 0..32u8 {
            let pos = (i + 1) & 31;
            let byte_idx = (pos / 2) as usize;
            let expected = if pos & 1 == 0 {
                ch.wave_ram[byte_idx] >> 4 // high nibble
            } else {
                ch.wave_ram[byte_idx] & 0x0F // low nibble
            };
            assert_eq!(
                samples[i as usize], expected,
                "sample {} (pos {}) should be {}, got {}",
                i, pos, expected, samples[i as usize]
            );
        }
    }

    /// Noise divisor table: verify all 8 codes produce correct timer values.
    #[test]
    fn noise_divisor_table_all_codes() {
        // Pan Docs: code 0=8, 1=16, 2=32, 3=48, 4=64, 5=80, 6=96, 7=112
        // With shift=0, timer = base_divisor << 0 = base_divisor
        let expected: [u16; 8] = [8, 16, 32, 48, 64, 80, 96, 112];

        for code in 0..8u8 {
            let mut ch = NoiseChannel::new();
            ch.write(2, code); // shift=0, 15-bit, divisor=code
            ch.write(1, 0xF0); // DAC on
            ch.write(3, 0x80); // trigger

            assert_eq!(
                ch.period_timer, expected[code as usize],
                "divisor code {} should give timer {}, got {}",
                code, expected[code as usize], ch.period_timer
            );
        }
    }

    /// Noise divisor with shift: timer = base_divisor << shift.
    #[test]
    fn noise_divisor_with_shift() {
        let mut ch = NoiseChannel::new();
        // code=1 (base=16), shift=3 → 16 << 3 = 128
        ch.write(2, 0x31); // shift=3, 15-bit, divisor=1
        ch.write(1, 0xF0);
        ch.write(3, 0x80);
        assert_eq!(ch.period_timer, 128);

        // code=0 (base=8), shift=4 → 8 << 4 = 128
        ch.write(2, 0x40); // shift=4, 15-bit, divisor=0
        ch.write(3, 0x80);
        assert_eq!(ch.period_timer, 128);
    }

    /// Sweep double overflow check: after updating shadow frequency,
    /// the sweep calculates again immediately. If THAT overflows, channel dies.
    #[test]
    fn sweep_double_overflow_check() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);

        // freq=0x400 (1024), sweep period=1, shift=1, add mode
        // First calc: 1024 + 512 = 1536 (ok, ≤ 2047)
        // Second calc (with new shadow 1536): 1536 + 768 = 2304 (overflow! > 2047)
        apu.write(NR10, 0x11); // period=1, shift=1, add
        apu.write(NR12, 0xF0); // DAC on
        apu.write(NR13, 0x00); // period low = 0
        apu.write(NR14, 0x84); // trigger, period high = 4 → period = 0x400
        assert!(apu.ch1.enabled, "channel should be enabled after trigger");

        // Clock sweep (happens at frame step 2)
        apu.frame_step = 2;
        apu.tick(true);

        // The sweep should have: set shadow to 1536, then recalculated 1536+768=2304 > 2047
        // Channel should be disabled by the double overflow check
        assert!(!apu.ch1.enabled, "channel should be disabled by double overflow");
    }

    /// Sweep trigger overflow: if shift!=0 and initial calc overflows, channel dies immediately.
    #[test]
    fn sweep_trigger_immediate_overflow() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);

        // freq=0x700, shift=1 → 0x700 + 0x380 = 0xA80 > 2047
        apu.write(NR10, 0x11); // period=1, shift=1, add
        apu.write(NR12, 0xF0); // DAC on
        apu.write(NR13, 0x00);
        apu.write(NR14, 0x87); // trigger, period = 0x700
        assert!(!apu.ch1.enabled, "channel should be disabled immediately by trigger overflow");
    }

    /// Sweep negate-to-add quirk: using negate, then writing NR10 with add mode
    /// disables the channel even without a sweep tick.
    #[test]
    fn sweep_negate_then_add_disables_channel() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);

        // Start with negate mode
        apu.write(NR10, 0x19); // period=1, negate, shift=1
        apu.write(NR12, 0xF0);
        apu.write(NR13, 0x00);
        apu.write(NR14, 0x82); // trigger, period = 0x200
        assert!(apu.ch1.enabled);

        // Clock sweep so negate is actually used
        apu.frame_step = 2;
        apu.tick(true);
        assert!(apu.ch1.enabled, "should still be enabled after negate sweep");

        // Now write NR10 with add mode → should disable
        apu.write(NR10, 0x11); // add mode
        assert!(!apu.ch1.enabled, "negate-to-add should disable channel");
    }

    /// Envelope does NOT change volume when period is 0, but timer still cycles.
    #[test]
    fn envelope_period_zero_timer_still_cycles() {
        let mut ch = PulseChannel::new(true);
        ch.write(2, 0xF0); // vol=15, down, period=0
        ch.write(4, 0x80); // trigger

        // Timer should be loaded as 8 (period 0 treated as 8)
        // Tick 100 times — volume should never change
        for _ in 0..100 {
            ch.tick_envelope();
        }
        assert_eq!(ch.envelope.volume, 15, "period=0 should never change volume");
    }

    /// DAC output covers full range linearly across all 16 digital values.
    #[test]
    fn dac_output_linearity() {
        let mut ch = PulseChannel::new(true);
        ch.write(2, 0xF0); // DAC on, vol=15
        ch.write(4, 0x80); // trigger
        ch.duty_step = 7; // high output

        // Test all 16 envelope volumes
        for vol in 0..=15u8 {
            ch.envelope.volume = vol;
            let out = ch.dac_output();
            let expected = (vol as f32 / 7.5) - 1.0;
            assert!(
                (out - expected).abs() < 0.001,
                "vol={}: expected {}, got {}", vol, expected, out
            );
        }
    }

    /// When duty_output=0, digital value is always 0 regardless of volume.
    /// This means DAC always outputs -1.0 (its minimum).
    #[test]
    fn dac_output_zero_when_duty_low() {
        let mut ch = PulseChannel::new(true);
        ch.write(2, 0xF0); // DAC on, vol=15
        ch.write(4, 0x80); // trigger
        ch.duty_step = 0; // low output for all duty patterns

        for vol in 0..=15u8 {
            ch.envelope.volume = vol;
            let out = ch.dac_output();
            assert!(
                (out - (-1.0)).abs() < 0.001,
                "duty=low, vol={}: should be -1.0, got {}", vol, out
            );
        }
    }

    /// Power off then on: all channels dead, no leftover state leaking audio.
    #[test]
    fn power_cycle_produces_silence() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);

        // Start all 4 channels with max volume
        apu.write(NR50, 0x77);
        apu.write(NR51, 0xFF);
        apu.write(NR12, 0xF0);
        apu.write(NR14, 0x80);
        apu.write(NR22, 0xF0);
        apu.write(NR24, 0x80);
        apu.write(NR30, 0x80);
        apu.write(NR34, 0x80);
        apu.write(NR42, 0xF0);
        apu.write(NR44, 0x80);

        // Verify something is playing
        let (l, _) = apu.mix_raw();
        assert!(l.abs() > 0.001, "should have audio before power off");

        // Power cycle
        apu.write(NR52, 0x00);
        apu.write(NR52, 0x80);

        // Mix should be silent (no channels enabled, NR50/NR51 zeroed)
        let (l, r) = apu.mix_raw();
        assert_eq!(l, 0.0, "left should be silent after power cycle");
        assert_eq!(r, 0.0, "right should be silent after power cycle");

        // NR52 should show no channels active
        assert_eq!(apu.read(NR52) & 0x0F, 0x00);
    }

    /// Verify all 4 channels mix independently with correct NR51 routing.
    #[test]
    fn all_four_channels_mix_independently() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);
        apu.write(NR50, 0x77); // max volume

        // Enable CH1 only, route to left
        apu.write(NR51, 0x10); // CH1 left only
        apu.write(NR12, 0xF0);
        apu.write(NR14, 0x80);

        let (l1, r1) = apu.mix_raw();
        assert!(l1.abs() > 0.001);
        assert!(r1.abs() < 0.001);

        // Add CH2, route to right
        apu.write(NR51, 0x12); // CH1 left, CH2 right
        apu.write(NR22, 0xF0);
        apu.write(NR24, 0x80);

        let (l2, r2) = apu.mix_raw();
        assert!(l2.abs() > 0.001, "CH1 still on left");
        assert!(r2.abs() > 0.001, "CH2 now on right");

        // Route CH3 to both
        apu.write(NR51, 0x14 | 0x04); // CH1 left, CH3 both
        apu.write(NR30, 0x80);
        apu.write(NR32, 0x20); // 100% volume
        apu.write(NR34, 0x80);

        let (l3, _r3) = apu.mix_raw();
        // Left has CH1 + CH3, right has CH3 only
        assert!(l3.abs() > l1.abs() * 0.5, "left should have CH1+CH3");

        // CH4 to right only
        apu.write(NR51, 0x18 | 0x08); // CH1 left, CH4 right
        apu.write(NR42, 0xF0);
        apu.write(NR44, 0x80);

        let (_, r4) = apu.mix_raw();
        assert!(r4.abs() > 0.001, "CH4 should be on right");
    }

    /// Length counter: triggering with counter=0 reloads to max, for all channels.
    #[test]
    fn trigger_reloads_zero_length_all_channels() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);

        // CH1: 64-step
        apu.write(NR12, 0xF0);
        apu.write(NR14, 0xC0); // trigger + length enable, no length written → counter was 0
        assert_eq!(apu.ch1.length.counter, 64);

        // CH2: 64-step
        apu.write(NR22, 0xF0);
        apu.write(NR24, 0xC0);
        assert_eq!(apu.ch2.length.counter, 64);

        // CH3: 256-step
        apu.write(NR30, 0x80);
        apu.write(NR34, 0xC0);
        assert_eq!(apu.ch3.length.counter, 256);

        // CH4: 64-step
        apu.write(NR42, 0xF0);
        apu.write(NR44, 0xC0);
        assert_eq!(apu.ch4.length.counter, 64);
    }

    /// Wave RAM is preserved across power off but all other wave state resets.
    #[test]
    fn wave_state_resets_but_ram_preserved() {
        let mut apu = Apu::new();
        apu.write(NR52, 0x80);

        // Load a known pattern and start the channel
        for i in 0..16u8 {
            apu.write(WAVE_RAM_START + i as u16, 0xA0 + i);
        }
        apu.write(NR30, 0x80);
        apu.write(NR32, 0x20);
        apu.write(NR34, 0x80);
        assert!(apu.ch3.enabled);

        // Advance wave position
        apu.ch3.period_timer = 0;
        apu.ch3.tick_period();
        assert_ne!(apu.ch3.wave_position, 0);

        // Power off
        apu.write(NR52, 0x00);

        // State should be reset
        assert!(!apu.ch3.enabled);
        assert_eq!(apu.ch3.wave_position, 0);
        assert_eq!(apu.ch3.sample_buffer, 0);

        // But RAM survives
        for i in 0..16u8 {
            assert_eq!(
                apu.read(WAVE_RAM_START + i as u16), 0xA0 + i,
                "wave RAM byte {} should survive power off", i
            );
        }
    }

    #[test]
    #[ignore] // slow: runs 1 second of emulation
    fn boot_rom_produces_audio_samples() {
        use crate::memory::bus::Bus;
        use crate::system::GameBoy;
        use crate::trace::Tracer;

        let mut bus = Bus::new();
        bus.load_boot_rom(crate::BOOT_ROM).unwrap();
        bus.load_cartridge(crate::DEFAULT_ROM);
        let mut gb = GameBoy::new(bus, Tracer::off());

        // Run 1 second, draining audio every ~16ms to avoid buffer overflow
        let mut all_samples = Vec::new();
        let chunk = 4_194_304u32 / 60; // ~1 frame
        for frame in 0..60 {
            for _ in 0..chunk {
                gb.tick();
            }
            let before = all_samples.len();
            gb.bus.apu.drain_audio_samples(&mut all_samples);
            let chunk_nonzero = all_samples[before..]
                .iter().filter(|&&s| s.abs() > 0.001).count();
            if chunk_nonzero > 0 {
                eprintln!("frame {}: {} new samples, {} non-silent",
                    frame, (all_samples.len() - before) / 2, chunk_nonzero);
            }
        }

        let total_pairs = all_samples.len() / 2;
        let nonzero = all_samples.iter().filter(|&&s| s.abs() > 0.001).count();

        eprintln!("audio: {} stereo pairs, {} non-silent ({:.1}%)",
            total_pairs, nonzero,
            nonzero as f64 / all_samples.len().max(1) as f64 * 100.0);
        eprintln!("NR52={:#04x}", gb.bus.apu.read(0xFF26));

        assert!(total_pairs > 0, "should have buffered audio samples");
        assert!(nonzero > 0, "boot ROM should produce audible audio");
    }

    #[test]
    #[ignore] // slow: runs 5 seconds of emulation
    fn diagnose_game_audio() {
        use crate::memory::bus::Bus;
        use crate::system::GameBoy;
        use crate::trace::Tracer;

        let mut bus = Bus::new();
        bus.load_boot_rom(crate::BOOT_ROM).unwrap();
        bus.load_cartridge(crate::DEFAULT_ROM);
        let mut gb = GameBoy::new(bus, Tracer::off());

        let chunk = 4_194_304u32 / 60;
        let mut samples = Vec::new();
        for frame in 0..300 { // 5 seconds
            for _ in 0..chunk {
                gb.tick();
            }

            if frame % 30 == 0 {
                let nr52 = gb.bus.apu.read(0xFF26);
                let nr50 = gb.bus.apu.read(0xFF24);
                let nr51 = gb.bus.apu.read(0xFF25);
                let (l, r) = gb.bus.apu.sample();
                let pc = gb.cpu.register_file.get_16bit(
                    crate::cpu::registers::Reg16::PC);
                let halted = gb.cpu.halted;
                let ly = gb.bus.read(0xFF44);
                let if_val = gb.bus.read(0xFF0F);
                let ie_val = gb.bus.read(0xFFFF);
                let ime = gb.cpu.ime;
                eprintln!(
                    "t={:.1}s: PC={:04X} halt={} IME={} IE={:02X} IF={:02X} LY={:02X} \
                     NR52={:02X} NR50={:02X} NR51={:02X} ch=[{}{}{}{}] s=({:.4},{:.4})",
                    frame as f64 / 60.0,
                    pc, halted, ime, ie_val, if_val, ly,
                    nr52, nr50, nr51,
                    if nr52 & 1 != 0 { '1' } else { '.' },
                    if nr52 & 2 != 0 { '2' } else { '.' },
                    if nr52 & 4 != 0 { '3' } else { '.' },
                    if nr52 & 8 != 0 { '4' } else { '.' },
                    l, r,
                );
                // Channel details
                eprintln!(
                    "  CH1: env={:02X} period={} duty_step={} dac={} vol={}",
                    gb.bus.apu.ch1.envelope_reg,
                    gb.bus.apu.ch1.period(),
                    gb.bus.apu.ch1.duty_step,
                    gb.bus.apu.ch1.dac_enabled,
                    gb.bus.apu.ch1.envelope.volume,
                );
                eprintln!(
                    "  CH2: env={:02X} period={} duty_step={} dac={} vol={}",
                    gb.bus.apu.ch2.envelope_reg,
                    gb.bus.apu.ch2.period(),
                    gb.bus.apu.ch2.duty_step,
                    gb.bus.apu.ch2.dac_enabled,
                    gb.bus.apu.ch2.envelope.volume,
                );
            }

            samples.clear();
            gb.bus.apu.drain_audio_samples(&mut samples);
            let nonzero = samples.iter().filter(|&&s| s.abs() > 0.001).count();
            if nonzero > 0 && frame >= 10 {
                eprintln!("  frame {}: {} non-silent samples", frame, nonzero);
            }
        }
    }

    #[test]
    #[ignore]
    fn trace_boot_ly_loop() {
        use std::fs::File;
        use crate::memory::bus::Bus;
        use crate::system::GameBoy;
        use crate::trace::Tracer;
        use crate::cpu::registers::Reg16;

        let mut bus = Bus::new();
        bus.load_boot_rom(crate::BOOT_ROM).unwrap();
        bus.load_cartridge(crate::DEFAULT_ROM);
        let mut gb = GameBoy::new(bus, Tracer::off());

        // Run for ~15 seconds of emulated time
        for t in 0..62_914_560u64 {
            let pc = gb.cpu.register_file.get_16bit(Reg16::PC);
            if pc >= 0x0100 {
                eprintln!("boot ROM done at T-cycle {} ({:.2}s)", t, t as f64 / 4_194_304.0);
                return;
            }
            gb.tick();
        }
        let pc = gb.cpu.register_file.get_16bit(Reg16::PC);
        eprintln!("did not finish boot after 15s. PC={:04X}", pc);
    }

    #[test]
    fn trace_ly_loop_microops() {
        use crate::cpu::CPU;
        use crate::cpu::registers::{Reg8, Reg16};
        use crate::cpu::microcode::FLAG_Z;
        use crate::memory::bus::Bus;
        use crate::trace::Tracer;

        let mut cpu = CPU::new(Tracer::off());
        let mut bus = Bus::new();

        // Write the LY loop instructions into WRAM starting at 0xC000:
        // F0 44  LDH A,(0x44)
        // FE 90  CP 0x90
        // 20 FA  JR NZ,-6
        // 00     NOP (should reach here if loop exits)
        let code: &[u8] = &[0xF0, 0x44, 0xFE, 0x90, 0x20, 0xFA, 0x00];
        for (i, &b) in code.iter().enumerate() {
            bus.write(0xC000 + i as u16, b);
        }

        // Set LY = 0x90
        bus.write(0xFF44, 0x90);

        // Point PC at our code
        cpu.register_file.set_16bit(Reg16::PC, 0xC000);

        // Each tick now executes a full instruction
        for i in 0..20 {
            let pc = cpu.register_file.get_16bit(Reg16::PC);
            let a = cpu.register_file.get_8bit(Reg8::A);
            let f = cpu.register_file.get_8bit(Reg8::F);
            let z = (f & FLAG_Z) != 0;
            eprintln!("I{:02}: PC={:04X} A={:02X} F={:02X} Z={}",
                i, pc, a, f, z);

            if pc == 0xC006 {
                eprintln!("SUCCESS: loop exited at instruction {}", i);
                return;
            }

            cpu.tick(&mut bus);
        }
        panic!("Loop did not exit after 20 instructions");
    }
}

/// Fixed-size ring buffer for audio samples.
///
/// Accumulates downsampled stereo f32 pairs from the APU (running at ~4.19 MHz)
/// into a buffer that frontends drain at their audio callback rate (e.g. 48 kHz).

/// Default output sample rate (Hz).
pub const SAMPLE_RATE: u32 = 48_000;

/// Buffer capacity in stereo sample pairs.
/// At 48 kHz, one 16.67ms frame ≈ 800 samples. 4096 gives ~5 frames of headroom.
const BUFFER_CAPACITY: usize = 4096;

#[derive(Clone)]
pub struct AudioBuffer {
    /// Interleaved stereo samples: [L0, R0, L1, R1, ...].
    buf: Box<[f32; BUFFER_CAPACITY * 2]>,
    /// Write position (in f32 units, so stereo pair = 2 units).
    write_pos: usize,
    /// Number of f32 values available to read.
    len: usize,
}

impl AudioBuffer {
    pub fn new() -> Self {
        Self {
            buf: Box::new([0.0; BUFFER_CAPACITY * 2]),
            write_pos: 0,
            len: 0,
        }
    }

    /// Push a stereo sample pair. Drops oldest if full.
    pub fn push(&mut self, left: f32, right: f32) {
        self.buf[self.write_pos] = left;
        self.buf[self.write_pos + 1] = right;
        self.write_pos = (self.write_pos + 2) % (BUFFER_CAPACITY * 2);
        if self.len < BUFFER_CAPACITY * 2 {
            self.len += 2;
        }
    }

    /// Drain all available samples into the provided vec (interleaved L,R,L,R...).
    /// Clears the buffer.
    pub fn drain(&mut self, out: &mut Vec<f32>) {
        if self.len == 0 {
            return;
        }
        let read_pos = (self.write_pos + BUFFER_CAPACITY * 2 - self.len) % (BUFFER_CAPACITY * 2);
        // Copy in one or two chunks (ring buffer wrap)
        if read_pos + self.len <= BUFFER_CAPACITY * 2 {
            out.extend_from_slice(&self.buf[read_pos..read_pos + self.len]);
        } else {
            let first = BUFFER_CAPACITY * 2 - read_pos;
            out.extend_from_slice(&self.buf[read_pos..]);
            out.extend_from_slice(&self.buf[..self.len - first]);
        }
        self.len = 0;
    }

    /// Number of stereo sample pairs available.
    pub fn available(&self) -> usize {
        self.len / 2
    }

    /// Reset the buffer.
    pub fn clear(&mut self) {
        self.len = 0;
    }
}

/// Tracks fractional T-cycle accumulation for downsampling.
///
/// At 4,194,304 Hz master clock and 48,000 Hz output, we emit one sample
/// every ~87.38 T-cycles. Uses fixed-point arithmetic to avoid drift.
#[derive(Clone)]
pub struct Downsampler {
    /// Accumulator: counts up by sample_rate each T-cycle,
    /// emits a sample when it reaches master_clock threshold.
    accum: u32,
    sample_rate: u32,
    master_clock: u32,
}

impl Downsampler {
    pub fn new(master_clock: u32, sample_rate: u32) -> Self {
        Self {
            accum: 0,
            sample_rate,
            master_clock,
        }
    }

    /// Call once per T-cycle. Returns true when a sample should be emitted.
    #[inline]
    pub fn tick(&mut self) -> bool {
        self.accum += self.sample_rate;
        if self.accum >= self.master_clock {
            self.accum -= self.master_clock;
            true
        } else {
            false
        }
    }

    /// Reset accumulator (e.g. on power cycle).
    pub fn reset(&mut self) {
        self.accum = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_drain() {
        let mut buf = AudioBuffer::new();
        buf.push(0.5, -0.5);
        buf.push(0.25, -0.25);
        assert_eq!(buf.available(), 2);

        let mut out = Vec::new();
        buf.drain(&mut out);
        assert_eq!(out, vec![0.5, -0.5, 0.25, -0.25]);
        assert_eq!(buf.available(), 0);
    }

    #[test]
    fn drain_empty() {
        let mut buf = AudioBuffer::new();
        let mut out = Vec::new();
        buf.drain(&mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn clear_resets() {
        let mut buf = AudioBuffer::new();
        buf.push(1.0, 1.0);
        buf.clear();
        assert_eq!(buf.available(), 0);
    }

    #[test]
    fn downsampler_correct_rate() {
        let master = 4_194_304u32;
        let rate = 48_000u32;
        let mut ds = Downsampler::new(master, rate);

        // Run for exactly 1 second of T-cycles
        let mut sample_count = 0u32;
        for _ in 0..master {
            if ds.tick() {
                sample_count += 1;
            }
        }
        // Should produce exactly sample_rate samples per second
        assert_eq!(sample_count, rate);
    }

    #[test]
    fn downsampler_no_drift_over_60_frames() {
        let master = 4_194_304u32;
        let rate = 48_000u32;
        let mut ds = Downsampler::new(master, rate);

        // 60 frames at ~16.67ms each ≈ 1 second
        let cycles_per_frame = master / 60;
        let mut total_samples = 0u32;
        for _ in 0..60 {
            for _ in 0..cycles_per_frame {
                if ds.tick() {
                    total_samples += 1;
                }
            }
        }
        // Should be very close to 48000 (within rounding of integer division)
        let expected = (cycles_per_frame as u64 * 60 * rate as u64 / master as u64) as u32;
        assert_eq!(total_samples, expected);
    }

    #[test]
    fn ring_buffer_wraps_correctly() {
        let mut buf = AudioBuffer::new();
        // Fill and drain multiple times to test wrap-around
        for cycle in 0..10 {
            for i in 0..1000 {
                let v = (cycle * 1000 + i) as f32;
                buf.push(v, -v);
            }
            let mut out = Vec::new();
            buf.drain(&mut out);
            assert_eq!(out.len(), 2000);
            assert_eq!(out[0], (cycle * 1000) as f32);
            assert_eq!(out[1], -(cycle * 1000) as f32);
        }
    }
}

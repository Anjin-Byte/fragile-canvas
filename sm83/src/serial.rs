/// Serial Transfer Controller (Link Cable).
///
/// The Game Boy serial port is a simple full-duplex SPI-like interface
/// clocked at 8192 Hz (master clock / 512 = one bit every 512 T-cycles).
///
/// Registers:
///   FF01  SB   Serial transfer data  (R/W)
///   FF02  SC   Serial transfer control (R/W)
///
/// SC layout (DMG):
///   Bit 7: Transfer enable / in-progress (auto-cleared on completion)
///   Bit 0: Clock select — 0 = external (slave), 1 = internal (master)
///   Bits 6-1: Unused, read as 1 on DMG → reads return 0x7E | active bits.
///
/// Transfer:
///   When the CPU writes SC with bit 7 = 1 and bit 0 = 1 (internal clock),
///   the hardware begins shifting SB left, one bit every 512 T-cycles.
///   With no link cable connected the input line is pulled high, so 1-bits
///   shift in from the right.  After 8 shifts SB contains $FF, SC bit 7
///   is cleared, and a serial interrupt is requested (IF bit 3).
///
///   External-clock mode (bit 0 = 0) waits indefinitely for an external
///   device to provide clocks.  With no device attached, the transfer
///   simply never completes.

/// T-cycles between each serial bit shift (master clock / 8192 Hz).
const CYCLES_PER_BIT: u16 = 512;

/// Address constants for the serial I/O registers.
pub const SB_ADDR: u16 = 0xFF01;
pub const SC_ADDR: u16 = 0xFF02;

#[derive(Clone)]
pub struct Serial {
    /// FF01 — SB: data register (shifted left during transfer).
    pub sb: u8,
    /// FF02 — SC: control register (only bits 7 and 0 are functional on DMG).
    sc: u8,
    /// T-cycle counter within the current bit period.
    tick_counter: u16,
    /// Number of bits shifted so far in the active transfer (0–8).
    bits_shifted: u8,
    /// Set by the serial controller when a transfer completes.
    /// `system.rs` reads and clears this to wire it into IF bit 3.
    pub interrupt_pending: bool,
    /// Accumulated bytes shifted out (for test-ROM serial capture).
    pub output_buffer: Vec<u8>,
}

impl Serial {
    pub fn new() -> Self {
        Self {
            sb: 0x00,
            sc: 0x7E, // unused bits read as 1 on DMG
            tick_counter: 0,
            bits_shifted: 0,
            interrupt_pending: false,
            output_buffer: Vec::new(),
        }
    }

    /// Advance the serial controller by one T-cycle.
    ///
    /// Only transfers using the internal clock (SC bit 0 = 1) are ticked;
    /// external-clock transfers require a connected peer, which we do not
    /// emulate.
    pub fn tick(&mut self) {
        // Transfer active (bit 7) with internal clock (bit 0)?
        if self.sc & 0x81 != 0x81 {
            return;
        }

        self.tick_counter += 1;
        if self.tick_counter < CYCLES_PER_BIT {
            return;
        }
        self.tick_counter = 0;

        // Shift SB left by one bit.  No link cable ⇒ input is pulled high,
        // so a 1-bit enters from the right.
        self.sb = (self.sb << 1) | 0x01;
        self.bits_shifted += 1;

        if self.bits_shifted >= 8 {
            self.complete_transfer();
        }
    }

    /// Finish the current transfer: capture the outgoing byte, clear the
    /// transfer-enable flag, and request the serial interrupt.
    fn complete_transfer(&mut self) {
        // At this point SB has been fully shifted out (now 0xFF from
        // pull-ups).  The original byte was captured bit-by-bit, but for
        // convenience we record it in `output_buffer` at trigger time
        // (see `write()`).
        self.sc &= !0x80; // clear transfer-enable (bit 7)
        self.bits_shifted = 0;
        self.interrupt_pending = true;
    }

    /// Read a serial register.
    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            SB_ADDR => self.sb,
            // Unused bits 6-1 always read as 1 on DMG.
            SC_ADDR => self.sc | 0x7E,
            _ => 0xFF,
        }
    }

    /// Write a serial register.
    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            SB_ADDR => self.sb = value,
            SC_ADDR => {
                // Only bits 7 and 0 are writable on DMG.
                self.sc = (value & 0x81) | 0x7E;

                // If the CPU just started a new transfer (bit 7 set),
                // capture the outgoing byte and reset shift state.
                if value & 0x80 != 0 {
                    self.output_buffer.push(self.sb);
                    self.tick_counter = 0;
                    self.bits_shifted = 0;
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_state() {
        let s = Serial::new();
        assert_eq!(s.sb, 0x00);
        assert_eq!(s.read(SC_ADDR), 0x7E); // no transfer, external clock
        assert!(!s.interrupt_pending);
        assert!(s.output_buffer.is_empty());
    }

    #[test]
    fn unused_sc_bits_read_as_one() {
        let mut s = Serial::new();
        s.write(SC_ADDR, 0x81); // only bits 7 and 0 set
        assert_eq!(s.read(SC_ADDR), 0xFF); // 0x81 | 0x7E = 0xFF
        s.write(SC_ADDR, 0x00);
        assert_eq!(s.read(SC_ADDR), 0x7E); // 0x00 | 0x7E
    }

    #[test]
    fn no_tick_without_internal_clock() {
        let mut s = Serial::new();
        s.sb = 0x42;
        // External clock (bit 0 = 0), transfer enabled.
        s.write(SC_ADDR, 0x80);
        for _ in 0..(CYCLES_PER_BIT as u32 * 8 + 100) {
            s.tick();
        }
        // Transfer never completes — SC bit 7 still set.
        assert_eq!(s.read(SC_ADDR) & 0x80, 0x80);
        assert!(!s.interrupt_pending);
    }

    #[test]
    fn internal_clock_transfer_completes() {
        let mut s = Serial::new();
        s.sb = 0xA5;
        s.write(SC_ADDR, 0x81); // start transfer, internal clock

        // 8 bits × 512 T-cycles = 4096 T-cycles to complete.
        for _ in 0..4096 {
            s.tick();
        }

        // SB should be 0xFF (all pull-up bits shifted in).
        assert_eq!(s.sb, 0xFF);
        // SC bit 7 should be cleared.
        assert_eq!(s.read(SC_ADDR) & 0x80, 0x00);
        // Interrupt should be pending.
        assert!(s.interrupt_pending);
    }

    #[test]
    fn transfer_takes_exactly_4096_cycles() {
        let mut s = Serial::new();
        s.sb = 0x00;
        s.write(SC_ADDR, 0x81);

        // One tick short — not done yet.
        for _ in 0..4095 {
            s.tick();
        }
        assert_eq!(s.read(SC_ADDR) & 0x80, 0x80);
        assert!(!s.interrupt_pending);

        // The 4096th tick completes it.
        s.tick();
        assert_eq!(s.read(SC_ADDR) & 0x80, 0x00);
        assert!(s.interrupt_pending);
    }

    #[test]
    fn output_buffer_captures_outgoing_byte() {
        let mut s = Serial::new();
        s.sb = 0x42;
        s.write(SC_ADDR, 0x81);
        assert_eq!(s.output_buffer, vec![0x42]);

        // Complete transfer and send another byte.
        for _ in 0..4096 {
            s.tick();
        }
        s.interrupt_pending = false;
        s.sb = 0x7F;
        s.write(SC_ADDR, 0x81);
        assert_eq!(s.output_buffer, vec![0x42, 0x7F]);
    }

    #[test]
    fn sb_shifts_correctly_during_transfer() {
        let mut s = Serial::new();
        s.sb = 0x80; // 1000_0000
        s.write(SC_ADDR, 0x81);

        // After 1 bit shift (512 T-cycles): 0000_0001
        for _ in 0..512 {
            s.tick();
        }
        assert_eq!(s.sb, 0x01);

        // After 2nd bit shift: 0000_0011
        for _ in 0..512 {
            s.tick();
        }
        assert_eq!(s.sb, 0x03);
    }

    #[test]
    fn write_sb_during_transfer() {
        let mut s = Serial::new();
        s.sb = 0x00;
        s.write(SC_ADDR, 0x81);
        // Mid-transfer write to SB is allowed (hardware does not block it).
        s.sb = 0xFF;
        assert_eq!(s.read(SB_ADDR), 0xFF);
    }
}

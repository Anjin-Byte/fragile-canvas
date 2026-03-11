/// OAM DMA controller.
///
/// A write to 0xFF46 copies 160 bytes from address `(page << 8)` into OAM
/// (0xFE00–0xFE9F) at one byte per M-cycle, after a 1 M-cycle startup delay
/// (161 M-cycles total).
///
/// During an active transfer the CPU can only access HRAM (0xFF80–0xFFFE).
/// All other CPU reads return `current_byte` — the byte currently being
/// transferred — rather than 0xFF.  This is observable: if the CPU executes
/// code from blocked memory during DMA it will execute DMA data as opcodes.
///
/// Re-triggering (writing 0xFF46 while a transfer is in progress) is ignored
/// on DMG; the original transfer completes unchanged.
#[derive(Clone, Default)]
pub struct DmaController {
    /// Whether a transfer is currently in progress.
    pub active: bool,
    /// Value written to 0xFF46 — the high byte of the source address.
    /// Readable back via 0xFF46 even when idle.
    pub source_page: u8,
    /// Startup delay countdown.  Set to 1 on trigger; decremented once per
    /// M-cycle before the first byte is copied.
    delay: u8,
    /// Bytes remaining to copy (160 → 0).
    pub remaining: u8,
    /// The last byte read from the source bus.
    /// Returned for blocked CPU reads during an active transfer.
    pub current_byte: u8,
}

impl DmaController {
    pub fn new() -> Self {
        Self::default()
    }

    /// Request an OAM DMA transfer from `page` (value written to 0xFF46).
    /// If a transfer is already active this write is silently ignored (DMG).
    pub fn trigger(&mut self, page: u8) {
        if !self.active {
            self.active = true;
            self.source_page = page;
            self.delay = 1;
            self.remaining = 160;
        }
    }

    /// Advance the controller by one M-cycle.
    ///
    /// Returns `Some(oam_index)` when a byte should be copied this cycle —
    /// the caller is responsible for reading the source byte and writing it
    /// to `oam[oam_index]` and back into `current_byte`.
    /// Returns `None` during the startup delay or when inactive.
    pub fn advance(&mut self) -> Option<usize> {
        if !self.active {
            return None;
        }
        if self.delay > 0 {
            self.delay -= 1;
            return None;
        }

        let byte_index = (160 - self.remaining) as usize;
        self.remaining -= 1;
        if self.remaining == 0 {
            self.active = false;
        }
        Some(byte_index)
    }
}

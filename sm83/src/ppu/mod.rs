/// Game Boy PPU (Picture Processing Unit) — behavioral stub.
///
/// Implements the correct mode/scanline timing and STAT interrupt
/// logic so that games that poll STAT or LYC behave correctly.
/// Pixel rendering is left for a future pass.
///
/// Register map:
///   FF40  LCDC  LCD control
///   FF41  STAT  LCD status (mode, LYC flag, interrupt enables)
///   FF42  SCY   Background scroll Y
///   FF43  SCX   Background scroll X
///   FF44  LY    Current scanline (read-only on real HW; settable here for testing)
///   FF45  LYC   LY compare
///   FF47  BGP   Background palette
///   FF48  OBP0  Object palette 0
///   FF49  OBP1  Object palette 1
///   FF4A  WY    Window Y
///   FF4B  WX    Window X

/// T-cycles (dots) per scanline.
const DOTS_PER_LINE: u16 = 456;
/// Total scanlines per frame.
const LINES_PER_FRAME: u8 = 154;
/// First VBlank scanline.
const VBLANK_LINE: u8 = 144;
/// End of Mode 2 (OAM scan) within a visible scanline.
const OAM_END: u16 = 80;
/// End of Mode 3 (Drawing) within a visible scanline.
const DRAW_END: u16 = 252;

/// Screen width in pixels.
pub const SCREEN_W: usize = 160;
/// Screen height in pixels.
pub const SCREEN_H: usize = 144;

#[derive(Clone)]
pub struct Ppu {
    // ── Registers ────────────────────────────────────────────────────────────
    /// 0xFF40: LCD Control.
    pub lcdc: u8,
    /// 0xFF41: STAT writable bits 3-6 (interrupt enables).
    /// Bits 0-2 are computed on read.
    stat_enables: u8,
    /// 0xFF42: Background scroll Y.
    pub scy: u8,
    /// 0xFF43: Background scroll X.
    pub scx: u8,
    /// 0xFF44: Current scanline. Normally managed by the PPU itself;
    /// writable here so tests can override LY without a full PPU tick loop.
    pub ly: u8,
    /// 0xFF45: LY Compare.
    pub lyc: u8,
    /// 0xFF47: Background palette.
    pub bgp: u8,
    /// 0xFF48: Object palette 0.
    pub obp0: u8,
    /// 0xFF49: Object palette 1.
    pub obp1: u8,
    /// 0xFF4A: Window Y.
    pub wy: u8,
    /// 0xFF4B: Window X.
    pub wx: u8,

    // ── Internal state ───────────────────────────────────────────────────────
    /// Current PPU mode (0=HBlank, 1=VBlank, 2=OAM, 3=Drawing).
    mode: u8,
    /// Dot counter within the current scanline (0..455).
    dot: u16,
    /// Previous STAT interrupt line value (for rising-edge detection).
    stat_line: bool,

    // ── Pending interrupt flags (cleared by system.rs after raising IF) ──────
    /// VBlank interrupt pending (IF bit 0).
    pub vblank_irq: bool,
    /// STAT interrupt pending (IF bit 1).
    pub stat_irq: bool,

    // ── Output ───────────────────────────────────────────────────────────────
    /// 160×144 framebuffer. Each byte is a shade index (0-3).
    pub framebuffer: Box<[u8; SCREEN_W * SCREEN_H]>,
    /// Set to true when a new frame has just completed.
    pub frame_ready: bool,
}

impl Ppu {
    pub fn new() -> Self {
        Self {
            lcdc: 0,
            stat_enables: 0,
            scy: 0,
            scx: 0,
            ly: 0,
            lyc: 0,
            bgp: 0,
            obp0: 0,
            obp1: 0,
            wy: 0,
            wx: 0,
            mode: 0,
            dot: 0,
            stat_line: false,
            vblank_irq: false,
            stat_irq: false,
            framebuffer: Box::new([0; SCREEN_W * SCREEN_H]),
            frame_ready: false,
        }
    }

    // ── Register I/O ─────────────────────────────────────────────────────────

    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0xFF40 => self.lcdc,
            0xFF41 => self.stat_register(),
            0xFF42 => self.scy,
            0xFF43 => self.scx,
            0xFF44 => self.ly,
            0xFF45 => self.lyc,
            0xFF47 => self.bgp,
            0xFF48 => self.obp0,
            0xFF49 => self.obp1,
            0xFF4A => self.wy,
            0xFF4B => self.wx,
            _ => 0xFF,
        }
    }

    pub fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0xFF40 => self.lcdc = val,
            // Only bits 3-6 are writable; bits 0-2 are read-only hardware state.
            0xFF41 => {
                self.stat_enables = val & 0x78;
                self.update_stat_line();
            }
            0xFF42 => self.scy = val,
            0xFF43 => self.scx = val,
            // LY is read-only on real hardware. We allow writes here so that
            // unit tests (which tick the CPU without a PPU) can override LY
            // to make VBlank wait loops exit.
            0xFF44 => self.ly = val,
            0xFF45 => {
                self.lyc = val;
                self.update_stat_line();
            }
            0xFF47 => self.bgp = val,
            0xFF48 => self.obp0 = val,
            0xFF49 => self.obp1 = val,
            0xFF4A => self.wy = val,
            0xFF4B => self.wx = val,
            _ => (),
        }
    }

    // ── Tick ─────────────────────────────────────────────────────────────────

    /// Advance the PPU by one T-cycle (dot).
    pub fn tick(&mut self) {
        if self.lcdc & 0x80 == 0 {
            // LCD disabled: hold at mode 0, LY 0.
            self.mode = 0;
            self.ly = 0;
            self.dot = 0;
            self.stat_line = false;
            return;
        }

        self.dot += 1;

        if self.dot >= DOTS_PER_LINE {
            self.dot = 0;
            self.ly = self.ly.wrapping_add(1);
            if self.ly >= LINES_PER_FRAME {
                self.ly = 0;
                self.frame_ready = true;
            }
        }

        let new_mode = if self.ly >= VBLANK_LINE {
            1 // VBlank
        } else if self.dot < OAM_END {
            2 // OAM scan
        } else if self.dot < DRAW_END {
            3 // Drawing
        } else {
            0 // HBlank
        };

        let mode_changed = new_mode != self.mode;
        self.mode = new_mode;

        if mode_changed && self.mode == 1 {
            self.vblank_irq = true;
        }

        self.update_stat_line();
    }

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn stat_register(&self) -> u8 {
        let lyc_flag = u8::from(self.ly == self.lyc) << 2;
        self.stat_enables | lyc_flag | (self.mode & 0x03)
    }

    /// Compute the STAT interrupt line and fire a STAT IRQ on a rising edge.
    fn update_stat_line(&mut self) {
        let lyc_match = self.ly == self.lyc;
        let line =
            (self.mode == 0 && self.stat_enables & (1 << 3) != 0)
            || (self.mode == 1 && self.stat_enables & (1 << 4) != 0)
            || (self.mode == 2 && self.stat_enables & (1 << 5) != 0)
            || (lyc_match    && self.stat_enables & (1 << 6) != 0);

        if line && !self.stat_line {
            self.stat_irq = true;
        }
        self.stat_line = line;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ticked(n: u32) -> Ppu {
        let mut ppu = Ppu::new();
        ppu.lcdc = 0x80; // LCD on
        for _ in 0..n {
            ppu.tick();
        }
        ppu
    }

    #[test]
    fn mode_sequence_visible_line() {
        // Dot 0: mode 2
        let ppu = ticked(1);
        assert_eq!(ppu.mode, 2, "dot 1 should be mode 2 (OAM)");

        // Dot 80: mode 3 starts
        let ppu = ticked(OAM_END as u32);
        assert_eq!(ppu.mode, 3, "dot 80 should be mode 3 (Drawing)");

        // Dot 252: mode 0 starts
        let ppu = ticked(DRAW_END as u32);
        assert_eq!(ppu.mode, 0, "dot 252 should be mode 0 (HBlank)");
    }

    #[test]
    fn ly_increments_at_end_of_line() {
        let ppu = ticked(DOTS_PER_LINE as u32);
        assert_eq!(ppu.ly, 1);
    }

    #[test]
    fn vblank_mode_at_line_144() {
        let ppu = ticked(DOTS_PER_LINE as u32 * VBLANK_LINE as u32 + 1);
        assert_eq!(ppu.ly, VBLANK_LINE);
        assert_eq!(ppu.mode, 1);
    }

    #[test]
    fn vblank_irq_fires_at_line_144() {
        let mut ppu = Ppu::new();
        ppu.lcdc = 0x80;
        let vblank_dot = DOTS_PER_LINE as u32 * VBLANK_LINE as u32;
        // Tick up to (but not including) the transition tick, clearing the flag.
        for _ in 0..vblank_dot - 1 {
            ppu.tick();
            ppu.vblank_irq = false;
        }
        // This tick increments LY to 144, triggering VBlank.
        ppu.tick();
        assert!(ppu.vblank_irq);
    }

    #[test]
    fn ly_wraps_after_153() {
        let ppu = ticked(DOTS_PER_LINE as u32 * LINES_PER_FRAME as u32);
        assert_eq!(ppu.ly, 0);
        assert!(ppu.frame_ready);
    }

    #[test]
    fn stat_mode_bits_in_register() {
        // Mode 2 at dot 1
        let ppu = ticked(1);
        assert_eq!(ppu.stat_register() & 0x03, 2);
    }

    #[test]
    fn lyc_coincidence_flag() {
        let mut ppu = Ppu::new();
        ppu.lcdc = 0x80;
        ppu.lyc = 1;
        // Tick to ly=1
        for _ in 0..=DOTS_PER_LINE {
            ppu.tick();
        }
        assert_eq!((ppu.stat_register() >> 2) & 1, 1, "LYC=LY flag should be set");
    }

    #[test]
    fn stat_irq_on_hblank_enable() {
        let mut ppu = Ppu::new();
        ppu.lcdc = 0x80;
        ppu.stat_enables = 1 << 3; // HBlank STAT enable
        let mut got_irq = false;
        for _ in 0..DOTS_PER_LINE {
            ppu.tick();
            if ppu.stat_irq {
                got_irq = true;
                ppu.stat_irq = false;
            }
        }
        assert!(got_irq, "should have received a STAT IRQ for HBlank");
    }

    #[test]
    fn lcd_off_holds_ly_zero() {
        let mut ppu = Ppu::new();
        // lcdc bit 7 = 0: LCD off
        for _ in 0..1000 {
            ppu.tick();
        }
        assert_eq!(ppu.ly, 0);
        assert_eq!(ppu.mode, 0);
    }
}

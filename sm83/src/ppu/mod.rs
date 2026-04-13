/// Game Boy PPU (Picture Processing Unit).
///
/// Implements correct mode/scanline timing, STAT interrupt logic, and a
/// scanline-based pixel renderer for background, window, and sprites.
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
/// Baseline end of Mode 3 (Drawing) within a visible scanline.
/// Actual end is computed per-scanline (scx/window/sprite penalties may extend it).
const DRAW_END: u16 = 252;
/// Baseline Mode 3 duration in dots.
const MODE3_BASE: u16 = 172;

/// Screen width in pixels.
pub const SCREEN_W: usize = 160;
/// Screen height in pixels.
pub const SCREEN_H: usize = 144;

/// Maximum sprites evaluated per scanline (hardware limit).
const MAX_SPRITES_PER_LINE: usize = 10;

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
    /// Window internal line counter. Increments each scanline where the
    /// window is visible. Resets to 0 at the start of each frame.
    win_y: u8,
    /// Set when LY == WY is observed at the start of Mode 2.
    /// Cleared at frame wrap and LCD-off. Once set, remains true for the frame.
    /// This matches the hardware: the WY condition is checked only at Mode 2 start,
    /// not continuously throughout the frame.
    wy_triggered: bool,
    /// Dot at which Mode 3 ends (start of HBlank). Recomputed at every Mode 2 start.
    /// Default: DRAW_END (172 base + OAM_END offset).
    mode3_end: u16,

    // ── Pending interrupt flags (cleared by bus.ppu_tick after raising IF) ──
    /// VBlank interrupt pending (IF bit 0).
    pub vblank_irq: bool,
    /// STAT interrupt pending (IF bit 1).
    pub stat_irq: bool,
    /// Scanline is fully drawn; bus.ppu_tick() should call render_scanline().
    pub scanline_ready: bool,
    /// Mode 2 just started; bus.ppu_tick() should call scan_oam_and_compute_mode3_end().
    pub oam_scan_ready: bool,

    // ── Output ───────────────────────────────────────────────────────────────
    /// Active render target. `render_scanline` writes here row by row during
    /// the active display period. Never read directly by the frontend.
    pub framebuffer: Box<[u8; SCREEN_W * SCREEN_H]>,
    /// Stable display snapshot. Copied from `framebuffer` atomically at the
    /// frame boundary (VBlank/ly-wrap) before rendering of the next frame
    /// can begin. `drain_frame` reads this buffer, so the frontend always
    /// sees a complete, tear-free frame regardless of when it calls.
    pub completed_frame: Box<[u8; SCREEN_W * SCREEN_H]>,
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
            win_y: 0,
            wy_triggered: false,
            mode3_end: DRAW_END,
            vblank_irq: false,
            stat_irq: false,
            scanline_ready: false,
            oam_scan_ready: false,
            framebuffer: Box::new([0; SCREEN_W * SCREEN_H]),
            completed_frame: Box::new([0; SCREEN_W * SCREEN_H]),
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
            self.win_y = 0;
            self.wy_triggered = false;
            self.mode3_end = DRAW_END;
            self.stat_line = false;
            return;
        }

        self.dot += 1;

        if self.dot >= DOTS_PER_LINE {
            self.dot = 0;
            self.ly = self.ly.wrapping_add(1);
            if self.ly >= LINES_PER_FRAME {
                self.ly = 0;
                self.win_y = 0;
                self.wy_triggered = false;
                // Snapshot the completed render into the stable display buffer
                // before the next frame's scanlines can overwrite framebuffer.
                self.completed_frame.copy_from_slice(&*self.framebuffer);
                self.frame_ready = true;
            }
        }

        let new_mode = if self.ly >= VBLANK_LINE {
            1 // VBlank
        } else if self.dot < OAM_END {
            2 // OAM scan
        } else if self.dot < self.mode3_end {
            3 // Drawing
        } else {
            0 // HBlank
        };

        let mode_changed = new_mode != self.mode;
        self.mode = new_mode;

        if mode_changed && self.mode == 1 {
            self.vblank_irq = true;
        }
        // At Mode 2 start: check WY condition and signal OAM scan to bus.
        if mode_changed && self.mode == 2 {
            if self.ly == self.wy {
                self.wy_triggered = true;
            }
            self.oam_scan_ready = true;
        }
        // Signal the bus to render this scanline at the Mode3→HBlank transition.
        if mode_changed && self.mode == 0 {
            self.scanline_ready = true;
        }

        self.update_stat_line();
    }

    /// Current PPU mode (0=HBlank, 1=VBlank, 2=OAM, 3=Drawing).
    pub fn mode(&self) -> u8 {
        self.mode
    }

    // ── OAM scan / Mode 3 duration ────────────────────────────────────────────

    /// Called by `Bus::ppu_tick()` when `oam_scan_ready` fires (Mode 2 start).
    ///
    /// Scans OAM to count sprites visible on the current scanline, then computes
    /// the Mode 3 end dot (= start of HBlank) using the hardware penalty model:
    ///   - Baseline: 172 dots (OAM_END + MODE3_BASE)
    ///   - SCX fine-scroll penalty: SCX % 8 dots
    ///   - Window penalty: +6 dots if the window will be visible this line
    ///   - Sprite penalty: +6 dots per sprite (simplified from 6–11 per hw)
    pub fn scan_oam_and_compute_mode3_end(&mut self, oam: &[u8]) {
        let obj_enabled = self.lcdc & 0x02 != 0;
        let obj_height: i16 = if self.lcdc & 0x04 != 0 { 16 } else { 8 };
        let ly_i16 = self.ly as i16;

        let mut sprite_count = 0u8;
        if obj_enabled {
            for i in 0..40 {
                let sprite_top = oam[i * 4] as i16 - 16;
                if ly_i16 >= sprite_top && ly_i16 < sprite_top + obj_height {
                    sprite_count += 1;
                    if sprite_count == 10 {
                        break;
                    }
                }
            }
        }

        let scx_penalty = (self.scx % 8) as u16;

        // Window penalty: window must be globally enabled (LCDC bits 0 and 5),
        // WY condition must have been triggered this frame, and WX must be on-screen.
        let win_globally_enabled = self.lcdc & 0x21 == 0x21;
        let win_x_screen = self.wx.saturating_sub(7) as usize;
        let win_penalty: u16 =
            if self.wy_triggered && win_globally_enabled && win_x_screen < SCREEN_W {
                6
            } else {
                0
            };

        let sprite_penalty = sprite_count as u16 * 6;

        self.mode3_end = OAM_END + MODE3_BASE + scx_penalty + win_penalty + sprite_penalty;
    }

    // ── Scanline renderer ─────────────────────────────────────────────────────

    /// Render the current scanline (self.ly) into the framebuffer.
    ///
    /// Called by `Bus::ppu_tick()` when `scanline_ready` fires, which
    /// happens once per visible scanline at the Mode3 → HBlank transition.
    /// `vram` and `oam` are the Bus sibling fields passed in via split borrow.
    pub fn render_scanline(&mut self, vram: &[u8], oam: &[u8]) {
        let ly = self.ly;
        let base = ly as usize * SCREEN_W;

        // LCDC control flags
        let bg_enabled  = self.lcdc & 0x01 != 0;
        let obj_enabled = self.lcdc & 0x02 != 0;
        let obj_tall    = self.lcdc & 0x04 != 0; // 8×16 sprites
        let win_enabled = bg_enabled && (self.lcdc & 0x20 != 0);
        let unsigned_tiles = self.lcdc & 0x10 != 0; // tile data addressing mode

        let bg_map_base:  usize = if self.lcdc & 0x08 != 0 { 0x9C00 } else { 0x9800 };
        let win_map_base: usize = if self.lcdc & 0x40 != 0 { 0x9C00 } else { 0x9800 };

        // WX=7 places the window at the left edge; WX<7 is off-screen.
        let win_x_screen = self.wx.saturating_sub(7) as usize;
        // Window visible on this line only if the WY condition was triggered at
        // Mode 2 start (ly == wy at some earlier or current Mode 2 this frame).
        let win_on_line  = win_enabled && self.wy_triggered && win_x_screen < SCREEN_W;

        // ── Background / Window pass ──────────────────────────────────────────
        // Also records BG color indices for later sprite priority checks.
        let mut bg_color_indices = [0u8; SCREEN_W];

        for px in 0..SCREEN_W {
            let color_index = if win_on_line && px >= win_x_screen {
                // Window pixel
                let wx_off = (px - win_x_screen) as u8;
                self.fetch_tile_color(vram, win_map_base, wx_off, self.win_y, unsigned_tiles)
            } else if bg_enabled {
                // Background pixel (scrolled, wraps at 256)
                let map_x = (px as u8).wrapping_add(self.scx);
                let map_y = ly.wrapping_add(self.scy);
                self.fetch_tile_color(vram, bg_map_base, map_x, map_y, unsigned_tiles)
            } else {
                0 // BG disabled → white (color 0)
            };

            bg_color_indices[px] = color_index;
            self.framebuffer[base + px] = apply_palette(self.bgp, color_index);
        }

        // Increment window internal line counter if window contributed pixels.
        if win_on_line {
            self.win_y = self.win_y.wrapping_add(1);
        }

        // ── Sprite pass ───────────────────────────────────────────────────────
        if !obj_enabled {
            return;
        }

        let obj_height: i16 = if obj_tall { 16 } else { 8 };

        // Collect up to 10 OAM entries visible on this scanline.
        let mut sprite_indices = [0usize; MAX_SPRITES_PER_LINE];
        let mut sprite_count = 0usize;

        for i in 0..40 {
            let sprite_top = oam[i * 4] as i16 - 16;
            let ly_i16 = ly as i16;
            if ly_i16 >= sprite_top && ly_i16 < sprite_top + obj_height {
                sprite_indices[sprite_count] = i;
                sprite_count += 1;
                if sprite_count == MAX_SPRITES_PER_LINE {
                    break;
                }
            }
        }

        // DMG drawing priority: lower X position wins; OAM index breaks ties.
        //
        // Hardware behaviour (Pan Docs):
        //  1. Sort sprites by (X asc, OAM-index asc). This gives the priority order.
        //  2. Iterate in priority order. The *first* non-transparent pixel at each
        //     screen position is the "object pixel" — lower-priority sprites cannot
        //     overwrite it, even if the winning sprite is transparent at that dot.
        //  3. Only *after* the winning sprite is determined is its BG-over-OBJ flag
        //     consulted to decide whether BG or the sprite is shown. This means a
        //     high-priority opaque sprite with BG-over-OBJ masks lower-priority
        //     sprites behind it, even those with BG-over-OBJ unset.

        // Sort collected sprites by (X, OAM index).
        let mut sorted = [(0usize, 0i16); MAX_SPRITES_PER_LINE];
        for k in 0..sprite_count {
            let i = sprite_indices[k];
            sorted[k] = (i, oam[i * 4 + 1] as i16 - 8);
        }
        sorted[..sprite_count].sort_unstable_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));

        // Per-pixel sprite result: shade already palette-applied, BG-over-OBJ flag,
        // and an occupancy flag so lower-priority sprites cannot overwrite.
        let mut sprite_shade:    [u8;   SCREEN_W] = [0; SCREEN_W];
        let mut sprite_bg_prio:  [bool; SCREEN_W] = [false; SCREEN_W];
        let mut sprite_occupied: [bool; SCREEN_W] = [false; SCREEN_W];

        for k in 0..sprite_count {
            let (i, sprite_left) = sorted[k];
            let sprite_top = oam[i * 4] as i16 - 16;
            let mut tile_idx = oam[i * 4 + 2];
            let attrs = oam[i * 4 + 3];

            let y_flip      = attrs & 0x40 != 0;
            let x_flip      = attrs & 0x20 != 0;
            let palette     = if attrs & 0x10 != 0 { self.obp1 } else { self.obp0 };
            let bg_priority = attrs & 0x80 != 0;

            let mut row = (ly as i16 - sprite_top) as u8;
            if y_flip {
                row = obj_height as u8 - 1 - row;
            }

            // In 8×16 mode the tile index selects a pair; bit 0 is forced by half.
            if obj_tall {
                tile_idx = if row < 8 { tile_idx & 0xFE } else { tile_idx | 0x01 };
            }

            let row_in_tile = row & 7;
            let tile_addr = (tile_idx as usize) * 16 + (row_in_tile as usize) * 2;
            let lo = vram[tile_addr];
            let hi = vram[tile_addr + 1];

            for col in 0..8i16 {
                let px = sprite_left + col;
                if px < 0 || px >= SCREEN_W as i16 {
                    continue;
                }
                let px = px as usize;

                // A lower-priority sprite cannot claim a pixel already owned.
                if sprite_occupied[px] {
                    continue;
                }

                let bit = if x_flip { col as u8 } else { 7 - col as u8 };
                let color_index = ((hi >> bit) & 1) << 1 | ((lo >> bit) & 1);

                // Transparent pixels don't claim the position — a lower-priority
                // sprite may still show through at this dot.
                if color_index == 0 {
                    continue;
                }

                sprite_shade[px]    = apply_palette(palette, color_index);
                sprite_bg_prio[px]  = bg_priority;
                sprite_occupied[px] = true;
            }
        }

        // Blit winning sprite pixels. BG-over-OBJ is checked here against the
        // BG color index recorded during the background/window pass.
        for px in 0..SCREEN_W {
            if !sprite_occupied[px] {
                continue;
            }
            // BG priority: the winning sprite hides behind BG/window colors 1-3.
            if sprite_bg_prio[px] && bg_color_indices[px] != 0 {
                continue;
            }
            self.framebuffer[base + px] = sprite_shade[px];
        }
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

    /// Fetch a 2-bit color index for a single background or window pixel.
    ///
    /// `map_base` is the absolute VRAM address of the tile map (0x9800 or 0x9C00).
    /// `tile_x` / `tile_y` are pixel coordinates within the 256×256 BG map or
    /// the window-local coordinate space.
    fn fetch_tile_color(
        &self,
        vram: &[u8],
        map_base: usize,
        tile_x: u8,
        tile_y: u8,
        unsigned_mode: bool,
    ) -> u8 {
        let tile_col = (tile_x / 8) as usize;
        let tile_row = (tile_y / 8) as usize;
        // Map address relative to start of VRAM (0x8000).
        let map_offset = (map_base - 0x8000) + tile_row * 32 + tile_col;
        let tile_index = vram[map_offset];

        // Tile data address within VRAM slice.
        // Unsigned (LCDC bit 4 = 1): 0x8000 base, index 0-255.
        // Signed   (LCDC bit 4 = 0): 0x9000 base, index -128..127.
        let tile_data_offset: usize = if unsigned_mode {
            (tile_index as usize) * 16
        } else {
            // 0x9000 - 0x8000 = 0x1000 within VRAM
            (0x1000i32 + (tile_index as i8 as i32) * 16) as usize
        };

        let row_in_tile = (tile_y & 7) as usize;
        let col_bit     = 7 - (tile_x & 7); // MSB = leftmost pixel

        let lo = vram[tile_data_offset + row_in_tile * 2];
        let hi = vram[tile_data_offset + row_in_tile * 2 + 1];

        ((hi >> col_bit) & 1) << 1 | ((lo >> col_bit) & 1)
    }
}

/// Map a 2-bit color index through a DMG palette register to a shade (0-3).
fn apply_palette(palette: u8, color_index: u8) -> u8 {
    (palette >> (color_index * 2)) & 0x3
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

    // ── Timing tests ─────────────────────────────────────────────────────────

    #[test]
    fn mode_sequence_visible_line() {
        let ppu = ticked(1);
        assert_eq!(ppu.mode, 2, "dot 1 should be mode 2 (OAM)");

        let ppu = ticked(OAM_END as u32);
        assert_eq!(ppu.mode, 3, "dot 80 should be mode 3 (Drawing)");

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
        for _ in 0..vblank_dot - 1 {
            ppu.tick();
            ppu.vblank_irq = false;
        }
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
        let ppu = ticked(1);
        assert_eq!(ppu.stat_register() & 0x03, 2);
    }

    #[test]
    fn lyc_coincidence_flag() {
        let mut ppu = Ppu::new();
        ppu.lcdc = 0x80;
        ppu.lyc = 1;
        for _ in 0..=DOTS_PER_LINE {
            ppu.tick();
        }
        assert_eq!((ppu.stat_register() >> 2) & 1, 1, "LYC=LY flag should be set");
    }

    #[test]
    fn stat_irq_on_hblank_enable() {
        let mut ppu = Ppu::new();
        ppu.lcdc = 0x80;
        ppu.stat_enables = 1 << 3;
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
        for _ in 0..1000 {
            ppu.tick();
        }
        assert_eq!(ppu.ly, 0);
        assert_eq!(ppu.mode, 0);
    }

    #[test]
    fn scanline_ready_fires_on_hblank_transition() {
        let mut ppu = Ppu::new();
        ppu.lcdc = 0x80;
        let mut fired = false;
        for _ in 0..DOTS_PER_LINE {
            ppu.tick();
            if ppu.scanline_ready {
                fired = true;
                ppu.scanline_ready = false;
            }
        }
        assert!(fired, "scanline_ready should fire once per visible scanline");
    }

    #[test]
    fn win_y_resets_each_frame() {
        let mut ppu = Ppu::new();
        ppu.lcdc = 0x80;
        ppu.win_y = 42;
        // Tick through a full frame
        for _ in 0..DOTS_PER_LINE as u32 * LINES_PER_FRAME as u32 {
            ppu.tick();
        }
        assert_eq!(ppu.win_y, 0, "win_y should reset at frame wrap");
    }

    // ── Renderer tests ────────────────────────────────────────────────────────

    /// Build a minimal VRAM where tile 0 in the 0x8000 area is a solid color-3
    /// tile, the BG tile map at 0x9800 points to tile 0, and BGP maps color 3
    /// to shade 3.  After rendering scanline 0 the first 8 pixels must be 3.
    #[test]
    fn bg_solid_tile_renders_correctly() {
        let mut ppu = Ppu::new();
        // LCDC: LCD on, BG enabled, tile data at 0x8000 (unsigned), BG map at 0x9800.
        ppu.lcdc = 0x91; // 1001_0001
        ppu.bgp  = 0b11100100; // identity palette: shade = color index
        ppu.ly   = 0;

        let mut vram = [0u8; 0x2000];
        // Tile 0 at 0x8000 (vram offset 0): solid color-3 for row 0.
        // Each row is 2 bytes: lo then hi.  Both 0xFF → every pixel = color 3.
        vram[0] = 0xFF; // lo
        vram[1] = 0xFF; // hi

        // Tile map at 0x9800 (vram offset 0x1800): all zeroes → tile index 0.
        // (Already zeroed.)

        let oam = [0u8; 0xA0];
        ppu.render_scanline(&vram, &oam);

        // First 8 pixels should be shade 3 (color 3, bgp identity).
        for px in 0..8 {
            assert_eq!(ppu.framebuffer[px], 3, "pixel {px} should be shade 3");
        }
    }

    /// A sprite at OAM position (y=16, x=8) — screen (0,0) — should overwrite
    /// the background pixel with its color.
    #[test]
    fn sprite_draws_over_background() {
        let mut ppu = Ppu::new();
        // LCDC: LCD on, BG + OBJ enabled, tile data 0x8000.
        ppu.lcdc = 0x93; // 1001_0011
        ppu.bgp  = 0b11100100;
        ppu.obp0 = 0b11100100;
        ppu.ly   = 0;

        let mut vram = [0u8; 0x2000];
        // Sprite tile 1 at vram offset 16: first row = color 1 (lo=0xFF, hi=0x00).
        vram[16] = 0xFF;
        vram[17] = 0x00;

        // OAM: sprite 0 at screen (0,0), tile 1, no flags.
        let mut oam = [0u8; 0xA0];
        oam[0] = 16; // raw Y = 16 → screen Y = 0
        oam[1] = 8;  // raw X = 8  → screen X = 0
        oam[2] = 1;  // tile index 1
        oam[3] = 0;  // no flags

        ppu.render_scanline(&vram, &oam);

        // Pixel 0 should be sprite color 1 (shade 1 via obp0 identity palette).
        assert_eq!(ppu.framebuffer[0], 1, "pixel 0 should be sprite shade 1");
    }

    /// DMG sprite priority: the sprite with the smaller X coordinate wins when
    /// two sprites overlap, regardless of OAM index order.
    #[test]
    fn sprite_x_position_priority_dmg() {
        let mut ppu = Ppu::new();
        ppu.lcdc = 0x93; // LCD on, BG + OBJ enabled, tile data 0x8000
        ppu.bgp  = 0b11100100;
        ppu.obp0 = 0b11100100; // shade = color index
        ppu.obp1 = 0b11111111; // all shade 3
        ppu.ly   = 0;

        let mut vram = [0u8; 0x2000];
        // Tile 1: solid color 1 (lo=0xFF, hi=0x00) — OBP0 → shade 1
        vram[16] = 0xFF; vram[17] = 0x00;
        // Tile 2: solid color 3 (lo=0xFF, hi=0xFF) — OBP1 → shade 3
        vram[32] = 0xFF; vram[33] = 0xFF;

        let mut oam = [0u8; 0xA0];
        // Sprite 0 (OAM index 0): raw X=17 → screen left=9, tile 2, OBP1 — lower OAM index
        // Covers screen pixels 9-16.
        oam[0] = 16; oam[1] = 17; oam[2] = 2; oam[3] = 0x10; // OBP1
        // Sprite 1 (OAM index 1): raw X=9 → screen left=1, tile 1, OBP0 — higher X priority
        // Covers screen pixels 1-8.  Does NOT overlap sprite 0.
        oam[4] = 16; oam[5] = 9; oam[6] = 1; oam[7] = 0x00; // OBP0

        ppu.render_scanline(&vram, &oam);

        // Sprite 1 (screen left=1, smaller X) wins at px=1 — shade 1 via OBP0.
        assert_eq!(ppu.framebuffer[1], 1, "smaller-X sprite should win at px=1");
        // Sprite 0 (screen left=9) is uncontested at px=9 — shade 3 via OBP1.
        assert_eq!(ppu.framebuffer[9], 3, "sprite 0 should be visible at its uncontested px=9");
    }

    /// BG-over-OBJ masking: a high-priority opaque sprite with BG-over-OBJ set
    /// prevents a lower-priority sprite from showing, even when the lower-priority
    /// sprite has BG-over-OBJ unset. The background shows at the masked pixels.
    #[test]
    fn bg_over_obj_masks_lower_priority_sprite() {
        let mut ppu = Ppu::new();
        ppu.lcdc = 0x93;
        ppu.bgp  = 0b11111111; // all BG → shade 3
        ppu.obp0 = 0b11100100; // shade = color index
        ppu.ly   = 0;

        let mut vram = [0u8; 0x2000];
        // BG tile 0 at 0x9800: all zeros → tile 0 (solid color 3 via bgp)
        vram[0] = 0xFF; vram[1] = 0xFF; // tile 0, row 0: color 3
        // Sprite tile 1: solid color 1
        vram[16] = 0xFF; vram[17] = 0x00;
        // Sprite tile 2: solid color 2
        vram[32] = 0x00; vram[33] = 0xFF;

        let mut oam = [0u8; 0xA0];
        // Sprite 0 (higher priority, X=8 → screen left=0): tile 1, BG-over-OBJ set
        oam[0] = 16; oam[1] = 8; oam[2] = 1; oam[3] = 0x80; // bg_priority
        // Sprite 1 (lower priority, X=16 → screen left=8, overlaps at px 0-7):
        // tile 2, no BG-over-OBJ — but should be masked by sprite 0
        oam[4] = 16; oam[5] = 16; oam[6] = 2; oam[7] = 0x00;

        ppu.render_scanline(&vram, &oam);

        // px 0-7: sprite 0 wins priority but has BG-over-OBJ AND bg_color=3 → BG wins.
        assert_eq!(ppu.framebuffer[0], 3, "BG should show through BG-over-OBJ sprite");
        // px 8-15: sprite 1 only, no BG-over-OBJ → sprite 1 shows (shade 2).
        assert_eq!(ppu.framebuffer[8], 2, "sprite 1 should show at px=8 (unmasked)");
    }

    // ── WY trigger timing tests ───────────────────────────────────────────────

    /// WY condition is checked at Mode 2 start (dot 0 of each visible line).
    /// wy_triggered must be false until ly == wy is seen at a Mode 2 start.
    #[test]
    fn wy_triggered_false_before_ly_equals_wy() {
        let mut ppu = Ppu::new();
        ppu.lcdc = 0x80;
        ppu.wy = 5;
        // At tick 456*5-1 we are at dot=455 on ly=4 (HBlank) — one tick before
        // the line wrap that starts ly=5 and its Mode 2 transition.
        let ticks: u32 = DOTS_PER_LINE as u32 * 5 - 1;
        for _ in 0..ticks {
            ppu.tick();
        }
        assert_eq!(ppu.ly, 4, "should still be on ly=4");
        assert!(!ppu.wy_triggered, "wy_triggered should be false before ly=wy at Mode 2");
    }

    /// Once ly == wy at Mode 2 start, wy_triggered is set and remains set.
    #[test]
    fn wy_triggered_set_at_mode2_when_ly_equals_wy() {
        let mut ppu = Ppu::new();
        ppu.lcdc = 0x80;
        ppu.wy = 5;
        // At tick 456*5 the line wraps: ly becomes 5 and mode 0→2 fires.
        // wy_triggered must be set on that same tick.
        let ticks: u32 = DOTS_PER_LINE as u32 * 5;
        for _ in 0..ticks {
            ppu.tick();
        }
        assert_eq!(ppu.ly, 5);
        assert!(ppu.wy_triggered, "wy_triggered should be set when ly == wy at Mode 2");
    }

    /// wy_triggered persists on subsequent scanlines — it's a frame-level latch.
    #[test]
    fn wy_triggered_persists_on_subsequent_lines() {
        let mut ppu = Ppu::new();
        ppu.lcdc = 0x80;
        ppu.wy = 5;
        // Tick past the trigger line all the way to line 10.
        let ticks: u32 = DOTS_PER_LINE as u32 * 10 + 1;
        for _ in 0..ticks {
            ppu.tick();
        }
        assert_eq!(ppu.ly, 10);
        assert!(ppu.wy_triggered, "wy_triggered should remain set after the trigger line");
    }

    /// wy_triggered resets at the start of a new frame (when ly wraps to 0).
    #[test]
    fn wy_triggered_resets_at_frame_wrap() {
        let mut ppu = Ppu::new();
        ppu.lcdc = 0x80;
        ppu.wy = 5;
        // Tick enough to trigger on line 5, then complete the frame.
        let full_frame: u32 = DOTS_PER_LINE as u32 * LINES_PER_FRAME as u32;
        for _ in 0..full_frame {
            ppu.tick();
        }
        assert_eq!(ppu.ly, 0, "ly should have wrapped");
        assert!(!ppu.wy_triggered, "wy_triggered should reset at frame wrap");
    }

    /// wy_triggered resets when the LCD is turned off.
    #[test]
    fn wy_triggered_resets_on_lcd_off() {
        let mut ppu = Ppu::new();
        ppu.lcdc = 0x80;
        ppu.wy = 0; // triggers immediately on line 0
        ppu.tick(); // Mode 2 start on line 0 → triggers
        assert!(ppu.wy_triggered);

        ppu.lcdc = 0x00; // turn LCD off
        ppu.tick();
        assert!(!ppu.wy_triggered, "wy_triggered should reset when LCD is turned off");
    }

    /// Mid-frame WY change after the trigger line has no further effect on the
    /// current frame (wy_triggered stays true; the window keeps drawing).
    #[test]
    fn midframe_wy_change_after_trigger_does_not_un_trigger() {
        let mut ppu = Ppu::new();
        ppu.lcdc = 0x80;
        ppu.wy = 5;
        // Tick to trigger on line 5.
        let ticks: u32 = DOTS_PER_LINE as u32 * 5 + 1;
        for _ in 0..ticks {
            ppu.tick();
        }
        assert!(ppu.wy_triggered);
        // Change WY to something that doesn't match any remaining line.
        ppu.wy = 100;
        // Advance a few more lines.
        for _ in 0..(DOTS_PER_LINE as u32 * 3) {
            ppu.tick();
        }
        assert!(ppu.wy_triggered, "changing WY after trigger must not un-trigger the latch");
    }

    /// WY=0 triggers on the very first scanline (line 0, Mode 2 start).
    #[test]
    fn wy_zero_triggers_on_line_zero() {
        let mut ppu = Ppu::new();
        ppu.lcdc = 0x80;
        ppu.wy = 0;
        ppu.tick(); // first tick: dot→1, mode 0→2, ly=0 == wy=0 → triggered
        assert!(ppu.wy_triggered, "WY=0 should trigger on the very first scanline");
    }

    // ── Mode 3 variable duration tests ───────────────────────────────────────

    /// With no scroll, no sprites, and no window, mode3_end == DRAW_END (252).
    #[test]
    fn mode3_end_baseline_no_penalties() {
        let mut ppu = Ppu::new();
        ppu.lcdc = 0x80;
        ppu.scx = 0;
        let oam = [0u8; 0xA0];
        ppu.scan_oam_and_compute_mode3_end(&oam);
        assert_eq!(ppu.mode3_end, DRAW_END, "baseline mode3_end should be {DRAW_END}");
    }

    /// SCX fine scroll adds SCX % 8 dots to Mode 3.
    #[test]
    fn mode3_end_scx_penalty() {
        let mut ppu = Ppu::new();
        ppu.lcdc = 0x80;
        for scx in 0u8..8 {
            ppu.scx = scx;
            let oam = [0u8; 0xA0];
            ppu.scan_oam_and_compute_mode3_end(&oam);
            assert_eq!(
                ppu.mode3_end,
                DRAW_END + (scx % 8) as u16,
                "SCX={scx}: mode3_end should be {} + {}",
                DRAW_END,
                scx % 8
            );
        }
    }

    /// Each sprite on the scanline adds 6 penalty dots.
    #[test]
    fn mode3_end_sprite_penalty() {
        let mut ppu = Ppu::new();
        ppu.lcdc = 0x83; // OBJ enabled, 8×8
        ppu.ly = 0;
        ppu.scx = 0;

        // Place 3 sprites visible on ly=0 (raw Y=16 → screen top=0).
        let mut oam = [0u8; 0xA0];
        for i in 0..3usize {
            oam[i * 4] = 16; // Y raw
            oam[i * 4 + 1] = 8; // X raw (on-screen but doesn't matter for count)
        }
        ppu.scan_oam_and_compute_mode3_end(&oam);
        assert_eq!(
            ppu.mode3_end,
            DRAW_END + 3 * 6,
            "3 sprites should add {} penalty dots",
            3 * 6
        );
    }

    /// Up to 10 sprites counted; an 11th does not add another penalty.
    #[test]
    fn mode3_end_sprite_cap_at_10() {
        let mut ppu = Ppu::new();
        ppu.lcdc = 0x83;
        ppu.ly = 0;
        ppu.scx = 0;

        // Place 15 sprites all visible on ly=0.
        let mut oam = [0u8; 0xA0];
        for i in 0..15usize {
            oam[i * 4] = 16; // visible
            oam[i * 4 + 1] = 8;
        }
        ppu.scan_oam_and_compute_mode3_end(&oam);
        assert_eq!(
            ppu.mode3_end,
            DRAW_END + 10 * 6,
            "should cap sprite count at 10"
        );
    }

    /// Window adds 6 penalty dots when active (wy_triggered, LCDC bits 0+5 set, WX on-screen).
    #[test]
    fn mode3_end_window_penalty() {
        let mut ppu = Ppu::new();
        ppu.lcdc = 0xA1; // LCD on, BG enabled (bit 0), window enabled (bit 5)
        ppu.wy = 0;
        ppu.wx = 7; // WX=7 → window at left edge
        ppu.wy_triggered = true; // simulate trigger already set
        ppu.scx = 0;
        let oam = [0u8; 0xA0];
        ppu.scan_oam_and_compute_mode3_end(&oam);
        assert_eq!(
            ppu.mode3_end,
            DRAW_END + 6,
            "window active should add 6 penalty dots"
        );
    }

    /// Window penalty is NOT applied when wy_triggered is false.
    #[test]
    fn mode3_end_no_window_penalty_when_not_triggered() {
        let mut ppu = Ppu::new();
        ppu.lcdc = 0xA1;
        ppu.wy = 10;
        ppu.wx = 7;
        ppu.wy_triggered = false;
        ppu.scx = 0;
        let oam = [0u8; 0xA0];
        ppu.scan_oam_and_compute_mode3_end(&oam);
        assert_eq!(ppu.mode3_end, DRAW_END, "no window penalty without wy_triggered");
    }

    /// Window penalty is NOT applied when WX is off-screen (wx < 7 → no visible start).
    #[test]
    fn mode3_end_no_window_penalty_when_wx_offscreen() {
        let mut ppu = Ppu::new();
        ppu.lcdc = 0xA1;
        ppu.wy_triggered = true;
        ppu.wx = 0; // WX=0 → win_x_screen = saturating_sub(7) = 0, but 0 < 160 → still on-screen
        // Actually WX=0..6 clips the window left. Let's use WX=167 which is >= 160+7=167? No: win_x_screen = 167-7=160, which is == SCREEN_W → off.
        ppu.wx = 167; // win_x_screen = 160 == SCREEN_W → off-screen
        ppu.scx = 0;
        let oam = [0u8; 0xA0];
        ppu.scan_oam_and_compute_mode3_end(&oam);
        assert_eq!(ppu.mode3_end, DRAW_END, "WX off-screen should not add window penalty");
    }

    /// Sprite penalty is NOT applied when OBJ is disabled (LCDC bit 1 = 0).
    #[test]
    fn mode3_end_no_sprite_penalty_when_obj_disabled() {
        let mut ppu = Ppu::new();
        ppu.lcdc = 0x81; // OBJ disabled (bit 1 = 0)
        ppu.ly = 0;
        ppu.scx = 0;
        let mut oam = [0u8; 0xA0];
        for i in 0..10usize {
            oam[i * 4] = 16;
            oam[i * 4 + 1] = 8;
        }
        ppu.scan_oam_and_compute_mode3_end(&oam);
        assert_eq!(ppu.mode3_end, DRAW_END, "OBJ disabled: no sprite penalty");
    }

    /// Mode 3 end is used by tick(): mode transitions to HBlank at mode3_end.
    #[test]
    fn hblank_starts_at_mode3_end() {
        // Use a PPU with 5-dot SCX penalty → mode3_end = 252 + 5 = 257.
        let mut ppu = Ppu::new();
        ppu.lcdc = 0x80;
        ppu.scx = 5;
        ppu.mode3_end = 257; // set directly (bus would do this via scan_oam_and_compute_mode3_end)

        // Tick to dot 256 (just inside Mode 3).
        for _ in 0..256 {
            ppu.tick();
        }
        assert_eq!(ppu.mode(), 3, "dot 256 should still be Mode 3");

        // One more tick: dot 257 → HBlank.
        ppu.tick();
        assert_eq!(ppu.mode(), 0, "dot 257 should be Mode 0 (HBlank) with mode3_end=257");
    }

    /// Sprite color 0 is transparent — the background pixel must show through.
    #[test]
    fn sprite_color_zero_is_transparent() {
        let mut ppu = Ppu::new();
        ppu.lcdc = 0x93;
        ppu.bgp  = 0b11111111; // all BG → shade 3
        ppu.obp0 = 0b11100100;
        ppu.ly   = 0;

        let mut vram = [0u8; 0x2000];
        // Tile map at 0x9800 → tile 0 (already zero).
        // BG tile 0: solid color 3 (lo=0xFF, hi=0xFF).
        vram[0] = 0xFF;
        vram[1] = 0xFF;

        // Sprite tile 1: all color 0 (lo=0x00, hi=0x00 → both bits 0).
        // (Already zero.)

        let mut oam = [0u8; 0xA0];
        oam[0] = 16;
        oam[1] = 8;
        oam[2] = 1; // tile 1 = all zeros = all color 0
        oam[3] = 0;

        ppu.render_scanline(&vram, &oam);

        // Background shade 3 should show through the transparent sprite.
        assert_eq!(ppu.framebuffer[0], 3, "transparent sprite should not occlude BG");
    }
}

use crate::apu::Apu;
use crate::memory::cartridge::Cartridge;
use crate::memory::dma::DmaController;
use crate::ppu::Ppu;
use crate::serial::{self, Serial};
use crate::timer::{self, Timer};

const BOOT_ROM_SIZE: usize = 0x100;
const BOOT_ROM_UNMAP: u16 = 0xFF50;

/// A logged bus access for debugging.
#[derive(Debug, Clone)]
pub struct BusAccessLog {
    pub kind: BusAccessKind,
    pub addr: u16,
    pub value: u8,
    /// Source of the access: "cpu", "ppu", "timer", "serial", "irq_dispatch"
    pub source: &'static str,
    /// CPU PC at the time of the access (0 for subsystem/internal sources).
    pub pc: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusAccessKind {
    Read,
    Write,
    /// Direct field write (subsystem setting IF bits, not a bus transaction)
    InternalSet,
}

#[derive(Clone)]
pub struct Bus {
    boot_rom: [u8; BOOT_ROM_SIZE],
    boot_rom_mapped: bool,
    pub cart: Cartridge,
    vram: [u8; 0x2000],
    wram: [u8; 0x2000],
    oam: [u8; 0xA0],
    io_registers: [u8; 0x80],
    hram: [u8; 0x7F],
    interrupt_enable_register: u8,
    /// 0xFF0F: Interrupt Flag register.  Exposed as a direct field so that
    /// hardware subsystems (timer, PPU) can set bits via internal wiring
    /// without going through the CPU bus — matching real SoC behaviour.
    pub if_reg: u8,
    pub timer: Timer,
    pub serial: Serial,
    pub apu: Apu,
    pub ppu: Ppu,
    pub dma: DmaController,

    // ── Debug tap ───────────────────────────────────────────────────
    /// When set, log all accesses to this address.
    watch_addr: Option<u16>,
    /// Accumulated log entries.
    pub access_log: Vec<BusAccessLog>,
    /// Current CPU PC, updated by the CPU before each instruction.
    /// Used to tag bus access log entries with the originating instruction.
    pub debug_pc: u16,
}

impl Bus {
    pub fn new() -> Self {
        Self {
            boot_rom: [0; BOOT_ROM_SIZE],
            boot_rom_mapped: true,
            cart: Cartridge::empty(),
            vram: [0; 0x2000],
            wram: [0; 0x2000],
            oam: [0; 0xA0],
            io_registers: [0; 0x80],
            hram: [0; 0x7F],
            interrupt_enable_register: 0,
            if_reg: 0,
            timer: Timer::new(),
            serial: Serial::new(),
            apu: Apu::new(),
            ppu: Ppu::new(),
            dma: DmaController::new(),
            watch_addr: None,
            access_log: Vec::new(),
            debug_pc: 0,
        }
    }

    pub fn load_boot_rom(&mut self, data: &[u8]) -> Result<(), String> {
        if data.len() != BOOT_ROM_SIZE {
            return Err(format!(
                "boot ROM must be exactly {} bytes, got {}",
                BOOT_ROM_SIZE,
                data.len()
            ));
        }
        self.boot_rom.copy_from_slice(data);
        self.boot_rom_mapped = true;
        Ok(())
    }

    pub fn load_cartridge(&mut self, data: &[u8]) {
        self.cart = Cartridge::new(data);
    }

    // ── Debug tap ────────────────────────────────────────────────────

    /// Start logging all accesses to `addr`.  Call `drain_access_log()`
    /// to retrieve and clear the log.
    pub fn watch(&mut self, addr: u16) {
        self.watch_addr = Some(addr);
        self.access_log.clear();
    }

    /// Stop watching.
    pub fn unwatch(&mut self) {
        self.watch_addr = None;
    }

    /// Drain accumulated log entries.
    pub fn drain_access_log(&mut self) -> Vec<BusAccessLog> {
        std::mem::take(&mut self.access_log)
    }

    /// Record a watched access (inlined check for zero cost when off).
    #[inline]
    fn log_access(&mut self, kind: BusAccessKind, addr: u16, value: u8, source: &'static str, pc: u16) {
        if self.watch_addr == Some(addr) {
            self.access_log.push(BusAccessLog { kind, addr, value, source, pc });
        }
    }

    /// Record a direct field modification to IF (for subsystem taps).
    /// Call this AFTER modifying `self.if_reg` directly.
    #[inline]
    pub fn log_if_write(&mut self, value: u8, source: &'static str) {
        if self.watch_addr == Some(0xFF0F) {
            self.access_log.push(BusAccessLog {
                kind: BusAccessKind::InternalSet,
                addr: 0xFF0F,
                value,
                source,
                pc: 0,
            });
        }
    }

    // ── Address helpers ─────────────────────────────────────────────

    /// Returns true if the address belongs to the timer (FF04-FF07).
    fn is_timer_addr(addr: u16) -> bool {
        matches!(addr, timer::DIV_ADDR..=timer::TAC_ADDR)
    }

    /// Returns true if the address belongs to the serial port (FF01-FF02).
    fn is_serial_addr(addr: u16) -> bool {
        matches!(addr, serial::SB_ADDR..=serial::SC_ADDR)
    }

    /// Returns true if the address belongs to the APU (FF10-FF3F).
    fn is_apu_addr(addr: u16) -> bool {
        matches!(addr, 0xFF10..=0xFF3F)
    }

    /// Returns true if the address belongs to the PPU (FF40-FF4B).
    fn is_ppu_addr(addr: u16) -> bool {
        matches!(addr, 0xFF40..=0xFF4B)
    }

    pub fn read(&self, addr: u16) -> u8 {
        // OAM DMA bus conflict: while a transfer is active the CPU can only
        // access HRAM.  All other addresses return the byte currently being
        // transferred — not 0xFF.  This is hardware-observable.
        if self.dma.active && !matches!(addr, 0xFF80..=0xFFFE) {
            return self.dma.current_byte;
        }

        match addr {
            0x0000..=0x00FF if self.boot_rom_mapped => self.boot_rom[addr as usize],
            0x0000..=0x7FFF => self.cart.read(addr),
            // VRAM inaccessible to CPU during Mode 3 (PPU drawing).
            0x8000..=0x9FFF => {
                if self.ppu.lcdc & 0x80 != 0 && self.ppu.mode() == 3 {
                    return 0xFF;
                }
                self.vram[addr as usize - 0x8000]
            }
            0xA000..=0xBFFF => self.cart.read(addr),
            0xC000..=0xDFFF => self.wram[addr as usize - 0xC000],
            // OAM inaccessible to CPU during Mode 2 (OAM scan) and Mode 3 (drawing).
            0xFE00..=0xFE9F => {
                if self.ppu.lcdc & 0x80 != 0 && matches!(self.ppu.mode(), 2 | 3) {
                    return 0xFF;
                }
                self.oam[addr as usize - 0xFE00]
            }
            // IF: upper 3 bits (5-7) always read as 1 on DMG.
            0xFF0F => self.if_reg | 0xE0,
            0xFF46 => self.dma.source_page,
            0xFF00..=0xFF7F if Self::is_serial_addr(addr) => self.serial.read(addr),
            0xFF00..=0xFF7F if Self::is_timer_addr(addr) => self.timer.read(addr),
            0xFF00..=0xFF7F if Self::is_apu_addr(addr) => self.apu.read(addr),
            0xFF00..=0xFF7F if Self::is_ppu_addr(addr) => self.ppu.read(addr),
            0xFF00..=0xFF7F => self.io_registers[addr as usize - 0xFF00],
            0xFF80..=0xFFFE => self.hram[addr as usize - 0xFF80],
            0xFFFF => self.interrupt_enable_register,
            _ => 0,
        }
    }

    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            // ROM space: MBC intercepts writes as register commands.
            0x0000..=0x7FFF => self.cart.write(addr, value),
            // VRAM writes ignored by CPU during Mode 3 (PPU holds the bus).
            0x8000..=0x9FFF => {
                if self.ppu.lcdc & 0x80 != 0 && self.ppu.mode() == 3 {
                    return;
                }
                self.vram[addr as usize - 0x8000] = value;
            }
            0xA000..=0xBFFF => self.cart.write(addr, value),
            0xC000..=0xDFFF => self.wram[addr as usize - 0xC000] = value,
            // OAM writes ignored by CPU during Mode 2 and Mode 3.
            0xFE00..=0xFE9F => {
                if self.ppu.lcdc & 0x80 != 0 && matches!(self.ppu.mode(), 2 | 3) {
                    return;
                }
                self.oam[addr as usize - 0xFE00] = value;
            }
            0xFF0F => {
                // Only bits 0-4 are writable; upper bits are unused.
                self.if_reg = value & 0x1F;
                let pc = self.debug_pc;
                self.log_access(BusAccessKind::Write, 0xFF0F, value & 0x1F, "cpu", pc);
            }
            0xFF46 => self.dma.trigger(value),
            0xFF00..=0xFF7F if Self::is_serial_addr(addr) => self.serial.write(addr, value),
            0xFF00..=0xFF7F if Self::is_timer_addr(addr) => self.timer.write(addr, value),
            0xFF00..=0xFF7F if Self::is_apu_addr(addr) => self.apu.write(addr, value),
            0xFF00..=0xFF7F if Self::is_ppu_addr(addr) => self.ppu.write(addr, value),
            0xFF00..=0xFF7F => {
                if addr == BOOT_ROM_UNMAP && value & 1 != 0 {
                    self.boot_rom_mapped = false;
                }
                self.io_registers[addr as usize - 0xFF00] = value;
            }
            0xFF80..=0xFFFE => self.hram[addr as usize - 0xFF80] = value,
            0xFFFF => self.interrupt_enable_register = value,
            _ => (),
        }
    }

    /// Advance the PPU by one T-cycle (dot), fire any pending IRQs into IF,
    /// and render the scanline when the Mode3→HBlank transition occurs.
    ///
    /// Rendering requires simultaneous access to `ppu`, `vram`, and `oam`.
    /// Rust allows this via split-field borrows — each field is a distinct
    /// memory location so there is no aliasing.
    pub fn ppu_tick(&mut self) {
        self.ppu.tick();

        // VBlank and STAT interrupts are internal SoC signals — they write
        // directly to IF rather than going through the CPU bus.
        if self.ppu.vblank_irq {
            self.ppu.vblank_irq = false;
            self.if_reg |= 0x01;
            self.log_if_write(self.if_reg, "ppu_vblank");
        }
        if self.ppu.stat_irq {
            self.ppu.stat_irq = false;
            self.if_reg |= 0x02;
            self.log_if_write(self.if_reg, "ppu_stat");
        }

        // Mode 2 start: compute Mode 3 end dot from sprite count + scroll + window.
        if self.ppu.oam_scan_ready {
            self.ppu.oam_scan_ready = false;
            let (oam, ppu) = (&self.oam, &mut self.ppu);
            ppu.scan_oam_and_compute_mode3_end(oam);
        }

        if self.ppu.scanline_ready {
            self.ppu.scanline_ready = false;
            let (vram, oam, ppu) = (&self.vram, &self.oam, &mut self.ppu);
            ppu.render_scanline(vram, oam);
        }
    }

    /// Advance the DMA controller by one M-cycle.
    ///
    /// Must be called once per M-cycle from `system.rs` after `cpu.tick()`.
    /// Reads are performed directly from internal arrays using the DMA address
    /// decoding scheme (external bus / VRAM bus), bypassing the CPU bus
    /// conflict check.
    pub fn dma_tick(&mut self) {
        if let Some(oam_index) = self.dma.advance() {
            let src_addr = ((self.dma.source_page as u16) << 8) | oam_index as u16;
            let byte = self.dma_read_source(src_addr);
            self.oam[oam_index] = byte;
            self.dma.current_byte = byte;
        }
    }

    /// Read from the DMA source bus.
    ///
    /// The DMA controller uses a different address decoding scheme from the
    /// CPU: it accesses the external bus (ROM / ext-RAM) or the VRAM bus
    /// directly.  This bypasses the CPU's memory map and the DMA conflict
    /// check, matching the hardware behaviour described in the gekkio reference.
    fn dma_read_source(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x00FF if self.boot_rom_mapped => self.boot_rom[addr as usize],
            0x0000..=0x7FFF => self.cart.read(addr),
            0x8000..=0x9FFF => self.vram[addr as usize - 0x8000],
            0xA000..=0xBFFF => self.cart.read(addr),
            // WRAM is internal but accessible to the DMA unit; most games
            // store sprite tables in WRAM and DMA from there.
            0xC000..=0xDFFF => self.wram[addr as usize - 0xC000],
            // 0xE0–0xFF: undefined/unsupported source pages.
            _ => 0xFF,
        }
    }

    pub fn dump(&self, range: (u16, u16)) {
        for address in range.0..=range.1 {
            println!("[{:#x}]: {:#x}", address, self.read(address));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tick the DMA controller N M-cycles directly (bypasses the CPU).
    fn tick_dma(bus: &mut Bus, m_cycles: u32) {
        for _ in 0..m_cycles {
            bus.dma_tick();
        }
    }

    #[test]
    fn full_transfer_copies_wram_to_oam() {
        let mut bus = Bus::new();
        // Write a known pattern to WRAM page 0xC0.
        for i in 0u8..160 {
            bus.wram[i as usize] = i.wrapping_mul(3);
        }
        // Trigger DMA from page 0xC0.
        bus.write(0xFF46, 0xC0);
        // 1 startup + 160 transfer = 161 M-cycles.
        tick_dma(&mut bus, 161);
        assert!(!bus.dma.active);
        for i in 0u8..160 {
            assert_eq!(bus.oam[i as usize], i.wrapping_mul(3), "oam[{i}] mismatch");
        }
    }

    #[test]
    fn startup_delay_defers_first_byte() {
        let mut bus = Bus::new();
        for i in 0u8..160 {
            bus.wram[i as usize] = i;
        }
        bus.write(0xFF46, 0xC0);

        // After 1 M-cycle (startup delay only) — no bytes transferred yet.
        tick_dma(&mut bus, 1);
        assert!(bus.dma.active);
        assert_eq!(bus.oam[0], 0, "no bytes should be copied during startup delay");

        // After 2 M-cycles — first byte transferred.
        tick_dma(&mut bus, 1);
        assert_eq!(bus.oam[0], 0, "first source byte is 0");
        // OAM[1] still untouched.
        assert_eq!(bus.oam[1], 0);
    }

    #[test]
    fn still_active_at_160_m_cycles_done_at_161() {
        let mut bus = Bus::new();
        bus.write(0xFF46, 0xC0);

        tick_dma(&mut bus, 160);
        assert!(bus.dma.active, "should still be active after 160 M-cycles");

        tick_dma(&mut bus, 1);
        assert!(!bus.dma.active, "should be inactive after 161 M-cycles");
    }

    #[test]
    fn hram_readable_during_dma() {
        let mut bus = Bus::new();
        bus.hram[0x10] = 0xAB; // address 0xFF90
        bus.write(0xFF46, 0xC0);
        tick_dma(&mut bus, 1); // transfer active

        assert!(bus.dma.active);
        assert_eq!(bus.read(0xFF90), 0xAB, "HRAM must be accessible during DMA");
    }

    #[test]
    fn blocked_read_returns_current_dma_byte() {
        let mut bus = Bus::new();
        bus.wram[0] = 0x42;
        bus.write(0xFF46, 0xC0);
        // After startup (1) + first transfer (1) = 2 M-cycles, current_byte = 0x42.
        tick_dma(&mut bus, 2);

        assert!(bus.dma.active);
        // CPU read of ROM should return current_byte, not the actual ROM value.
        assert_eq!(bus.read(0x0100), bus.dma.current_byte);
    }

    #[test]
    fn retrigger_during_active_transfer_is_ignored() {
        let mut bus = Bus::new();
        for i in 0u8..160 {
            bus.wram[i as usize] = i;
        }
        // Different page with different data.
        for i in 0u8..160 {
            bus.wram[0x1000 + i as usize] = i.wrapping_add(0x80);
        }
        bus.write(0xFF46, 0xC0); // start from page 0xC0
        tick_dma(&mut bus, 5);

        // Try to re-trigger with a different page mid-transfer.
        bus.write(0xFF46, 0xD0);
        assert_eq!(bus.dma.source_page, 0xC0, "source_page must not change on re-trigger");

        // Complete the transfer — OAM should contain page 0xC0 data.
        tick_dma(&mut bus, 156);
        for i in 0u8..160 {
            assert_eq!(bus.oam[i as usize], i, "oam[{i}] should come from original source");
        }
    }

    #[test]
    fn source_page_readable_via_ff46() {
        let mut bus = Bus::new();
        bus.write(0xFF46, 0xC5);
        // DMA is now active; the source_page field holds 0xC5.
        assert_eq!(bus.dma.source_page, 0xC5);
        // After the transfer completes the CPU bus is unblocked and 0xFF46 is
        // readable again.
        tick_dma(&mut bus, 161);
        assert!(!bus.dma.active);
        assert_eq!(bus.read(0xFF46), 0xC5);
    }

    // ── PPU VRAM/OAM access restriction tests ────────────────────────────────

    /// Advance the bus PPU to exactly Mode 3 (dot 81–251 on line 0).
    /// Returns the bus with LCDC=0x80 and PPU in Mode 3.
    fn bus_in_mode3() -> Bus {
        let mut bus = Bus::new();
        bus.ppu.lcdc = 0x80;
        // Tick to dot 81 (first dot of Mode 3 on line 0, baseline mode3_end=252).
        for _ in 0..81 {
            bus.ppu_tick();
        }
        assert_eq!(bus.ppu.mode(), 3, "should be in Mode 3 for test setup");
        bus
    }

    /// Advance the bus PPU to Mode 2 (dot 1–79 on line 0).
    fn bus_in_mode2() -> Bus {
        let mut bus = Bus::new();
        bus.ppu.lcdc = 0x80;
        // Tick to dot 1 (first dot of Mode 2 on line 0).
        bus.ppu_tick();
        assert_eq!(bus.ppu.mode(), 2, "should be in Mode 2 for test setup");
        bus
    }

    #[test]
    fn vram_readable_in_mode0() {
        // Mode 0 (HBlank): advance past Mode 3 on line 0 (dot 253).
        let mut bus = Bus::new();
        bus.ppu.lcdc = 0x80;
        for _ in 0..253 {
            bus.ppu_tick();
        }
        assert_eq!(bus.ppu.mode(), 0);
        bus.vram[0] = 0x42;
        assert_eq!(bus.read(0x8000), 0x42, "VRAM readable in Mode 0");
    }

    #[test]
    fn vram_readable_in_mode1() {
        // Mode 1 (VBlank): advance to line 144.
        let mut bus = Bus::new();
        bus.ppu.lcdc = 0x80;
        let vblank_dot = 456 * 144 + 1;
        for _ in 0..vblank_dot {
            bus.ppu_tick();
        }
        assert_eq!(bus.ppu.mode(), 1);
        bus.vram[0] = 0x55;
        assert_eq!(bus.read(0x8000), 0x55, "VRAM readable in Mode 1");
    }

    #[test]
    fn vram_readable_in_mode2() {
        let mut bus = bus_in_mode2();
        bus.vram[0] = 0xAB;
        assert_eq!(bus.read(0x8000), 0xAB, "VRAM readable in Mode 2");
    }

    #[test]
    fn vram_returns_0xff_during_mode3() {
        let mut bus = bus_in_mode3();
        bus.vram[0] = 0x42;
        assert_eq!(bus.read(0x8000), 0xFF, "VRAM read should return 0xFF during Mode 3");
        // Entire VRAM range should be blocked.
        bus.vram[0x1FFF] = 0x55;
        assert_eq!(bus.read(0x9FFF), 0xFF, "VRAM at 0x9FFF should return 0xFF during Mode 3");
    }

    #[test]
    fn vram_write_ignored_during_mode3() {
        let mut bus = bus_in_mode3();
        bus.vram[0] = 0x99;
        bus.write(0x8000, 0xAA); // should be silently ignored
        assert_eq!(bus.vram[0], 0x99, "VRAM write during Mode 3 must be ignored");
    }

    #[test]
    fn vram_accessible_when_lcd_off() {
        let mut bus = Bus::new();
        // LCD off (LCDC bit 7 = 0): VRAM always accessible.
        bus.ppu.lcdc = 0x00;
        bus.vram[0] = 0x77;
        assert_eq!(bus.read(0x8000), 0x77, "VRAM accessible when LCD is off");
        bus.write(0x8000, 0x88);
        assert_eq!(bus.vram[0], 0x88, "VRAM writable when LCD is off");
    }

    #[test]
    fn oam_readable_in_mode0() {
        let mut bus = Bus::new();
        bus.ppu.lcdc = 0x80;
        for _ in 0..253 {
            bus.ppu_tick();
        }
        assert_eq!(bus.ppu.mode(), 0);
        bus.oam[0] = 0x33;
        assert_eq!(bus.read(0xFE00), 0x33, "OAM readable in Mode 0");
    }

    #[test]
    fn oam_readable_in_mode1() {
        let mut bus = Bus::new();
        bus.ppu.lcdc = 0x80;
        let vblank_dot = 456 * 144 + 1;
        for _ in 0..vblank_dot {
            bus.ppu_tick();
        }
        assert_eq!(bus.ppu.mode(), 1);
        bus.oam[0] = 0x44;
        assert_eq!(bus.read(0xFE00), 0x44, "OAM readable in Mode 1");
    }

    #[test]
    fn oam_returns_0xff_during_mode2() {
        let mut bus = bus_in_mode2();
        bus.oam[0] = 0xBB;
        assert_eq!(bus.read(0xFE00), 0xFF, "OAM read should return 0xFF during Mode 2");
    }

    #[test]
    fn oam_write_ignored_during_mode2() {
        let mut bus = bus_in_mode2();
        bus.oam[0] = 0xCC;
        bus.write(0xFE00, 0xDD);
        assert_eq!(bus.oam[0], 0xCC, "OAM write during Mode 2 must be ignored");
    }

    #[test]
    fn oam_returns_0xff_during_mode3() {
        let mut bus = bus_in_mode3();
        bus.oam[0] = 0xBE;
        assert_eq!(bus.read(0xFE00), 0xFF, "OAM read should return 0xFF during Mode 3");
    }

    #[test]
    fn oam_write_ignored_during_mode3() {
        let mut bus = bus_in_mode3();
        bus.oam[0] = 0xEF;
        bus.write(0xFE00, 0x12);
        assert_eq!(bus.oam[0], 0xEF, "OAM write during Mode 3 must be ignored");
    }

    #[test]
    fn oam_accessible_when_lcd_off() {
        let mut bus = Bus::new();
        bus.ppu.lcdc = 0x00;
        bus.oam[0] = 0x66;
        assert_eq!(bus.read(0xFE00), 0x66, "OAM accessible when LCD is off");
        bus.write(0xFE00, 0x77);
        assert_eq!(bus.oam[0], 0x77, "OAM writable when LCD is off");
    }

    #[test]
    fn if_reg_direct_and_bus_access_agree() {
        let mut bus = Bus::new();
        bus.if_reg = 0x03;
        // Read returns if_reg | 0xE0 (upper bits always 1 on DMG)
        assert_eq!(bus.read(0xFF0F), 0x03 | 0xE0);
        // Write masks to lower 5 bits
        bus.write(0xFF0F, 0x1F);
        assert_eq!(bus.if_reg, 0x1F);
        // Writing upper bits has no effect
        bus.write(0xFF0F, 0xFF);
        assert_eq!(bus.if_reg, 0x1F);
    }
}

use crate::apu::Apu;
use crate::memory::dma::DmaController;
use crate::ppu::Ppu;
use crate::timer::{self, Timer};

const BOOT_ROM_SIZE: usize = 0x100;
const BOOT_ROM_UNMAP: u16 = 0xFF50;

#[derive(Clone)]
pub struct Bus {
    boot_rom: [u8; BOOT_ROM_SIZE],
    boot_rom_mapped: bool,
    rom_bank_0: [u8; 0x4000],
    switchable_rom_bank: [u8; 0x4000],
    vram: [u8; 0x2000],
    external_ram: [u8; 0x2000],
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
    pub apu: Apu,
    pub ppu: Ppu,
    pub dma: DmaController,
}

impl Bus {
    pub fn new() -> Self {
        Self {
            boot_rom: [0; BOOT_ROM_SIZE],
            boot_rom_mapped: true,
            rom_bank_0: [0; 0x4000],
            switchable_rom_bank: [0; 0x4000],
            vram: [0; 0x2000],
            external_ram: [0; 0x2000],
            wram: [0; 0x2000],
            oam: [0; 0xA0],
            io_registers: [0; 0x80],
            hram: [0; 0x7F],
            interrupt_enable_register: 0,
            if_reg: 0,
            timer: Timer::new(),
            apu: Apu::new(),
            ppu: Ppu::new(),
            dma: DmaController::new(),
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
        let bank_0_end = data.len().min(0x4000);
        self.rom_bank_0[..bank_0_end].copy_from_slice(&data[..bank_0_end]);

        if data.len() > 0x4000 {
            let bank_1_end = (data.len() - 0x4000).min(0x4000);
            self.switchable_rom_bank[..bank_1_end]
                .copy_from_slice(&data[0x4000..0x4000 + bank_1_end]);
        }
    }

    /// Returns true if the address belongs to the timer (FF04-FF07).
    fn is_timer_addr(addr: u16) -> bool {
        matches!(addr, timer::DIV_ADDR..=timer::TAC_ADDR)
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
            0x0000..=0x3FFF => self.rom_bank_0[addr as usize],
            0x4000..=0x7FFF => self.switchable_rom_bank[addr as usize - 0x4000],
            0x8000..=0x9FFF => self.vram[addr as usize - 0x8000],
            0xA000..=0xBFFF => self.external_ram[addr as usize - 0xA000],
            0xC000..=0xDFFF => self.wram[addr as usize - 0xC000],
            0xFE00..=0xFE9F => self.oam[addr as usize - 0xFE00],
            0xFF0F => self.if_reg,
            0xFF46 => self.dma.source_page,
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
            0x0000..=0x3FFF => (), // ROM is read-only
            0x4000..=0x7FFF => (), // likewise - used for bank switching
            0x8000..=0x9FFF => self.vram[addr as usize - 0x8000] = value,
            0xA000..=0xBFFF => self.external_ram[addr as usize - 0xA000] = value,
            0xC000..=0xDFFF => self.wram[addr as usize - 0xC000] = value,
            0xFE00..=0xFE9F => self.oam[addr as usize - 0xFE00] = value,
            0xFF0F => self.if_reg = value,
            0xFF46 => self.dma.trigger(value),
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
            0x0000..=0x3FFF => self.rom_bank_0[addr as usize],
            0x4000..=0x7FFF => self.switchable_rom_bank[addr as usize - 0x4000],
            0x8000..=0x9FFF => self.vram[addr as usize - 0x8000],
            0xA000..=0xBFFF => self.external_ram[addr as usize - 0xA000],
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

    #[test]
    fn if_reg_direct_and_bus_access_agree() {
        let mut bus = Bus::new();
        bus.if_reg = 0x03;
        assert_eq!(bus.read(0xFF0F), 0x03);
        bus.write(0xFF0F, 0x1F);
        assert_eq!(bus.if_reg, 0x1F);
    }
}

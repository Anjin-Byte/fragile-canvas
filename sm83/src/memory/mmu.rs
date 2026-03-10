
use crate::memory::bus::Bus;

const BOOT_ROM_SIZE: usize = 0x100;
const BOOT_ROM_UNMAP: u16 = 0xFF50;

#[derive(Clone)]
pub struct MMU {
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
}

impl MMU {
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
        }
    }

    pub fn load_boot_rom(&mut self, data: &[u8]) {
        assert!(
            data.len() == BOOT_ROM_SIZE,
            "boot ROM must be exactly {BOOT_ROM_SIZE} bytes, got {}",
            data.len()
        );
        self.boot_rom.copy_from_slice(data);
        self.boot_rom_mapped = true;
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
}

impl Bus for MMU {
    fn read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x00FF if self.boot_rom_mapped => self.boot_rom[addr as usize],
            0x0000..=0x3FFF => self.rom_bank_0[addr as usize],
            0x4000..=0x7FFF => self.switchable_rom_bank[addr as usize - 0x4000],
            0x8000..=0x9FFF => self.vram[addr as usize - 0x8000],
            0xA000..=0xBFFF => self.external_ram[addr as usize - 0xA000],
            0xC000..=0xDFFF => self.wram[addr as usize - 0xC000],
            0xFE00..=0xFE9F => self.oam[addr as usize - 0xFE00],
            0xFF00..=0xFF7F => self.io_registers[addr as usize - 0xFF00],
            0xFF80..=0xFFFE => self.hram[addr as usize - 0xFF80],
            0xFFFF => self.interrupt_enable_register,
            _ => 0,
        }
    }

    fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x3FFF => (), // ROM is read-only
            0x4000..=0x7FFF => (), // likewise - used for bank switching
            0x8000..=0x9FFF => self.vram[addr as usize - 0x8000] = value,
            0xA000..=0xBFFF => self.external_ram[addr as usize - 0xA000] = value,
            0xC000..=0xDFFF => self.wram[addr as usize - 0xC000] = value,
            0xFE00..=0xFE9F => self.oam[addr as usize - 0xFE00] = value,
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

    fn dump(&self, range: (u16, u16)) {
        for address in range.0..=range.1 {
            println!("[{:#x}]: {:#x}", address, self.read(address));
        }
    }
}

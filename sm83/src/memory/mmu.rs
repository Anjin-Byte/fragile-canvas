
use crate::memory::bus::Bus;

#[derive(Clone)]
pub struct MMU {
    rom_bank_0: [u8; 0x4000],          // 16KB
    switchable_rom_bank: [u8; 0x4000], // 16KB
    vram: [u8; 0x2000],                // 8KB
    external_ram: [u8; 0x2000],        // 8KB
    wram: [u8; 0x2000],                // 8KB Work RAM (including both banks)
    oam: [u8; 0xA0],                   // Sprite attribute table
    io_registers: [u8; 0x80],          // I/O Registers
    hram: [u8; 0x7F],                  // High RAM
    interrupt_enable_register: u8,
}

impl MMU {
    pub fn new() -> Self {
        Self {
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

    pub fn load_rom(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x3FFF => self.rom_bank_0[addr as usize - 0x0000] = value,
            0x4000..=0x7FFF => self.switchable_rom_bank[addr as usize - 0x4000] = value,
            _ => (),
        }
    }
}

impl Bus for MMU {
    fn read(&self, addr: u16) -> u8 {
        match addr {
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
            0xFF00..=0xFF7F => self.io_registers[addr as usize - 0xFF00] = value,
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

/// Game Boy cartridge with Memory Bank Controller emulation.
///
/// Models the physical MBC chip on the cartridge PCB: it watches the address
/// and data buses, intercepts writes to ROM space (0x0000–0x7FFF) as register
/// commands, and gates the SRAM /CE line for external RAM (0xA000–0xBFFF).
///
/// The ROM data and save RAM are owned here so they can be serialised as part
/// of a save-state snapshot without extra indirection.

const ROM_BANK_SIZE: usize = 0x4000; // 16 KiB
const RAM_BANK_SIZE: usize = 0x2000; //  8 KiB

// ── MBC1 ─────────────────────────────────────────────────────────────────────
//
// Up to 2 MiB ROM (128 × 16 KiB banks) and 32 KiB RAM (4 × 8 KiB banks).
// Two banking modes:
//   Mode 0 (default) – 0x0000 fixed to bank 0; upper 2 bits only apply to
//                      the 0x4000 window.  RAM always bank 0.
//   Mode 1 (advanced)– 0x0000 also shifted by the upper 2-bit register;
//                      RAM banking enabled.

#[derive(Debug, Clone)]
struct Mbc1 {
    /// Lower 5-bit ROM bank register (0x2000–0x3FFF). 0→1 aliased by hardware.
    rom_bank_lo: u8,
    /// Upper 2-bit register (0x4000–0x5FFF). Selects ROM block or RAM bank.
    bank_hi: u8,
    /// Banking mode: false = ROM mode (0), true = RAM/advanced mode (1).
    mode: bool,
    /// RAM enabled when lower nibble of write to 0x0000–0x1FFF equals 0xA.
    ram_enabled: bool,
}

impl Mbc1 {
    fn new() -> Self {
        Self { rom_bank_lo: 1, bank_hi: 0, mode: false, ram_enabled: false }
    }

    /// ROM bank shown in the 0x0000–0x3FFF window.
    fn lower_bank(&self, total_banks: usize) -> usize {
        if self.mode {
            // Mode 1: upper 2 bits select 512 KiB block; lower 5 bits = 0.
            ((self.bank_hi as usize) << 5) % total_banks.max(1)
        } else {
            0
        }
    }

    /// ROM bank shown in the 0x4000–0x7FFF window.
    fn upper_bank(&self, total_banks: usize) -> usize {
        let bank = ((self.bank_hi as usize) << 5) | (self.rom_bank_lo as usize);
        bank % total_banks.max(1)
    }

    /// RAM bank in use (mode 1 only; mode 0 always 0).
    fn ram_bank(&self) -> usize {
        if self.mode { self.bank_hi as usize } else { 0 }
    }
}

fn mbc1_read(s: &Mbc1, rom: &[u8], ram: &[u8], addr: u16) -> u8 {
    let total_banks = rom.len() / ROM_BANK_SIZE;
    match addr {
        0x0000..=0x3FFF => {
            rom_read(rom, s.lower_bank(total_banks), addr as usize)
        }
        0x4000..=0x7FFF => {
            rom_read(rom, s.upper_bank(total_banks), addr as usize - 0x4000)
        }
        0xA000..=0xBFFF => {
            if !s.ram_enabled { return 0xFF; }
            ram_read(ram, s.ram_bank(), addr as usize - 0xA000)
        }
        _ => 0xFF,
    }
}

fn mbc1_write(s: &mut Mbc1, ram: &mut [u8], addr: u16, val: u8) {
    match addr {
        0x0000..=0x1FFF => {
            s.ram_enabled = (val & 0x0F) == 0x0A;
        }
        0x2000..=0x3FFF => {
            // 5-bit register; hardware translates 0x00 → 0x01.
            let bank = val & 0x1F;
            s.rom_bank_lo = if bank == 0 { 1 } else { bank };
        }
        0x4000..=0x5FFF => {
            s.bank_hi = val & 0x03;
        }
        0x6000..=0x7FFF => {
            s.mode = val & 0x01 != 0;
        }
        0xA000..=0xBFFF => {
            if !s.ram_enabled { return; }
            let idx = s.ram_bank() * RAM_BANK_SIZE + (addr as usize - 0xA000);
            if let Some(b) = ram.get_mut(idx) { *b = val; }
        }
        _ => {}
    }
}

// ── MBC3 ─────────────────────────────────────────────────────────────────────
//
// Up to 2 MiB ROM (128 × 16 KiB banks), 32 KiB RAM (4 × 8 KiB banks), and
// an optional Real-Time Clock (RTC).
//
// Unlike MBC1 there is no mode bit and no 0x20/0x40/0x60 aliasing — all 7-bit
// bank values are used directly.  Bank 0 still aliases to 1.

#[derive(Debug, Clone)]
struct Rtc {
    seconds: u8,   // 0–59
    minutes: u8,   // 0–59
    hours: u8,     // 0–23
    day_lo: u8,    // day counter bits 7:0
    day_hi: u8,    // bit7=carry, bit6=halt, bit0=day MSB

    /// Latched copies (frozen snapshot read by the CPU).
    lat_seconds: u8,
    lat_minutes: u8,
    lat_hours: u8,
    lat_day_lo: u8,
    lat_day_hi: u8,

    /// Latch sequence: armed after 0x00 write, fires on subsequent 0x01 write.
    latch_arm: bool,
}

impl Rtc {
    fn new() -> Self {
        // Start halted (bit6 of day_hi set); latched copies match live values.
        Self {
            seconds: 0, minutes: 0, hours: 0, day_lo: 0, day_hi: 0x40,
            lat_seconds: 0, lat_minutes: 0, lat_hours: 0,
            lat_day_lo: 0, lat_day_hi: 0x40,
            latch_arm: false,
        }
    }

    /// Copy live registers into the latched read registers.
    fn latch(&mut self) {
        self.lat_seconds = self.seconds;
        self.lat_minutes = self.minutes;
        self.lat_hours   = self.hours;
        self.lat_day_lo  = self.day_lo;
        self.lat_day_hi  = self.day_hi;
    }

    fn read_reg(&self, reg: u8) -> u8 {
        match reg {
            0x08 => self.lat_seconds,
            0x09 => self.lat_minutes,
            0x0A => self.lat_hours,
            0x0B => self.lat_day_lo,
            0x0C => self.lat_day_hi,
            _    => 0xFF,
        }
    }

    fn write_reg(&mut self, reg: u8, val: u8) {
        // Writes update both the live and latched values (matches hardware).
        match reg {
            0x08 => { self.seconds = val & 0x3F; self.lat_seconds = self.seconds; }
            0x09 => { self.minutes = val & 0x3F; self.lat_minutes = self.minutes; }
            0x0A => { self.hours   = val & 0x1F; self.lat_hours   = self.hours;   }
            0x0B => { self.day_lo  = val;         self.lat_day_lo  = val;          }
            0x0C => { self.day_hi  = val & 0xC1;  self.lat_day_hi  = self.day_hi; }
            _    => {}
        }
    }
}

#[derive(Debug, Clone)]
struct Mbc3 {
    /// 7-bit ROM bank register (0x2000–0x3FFF). 0→1 aliased.
    rom_bank: u8,
    /// 0x00–0x07 selects RAM bank; 0x08–0x0C maps an RTC register.
    ram_bank: u8,
    /// RAM and RTC both enabled by writing 0x0A to 0x0000–0x1FFF.
    ram_enabled: bool,
    rtc: Rtc,
}

impl Mbc3 {
    fn new() -> Self {
        Self { rom_bank: 1, ram_bank: 0, ram_enabled: false, rtc: Rtc::new() }
    }
}

fn mbc3_read(s: &Mbc3, rom: &[u8], ram: &[u8], addr: u16) -> u8 {
    let total_banks = rom.len() / ROM_BANK_SIZE;
    match addr {
        0x0000..=0x3FFF => rom_read(rom, 0, addr as usize),
        0x4000..=0x7FFF => {
            let bank = (s.rom_bank as usize) % total_banks.max(1);
            rom_read(rom, bank, addr as usize - 0x4000)
        }
        0xA000..=0xBFFF => {
            if !s.ram_enabled { return 0xFF; }
            if s.ram_bank >= 0x08 {
                s.rtc.read_reg(s.ram_bank)
            } else {
                ram_read(ram, s.ram_bank as usize, addr as usize - 0xA000)
            }
        }
        _ => 0xFF,
    }
}

fn mbc3_write(s: &mut Mbc3, ram: &mut [u8], addr: u16, val: u8) {
    match addr {
        0x0000..=0x1FFF => {
            s.ram_enabled = (val & 0x0F) == 0x0A;
        }
        0x2000..=0x3FFF => {
            let bank = val & 0x7F;
            s.rom_bank = if bank == 0 { 1 } else { bank };
        }
        0x4000..=0x5FFF => {
            s.ram_bank = val; // 0x00–0x07 = RAM, 0x08–0x0C = RTC register
        }
        0x6000..=0x7FFF => {
            // RTC latch sequence: write 0x00 to arm, then 0x01 to fire.
            if val == 0x00 {
                s.rtc.latch_arm = true;
            } else if val == 0x01 && s.rtc.latch_arm {
                s.rtc.latch();
                s.rtc.latch_arm = false;
            } else {
                s.rtc.latch_arm = false;
            }
        }
        0xA000..=0xBFFF => {
            if !s.ram_enabled { return; }
            if s.ram_bank >= 0x08 {
                s.rtc.write_reg(s.ram_bank, val);
            } else {
                let idx = (s.ram_bank as usize) * RAM_BANK_SIZE + (addr as usize - 0xA000);
                if let Some(b) = ram.get_mut(idx) { *b = val; }
            }
        }
        _ => {}
    }
}

// ── MBC5 ─────────────────────────────────────────────────────────────────────
//
// Up to 8 MiB ROM (512 × 16 KiB banks) and 128 KiB RAM (16 × 8 KiB banks).
// No zero-bank aliasing (bank 0 is directly selectable).
// No banking mode bit.
// ROM bank split across two registers: low byte at 0x2000–0x2FFF, high bit
// (bit 8) at 0x3000–0x3FFF.

#[derive(Debug, Clone)]
struct Mbc5 {
    /// 9-bit ROM bank (0x000–0x1FF).
    rom_bank: u16,
    /// 4-bit RAM bank (0x00–0x0F).
    ram_bank: u8,
    ram_enabled: bool,
}

impl Mbc5 {
    fn new() -> Self {
        Self { rom_bank: 0, ram_bank: 0, ram_enabled: false }
    }
}

fn mbc5_read(s: &Mbc5, rom: &[u8], ram: &[u8], addr: u16) -> u8 {
    let total_banks = rom.len() / ROM_BANK_SIZE;
    match addr {
        0x0000..=0x3FFF => rom_read(rom, 0, addr as usize),
        0x4000..=0x7FFF => {
            let bank = (s.rom_bank as usize) % total_banks.max(1);
            rom_read(rom, bank, addr as usize - 0x4000)
        }
        0xA000..=0xBFFF => {
            if !s.ram_enabled { return 0xFF; }
            ram_read(ram, s.ram_bank as usize, addr as usize - 0xA000)
        }
        _ => 0xFF,
    }
}

fn mbc5_write(s: &mut Mbc5, ram: &mut [u8], addr: u16, val: u8) {
    match addr {
        0x0000..=0x1FFF => {
            s.ram_enabled = (val & 0x0F) == 0x0A;
        }
        0x2000..=0x2FFF => {
            // Low 8 bits of ROM bank; no zero aliasing.
            s.rom_bank = (s.rom_bank & 0x100) | (val as u16);
        }
        0x3000..=0x3FFF => {
            // High bit (bit 8) of ROM bank.
            s.rom_bank = (s.rom_bank & 0x0FF) | (((val & 0x01) as u16) << 8);
        }
        0x4000..=0x5FFF => {
            s.ram_bank = val & 0x0F;
        }
        0xA000..=0xBFFF => {
            if !s.ram_enabled { return; }
            let idx = (s.ram_bank as usize) * RAM_BANK_SIZE + (addr as usize - 0xA000);
            if let Some(b) = ram.get_mut(idx) { *b = val; }
        }
        _ => {}
    }
}

// ── MBC enum ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum Mbc {
    /// ROM-only cartridge (type 0x00). No bank switching.
    None,
    Mbc1(Mbc1),
    Mbc3(Mbc3),
    Mbc5(Mbc5),
}

// ── Helpers ───────────────────────────────────────────────────────────────────

#[inline]
fn rom_read(rom: &[u8], bank: usize, offset: usize) -> u8 {
    rom.get(bank * ROM_BANK_SIZE + offset).copied().unwrap_or(0xFF)
}

#[inline]
fn ram_read(ram: &[u8], bank: usize, offset: usize) -> u8 {
    ram.get(bank * RAM_BANK_SIZE + offset).copied().unwrap_or(0xFF)
}

// ── Cartridge ─────────────────────────────────────────────────────────────────

/// Owned cartridge state: ROM data + save RAM + MBC register state.
///
/// Clone-able for save states.
#[derive(Debug, Clone)]
pub struct Cartridge {
    rom: Vec<u8>,
    ram: Vec<u8>,
    mbc: Mbc,
    /// BATTERY flag from header byte 0x0147. Save RAM should be persisted.
    pub has_battery: bool,
}

impl Cartridge {
    /// Construct an empty cartridge (all ROM reads → 0xFF, no MBC).
    pub fn empty() -> Self {
        Self {
            rom: vec![0xFF; ROM_BANK_SIZE * 2],
            ram: vec![0xFF; RAM_BANK_SIZE],
            mbc: Mbc::None,
            has_battery: false,
        }
    }

    /// Parse the cartridge header and load the ROM.
    pub fn new(data: &[u8]) -> Self {
        let type_byte    = data.get(0x0147).copied().unwrap_or(0x00);
        let rom_sz_byte  = data.get(0x0148).copied().unwrap_or(0x00);
        let ram_sz_byte  = data.get(0x0149).copied().unwrap_or(0x00);

        // ROM: 32 KiB × 2^N  (header byte 0x0148)
        let rom_banks: usize = if rom_sz_byte <= 8 {
            2usize << rom_sz_byte
        } else {
            2 // fall back to minimum for unknown values
        };

        // RAM: decoded from byte 0x0149
        let ram_size: usize = match ram_sz_byte {
            0x02 => RAM_BANK_SIZE,          //  8 KiB (1 bank)
            0x03 => RAM_BANK_SIZE * 4,      // 32 KiB (4 banks)
            0x04 => RAM_BANK_SIZE * 16,     // 128 KiB (16 banks)
            0x05 => RAM_BANK_SIZE * 8,      // 64 KiB (8 banks)
            _    => 0,
        };

        let has_battery = matches!(type_byte,
            0x03 | 0x06 | 0x09 | 0x0D | 0x0F | 0x10 | 0x13 | 0x1B | 0x1E | 0x22 | 0xFF
        );

        let mbc = match type_byte {
            0x00 | 0x08 | 0x09           => Mbc::None,
            0x01..=0x03                  => Mbc::Mbc1(Mbc1::new()),
            0x0F..=0x13                  => Mbc::Mbc3(Mbc3::new()),
            0x19..=0x1E                  => Mbc::Mbc5(Mbc5::new()),
            _                            => Mbc::None, // unknown → treat as ROM-only
        };

        // Allocate and fill ROM, padded to the declared bank count.
        let rom_total = rom_banks * ROM_BANK_SIZE;
        let mut rom = vec![0xFF; rom_total];
        let copy = data.len().min(rom_total);
        rom[..copy].copy_from_slice(&data[..copy]);

        // RAM: at least one bank so out-of-bounds accesses return 0xFF cleanly.
        let ram = vec![0x00; ram_size.max(RAM_BANK_SIZE)];

        Self { rom, ram, mbc, has_battery }
    }

    pub fn read(&self, addr: u16) -> u8 {
        match &self.mbc {
            Mbc::None    => self.read_none(addr),
            Mbc::Mbc1(s) => mbc1_read(s, &self.rom, &self.ram, addr),
            Mbc::Mbc3(s) => mbc3_read(s, &self.rom, &self.ram, addr),
            Mbc::Mbc5(s) => mbc5_read(s, &self.rom, &self.ram, addr),
        }
    }

    pub fn write(&mut self, addr: u16, val: u8) {
        // Destructure here so Rust sees disjoint field borrows in each arm.
        let Cartridge { mbc, ram, .. } = self;
        match mbc {
            Mbc::None => {
                // ROM-only: external RAM at 0xA000 is still writable.
                if let 0xA000..=0xBFFF = addr {
                    let idx = addr as usize - 0xA000;
                    if let Some(b) = ram.get_mut(idx) { *b = val; }
                }
            }
            Mbc::Mbc1(s) => mbc1_write(s, ram, addr, val),
            Mbc::Mbc3(s) => mbc3_write(s, ram, addr, val),
            Mbc::Mbc5(s) => mbc5_write(s, ram, addr, val),
        }
    }

    fn read_none(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => self.rom.get(addr as usize).copied().unwrap_or(0xFF),
            0xA000..=0xBFFF => self.ram.get(addr as usize - 0xA000).copied().unwrap_or(0xFF),
            _ => 0xFF,
        }
    }

    /// Return the raw save RAM slice (for persistence / save states).
    pub fn save_ram(&self) -> &[u8] {
        &self.ram
    }

    /// Overwrite save RAM from external data (e.g. loaded from disk).
    /// Only copies up to the cartridge's allocated RAM size.
    pub fn load_save_ram(&mut self, data: &[u8]) {
        let n = data.len().min(self.ram.len());
        self.ram[..n].copy_from_slice(&data[..n]);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal ROM header with the given type/rom-size/ram-size bytes
    /// and pad it to the declared ROM size.
    fn make_rom(type_byte: u8, rom_sz: u8, ram_sz: u8) -> Vec<u8> {
        let banks: usize = if rom_sz <= 8 { 2usize << rom_sz } else { 2 };
        let mut rom = vec![0x00u8; banks * ROM_BANK_SIZE];
        rom[0x0147] = type_byte;
        rom[0x0148] = rom_sz;
        rom[0x0149] = ram_sz;
        rom
    }

    /// Tag ROM banks: set only the first byte of each bank to the bank number.
    /// This identifies which bank is mapped without clobbering the header
    /// (header bytes are at 0x0147–0x0149, not at bank-start offsets).
    fn tag_banks(rom: &mut Vec<u8>) {
        let banks = rom.len() / ROM_BANK_SIZE;
        for bank in 0..banks {
            rom[bank * ROM_BANK_SIZE] = bank as u8;
        }
    }

    // ── Cartridge::new header parsing ────────────────────────────────────────

    #[test]
    fn rom_only_type_produces_mbc_none() {
        let rom = make_rom(0x00, 0x00, 0x00);
        let cart = Cartridge::new(&rom);
        assert!(matches!(cart.mbc, Mbc::None));
        assert!(!cart.has_battery);
    }

    #[test]
    fn mbc1_type_detected() {
        let rom = make_rom(0x01, 0x04, 0x02); // MBC1, 512 KiB, 8 KiB RAM
        let cart = Cartridge::new(&rom);
        assert!(matches!(cart.mbc, Mbc::Mbc1(_)));
    }

    #[test]
    fn mbc1_battery_detected() {
        let rom = make_rom(0x03, 0x04, 0x03); // MBC1+RAM+BATTERY
        let cart = Cartridge::new(&rom);
        assert!(cart.has_battery);
    }

    #[test]
    fn mbc3_type_detected() {
        let rom = make_rom(0x13, 0x05, 0x03); // MBC3+RAM+BATTERY, 1 MiB, 32 KiB
        let cart = Cartridge::new(&rom);
        assert!(matches!(cart.mbc, Mbc::Mbc3(_)));
        assert!(cart.has_battery);
    }

    #[test]
    fn mbc5_type_detected() {
        let rom = make_rom(0x1B, 0x07, 0x03); // MBC5+RAM+BATTERY, 4 MiB, 32 KiB
        let cart = Cartridge::new(&rom);
        assert!(matches!(cart.mbc, Mbc::Mbc5(_)));
    }

    #[test]
    fn rom_size_decoded_correctly() {
        // rom_sz=0x04 → 2^5 = 32 banks × 16 KiB = 512 KiB
        let rom = make_rom(0x01, 0x04, 0x00);
        let cart = Cartridge::new(&rom);
        assert_eq!(cart.rom.len(), 32 * ROM_BANK_SIZE);
    }

    #[test]
    fn ram_size_decoded_correctly() {
        // ram_sz=0x03 → 4 banks × 8 KiB = 32 KiB
        let rom = make_rom(0x01, 0x01, 0x03);
        let cart = Cartridge::new(&rom);
        assert_eq!(cart.ram.len(), 4 * RAM_BANK_SIZE);
    }

    #[test]
    fn rom_data_copied_correctly() {
        let mut data = make_rom(0x00, 0x00, 0x00);
        data[0x0100] = 0xAB;
        data[0x7FFF] = 0xCD;
        let cart = Cartridge::new(&data);
        assert_eq!(cart.read(0x0100), 0xAB);
        assert_eq!(cart.read(0x7FFF), 0xCD);
    }

    // ── ROM-only cartridge ───────────────────────────────────────────────────

    #[test]
    fn rom_only_reads_full_32k() {
        let mut data = make_rom(0x00, 0x00, 0x00);
        data[0x0000] = 0x11;
        data[0x3FFF] = 0x22;
        data[0x4000] = 0x33;
        data[0x7FFF] = 0x44;
        let cart = Cartridge::new(&data);
        assert_eq!(cart.read(0x0000), 0x11);
        assert_eq!(cart.read(0x3FFF), 0x22);
        assert_eq!(cart.read(0x4000), 0x33);
        assert_eq!(cart.read(0x7FFF), 0x44);
    }

    #[test]
    fn rom_only_writes_to_rom_are_ignored() {
        let data = make_rom(0x00, 0x00, 0x00);
        let mut cart = Cartridge::new(&data);
        cart.write(0x0000, 0xFF); // should be silently ignored
        // Original ROM contents unchanged (all 0x00 from make_rom)
        assert_eq!(cart.read(0x0000), 0x00);
    }

    // ── MBC1 ─────────────────────────────────────────────────────────────────

    /// Helper: 512 KiB MBC1 cartridge (32 banks). The first byte of each bank
    /// is its bank number so reads from offset 0 identify which bank is mapped.
    fn mbc1_cart() -> Cartridge {
        let mut data = make_rom(0x01, 0x04, 0x03); // MBC1, 512 KiB, 32 KiB RAM
        tag_banks(&mut data);
        Cartridge::new(&data)
    }

    #[test]
    fn mbc1_default_upper_window_is_bank1() {
        let cart = mbc1_cart();
        // After reset, upper window shows bank 1.
        assert_eq!(cart.read(0x4000), 1);
    }

    #[test]
    fn mbc1_bank_switch_selects_correct_bank() {
        let mut cart = mbc1_cart();
        cart.write(0x2000, 5); // select bank 5
        assert_eq!(cart.read(0x4000), 5);
    }

    #[test]
    fn mbc1_zero_bank_aliased_to_one() {
        let mut cart = mbc1_cart();
        cart.write(0x2000, 0x00); // 0 → 1
        assert_eq!(cart.read(0x4000), 1);
    }

    #[test]
    fn mbc1_lower_window_fixed_in_mode0() {
        let mut cart = mbc1_cart();
        cart.write(0x2000, 5);
        cart.write(0x4000, 1); // bank_hi = 1 → would affect lower in mode1
        // mode 0: lower window still bank 0
        assert_eq!(cart.read(0x0000), 0);
    }

    #[test]
    fn mbc1_upper_window_uses_bank_hi() {
        let mut cart = mbc1_cart();
        // bank_hi=1, rom_bank_lo=1 → bank = (1<<5)|1 = 33 → wraps to 33 % 32 = 1
        // Use a cart with more banks to avoid wrapping.
        // rom_sz=0x06 = 128 banks = 2 MiB
        let mut data = make_rom(0x01, 0x06, 0x00);
        tag_banks(&mut data);
        let mut cart = Cartridge::new(&data);
        cart.write(0x4000, 1); // bank_hi = 1
        cart.write(0x2000, 1); // rom_bank_lo = 1
        // upper bank = (1<<5)|1 = 33
        assert_eq!(cart.read(0x4000), 33);
    }

    #[test]
    fn mbc1_mode1_lower_window_shifts() {
        // 2 MiB cart; set bank_hi=1 and enable mode1.
        let mut data = make_rom(0x01, 0x06, 0x00);
        tag_banks(&mut data);
        let mut cart = Cartridge::new(&data);
        cart.write(0x4000, 1);    // bank_hi = 1
        cart.write(0x6000, 0x01); // mode 1
        // lower window = (1<<5)|00000 = bank 32
        assert_eq!(cart.read(0x0000), 32);
    }

    #[test]
    fn mbc1_ram_disabled_returns_0xff() {
        let mut cart = mbc1_cart();
        cart.write(0xA000, 0x42); // write while RAM disabled
        assert_eq!(cart.read(0xA000), 0xFF);
    }

    #[test]
    fn mbc1_ram_enable_and_write() {
        let mut cart = mbc1_cart();
        cart.write(0x0000, 0x0A); // enable RAM
        cart.write(0xA000, 0x77);
        assert_eq!(cart.read(0xA000), 0x77);
    }

    #[test]
    fn mbc1_ram_disable_does_not_expose_data() {
        let mut cart = mbc1_cart();
        cart.write(0x0000, 0x0A); // enable
        cart.write(0xA000, 0x55);
        cart.write(0x0000, 0x00); // disable
        assert_eq!(cart.read(0xA000), 0xFF);
    }

    #[test]
    fn mbc1_mode1_ram_banking() {
        let mut cart = mbc1_cart();
        cart.write(0x0000, 0x0A); // enable RAM
        cart.write(0x6000, 0x01); // mode 1

        cart.write(0x4000, 0x00); // RAM bank 0
        cart.write(0xA000, 0xAA);

        cart.write(0x4000, 0x01); // RAM bank 1
        cart.write(0xA000, 0xBB);

        // Verify bank 0 unchanged after bank 1 write.
        cart.write(0x4000, 0x00);
        assert_eq!(cart.read(0xA000), 0xAA);
        cart.write(0x4000, 0x01);
        assert_eq!(cart.read(0xA000), 0xBB);
    }

    #[test]
    fn mbc1_only_lower_nibble_enables_ram() {
        let mut cart = mbc1_cart();
        cart.write(0x0000, 0xBA); // lower nibble = A → enable
        assert_eq!(cart.read(0xA000), 0x00); // RAM accessible (returns 0x00, initialized)
        // If it were disabled it would return 0xFF.
    }

    // ── MBC3 ─────────────────────────────────────────────────────────────────

    fn mbc3_cart() -> Cartridge {
        let mut data = make_rom(0x13, 0x05, 0x03); // MBC3+RAM+BATTERY, 1 MiB, 32 KiB
        tag_banks(&mut data);
        Cartridge::new(&data)
    }

    #[test]
    fn mbc3_default_upper_window_is_bank1() {
        let cart = mbc3_cart();
        assert_eq!(cart.read(0x4000), 1);
    }

    #[test]
    fn mbc3_zero_bank_aliased_to_one() {
        let mut cart = mbc3_cart();
        cart.write(0x2000, 0x00);
        assert_eq!(cart.read(0x4000), 1);
    }

    #[test]
    fn mbc3_all_banks_reachable() {
        // MBC3 has no 0x20/0x40/0x60 aliasing — write directly selects the bank.
        let mut data = make_rom(0x13, 0x06, 0x00); // 128 banks
        tag_banks(&mut data);
        let mut cart = Cartridge::new(&data);
        for bank in 1u8..=127 {
            cart.write(0x2000, bank);
            assert_eq!(cart.read(0x4000), bank, "bank {bank} unreachable");
        }
    }

    #[test]
    fn mbc3_lower_window_always_bank0() {
        let mut cart = mbc3_cart();
        cart.write(0x2000, 10);
        // 0x0000 always reads bank 0 on MBC3 (no mode bit).
        assert_eq!(cart.read(0x0000), 0);
    }

    #[test]
    fn mbc3_ram_banking() {
        let mut cart = mbc3_cart();
        cart.write(0x0000, 0x0A); // enable RAM+RTC

        cart.write(0x4000, 0x00); // RAM bank 0
        cart.write(0xA000, 0x11);
        cart.write(0x4000, 0x01); // RAM bank 1
        cart.write(0xA000, 0x22);
        cart.write(0x4000, 0x00);
        assert_eq!(cart.read(0xA000), 0x11);
        cart.write(0x4000, 0x01);
        assert_eq!(cart.read(0xA000), 0x22);
    }

    #[test]
    fn mbc3_rtc_latch_and_read() {
        let mut cart = mbc3_cart();
        cart.write(0x0000, 0x0A); // enable

        // Set live RTC seconds to 42.
        cart.write(0x4000, 0x08); // select RTC seconds register
        cart.write(0xA000, 42);   // write live value

        // Before latch: latched value was set on write, so it matches live.
        assert_eq!(cart.read(0xA000), 42);

        // Manually update live value (simulating time passing) by writing again.
        cart.write(0xA000, 55);
        // Latched copy also updates on direct write.
        assert_eq!(cart.read(0xA000), 55);

        // Now test latch sequence: changes to live registers after latch
        // are not visible until relatch.
        // (Full test would require ticking the RTC, but we verify the
        // latch sequence fires without panicking.)
        cart.write(0x6000, 0x00); // arm latch
        cart.write(0x6000, 0x01); // fire latch
        assert_eq!(cart.read(0xA000), 55); // latched value now 55
    }

    #[test]
    fn mbc3_rtc_partial_latch_sequence_does_not_fire() {
        let mut cart = mbc3_cart();
        cart.write(0x0000, 0x0A);
        cart.write(0x4000, 0x08);
        cart.write(0xA000, 10); // live + latched = 10

        // Write 0x01 without first writing 0x00 — no latch.
        cart.write(0x6000, 0x01);
        // Value should still be 10 (no corruption).
        assert_eq!(cart.read(0xA000), 10);
    }

    #[test]
    fn mbc3_rtc_disabled_returns_0xff() {
        let mut cart = mbc3_cart();
        // RAM/RTC disabled (default)
        cart.write(0x4000, 0x08); // point at RTC seconds
        assert_eq!(cart.read(0xA000), 0xFF);
    }

    #[test]
    fn mbc3_rtc_all_registers_readable() {
        let mut cart = mbc3_cart();
        cart.write(0x0000, 0x0A);
        for reg in [0x08u8, 0x09, 0x0A, 0x0B, 0x0C] {
            cart.write(0x4000, reg);
            // Just verify no panic and returns a value (not 0xFF with RAM enabled).
            let _ = cart.read(0xA000);
        }
    }

    // ── MBC5 ─────────────────────────────────────────────────────────────────

    fn mbc5_cart() -> Cartridge {
        // MBC5+RAM+BATTERY, 4 MiB (256 banks), 32 KiB RAM
        let mut data = make_rom(0x1B, 0x07, 0x03);
        tag_banks(&mut data);
        Cartridge::new(&data)
    }

    #[test]
    fn mbc5_default_bank_is_zero() {
        let cart = mbc5_cart();
        // MBC5 resets to bank 0 (unlike MBC1/MBC3 which start at 1).
        assert_eq!(cart.read(0x4000), 0);
    }

    #[test]
    fn mbc5_bank0_directly_selectable() {
        let mut cart = mbc5_cart();
        cart.write(0x2000, 5);   // select bank 5
        cart.write(0x2000, 0);   // back to 0 — NO aliasing on MBC5
        assert_eq!(cart.read(0x4000), 0);
    }

    #[test]
    fn mbc5_lower_window_always_bank0() {
        let mut cart = mbc5_cart();
        cart.write(0x2000, 10);
        assert_eq!(cart.read(0x0000), 0);
    }

    #[test]
    fn mbc5_bank_switch_selects_correct_bank() {
        let mut cart = mbc5_cart();
        cart.write(0x2000, 42);
        assert_eq!(cart.read(0x4000), 42);
    }

    #[test]
    fn mbc5_high_bit_extends_to_9bit_bank() {
        // Need a cart large enough — 8 MiB = 512 banks (rom_sz=0x08)
        let mut data = make_rom(0x1B, 0x08, 0x00);
        for bank in 0..512usize {
            let base = bank * ROM_BANK_SIZE;
            data[base] = (bank & 0xFF) as u8;
            data[base + 1] = ((bank >> 8) & 0x01) as u8;
        }
        let mut cart = Cartridge::new(&data);
        // Select bank 0x100 = 256: low byte = 0x00, high bit = 1
        cart.write(0x2000, 0x00);
        cart.write(0x3000, 0x01);
        assert_eq!(cart.read(0x4000), 0x00); // low byte of bank 256
        assert_eq!(cart.read(0x4001), 0x01); // high bit
    }

    #[test]
    fn mbc5_ram_enable_disable() {
        let mut cart = mbc5_cart();
        cart.write(0x0000, 0x0A); // enable
        cart.write(0xA000, 0x99);
        assert_eq!(cart.read(0xA000), 0x99);
        cart.write(0x0000, 0x00); // disable
        assert_eq!(cart.read(0xA000), 0xFF);
    }

    #[test]
    fn mbc5_ram_bank_select() {
        let mut cart = mbc5_cart();
        cart.write(0x0000, 0x0A);
        cart.write(0x4000, 0x00); // RAM bank 0
        cart.write(0xA000, 0x11);
        cart.write(0x4000, 0x03); // RAM bank 3
        cart.write(0xA000, 0x33);
        cart.write(0x4000, 0x00);
        assert_eq!(cart.read(0xA000), 0x11);
        cart.write(0x4000, 0x03);
        assert_eq!(cart.read(0xA000), 0x33);
    }

    // ── save RAM ─────────────────────────────────────────────────────────────

    #[test]
    fn save_ram_round_trip() {
        let mut cart = mbc1_cart();
        cart.write(0x0000, 0x0A);
        cart.write(0xA000, 0xDE);
        cart.write(0xA001, 0xAD);

        let saved = cart.save_ram().to_vec();

        let mut cart2 = mbc1_cart();
        cart2.load_save_ram(&saved);
        cart2.write(0x0000, 0x0A);
        assert_eq!(cart2.read(0xA000), 0xDE);
        assert_eq!(cart2.read(0xA001), 0xAD);
    }
}

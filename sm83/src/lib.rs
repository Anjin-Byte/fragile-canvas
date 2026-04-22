pub mod apu;
pub mod clock;
pub mod cpu;
pub mod joypad;
pub mod memory;
pub mod ppu;
pub mod serial;
pub mod session;
pub mod system;
pub mod timer;
pub mod trace;

pub const BOOT_ROM: &[u8; 256] = include_bytes!("../roms/bootrom.bin");
pub const DEFAULT_ROM: &[u8] = include_bytes!("../roms/tobudx.gb");

/// A ROM bundled into the binary at compile time.
pub struct BundledRom {
    pub id: &'static str,
    pub title: &'static str,
    pub author: &'static str,
    pub data: &'static [u8],
}

pub const BUNDLED_ROMS: &[BundledRom] = &[
    BundledRom {
        id: "tobu-tobu-girl-dx",
        title: "Tobu Tobu Girl DX",
        author: "Tangram Games",
        data: include_bytes!("../roms/tobudx.gb"),
    },
    BundledRom {
        id: "cryohazard",
        title: "Cryohazard",
        author: "Incube8 Games",
        data: include_bytes!("../roms/Cryohazard v1_0.gb"),
    },
];

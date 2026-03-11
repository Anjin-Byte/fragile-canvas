pub mod apu;
pub mod clock;
pub mod cpu;
pub mod memory;
pub mod session;
pub mod system;
pub mod timer;
pub mod trace;

pub const BOOT_ROM: &[u8; 256] = include_bytes!("../roms/bootrom.bin");
pub const DEFAULT_ROM: &[u8] = include_bytes!("../roms/tobudx.gb");

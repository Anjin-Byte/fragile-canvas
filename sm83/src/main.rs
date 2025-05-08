use cpu::CPU;
use memory::mmu::MMU;
use std::{cell::RefCell, fs::File, io::Read, path::Path, rc::Rc};

pub mod cpu;
pub mod memory;
pub mod utils;

fn main() {
    let mut mmu = MMU::new();

    let path = Path::new("../ROMs/DMG_ROM.bin");
    let mut file = match File::open(&path) {
        Err(why) => panic!("couldn't open {}: {}", path.display(), why),
        Ok(file) => file,
    };

    let mut buffer = [0; 1];
    for address in 0x0000..=0x3FFF {
        if let Ok(bytes_read) = file.read(&mut buffer) {
            if bytes_read == 0 {
                break;
            }
            mmu.load_rom(address, buffer[0]);
        }
    }

    let shared_bus = Rc::new(RefCell::new(mmu));
    let mut cpu = CPU::new(shared_bus);
    loop { cpu.tick(); }
}

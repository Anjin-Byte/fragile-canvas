use cpu::CPU;
use memory::mmu::MMU;
use std::{env, fs, path::Path, process};
use trace::Tracer;

pub mod cpu;
pub mod memory;
pub mod trace;

fn load_file(path: &Path) -> Vec<u8> {
    fs::read(path).unwrap_or_else(|e| {
        eprintln!("couldn't open {}: {}", path.display(), e);
        process::exit(1);
    })
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    let debug = args.iter().any(|a| a == "--debug" || a == "-d");
    let cart_path = args.iter().find(|a| !a.starts_with('-')).unwrap_or_else(|| {
        eprintln!("usage: fragile-canvas [--debug] <rom.gb>");
        process::exit(1);
    });

    let tracer = if debug {
        let file = fs::File::create("fragile-canvas.log").unwrap_or_else(|e| {
            eprintln!("couldn't create log file: {}", e);
            process::exit(1);
        });
        Tracer::to_file(file)
    } else {
        Tracer::off()
    };

    let mut mmu = MMU::new();

    let boot_rom = load_file(Path::new("../ROMs/DMG_ROM.bin"));
    mmu.load_boot_rom(&boot_rom);

    let cartridge = load_file(Path::new(cart_path));
    mmu.load_cartridge(&cartridge);

    let mut cpu = CPU::new(mmu, tracer);
    loop { cpu.tick(); }
}

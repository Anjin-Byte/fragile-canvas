use serde::Deserialize;
use sm83::cpu::CPU;
use sm83::memory::mmu::MMU;
use sm83::trace::Tracer;
use std::{env, fs, path::Path, process};
use std::time::SystemTime;

#[derive(Deserialize)]
struct Config {
    boot_rom: String,
    cart_rom: String,
}

fn load_config() -> Config {
    let data = fs::read_to_string("config.yaml").unwrap_or_else(|e| {
        eprintln!("couldn't read config.yaml: {}", e);
        process::exit(1);
    });
    serde_yaml::from_str(&data).unwrap_or_else(|e| {
        eprintln!("invalid config.yaml: {}", e);
        process::exit(1);
    })
}

fn load_file(path: &Path) -> Vec<u8> {
    fs::read(path).unwrap_or_else(|e| {
        eprintln!("couldn't open {}: {}", path.display(), e);
        process::exit(1);
    })
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let debug = args.iter().any(|a| a == "--debug" || a == "-d");
    let cart_override = args.iter().find(|a| !a.starts_with('-'));

    let config = load_config();
    let cart_path = cart_override.map(|s| s.as_str()).unwrap_or(&config.cart_rom);

    let tracer = if debug {
        fs::create_dir_all("logs").unwrap_or_else(|e| {
            eprintln!("couldn't create logs directory: {}", e);
            process::exit(1);
        });
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let log_path = format!("logs/{timestamp}.log");
        eprintln!("trace log: {log_path}");
        let file = fs::File::create(&log_path).unwrap_or_else(|e| {
            eprintln!("couldn't create {}: {}", log_path, e);
            process::exit(1);
        });
        Tracer::to_file(file)
    } else {
        Tracer::off()
    };

    let mut mmu = MMU::new();

    let boot_rom = load_file(Path::new(&config.boot_rom));
    mmu.load_boot_rom(&boot_rom).unwrap_or_else(|e| {
        eprintln!("{}", e);
        process::exit(1);
    });

    let cartridge = load_file(Path::new(cart_path));
    mmu.load_cartridge(&cartridge);

    let mut cpu = CPU::new(mmu, tracer);
    loop { cpu.tick(); }
}

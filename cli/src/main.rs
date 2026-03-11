use serde::Deserialize;
use sm83::clock::ClockGovernor;
use sm83::memory::bus::Bus;
use sm83::system::GameBoy;
use sm83::trace::Tracer;
use std::{env, fs, path::Path, process, thread};
use std::time::{Duration, Instant, SystemTime};

#[derive(Deserialize)]
struct Config {
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
    let uncapped = args.iter().any(|a| a == "--uncapped" || a == "-u");
    let bench = args.iter().any(|a| a == "--bench" || a == "-b");
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

    let mut bus = Bus::new();
    bus.load_boot_rom(sm83::BOOT_ROM).unwrap();

    let cartridge = load_file(Path::new(cart_path));
    bus.load_cartridge(&cartridge);

    let mut gb = GameBoy::new(bus, tracer);

    if bench {
        // Benchmark: run governed for 5 seconds, report accuracy.
        eprintln!("benchmarking governed loop for 5 seconds...");
        let target_hz = sm83::timer::MASTER_CLOCK_HZ as f64;
        let bench_duration = Duration::from_secs(5);
        let mut gov = ClockGovernor::new();
        let bench_start = Instant::now();
        let mut last = bench_start;
        let mut report_at = Duration::from_secs(1);
        let mut total_executed: u64 = 0;
        let mut loop_count: u64 = 0;
        let mut sleep_count: u64 = 0;

        loop {
            let now = Instant::now();
            let wall_elapsed = now.duration_since(bench_start);
            if wall_elapsed >= bench_duration {
                break;
            }

            let dt = now.duration_since(last);
            last = now;

            let cycles = gov.cycles_due(dt.as_nanos() as u64);
            if cycles > 0 {
                gb.tick_t(cycles);
                total_executed += cycles as u64;
            } else {
                thread::sleep(Duration::from_micros(100));
                sleep_count += 1;
            }
            loop_count += 1;

            // Report every second.
            if wall_elapsed >= report_at {
                let secs = wall_elapsed.as_secs_f64();
                let actual_hz = total_executed as f64 / secs;
                let error_ppm = ((actual_hz - target_hz) / target_hz) * 1_000_000.0;
                let avg_batch = if loop_count - sleep_count > 0 {
                    total_executed / (loop_count - sleep_count)
                } else { 0 };
                eprintln!(
                    "  {:.1}s: {:.0} Hz (target {:.0}) | error: {:.1} ppm | \
                     loops: {} (sleep: {:.1}%) | avg batch: {} T-cycles",
                    secs, actual_hz, target_hz, error_ppm,
                    loop_count,
                    sleep_count as f64 / loop_count as f64 * 100.0,
                    avg_batch,
                );
                report_at += Duration::from_secs(1);
            }
        }

        let total_secs = bench_start.elapsed().as_secs_f64();
        let actual_hz = total_executed as f64 / total_secs;
        let error_ppm = ((actual_hz - target_hz) / target_hz) * 1_000_000.0;
        let expected = (target_hz * total_secs) as u64;
        eprintln!("\n  final: {} T-cycles in {:.3}s", total_executed, total_secs);
        eprintln!("  actual: {:.2} Hz", actual_hz);
        eprintln!("  target: {:.0} Hz (2^22)", target_hz);
        eprintln!("  error:  {:.1} ppm ({:+} T-cycles vs expected {})",
            error_ppm, total_executed as i64 - expected as i64, expected);
    } else if uncapped {
        eprintln!("running uncapped (as fast as possible)");
        loop { gb.tick(); }
    } else {
        eprintln!("running at 2^22 Hz (4,194,304 T-cycles/s)");
        let mut gov = ClockGovernor::new();
        let mut last = Instant::now();
        loop {
            let now = Instant::now();
            let elapsed = now.duration_since(last);
            last = now;

            let cycles = gov.cycles_due(elapsed.as_nanos() as u64);
            if cycles > 0 {
                gb.tick_t(cycles);
            } else {
                thread::sleep(Duration::from_micros(100));
            }
        }
    }
}

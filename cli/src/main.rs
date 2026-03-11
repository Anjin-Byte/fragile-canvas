use serde::Deserialize;
use sm83::clock::ClockGovernor;
use sm83::memory::bus::Bus;
use sm83::system::GameBoy;
use sm83::trace::Tracer;
use std::sync::{Arc, Mutex};
use std::{env, fs, path::Path, process, thread};
use std::time::{Duration, Instant, SystemTime};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

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

/// Simple thread-safe ring buffer for passing audio between emulation and cpal.
struct AudioRing {
    buf: Vec<f32>,
    read_pos: usize,
    write_pos: usize,
    count: usize,
}

impl AudioRing {
    fn new(capacity: usize) -> Self {
        Self {
            buf: vec![0.0; capacity],
            read_pos: 0,
            write_pos: 0,
            count: 0,
        }
    }

    fn push(&mut self, samples: &[f32]) {
        let cap = self.buf.len();
        for &s in samples {
            if self.count < cap {
                self.buf[self.write_pos] = s;
                self.write_pos = (self.write_pos + 1) % cap;
                self.count += 1;
            }
        }
    }

    fn pop(&mut self) -> Option<f32> {
        if self.count == 0 {
            return None;
        }
        let val = self.buf[self.read_pos];
        self.read_pos = (self.read_pos + 1) % self.buf.len();
        self.count -= 1;
        Some(val)
    }
}

fn start_audio() -> Arc<Mutex<AudioRing>> {
    let ring = Arc::new(Mutex::new(AudioRing::new(16384)));
    let ring_clone = ring.clone();

    let host = cpal::default_host();
    let device = host.default_output_device().unwrap_or_else(|| {
        eprintln!("no audio output device found");
        process::exit(1);
    });

    let config = cpal::StreamConfig {
        channels: 2,
        sample_rate: cpal::SampleRate(48000),
        buffer_size: cpal::BufferSize::Default,
    };

    let stream = device.build_output_stream(
        &config,
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            let mut ring = ring_clone.lock().unwrap();
            for sample in data.iter_mut() {
                *sample = ring.pop().unwrap_or(0.0);
            }
        },
        |err| eprintln!("audio stream error: {}", err),
        None,
    ).unwrap_or_else(|e| {
        eprintln!("couldn't build audio stream: {}", e);
        process::exit(1);
    });

    stream.play().unwrap_or_else(|e| {
        eprintln!("couldn't start audio stream: {}", e);
        process::exit(1);
    });

    // Leak the stream so it lives for the program's lifetime
    std::mem::forget(stream);

    ring
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let debug = args.iter().any(|a| a == "--debug" || a == "-d");
    let uncapped = args.iter().any(|a| a == "--uncapped" || a == "-u");
    let bench = args.iter().any(|a| a == "--bench" || a == "-b");
    let mute = args.iter().any(|a| a == "--mute" || a == "-m");
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

    let audio_ring = if !mute && !bench && !uncapped {
        Some(start_audio())
    } else {
        None
    };

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
        if audio_ring.is_some() {
            eprintln!("audio output: 48 kHz stereo (use --mute to disable)");
        }
        let mut gov = ClockGovernor::new();
        let mut last = Instant::now();
        let mut audio_samples = Vec::new();
        loop {
            let now = Instant::now();
            let elapsed = now.duration_since(last);
            last = now;

            let cycles = gov.cycles_due(elapsed.as_nanos() as u64);
            if cycles > 0 {
                gb.tick_t(cycles);

                // Drain audio samples from the APU and push to the audio ring
                if let Some(ref ring) = audio_ring {
                    gb.bus.apu.drain_audio_samples(&mut audio_samples);
                    if !audio_samples.is_empty() {
                        let mut ring = ring.lock().unwrap();
                        ring.push(&audio_samples);
                        audio_samples.clear();
                    }
                }
            } else {
                thread::sleep(Duration::from_micros(100));
            }
        }
    }
}

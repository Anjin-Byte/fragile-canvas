# SM83 Integration Tests

Hardware-accuracy tests using [Blargg's Game Boy test ROMs](https://github.com/c-sp/game-boy-test-roms).

## Quick Start

```bash
# Run the 11 cpu_instrs tests (default, always run)
cargo test -p sm83 --test blargg

# Run all tests including ignored suites
cargo test -p sm83 --test blargg -- --include-ignored

# Run a specific test
cargo test -p sm83 --test blargg cpu_instrs_01_special
```

## Suite Reports

Reports run every ROM in a suite and print a results table without panicking.
Use `--nocapture` to see the output.

```bash
# Individual suite reports
cargo test -p sm83 --test blargg report_cpu_instrs -- --ignored --nocapture
cargo test -p sm83 --test blargg report_timing    -- --ignored --nocapture
cargo test -p sm83 --test blargg report_dmg_sound -- --ignored --nocapture
cargo test -p sm83 --test blargg report_oam_bug   -- --ignored --nocapture

# All suites at once
cargo test -p sm83 --test blargg report_all -- --ignored --nocapture
```

Example output:
```
╔════════════════════════════════════════════════════╗
║  timing                                            ║
╠════════════════════════════════════════════════════╣
║  [PASS]  instr_timing                              ║
║  [PASS]  mem_timing 01-read                        ║
║  [FAIL]  halt_bug                                  ║
║         IE IF IF DE                                ║
║         01 10 F1 0C04                              ║
╠════════════════════════════════════════════════════╣
║  Result: 4/9 passed                                ║
╚════════════════════════════════════════════════════╝
```

Failure detail comes from two sources:
- **Serial output** (v1 shell) — test ROMs write ASCII results to the serial port
- **Cart RAM at 0xA000** (v2 shell) — some ROMs write results to cartridge RAM
  with a signature (`DE B0 61`) at 0xA001-0xA003

## Current Results

| Suite | Passing | Notes |
|-------|---------|-------|
| cpu_instrs (01-11) | **11/11** | All CPU instructions correct |
| instr_timing | **1/1** | Instruction cycle counts correct |
| mem_timing (01-03) | **3/3** | Memory access timing (M-cycle accurate) |
| mem_timing2 (01-03) | **3/3** | Memory timing v2 (detected via cart RAM) |
| halt_bug | 0/1 | IF register persistence issue |
| interrupt_time | 0/1 | Interrupt dispatch timing |
| dmg_sound (01-12) | 2/12 | APU edge cases (sweep, trigger, wave) |
| cgb_sound (01-12) | 0/12 | CGB-only (not applicable to DMG core) |
| oam_bug (1-8) | 0/8 | OAM corruption (not implemented) |

## Debug Tools

The test file includes a `DebugRunner` for investigating ROM hangs and failures.

```bash
# Run debug tests (modify the test body for your investigation)
cargo test -p sm83 --test blargg debug_halt_bug -- --ignored --nocapture
```

### DebugRunner Features

- **Boot ROM skip** — `load_rom_no_boot()` starts at PC=0x0100 with DMG post-boot state
- **PC ring buffer** — last 16 PCs with automatic loop detection
- **Instruction trace** — `run_traced()` captures a ring buffer of `InstrTrace` records
  showing PC, opcode, registers before/after, IF/IE/IME, halt state per instruction
- **Bus access tap** — `session.watch_bus(0xFF0F)` logs every read/write to an address
  with the source (cpu/ppu/timer/serial/irq_dispatch) and originating PC
- **I/O snapshot** — `session.io_snapshot()` captures all I/O registers in one call
- **Programmatic breakpoints** — `run_until()` and `run_traced()` accept stop predicates

### Example: Trace a Hang

```rust
#[test]
#[ignore]
fn debug_my_rom() {
    let mut r = DebugRunner::new("halt_bug.gb", true);
    r.session.watch_bus(0xFF0F); // tap IF register

    let reason = r.run_traced(1_000_000, |t| t.pc == 0xC818);
    println!("{}", r.dump_state());
    r.dump_trace(Some(32)); // last 32 instructions

    let log = r.session.drain_bus_log();
    for entry in &log {
        println!("{:?} val={:02X} pc={:04X} src={}",
            entry.kind, entry.value, entry.pc, entry.source);
    }
}
```

## ROM Location

Test ROMs are at `assets/ROMs/gb-test-roms/`. The harness resolves paths
relative to `CARGO_MANIFEST_DIR/../assets/ROMs/gb-test-roms/`.

## Architecture

The test harness checks both output channels:

1. **Serial port** — `session.serial_output_as_string()` checks for "Passed"/"Failed"
2. **Cart RAM** — `check_cart_ram()` reads 0xA000 for the v2 shell signature

All tests use `load_rom_no_boot()` to skip the ~126-frame boot ROM animation,
reducing test runtime from ~10s to ~6.5s for the cpu_instrs suite.

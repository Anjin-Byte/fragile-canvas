/// Integration tests using Blargg's Game Boy hardware test ROMs.
///
/// Each test loads a ROM, runs it until the serial output contains
/// "Passed" or "Failed" (or a cycle budget is exhausted), and asserts
/// that the test passed.
///
/// The cpu_instrs individual tests run by default — they validate every
/// CPU instruction and are the first correctness milestone.  All other
/// suites are `#[ignore]`d until the subsystems they test are accurate
/// enough to pass.
///
/// Run with:
///   cargo test -p sm83 --test blargg              # cpu_instrs only
///   cargo test -p sm83 --test blargg -- --ignored  # everything else
use std::collections::BTreeSet;
use std::io::Write as IoWrite;
use std::path::PathBuf;

use sm83::session::{CpuSnapshot, InstrTrace, Session};

// ── Helpers ──────────────────────────────────────────────────────────────

fn rom_base() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("assets")
        .join("ROMs")
        .join("gb-test-roms")
}

enum BlarggResult {
    Passed,
    Failed(String),
    Timeout(String),
}

impl BlarggResult {
    fn tag(&self) -> &'static str {
        match self {
            BlarggResult::Passed => "PASS",
            BlarggResult::Failed(_) => "FAIL",
            BlarggResult::Timeout(_) => "TIME",
        }
    }

    fn is_pass(&self) -> bool {
        matches!(self, BlarggResult::Passed)
    }
}

/// A named ROM entry for suite-level reporting.
struct RomEntry {
    name: &'static str,
    path: &'static str,
}

/// Structured result for a single ROM test.
struct TestResult {
    name: String,
    status: &'static str, // "PASS", "FAIL", "TIME"
    /// Full diagnostic output (serial or cart RAM).
    detail: String,
}

/// Structured results for a suite.
struct SuiteReport {
    name: String,
    results: Vec<TestResult>,
    passed: usize,
    total: usize,
}

/// Run a suite and return structured results (also prints to terminal).
fn run_suite_report(suite_name: &str, roms: &[RomEntry]) -> SuiteReport {
    const W: usize = 52;

    println!();
    println!("╔{:═<W$}╗", "");
    println!("║  {:<w$}║", suite_name, w = W - 2);
    println!("╠{:═<W$}╣", "");

    let mut results = Vec::new();
    let mut passed = 0;

    for rom in roms {
        let result = run_blargg_test(rom.path);
        let tag = result.tag();
        if result.is_pass() {
            passed += 1;
        }

        let detail = match &result {
            BlarggResult::Passed => String::new(),
            BlarggResult::Failed(out) => out.clone(),
            BlarggResult::Timeout(out) => {
                if out.is_empty() {
                    "(no output)".to_string()
                } else {
                    out.clone()
                }
            }
        };

        // Terminal output
        let line = format!("  [{tag}]  {}", rom.name);
        println!("║{line:<W$}║");
        match &result {
            BlarggResult::Passed => {}
            BlarggResult::Failed(out) => {
                for dl in out.lines().filter(|l| !l.is_empty()).skip(1) {
                    let dl = format!("         {dl}");
                    println!("║{dl:<W$}║");
                }
            }
            BlarggResult::Timeout(out) => {
                let msg = if out.is_empty() {
                    "(no serial output)".to_string()
                } else {
                    out.lines().filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" | ")
                };
                let dl = format!("         {msg}");
                println!("║{dl:<W$}║");
            }
        }

        results.push(TestResult {
            name: rom.name.to_string(),
            status: tag,
            detail,
        });
    }

    let summary = format!("  Result: {}/{} passed", passed, roms.len());
    println!("╠{:═<W$}╣", "");
    println!("║{summary:<W$}║");
    println!("╚{:═<W$}╝", "");
    println!();

    SuiteReport {
        name: suite_name.to_string(),
        results,
        passed,
        total: roms.len(),
    }
}

/// Write a markdown report file from structured suite results.
fn write_markdown_report(suites: &[SuiteReport]) {
    let report_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("assets")
        .join("docs");
    let report_path = report_dir.join("blargg_test_results.md");

    let mut total_pass = 0;
    let mut total_count = 0;
    for s in suites {
        total_pass += s.passed;
        total_count += s.total;
    }

    let mut f = std::fs::File::create(&report_path)
        .unwrap_or_else(|e| panic!("cannot create {}: {}", report_path.display(), e));

    writeln!(f, "# Blargg Test Results").unwrap();
    writeln!(f).unwrap();
    writeln!(f, "**{total_pass}/{total_count} passing**").unwrap();
    writeln!(f).unwrap();

    // Summary table
    writeln!(f, "## Summary").unwrap();
    writeln!(f).unwrap();
    writeln!(f, "| Suite | Result |").unwrap();
    writeln!(f, "|-------|--------|").unwrap();
    for s in suites {
        let icon = if s.passed == s.total { "pass" } else if s.passed > 0 { "partial" } else { "fail" };
        writeln!(f, "| {} | {}/{} ({icon}) |", s.name, s.passed, s.total).unwrap();
    }
    writeln!(f).unwrap();

    // Detailed results per suite
    for s in suites {
        writeln!(f, "## {}", s.name).unwrap();
        writeln!(f).unwrap();
        writeln!(f, "| Test | Status | Detail |").unwrap();
        writeln!(f, "|------|--------|--------|").unwrap();

        for r in &s.results {
            let status_icon = match r.status {
                "PASS" => "PASS",
                "FAIL" => "FAIL",
                "TIME" => "TIMEOUT",
                _ => r.status,
            };

            // Extract the failure message: skip the ROM name echo, take
            // the first meaningful line as a short summary.
            let short_detail = if r.detail.is_empty() {
                String::new()
            } else {
                let lines: Vec<&str> = r.detail.lines()
                    .filter(|l| !l.is_empty())
                    .collect();
                // Skip first line (ROM name) if there are more lines
                if lines.len() > 1 {
                    lines[1..].iter()
                        .take(2)
                        .copied()
                        .collect::<Vec<_>>()
                        .join("; ")
                } else if !lines.is_empty() {
                    lines[0].to_string()
                } else {
                    String::new()
                }
            };

            writeln!(f, "| {} | {} | {} |", r.name, status_icon, short_detail).unwrap();
        }
        writeln!(f).unwrap();

        // For failing tests with longer output, add a details section
        let failures: Vec<&TestResult> = s.results.iter()
            .filter(|r| r.status != "PASS" && !r.detail.is_empty() && r.detail != "(no output)")
            .collect();

        if !failures.is_empty() {
            writeln!(f, "<details>").unwrap();
            writeln!(f, "<summary>Failure details</summary>").unwrap();
            writeln!(f).unwrap();

            for r in failures {
                writeln!(f, "### {}", r.name).unwrap();
                writeln!(f).unwrap();
                writeln!(f, "```").unwrap();
                for line in r.detail.lines() {
                    writeln!(f, "{line}").unwrap();
                }
                writeln!(f, "```").unwrap();
                writeln!(f).unwrap();
            }

            writeln!(f, "</details>").unwrap();
            writeln!(f).unwrap();
        }
    }

    println!("Report written to {}", report_path.display());
}

fn load_rom_data(rom_relative_path: &str) -> Vec<u8> {
    let rom_path = rom_base().join(rom_relative_path);
    std::fs::read(&rom_path)
        .unwrap_or_else(|e| panic!("cannot read ROM {}: {}", rom_path.display(), e))
}

/// Check cart RAM at 0xA000 for v2 test shell results.
/// Returns Some(text) if the v2 signature (DE B0 61) is found.
fn check_cart_ram(session: &Session) -> Option<(u8, String)> {
    let ram = session.read_memory(0xA000, 128).ok()?;
    if ram.len() >= 4 && ram[1] == 0xDE && ram[2] == 0xB0 && ram[3] == 0x61 {
        let code = ram[0];
        let text_end = ram[4..].iter().position(|&b| b == 0).unwrap_or(124);
        let text = String::from_utf8_lossy(&ram[4..4 + text_end]).into_owned();
        Some((code, text))
    } else {
        None
    }
}

/// Load a ROM from `rom_relative_path` (relative to the test-ROM root),
/// run it for up to ~30 s of emulated time, and return the outcome.
/// Checks both serial output (v1 shell) and cart RAM (v2 shell).
/// Skips the boot ROM for faster execution.
fn run_blargg_test(rom_relative_path: &str) -> BlarggResult {
    let rom_data = load_rom_data(rom_relative_path);

    let mut session = Session::new();
    session.load_rom_no_boot(&rom_data);

    // Budget: ~30 s emulated time.
    // 4_194_304 T-cycles/s × 30 s = 125_829_120 T-cycles
    // ÷ 4 = 31_457_280 M-cycles.
    const CHUNK: u32 = 4096;
    const BUDGET: u64 = 31_457_280;
    let mut executed: u64 = 0;

    while executed < BUDGET {
        session.step(CHUNK).unwrap();
        executed += CHUNK as u64;

        // Check serial output (v1 shell)
        let output = session.serial_output_as_string();
        if output.contains("Passed") {
            return BlarggResult::Passed;
        }
        if output.contains("Failed") {
            return BlarggResult::Failed(output);
        }

        // Check cart RAM (v2 shell) — only after enough execution time
        // for the ROM to have actually run (skip first ~1M M-cycles)
        if executed > 1_000_000 {
            if let Some((code, text)) = check_cart_ram(&session) {
                if !text.is_empty() {
                    if code == 0x00 && text.contains("Passed") {
                        return BlarggResult::Passed;
                    }
                    if text.contains("Failed") || code != 0x00 {
                        return BlarggResult::Failed(text);
                    }
                }
            }
        }
    }

    // On timeout, check both sources for partial output
    let serial = session.serial_output_as_string();
    if !serial.is_empty() {
        return BlarggResult::Timeout(serial);
    }
    if let Some((_code, text)) = check_cart_ram(&session) {
        if !text.is_empty() {
            return BlarggResult::Timeout(text);
        }
    }
    BlarggResult::Timeout(String::new())
}

fn assert_blargg_passes(rom_relative_path: &str) {
    match run_blargg_test(rom_relative_path) {
        BlarggResult::Passed => {}
        BlarggResult::Failed(output) => {
            panic!(
                "Blargg test FAILED.\nROM: {}\nSerial output:\n{}",
                rom_relative_path, output
            );
        }
        BlarggResult::Timeout(output) => {
            panic!(
                "Blargg test TIMED OUT (~30 s emulated, no Passed/Failed detected).\n\
                 ROM: {}\nSerial output so far:\n{}",
                rom_relative_path, output
            );
        }
    }
}

// ── cpu_instrs (individual) ─────────────────────────────────────────────

#[test]
fn cpu_instrs_01_special() {
    assert_blargg_passes("cpu_instrs/individual/01-special.gb");
}

#[test]
fn cpu_instrs_02_interrupts() {
    assert_blargg_passes("cpu_instrs/individual/02-interrupts.gb");
}

#[test]
fn cpu_instrs_03_op_sp_hl() {
    assert_blargg_passes("cpu_instrs/individual/03-op sp,hl.gb");
}

#[test]
fn cpu_instrs_04_op_r_imm() {
    assert_blargg_passes("cpu_instrs/individual/04-op r,imm.gb");
}

#[test]
fn cpu_instrs_05_op_rp() {
    assert_blargg_passes("cpu_instrs/individual/05-op rp.gb");
}

#[test]
fn cpu_instrs_06_ld_r_r() {
    assert_blargg_passes("cpu_instrs/individual/06-ld r,r.gb");
}

#[test]
fn cpu_instrs_07_jr_jp_call_ret_rst() {
    assert_blargg_passes("cpu_instrs/individual/07-jr,jp,call,ret,rst.gb");
}

#[test]
fn cpu_instrs_08_misc_instrs() {
    assert_blargg_passes("cpu_instrs/individual/08-misc instrs.gb");
}

#[test]
fn cpu_instrs_09_op_r_r() {
    assert_blargg_passes("cpu_instrs/individual/09-op r,r.gb");
}

#[test]
fn cpu_instrs_10_bit_ops() {
    assert_blargg_passes("cpu_instrs/individual/10-bit ops.gb");
}

#[test]
fn cpu_instrs_11_op_a_hl() {
    assert_blargg_passes("cpu_instrs/individual/11-op a,(hl).gb");
}

// ── instr_timing ────────────────────────────────────────────────────────

#[test]
#[ignore]
fn instr_timing() {
    assert_blargg_passes("instr_timing/instr_timing.gb");
}

// ── mem_timing (individual) ─────────────────────────────────────────────

#[test]
#[ignore]
fn mem_timing_01_read_timing() {
    assert_blargg_passes("mem_timing/individual/01-read_timing.gb");
}

#[test]
#[ignore]
fn mem_timing_02_write_timing() {
    assert_blargg_passes("mem_timing/individual/02-write_timing.gb");
}

#[test]
#[ignore]
fn mem_timing_03_modify_timing() {
    assert_blargg_passes("mem_timing/individual/03-modify_timing.gb");
}

// ── mem_timing-2 (individual) ───────────────────────────────────────────

#[test]
#[ignore]
fn mem_timing2_01_read_timing() {
    assert_blargg_passes("mem_timing-2/rom_singles/01-read_timing.gb");
}

#[test]
#[ignore]
fn mem_timing2_02_write_timing() {
    assert_blargg_passes("mem_timing-2/rom_singles/02-write_timing.gb");
}

#[test]
#[ignore]
fn mem_timing2_03_modify_timing() {
    assert_blargg_passes("mem_timing-2/rom_singles/03-modify_timing.gb");
}

// ── halt_bug ────────────────────────────────────────────────────────────

#[test]
#[ignore]
fn halt_bug() {
    assert_blargg_passes("halt_bug.gb");
}

// ── interrupt_time ──────────────────────────────────────────────────────

#[test]
#[ignore]
fn interrupt_time() {
    assert_blargg_passes("interrupt_time/interrupt_time.gb");
}

// ── dmg_sound (individual) ──────────────────────────────────────────────

#[test]
#[ignore]
fn dmg_sound_01_registers() {
    assert_blargg_passes("dmg_sound/rom_singles/01-registers.gb");
}

#[test]
#[ignore]
fn dmg_sound_02_len_ctr() {
    assert_blargg_passes("dmg_sound/rom_singles/02-len ctr.gb");
}

#[test]
#[ignore]
fn dmg_sound_03_trigger() {
    assert_blargg_passes("dmg_sound/rom_singles/03-trigger.gb");
}

#[test]
#[ignore]
fn dmg_sound_04_sweep() {
    assert_blargg_passes("dmg_sound/rom_singles/04-sweep.gb");
}

#[test]
#[ignore]
fn dmg_sound_05_sweep_details() {
    assert_blargg_passes("dmg_sound/rom_singles/05-sweep details.gb");
}

#[test]
#[ignore]
fn dmg_sound_06_overflow_on_trigger() {
    assert_blargg_passes("dmg_sound/rom_singles/06-overflow on trigger.gb");
}

#[test]
#[ignore]
fn dmg_sound_07_len_sweep_period_sync() {
    assert_blargg_passes("dmg_sound/rom_singles/07-len sweep period sync.gb");
}

#[test]
#[ignore]
fn dmg_sound_08_len_ctr_during_power() {
    assert_blargg_passes("dmg_sound/rom_singles/08-len ctr during power.gb");
}

#[test]
#[ignore]
fn dmg_sound_09_wave_read_while_on() {
    assert_blargg_passes("dmg_sound/rom_singles/09-wave read while on.gb");
}

#[test]
#[ignore]
fn dmg_sound_10_wave_trigger_while_on() {
    assert_blargg_passes("dmg_sound/rom_singles/10-wave trigger while on.gb");
}

#[test]
#[ignore]
fn dmg_sound_11_regs_after_power() {
    assert_blargg_passes("dmg_sound/rom_singles/11-regs after power.gb");
}

#[test]
#[ignore]
fn dmg_sound_12_wave_write_while_on() {
    assert_blargg_passes("dmg_sound/rom_singles/12-wave write while on.gb");
}

// ── cgb_sound (individual) ──────────────────────────────────────────────

#[test]
#[ignore]
fn cgb_sound_01_registers() {
    assert_blargg_passes("cgb_sound/rom_singles/01-registers.gb");
}

#[test]
#[ignore]
fn cgb_sound_02_len_ctr() {
    assert_blargg_passes("cgb_sound/rom_singles/02-len ctr.gb");
}

#[test]
#[ignore]
fn cgb_sound_03_trigger() {
    assert_blargg_passes("cgb_sound/rom_singles/03-trigger.gb");
}

#[test]
#[ignore]
fn cgb_sound_04_sweep() {
    assert_blargg_passes("cgb_sound/rom_singles/04-sweep.gb");
}

#[test]
#[ignore]
fn cgb_sound_05_sweep_details() {
    assert_blargg_passes("cgb_sound/rom_singles/05-sweep details.gb");
}

#[test]
#[ignore]
fn cgb_sound_06_overflow_on_trigger() {
    assert_blargg_passes("cgb_sound/rom_singles/06-overflow on trigger.gb");
}

#[test]
#[ignore]
fn cgb_sound_07_len_sweep_period_sync() {
    assert_blargg_passes("cgb_sound/rom_singles/07-len sweep period sync.gb");
}

#[test]
#[ignore]
fn cgb_sound_08_len_ctr_during_power() {
    assert_blargg_passes("cgb_sound/rom_singles/08-len ctr during power.gb");
}

#[test]
#[ignore]
fn cgb_sound_09_wave_read_while_on() {
    assert_blargg_passes("cgb_sound/rom_singles/09-wave read while on.gb");
}

#[test]
#[ignore]
fn cgb_sound_10_wave_trigger_while_on() {
    assert_blargg_passes("cgb_sound/rom_singles/10-wave trigger while on.gb");
}

#[test]
#[ignore]
fn cgb_sound_11_regs_after_power() {
    assert_blargg_passes("cgb_sound/rom_singles/11-regs after power.gb");
}

#[test]
#[ignore]
fn cgb_sound_12_wave() {
    assert_blargg_passes("cgb_sound/rom_singles/12-wave.gb");
}

// ── oam_bug (individual) ────────────────────────────────────────────────

#[test]
#[ignore]
fn oam_bug_1_lcd_sync() {
    assert_blargg_passes("oam_bug/rom_singles/1-lcd_sync.gb");
}

#[test]
#[ignore]
fn oam_bug_2_causes() {
    assert_blargg_passes("oam_bug/rom_singles/2-causes.gb");
}

#[test]
#[ignore]
fn oam_bug_3_non_causes() {
    assert_blargg_passes("oam_bug/rom_singles/3-non_causes.gb");
}

#[test]
#[ignore]
fn oam_bug_4_scanline_timing() {
    assert_blargg_passes("oam_bug/rom_singles/4-scanline_timing.gb");
}

#[test]
#[ignore]
fn oam_bug_5_timing_bug() {
    assert_blargg_passes("oam_bug/rom_singles/5-timing_bug.gb");
}

#[test]
#[ignore]
fn oam_bug_6_timing_no_bug() {
    assert_blargg_passes("oam_bug/rom_singles/6-timing_no_bug.gb");
}

#[test]
#[ignore]
fn oam_bug_7_timing_effect() {
    assert_blargg_passes("oam_bug/rom_singles/7-timing_effect.gb");
}

#[test]
#[ignore]
fn oam_bug_8_instr_effect() {
    assert_blargg_passes("oam_bug/rom_singles/8-instr_effect.gb");
}

// ── Suite reports ───────────────────────────────────────────────────────
//
// Run with:  cargo test -p sm83 --test blargg report -- --ignored --nocapture
//
// Each report runs every ROM in a suite sequentially and prints a
// results table.  They never panic — use them to gauge progress, not
// as CI gates.

const CPU_INSTRS: &[RomEntry] = &[
    RomEntry { name: "01-special",             path: "cpu_instrs/individual/01-special.gb" },
    RomEntry { name: "02-interrupts",          path: "cpu_instrs/individual/02-interrupts.gb" },
    RomEntry { name: "03-op sp,hl",            path: "cpu_instrs/individual/03-op sp,hl.gb" },
    RomEntry { name: "04-op r,imm",            path: "cpu_instrs/individual/04-op r,imm.gb" },
    RomEntry { name: "05-op rp",               path: "cpu_instrs/individual/05-op rp.gb" },
    RomEntry { name: "06-ld r,r",              path: "cpu_instrs/individual/06-ld r,r.gb" },
    RomEntry { name: "07-jr,jp,call,ret,rst",  path: "cpu_instrs/individual/07-jr,jp,call,ret,rst.gb" },
    RomEntry { name: "08-misc instrs",         path: "cpu_instrs/individual/08-misc instrs.gb" },
    RomEntry { name: "09-op r,r",              path: "cpu_instrs/individual/09-op r,r.gb" },
    RomEntry { name: "10-bit ops",             path: "cpu_instrs/individual/10-bit ops.gb" },
    RomEntry { name: "11-op a,(hl)",           path: "cpu_instrs/individual/11-op a,(hl).gb" },
];

const TIMING: &[RomEntry] = &[
    RomEntry { name: "instr_timing",           path: "instr_timing/instr_timing.gb" },
    RomEntry { name: "mem_timing 01-read",     path: "mem_timing/individual/01-read_timing.gb" },
    RomEntry { name: "mem_timing 02-write",    path: "mem_timing/individual/02-write_timing.gb" },
    RomEntry { name: "mem_timing 03-modify",   path: "mem_timing/individual/03-modify_timing.gb" },
    RomEntry { name: "mem_timing2 01-read",    path: "mem_timing-2/rom_singles/01-read_timing.gb" },
    RomEntry { name: "mem_timing2 02-write",   path: "mem_timing-2/rom_singles/02-write_timing.gb" },
    RomEntry { name: "mem_timing2 03-modify",  path: "mem_timing-2/rom_singles/03-modify_timing.gb" },
    RomEntry { name: "halt_bug",               path: "halt_bug.gb" },
    RomEntry { name: "interrupt_time",         path: "interrupt_time/interrupt_time.gb" },
];

const DMG_SOUND: &[RomEntry] = &[
    RomEntry { name: "01-registers",           path: "dmg_sound/rom_singles/01-registers.gb" },
    RomEntry { name: "02-len ctr",             path: "dmg_sound/rom_singles/02-len ctr.gb" },
    RomEntry { name: "03-trigger",             path: "dmg_sound/rom_singles/03-trigger.gb" },
    RomEntry { name: "04-sweep",               path: "dmg_sound/rom_singles/04-sweep.gb" },
    RomEntry { name: "05-sweep details",       path: "dmg_sound/rom_singles/05-sweep details.gb" },
    RomEntry { name: "06-overflow on trigger",  path: "dmg_sound/rom_singles/06-overflow on trigger.gb" },
    RomEntry { name: "07-len sweep period sync", path: "dmg_sound/rom_singles/07-len sweep period sync.gb" },
    RomEntry { name: "08-len ctr during power", path: "dmg_sound/rom_singles/08-len ctr during power.gb" },
    RomEntry { name: "09-wave read while on",  path: "dmg_sound/rom_singles/09-wave read while on.gb" },
    RomEntry { name: "10-wave trigger while on", path: "dmg_sound/rom_singles/10-wave trigger while on.gb" },
    RomEntry { name: "11-regs after power",    path: "dmg_sound/rom_singles/11-regs after power.gb" },
    RomEntry { name: "12-wave write while on", path: "dmg_sound/rom_singles/12-wave write while on.gb" },
];

const OAM_BUG: &[RomEntry] = &[
    RomEntry { name: "1-lcd_sync",             path: "oam_bug/rom_singles/1-lcd_sync.gb" },
    RomEntry { name: "2-causes",               path: "oam_bug/rom_singles/2-causes.gb" },
    RomEntry { name: "3-non_causes",           path: "oam_bug/rom_singles/3-non_causes.gb" },
    RomEntry { name: "4-scanline_timing",      path: "oam_bug/rom_singles/4-scanline_timing.gb" },
    RomEntry { name: "5-timing_bug",           path: "oam_bug/rom_singles/5-timing_bug.gb" },
    RomEntry { name: "6-timing_no_bug",        path: "oam_bug/rom_singles/6-timing_no_bug.gb" },
    RomEntry { name: "7-timing_effect",        path: "oam_bug/rom_singles/7-timing_effect.gb" },
    RomEntry { name: "8-instr_effect",         path: "oam_bug/rom_singles/8-instr_effect.gb" },
];

#[test]
#[ignore]
fn report_cpu_instrs() {
    let r = run_suite_report("cpu_instrs", CPU_INSTRS);
    write_markdown_report(&[r]);
}

#[test]
#[ignore]
fn report_timing() {
    let r = run_suite_report("timing", TIMING);
    write_markdown_report(&[r]);
}

#[test]
#[ignore]
fn report_dmg_sound() {
    let r = run_suite_report("dmg_sound", DMG_SOUND);
    write_markdown_report(&[r]);
}

#[test]
#[ignore]
fn report_oam_bug() {
    let r = run_suite_report("oam_bug", OAM_BUG);
    write_markdown_report(&[r]);
}

#[test]
#[ignore]
fn report_all() {
    let suites_def: &[(&str, &[RomEntry])] = &[
        ("cpu_instrs", CPU_INSTRS),
        ("timing",     TIMING),
        ("dmg_sound",  DMG_SOUND),
        ("oam_bug",    OAM_BUG),
    ];

    let reports: Vec<SuiteReport> = suites_def
        .iter()
        .map(|(name, roms)| run_suite_report(name, roms))
        .collect();

    let total_pass: usize = reports.iter().map(|r| r.passed).sum();
    let total_count: usize = reports.iter().map(|r| r.total).sum();

    println!("════════════════════════════════════════════════════");
    println!("  TOTAL: {total_pass}/{total_count} passed");
    println!("════════════════════════════════════════════════════");

    write_markdown_report(&reports);
}

// ── Debug runner ────────────────────────────────────────────────────────
//
// Reusable harness for investigating ROM hangs.
//
// Usage:
//   cargo test -p sm83 --test blargg debug_halt_bug -- --ignored --nocapture

const PC_RING_SIZE: usize = 16;
const TRACE_RING_SIZE: usize = 64;

#[derive(Debug)]
enum StopReason {
    Condition,
    BudgetExhausted,
    SerialPassed,
    #[allow(dead_code)]
    SerialFailed(String),
}

struct DebugRunner {
    session: Session,
    pc_ring: [u16; PC_RING_SIZE],
    ring_idx: usize,
    steps_executed: u64,
    /// Ring buffer of the last N instruction traces.
    trace_ring: Vec<InstrTrace>,
    trace_idx: usize,
}

impl DebugRunner {
    fn new(rom_relative_path: &str, skip_boot: bool) -> Self {
        let rom_data = load_rom_data(rom_relative_path);
        let mut session = Session::new();
        if skip_boot {
            session.load_rom_no_boot(&rom_data);
        } else {
            session.load_rom(&rom_data);
        }
        Self {
            session,
            pc_ring: [0; PC_RING_SIZE],
            ring_idx: 0,
            steps_executed: 0,
            trace_ring: Vec::with_capacity(TRACE_RING_SIZE),
            trace_idx: 0,
        }
    }

    /// Run until `stop` returns true, serial contains a result, or budget runs out.
    /// Each iteration steps by `chunk` M-cycles and records PC in the ring buffer.
    fn run_until(
        &mut self,
        budget_m: u64,
        chunk: u32,
        stop: impl Fn(&Session, &CpuSnapshot) -> bool,
    ) -> StopReason {
        let mut executed: u64 = 0;
        while executed < budget_m {
            self.session.step(chunk).unwrap();
            executed += chunk as u64;
            self.steps_executed += chunk as u64;

            let snap = self.session.cpu_snapshot().unwrap();
            self.pc_ring[self.ring_idx % PC_RING_SIZE] = snap.pc;
            self.ring_idx += 1;

            if stop(&self.session, &snap) {
                return StopReason::Condition;
            }

            let output = self.session.serial_output_as_string();
            if output.contains("Passed") {
                return StopReason::SerialPassed;
            }
            if output.contains("Failed") {
                return StopReason::SerialFailed(output);
            }
        }
        StopReason::BudgetExhausted
    }

    /// Format the PC ring buffer, detecting loops.
    fn format_pc_ring(&self) -> String {
        let len = self.ring_idx.min(PC_RING_SIZE);
        if len == 0 {
            return "  PC history: (empty)".to_string();
        }

        // Collect the last `len` PCs in order
        let start = if self.ring_idx > PC_RING_SIZE {
            self.ring_idx - PC_RING_SIZE
        } else {
            0
        };
        let pcs: Vec<u16> = (start..self.ring_idx)
            .map(|i| self.pc_ring[i % PC_RING_SIZE])
            .collect();

        // Detect loop: collect unique PCs
        let unique: BTreeSet<u16> = pcs.iter().copied().collect();
        if unique.len() <= 4 && len >= 8 {
            let addrs: Vec<String> = unique.iter().map(|p| format!("{:#06X}", p)).collect();
            format!(
                "  PC history: LOOP [{}] ({}-instruction cycle)",
                addrs.join(", "),
                unique.len()
            )
        } else {
            let entries: Vec<String> = pcs.iter().map(|p| format!("{:#06X}", p)).collect();
            format!("  PC history: {}", entries.join(" → "))
        }
    }

    /// Dump full debug state as a formatted string.
    fn dump_state(&self) -> String {
        let snap = self.session.cpu_snapshot().unwrap();
        let io = self.session.io_snapshot().unwrap();
        let serial = self.session.serial_output_as_string();
        let serial_display = if serial.is_empty() {
            "(empty)".to_string()
        } else {
            let truncated = &serial[..serial.len().min(200)];
            format!("({} bytes): {:?}", serial.len(), truncated)
        };

        format!(
            "\n\
             ══ DebugRunner State ══════════════════════════════\n\
             {}\n\
             {}\n\
             {}\n\
             {}\n\
             ═══════════════════════════════════════════════════",
            format!(
                "  CPU: PC={:#06X} SP={:#06X} AF={:04X} BC={:04X} DE={:04X} HL={:04X} halted={}",
                snap.pc, snap.sp, snap.af, snap.bc, snap.de, snap.hl, snap.halted
            ),
            format!("  I/O: {io}"),
            self.format_pc_ring(),
            format!("  Serial {serial_display}"),
        )
    }

    /// Run one instruction at a time, capturing a trace ring buffer.
    /// Stops when `stop` returns true, serial has a result, or budget
    /// (in instructions) is exhausted.
    fn run_traced(
        &mut self,
        budget_instrs: u64,
        stop: impl Fn(&InstrTrace) -> bool,
    ) -> StopReason {
        for _ in 0..budget_instrs {
            let trace = self.session.step_traced().unwrap();
            self.steps_executed += 1;

            // Record in PC ring
            self.pc_ring[self.ring_idx % PC_RING_SIZE] = trace.pc;
            self.ring_idx += 1;

            // Record in trace ring (circular)
            if self.trace_ring.len() < TRACE_RING_SIZE {
                self.trace_ring.push(trace.clone());
            } else {
                self.trace_ring[self.trace_idx % TRACE_RING_SIZE] = trace.clone();
            }
            self.trace_idx += 1;

            if stop(&trace) {
                return StopReason::Condition;
            }

            let output = self.session.serial_output_as_string();
            if output.contains("Passed") {
                return StopReason::SerialPassed;
            }
            if output.contains("Failed") {
                return StopReason::SerialFailed(output);
            }
        }
        StopReason::BudgetExhausted
    }

    /// Print the last N instruction traces from the ring buffer.
    /// If `last_n` is None, prints all entries.
    #[allow(dead_code)]
    fn dump_trace(&self, last_n: Option<usize>) {
        let len = self.trace_ring.len();
        let total = self.trace_idx.min(len);
        let show = last_n.unwrap_or(total).min(total);

        let first = if self.trace_idx > show {
            self.trace_idx - show
        } else {
            0
        };

        println!("\n══ Instruction Trace (last {} of {}) ══", show, self.trace_idx);
        for i in first..self.trace_idx {
            let entry = &self.trace_ring[i % len];
            println!("  {entry}");
        }
        println!("══════════════════════════════════════════════════");
    }
}

// ── Debug test stubs ────────────────────────────────────────────────────
//
// Uncomment / modify these to investigate specific ROM hangs.
// Run with: cargo test -p sm83 --test blargg debug_ -- --ignored --nocapture

#[test]
#[ignore]
fn debug_halt_bug() {
    let mut r = DebugRunner::new("halt_bug.gb", true);

    // Watch IF to see timer interrupt firing
    r.session.watch_bus(0xFF0F);

    // Trace instruction-by-instruction, stop when test is done
    let reason = r.run_traced(500_000, |t| t.pc == 0xC818);
    println!("Stop reason: {:?}", reason);

    // Show IF log — filter to only timer-related entries
    let log = r.session.drain_bus_log();
    println!("\n══ IF Access Log — timer entries ({} total) ══", log.len());
    for (i, entry) in log.iter().enumerate() {
        if entry.source == "timer" || (entry.source == "cpu" && entry.value & 0x04 != 0) {
            println!(
                "  [{:>4}] {:10?} IF={:02X} pc={:04X} src={}",
                i, entry.kind, entry.value, entry.pc, entry.source
            );
        }
    }

    // Also check: what are TAC/TIMA/TMA at the point of failure?
    let io = r.session.io_snapshot().unwrap();
    println!("\nI/O at stop: TAC={:02X} TIMA={:02X} TMA={:02X} DIV={:02X}",
        io.tac, io.tima, io.tma, io.div);

    // Show trace around the first HALT that gets F1 instead of E1
    // Look for IF changing to include bit 4 (timer)
    let len = r.trace_ring.len();
    let first = if r.trace_idx > len { r.trace_idx - len } else { 0 };
    println!("\n══ Trace entries where IF has bit 4 set ══");
    let mut count = 0;
    for i in first..r.trace_idx {
        let t = &r.trace_ring[i % len];
        if (t.if_after & 0x14 != 0) && !t.halted_before && count < 20 {
            println!("  {t}");
            count += 1;
        }
    }

    let ram = r.session.read_memory(0xA000, 128).unwrap();
    if ram[1] == 0xDE && ram[2] == 0xB0 && ram[3] == 0x61 {
        let text_end = ram[4..].iter().position(|&b| b == 0).unwrap_or(124);
        let text = String::from_utf8_lossy(&ram[4..4 + text_end]);
        println!("\nCart RAM result (code={:#04X}):\n{}", ram[0], text);
    }
}

#[test]
#[ignore]
fn debug_mem_timing2() {
    let mut r = DebugRunner::new("mem_timing-2/rom_singles/01-read_timing.gb", true);
    let reason = r.run_until(5_000_000, 4096, |_, _| false);
    println!("Stop reason: {:?}", reason);
    println!("{}", r.dump_state());
}

#[test]
#[ignore]
fn debug_interrupt_time() {
    let mut r = DebugRunner::new("interrupt_time/interrupt_time.gb", true);
    let reason = r.run_until(5_000_000, 4096, |_, _| false);
    println!("Stop reason: {:?}", reason);
    println!("{}", r.dump_state());
}

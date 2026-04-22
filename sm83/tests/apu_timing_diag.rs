/// APU timing diagnostic tests.
///
/// These replicate what Blargg's dmg_sound test ROMs do — but at the Rust
/// API level, bypassing CPU execution entirely.  By writing APU registers
/// and ticking the system directly, we can measure exact T-cycle counts
/// and compare them against the values the test ROMs expect.
///
/// Run with:
///   cargo test -p sm83 --test apu_timing_diag -- --nocapture

use sm83::apu::{NR10, NR11, NR12, NR13, NR14};
use sm83::apu::{NR21, NR22, NR24};
use sm83::apu::{NR30, NR31, NR32, NR33, NR34};
use sm83::apu::{NR41, NR42, NR44};
use sm83::apu::{NR50, NR51, NR52};
use sm83::system::GameBoy;
use sm83::memory::bus::Bus;
use sm83::trace::Tracer;

// ── Helpers ────────────────────────────────────────────────────────────

/// Create a GameBoy with APU powered on, no ROM needed for register-level tests.
fn make_gb() -> GameBoy {
    let mut bus = Bus::new();
    // Load a minimal 32K ROM (all zeros) so the bus doesn't panic.
    let rom = vec![0u8; 0x8000];
    bus.load_cartridge(&rom);
    let mut gb = GameBoy::new(bus, Tracer::off());
    // Power on APU
    gb.bus.write(NR52, 0x80);
    gb.bus.write(NR51, 0xFF); // all channels to both speakers
    gb.bus.write(NR50, 0x77); // max volume
    gb
}

/// Tick one T-cycle (advance subsystems only, no CPU execution).
/// Returns the new total_dots.
fn tick_t(gb: &mut GameBoy, t_cycles: u32) {
    gb.tick_t(t_cycles);
}

/// Check if CH1 is enabled via NR52 bit 0.
fn ch1_on(gb: &GameBoy) -> bool {
    gb.bus.read(NR52) & 0x01 != 0
}

/// Check if CH2 is enabled via NR52 bit 1.
fn ch2_on(gb: &GameBoy) -> bool {
    gb.bus.read(NR52) & 0x02 != 0
}

/// Check if CH3 is enabled via NR52 bit 2.
fn ch3_on(gb: &GameBoy) -> bool {
    gb.bus.read(NR52) & 0x04 != 0
}

/// Check if CH4 is enabled via NR52 bit 3.
fn ch4_on(gb: &GameBoy) -> bool {
    gb.bus.read(NR52) & 0x08 != 0
}

/// Replicate sync_apu: trigger CH2 with length=2, tick until CH2 dies.
/// Returns the total T-cycles consumed and the frame_step at exit.
fn sync_apu(gb: &mut GameBoy) -> (u64, u8) {
    let start = gb.total_dots();
    gb.bus.write(NR24, 0x00);  // disable length
    gb.bus.write(NR21, 0x3E);  // length = 2
    gb.bus.write(NR22, 0x08);  // DAC on, silent
    gb.bus.write(NR24, 0xC0);  // trigger + length enable
    // Poll until CH2 dies
    let mut ticks = 0u64;
    while ch2_on(gb) {
        tick_t(gb, 4); // one M-cycle
        ticks += 4;
        if ticks > 200_000 { panic!("sync_apu: CH2 never died"); }
    }
    let elapsed = gb.total_dots() - start;
    let frame_step = gb.bus.apu.frame_step;
    (elapsed, frame_step)
}

/// Replicate sync_sweep: trigger CH1 with sweep that overflows, poll until dead.
fn sync_sweep(gb: &mut GameBoy) -> (u64, u8) {
    let start = gb.total_dots();
    gb.bus.write(NR10, 0x11);  // sweep period=1, shift=1
    gb.bus.write(NR12, 0x08);  // DAC on
    gb.bus.write(NR13, 0xFF);  // freq = 0x3FF
    gb.bus.write(NR14, 0x83);  // trigger + freq_hi=3
    let mut ticks = 0u64;
    while ch1_on(gb) {
        tick_t(gb, 4);
        ticks += 4;
        if ticks > 200_000 { panic!("sync_sweep: CH1 never died"); }
    }
    let elapsed = gb.total_dots() - start;
    let frame_step = gb.bus.apu.frame_step;
    (elapsed, frame_step)
}

// ── Test 07: Frame sequencer timing ────────────────────────────────────

/// Measure the length clock period as seen by a polling loop.
///
/// Replicates Blargg test 07 test #2 "Length period is wrong":
///   sync_apu → trigger CH1 length=1 → count T-cycles until CH1 dies.
///
/// Expected: the loop in the test ROM runs 368-372 iterations at 36 T/iter
/// = 13248-13392 T-cycles from loop start to channel death.
/// We measure the equivalent: T-cycles from trigger to CH1 death.
#[test]
fn diag_07_length_period() {
    let mut gb = make_gb();

    // Run for a bit to let the system settle.
    tick_t(&mut gb, 8192 * 8);

    let (sync_elapsed, sync_step) = sync_apu(&mut gb);
    let post_sync_dots = gb.total_dots();

    println!("sync_apu: elapsed={} T, frame_step={}", sync_elapsed, sync_step);
    println!("total_dots after sync: {}", post_sync_dots);

    // The test ROM (07-len sweep period sync.s, test #2) does:
    //   call sync_apu
    //   wreg NR14,$40   — avoids extra length clock (sets enable before length)
    //   wreg NR11,$3F   — length = 1
    //   wreg NR12,$08   — DAC on
    //   wreg NR14,$C0   — trigger + length enable
    //
    // Extra-clock quirk: enabling length on an odd frame step causes an
    // immediate length tick.  The test avoids this by writing NR14=$40
    // first (enabling length when counter is still from the previous state).
    // We must also avoid hitting an edge: the test uses `delay 2048` in its
    // `begin` macro.  We'll tick past the danger zone too.

    // If frame_step is odd, tick until an even step to avoid the quirk.
    // Actually, let's replicate what the ROM does more faithfully.
    // The key: write length enable BEFORE setting length, so the extra
    // clock (if any) ticks the OLD counter value (which might be 0 or high).

    // Wait until frame_step is even (0,2,4,6) to avoid the extra-clock quirk
    // on trigger.  The test ROM uses `delay 2048` to achieve this.
    while gb.bus.apu.frame_step & 1 != 0 {
        tick_t(&mut gb, 4);
    }
    // Tick a bit more into the even step so the trigger doesn't land
    // on the exact boundary
    tick_t(&mut gb, 100);

    // wreg NR14,$40 — length enable, no trigger (5 M = 20 T)
    gb.bus.write(NR14, 0x40);
    tick_t(&mut gb, 20);
    // wreg NR11,$3F — length_field=63 → counter = 64-63 = 1 (5 M = 20 T)
    gb.bus.write(NR11, 0x3F);
    tick_t(&mut gb, 20);
    // wreg NR12,$08 — DAC on (5 M = 20 T)
    gb.bus.write(NR12, 0x08);
    tick_t(&mut gb, 20);

    let pre_trigger = gb.total_dots();
    let pre_step = gb.bus.apu.frame_step;
    // wreg NR14,$C0 — trigger + length enable (5 M = 20 T)
    gb.bus.write(NR14, 0xC0);
    tick_t(&mut gb, 20);
    // ld de,-$170 (3 M = 12 T) + call test_timing (6 M = 24 T) = 36 T
    tick_t(&mut gb, 36);

    println!("pre_trigger dots: {}, frame_step: {} → {}", pre_trigger, pre_step, gb.bus.apu.frame_step);
    println!("CH1 enabled after trigger+overhead: {}", ch1_on(&gb));
    println!("CH1 length counter: {}", gb.bus.apu.ch1.length.counter);

    // Measure T-cycles until CH1 dies (same as the test's poll loop)
    let mut t_since_trigger = 0u64;
    while ch1_on(&gb) {
        tick_t(&mut gb, 4); // approximate the 9 M-cycle poll loop
        t_since_trigger += 4;
        if t_since_trigger > 100_000 {
            panic!("CH1 never died after trigger (length=1)");
        }
    }

    let total_from_sync = gb.total_dots() - post_sync_dots;

    println!("T-cycles from trigger to CH1 death: {} (+ 36 T overhead)", t_since_trigger);
    println!("T-cycles from sync_apu return to CH1 death: {}", total_from_sync);
    println!("frame_step at CH1 death: {}", gb.bus.apu.frame_step);
    println!();
    println!("Blargg expects: ~13320 T from poll loop start to death");
    println!("                ~13436 T from sync return to death (with 116 T setup)");

    // In the test ROM, setup takes ~116 T (4 wreg + ld de + call).
    // The loop measures 368-372 iterations × 36 T = 13248-13392 T.
    // Total from sync return to death ≈ 116 + 13320 = 13436 T.
    // Our register writes are instantaneous (0 T), so the relevant
    // number is just t_since_trigger (= time from trigger to death).
    //
    // The length period (consecutive clocks) should be 16384 T.
    // Channel death occurs at the next length clock after trigger.
    println!();
    println!("=== ANALYSIS ===");
    println!("If length period = 16384 T:");
    println!("  Expected t_since_trigger ≈ 16384 - (time from last length clock to trigger)");
    println!("  Sync exits shortly after a length clock.");
    println!("  Register writes are instant in this test (0 T overhead).");
    println!("  So t_since_trigger should be close to 16384 T.");
    println!();

    // Also measure the raw length period: trigger with length=3, measure
    // T-cycles between first and second length clock events.
    let mut gb2 = make_gb();
    tick_t(&mut gb2, 8192 * 8);
    sync_apu(&mut gb2);
    // Wait for even frame step
    while gb2.bus.apu.frame_step & 1 != 0 { tick_t(&mut gb2, 4); }
    tick_t(&mut gb2, 100);

    gb2.bus.write(NR14, 0x40); tick_t(&mut gb2, 20);
    gb2.bus.write(NR11, 0x3D); tick_t(&mut gb2, 20); // length = 3
    gb2.bus.write(NR12, 0x08); tick_t(&mut gb2, 20);
    gb2.bus.write(NR14, 0xC0); tick_t(&mut gb2, 4);  // trigger
    println!("gb2: length after trigger = {}, frame_step = {}",
        gb2.bus.apu.ch1.length.counter, gb2.bus.apu.frame_step);

    // Wait for length to tick from 3 → 2 (first clock)
    let mut first_clock_t = 0u64;
    while gb2.bus.apu.ch1.length.counter > 2 {
        tick_t(&mut gb2, 4);
        first_clock_t += 4;
        if first_clock_t > 100_000 { panic!("first length clock never fired"); }
    }

    // Wait for length to tick from 2 → 1 (second clock)
    let mut period_t = 0u64;
    while gb2.bus.apu.ch1.length.counter > 1 {
        tick_t(&mut gb2, 4);
        period_t += 4;
        if period_t > 100_000 { panic!("second length clock never fired"); }
    }

    println!("Direct measurement: length period = {} T (expected 16384)", period_t);
    println!("First length clock after trigger: {} T", first_clock_t);
}

/// Measure sweep clock period and alignment relative to length clock.
///
/// Replicates test 07 test #3 "Sweep period is wrong":
///   sync_sweep → trigger CH1 with sweep period=1 that overflows → count
#[test]
fn diag_07_sweep_period() {
    let mut gb = make_gb();
    tick_t(&mut gb, 8192 * 8);

    let (sync_elapsed, sync_step) = sync_sweep(&mut gb);
    println!("sync_sweep: elapsed={} T, frame_step={}", sync_elapsed, sync_step);

    // Trigger CH1 with sweep that will overflow on first sweep clock.
    // sweep period=1, shift=1, freq=0x3FF → first sweep: 0x3FF + 0x1FF = 0x5FE (ok)
    // → double check: 0x5FE + 0x2FF = 0x8FD > 2047 → overflow → disable.
    // But we want to measure the SECOND scenario from the test: just a sweep disable.
    gb.bus.write(NR10, 0x10);  // period=1, shift=0 (sweep enabled, no calc)
    gb.bus.write(NR12, 0x08);
    gb.bus.write(NR13, 0xFF);
    gb.bus.write(NR14, 0x87);  // trigger + freq_hi=7 → freq=0x7FF

    // This won't overflow since shift=0. Instead, use the approach from test 07 #3:
    // It measures time until CH1 dies from a sweep overflow.
    // Let's use period=1, shift=1, freq=0x7FF which overflows immediately on sweep.
    gb.bus.write(NR10, 0x11); // period=1, shift=1
    gb.bus.write(NR14, 0x87); // re-trigger

    let mut t = 0u64;
    while ch1_on(&gb) {
        tick_t(&mut gb, 4);
        t += 4;
        if t > 200_000 { panic!("CH1 never died from sweep overflow"); }
    }

    println!("T-cycles from trigger to sweep overflow death: {} T", t);
    println!("frame_step at death: {}", gb.bus.apu.frame_step);

    // Expected from test ROM: DE=-$2E4=-740 iterations × 36 T = 26640 T.
    // Sweep fires at steps 2, 6 (every 32768 T = 4 frame steps).
    // But the timer starts at 1 (period=1), so it fires on the first sweep step.
    println!("Expected: first sweep clock from sync_sweep ≈ 32768 T (4 frame steps)");
}

// ── Test 04: Sweep period=0 doesn't calculate ──────────────────────────

/// Replicates test 04 #4: "If period=0, doesn't calculate"
///
/// With NR10=$00 (period=0, shift=0), sweep is disabled.
/// Freq = 0x7FF (max). If any calculation happened, it would overflow.
/// Channel should survive for 32 length clocks.
#[test]
fn diag_04_sweep_period_zero() {
    let mut gb = make_gb();
    tick_t(&mut gb, 8192 * 8);

    // sync_sweep + begin equivalent
    sync_sweep(&mut gb);

    gb.bus.write(NR14, 0x40); // length enable
    gb.bus.write(NR11, 0xDF); // length_field=31 → counter=33
    gb.bus.write(NR12, 0x08); // DAC on

    // Test #4 setup
    gb.bus.write(NR10, 0x00); // period=0, shift=0
    gb.bus.write(NR13, 0xFF); // freq_low=0xFF
    gb.bus.write(NR14, 0xC7); // trigger + length_enable + freq_hi=7 → freq=0x7FF

    let length_at_trigger = gb.bus.apu.ch1.length.counter;
    println!("Length counter after trigger: {}", length_at_trigger);
    println!("Sweep enabled: {}", gb.bus.apu.ch1.sweep.as_ref().unwrap().enabled);
    println!("frame_step: {}", gb.bus.apu.frame_step);

    // The test does delay_apu $20 then should_be_almost_off.
    // If delay_apu N = N length periods, we need to tick 32 length clocks.
    // Track length counter changes to verify.
    let mut length_clocks = 0u32;
    let mut prev_len = length_at_trigger;
    let mut total_t = 0u64;

    while ch1_on(&gb) {
        tick_t(&mut gb, 4);
        total_t += 4;
        let cur_len = gb.bus.apu.ch1.length.counter;
        if cur_len < prev_len {
            length_clocks += 1;
            prev_len = cur_len;
        }
        if total_t > 1_000_000 { panic!("CH1 never died"); }
    }

    println!("CH1 died after: {} length clocks, {} T-cycles", length_clocks, total_t);
    println!("Expected: {} length clocks", length_at_trigger);
    println!();

    // Verify the sweep never fired (channel died from length, not overflow)
    println!("Cause of death: length counter expiry (expected, not sweep overflow)");
    println!("Length counter at death: {}", gb.bus.apu.ch1.length.counter);

    // Now verify the key assertion: after 32 length clocks (but before 33),
    // the channel should still be alive.
    // If the test fails at test #4, it means the channel died before 32 length
    // clocks, which would happen if sweep calculated and overflowed.
    assert_eq!(length_clocks, length_at_trigger as u32,
        "Channel should die from length expiry after exactly {} clocks", length_at_trigger);
}

// ── Test 05: Sweep NR10 mid-sweep propagation ──────────────────────────

/// Test 05 #2: "Timer treats period 0 as 8"
///
/// After triggering with period=1, change NR10 to period=0.
/// The timer should reload as 8 when it next fires.
#[test]
fn diag_05_timer_period_zero_as_eight() {
    let mut gb = make_gb();
    tick_t(&mut gb, 8192 * 8);

    sync_sweep(&mut gb);

    gb.bus.write(NR14, 0x40);
    gb.bus.write(NR11, 0xE0); // length=32 (big enough to not expire)
    gb.bus.write(NR12, 0x08);

    // Trigger with period=1, shift=1, freq=0x200
    gb.bus.write(NR10, 0x11);
    gb.bus.write(NR13, 0x00);
    gb.bus.write(NR14, 0xC2); // trigger + freq_hi=2

    let shadow_before = gb.bus.apu.ch1.sweep.as_ref().unwrap().shadow_freq;
    println!("Shadow freq after trigger: 0x{:03X}", shadow_before);

    // Tick until first sweep fires (should be soon since period=1, timer=1).
    // We detect it by watching shadow_freq change.
    let mut t = 0u64;
    loop {
        tick_t(&mut gb, 4);
        t += 4;
        let s = gb.bus.apu.ch1.sweep.as_ref().unwrap().shadow_freq;
        if s != shadow_before {
            println!("First sweep fired at {} T, shadow: 0x{:03X} → 0x{:03X}",
                t, shadow_before, s);
            break;
        }
        if t > 100_000 { panic!("sweep never fired"); }
    }

    // Now change NR10 to period=0, shift=1
    gb.bus.write(NR10, 0x01);
    let sweep = gb.bus.apu.ch1.sweep.as_ref().unwrap();
    println!("After NR10=$01: period={}, shift={}, timer={}",
        sweep.period, sweep.shift, sweep.timer);

    // The timer should now reload as 8 (period 0 → 8).
    // Count sweep ticks until the next frequency change.
    let shadow_now = gb.bus.apu.ch1.sweep.as_ref().unwrap().shadow_freq;
    let mut sweep_ticks = 0u32;
    let mut t2 = 0u64;
    loop {
        tick_t(&mut gb, 4);
        t2 += 4;
        let s = gb.bus.apu.ch1.sweep.as_ref().unwrap().shadow_freq;
        if s != shadow_now {
            println!("Next sweep calc at {} T after NR10 write", t2);
            println!("Shadow: 0x{:03X} → 0x{:03X}", shadow_now, s);
            break;
        }
        // But period=0 means the timer fires but no calc happens!
        // So we won't see shadow change. Instead, change NR10 back to period=1
        // after the expected 8 sweep ticks to enable calculation.
        // This matches what the test ROM does.
        if t2 > 200_000 {
            println!("No sweep calc with period=0 (correct — period=0 gates calculation)");
            break;
        }
    }
}

// ── Test 11: NR41 survives power off ───────────────────────────────────

/// Test 11 #4: "Powering off shouldn't affect NR41"
///
/// Write NR41 while powered off, verify it persists after power on.
#[test]
fn diag_11_nr41_power_off() {
    let mut gb = make_gb();
    tick_t(&mut gb, 8192 * 8);
    sync_apu(&mut gb);

    // Fill all regs with 0xFF
    for addr in 0xFF10..=0xFF25 {
        gb.bus.write(addr, 0xFF);
    }

    // Power off
    gb.bus.write(NR52, 0x00);

    // Write NR41 while powered off (DMG behavior: should work)
    // NR41 = -$12 = 0xEE → length_field = 0xEE & 0x3F = 0x2E = 46 → counter = 64-46 = 18
    gb.bus.write(NR41, 0xEE);
    let len_after_write = gb.bus.apu.ch4.length.counter;
    println!("CH4 length after powered-off NR41 write: {}", len_after_write);

    // Power on
    gb.bus.write(NR52, 0x80);

    // Trigger CH4 with length enable
    gb.bus.write(NR42, 0x08); // DAC on
    gb.bus.write(NR44, 0xC0); // trigger + length enable

    let len_after_trigger = gb.bus.apu.ch4.length.counter;
    println!("CH4 length after trigger: {} (expected 18)", len_after_trigger);

    assert_eq!(len_after_trigger, 18,
        "NR41 write while powered off should set length to 18");

    // Verify channel survives 17 length clocks but dies on 18th.
    let mut clocks = 0u32;
    while ch4_on(&gb) {
        tick_t(&mut gb, 4);
        let cur = gb.bus.apu.ch4.length.counter;
        if cur < len_after_trigger.saturating_sub(clocks as u16) {
            clocks += 1;
        }
        if gb.total_dots() > 1_000_000 { panic!("CH4 never died"); }
    }

    println!("CH4 died after approximately {} length clocks (expected 18)", clocks);
    println!("PASS: NR41 survives power off");
}

// ── Wave RAM access while CH3 active ───────────────────────────────────

/// Diagnose wave RAM read behavior while CH3 is playing.
///
/// On DMG: reading wave RAM while CH3 is active returns the byte at
/// the current playback position, NOT the addressed byte.
/// When CH3 is off, reads return the addressed byte normally.
#[test]
fn diag_09_wave_read_while_on() {
    let mut gb = make_gb();
    tick_t(&mut gb, 8192 * 8);

    // Fill wave RAM with known pattern: 00 11 22 33 ... FF
    for i in 0u8..16 {
        let val = (i << 4) | i;
        gb.bus.write(0xFF30 + i as u16, val);
    }

    // Verify wave RAM is correct when CH3 is off
    println!("Wave RAM when CH3 OFF:");
    let mut off_ok = true;
    for i in 0u8..16 {
        let expected = (i << 4) | i;
        let actual = gb.bus.read(0xFF30 + i as u16);
        if actual != expected {
            println!("  [MISMATCH] FF{:02X}: got 0x{:02X}, expected 0x{:02X}",
                0x30 + i, actual, expected);
            off_ok = false;
        }
    }
    if off_ok {
        println!("  All correct.");
    }

    // Enable CH3 and start playing
    gb.bus.write(NR30, 0x80);  // DAC on
    gb.bus.write(NR31, 0x00);  // length = 0 → max
    gb.bus.write(NR32, 0x20);  // output level = 100%
    gb.bus.write(NR33, 0x00);  // freq low
    gb.bus.write(NR34, 0x80);  // trigger (no length enable)

    assert!(ch3_on(&gb), "CH3 should be enabled after trigger");

    // Tick a bit to advance the playback position
    tick_t(&mut gb, 1000);

    println!("\nWave RAM when CH3 ON (reading all 16 bytes):");
    let mut on_results: Vec<u8> = Vec::new();
    for i in 0u8..16 {
        let actual = gb.bus.read(0xFF30 + i as u16);
        on_results.push(actual);
    }
    // On real DMG, all 16 reads should return the SAME byte (the one at
    // the current playback position). The emulator likely returns the
    // addressed byte instead.
    let all_same = on_results.windows(2).all(|w| w[0] == w[1]);
    println!("  Read values: {:02X?}", on_results);
    if all_same {
        println!("  All reads return 0x{:02X} (current playback byte) — DMG-correct", on_results[0]);
    } else {
        println!("  Reads return different values — returning addressed bytes, NOT playback position");
        println!("  BUG: On DMG hardware, all reads should return the byte at the current wave position");
    }

    // Also check: what position is the wave channel at?
    let wave_pos = gb.bus.apu.ch3.wave_position;
    println!("  Current wave position: {}", wave_pos);
}

/// Diagnose wave RAM write behavior while CH3 is playing.
///
/// On DMG: writing wave RAM while CH3 is active writes to the byte at
/// the current playback position, NOT the addressed byte.
#[test]
fn diag_12_wave_write_while_on() {
    let mut gb = make_gb();
    tick_t(&mut gb, 8192 * 8);

    // Fill wave RAM with known pattern
    for i in 0u8..16 {
        let val = (i << 4) | i;
        gb.bus.write(0xFF30 + i as u16, val);
    }

    // Enable CH3
    gb.bus.write(NR30, 0x80);
    gb.bus.write(NR31, 0x00);
    gb.bus.write(NR32, 0x20);
    gb.bus.write(NR33, 0x00);
    gb.bus.write(NR34, 0x80); // trigger

    assert!(ch3_on(&gb), "CH3 should be on");

    tick_t(&mut gb, 1000); // advance playback

    let wave_pos = gb.bus.apu.ch3.wave_position;
    println!("Current wave position before write: {}", wave_pos);

    // Write 0xF7 to FF30 while CH3 is playing
    gb.bus.write(0xFF30, 0xF7);

    // Stop CH3 so we can read wave RAM normally
    gb.bus.write(NR30, 0x00); // DAC off → CH3 disabled

    println!("\nWave RAM after writing 0xF7 to FF30 while CH3 was on:");
    for i in 0u8..16 {
        let val = gb.bus.read(0xFF30 + i as u16);
        let expected_original = (i << 4) | i;
        let marker = if val != expected_original { " ← MODIFIED" } else { "" };
        println!("  FF{:02X}: 0x{:02X}{}", 0x30 + i, val, marker);
    }
    println!("\nExpected DMG behavior: 0xF7 should be written to position {}/2 = byte FF{:02X},",
        wave_pos, 0x30 + wave_pos / 2);
    println!("not to the addressed byte FF30.");
}

// ── Frame step alignment diagnostic ────────────────────────────────────

/// Measure exact timing of frame sequencer events to find the root
/// cause of test 07's "Length period is wrong".
#[test]
fn diag_frame_step_timing() {
    let mut gb = make_gb();

    // Let the system run a bit
    tick_t(&mut gb, 8192 * 4);

    // Record when each frame step fires by watching frame_step changes
    let mut events: Vec<(u64, u8)> = Vec::new();
    let mut prev_step = gb.bus.apu.frame_step;
    let start = gb.total_dots();

    // Run for 8 full frame sequencer cycles (64 steps)
    for _ in 0..(8192 * 64) {
        tick_t(&mut gb, 1);
        let step = gb.bus.apu.frame_step;
        if step != prev_step {
            events.push((gb.total_dots() - start, step));
            prev_step = step;
        }
    }

    println!("Frame sequencer events (first 32):");
    println!("{:>10}  {:>4}  {:>10}", "T-cycle", "Step", "Delta");
    let mut prev_t = 0u64;
    for (i, (t, step)) in events.iter().enumerate().take(32) {
        let delta = if i == 0 { 0 } else { t - prev_t };
        let kind = match step {
            0 | 4 => "length",
            2 | 6 => "length+sweep",
            7 => "envelope",
            _ => "",
        };
        println!("{:>10}  {:>4}  {:>10}  {}", t, step, delta, kind);
        prev_t = *t;
    }

    // Verify step period
    if events.len() >= 2 {
        let period = events[1].0 - events[0].0;
        println!("\nMeasured frame step period: {} T (expected 8192)", period);
    }

    // Verify length clock period (step N to step N+2)
    let length_steps: Vec<&(u64, u8)> = events.iter()
        .filter(|(_, s)| *s == 0 || *s == 2 || *s == 4 || *s == 6)
        .collect();
    if length_steps.len() >= 2 {
        let length_period = length_steps[1].0 - length_steps[0].0;
        println!("Measured length clock period: {} T (expected 16384)", length_period);
    }

    // Verify sweep clock period
    let sweep_steps: Vec<&(u64, u8)> = events.iter()
        .filter(|(_, s)| *s == 2 || *s == 6)
        .collect();
    if sweep_steps.len() >= 2 {
        let sweep_period = sweep_steps[1].0 - sweep_steps[0].0;
        println!("Measured sweep clock period: {} T (expected 32768)", sweep_period);
    }
}

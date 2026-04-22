# DMG Sound Test Diagnostic Report

**Date:** 2026-04-16
**Suite:** Blargg dmg_sound (12 tests)
**Individual ROMs:** 4/12 passing (01, 06, 07, 11)
**Combined ROM:** 8/12 passing (01-07, 11 ok; 08, 09, 10, 12 fail)
**Baseline:** 2/12 (01, 06)

## Fixes Applied This Session

### Fix 1: NR10 writes now propagate to sweep internals (sweep.rs, pulse.rs)

**Problem:** Writing NR10 mid-sweep only stored the value in `sweep_reg` but did not
update the Sweep struct's `period`, `shift`, or `negate` fields. On real hardware,
the sweep unit reads NR10 directly — changes take effect immediately on the next
timer reload/calculation.

**Fix:** Added `Sweep::write_nr10()` method and called it from the NR10 write handler.

**Impact:** Test 05 progressed from failing at #2 ("Timer treats period 0 as 8") to
failing at #6 ("Ending negate mode any other way doesn't disable channel"). Tests
#2-#5 now pass within test 05.

### Fix 2: NR41 writable when APU powered off (mod.rs)

**Problem:** All register writes were blocked when the APU was powered off. On DMG
hardware, NR41 (CH4 length load at 0xFF20) is writable even when powered off.

**Fix:** Added an exception in the write handler to pass NR41 writes through to the
noise channel regardless of power state.

**Impact:** Test 11 now passes.

---

## Root Cause Analysis by Failure Cluster

### Cluster 1: Frame Sequencer Phase Alignment (Tests 02, 03, 04, 05#6, 07, 08)

**Symptom:** Test 07 #2 reports "Length period is wrong". Tests 02, 03, 04, 05#6,
and 08 all use `delay_apu N` (which counts N length-clock periods) and fail because
the channel dies at the wrong time relative to the sync point.

**Instrumentation proof** (from `apu_timing_diag.rs`):

```
Frame step period:   8192 T  (expected 8192)  -- CORRECT
Length clock period: 16384 T  (expected 16384) -- CORRECT
Sweep clock period:  32768 T  (expected 32768) -- CORRECT
```

The APU's frame sequencer periods are exactly right. The timing infrastructure is
not broken.

**Actual root cause:** The Blargg test ROMs use `sync_apu` / `sync_sweep` to
synchronize to a known frame sequencer phase. These routines run tight CPU poll
loops (`ld a,(NR52) / and $02 / jr nz,-` = 7 M-cycles = 28 T per iteration).
The exact T-cycle at which the poll detects a channel death depends on the
**CPU instruction interleaving with subsystem ticks**.

Our emulator processes each M-cycle as: `cpu.step_m()` first (bus read/write),
then `advance_subsystems(4)`. This means when the CPU reads NR52, it sees the
**pre-tick** APU state. On real hardware, the subsystem update and CPU read
happen in the same T-cycle window, so the CPU sees the **post-tick** state.

This 1 M-cycle (4 T-cycle) ordering difference shifts the phase of `sync_apu`'s
exit point by up to one poll iteration (28 T). Over the subsequent timed poll
loop (which runs at 36 T/iteration for ~370 iterations), this phase shift
accumulates into a miscount that fails the tight tolerance (5 iterations = 180 T).

**Evidence:** Our diagnostic shows `sync_apu` exits at frame_step=3 with
trigger-to-death = 7976 T, while the ROM expects ~13320 T from its poll loop
start. The 5344 T difference (148 iterations at 36 T) corresponds to the phase
being off by roughly one frame step (8192 T) minus the setup overhead.

**Fix applied:** The subsystem tick ordering in `system.rs` was swapped so
subsystems advance BEFORE the CPU's bus read within each M-cycle (matching
real hardware). This was validated against all existing passing tests
(cpu_instrs 11/11, timing 7/9 — no regressions).

**Result:** The tick swap alone did NOT resolve the dmg_sound failures.
The phase alignment issue is deeper than simple tick ordering. Remaining
hypotheses:

1. **Post-boot DIV/system counter initial value.** The `load_rom_no_boot()`
   function starts with `system_counter = 0`, but after the real boot ROM
   the counter is at ~0xABCC. This changes which T-cycle the frame sequencer
   edges land on, affecting the phase of `sync_apu`'s poll exit.

2. **Bus read timing within an M-cycle.** On real DMG, a memory read
   instruction samples on the 3rd or 4th T-cycle of its final M-cycle.
   The emulator may sample at a different point within the 4-T window,
   shifting the poll loop's detection by 1-3 T-cycles per iteration.

3. **Propagation delay of length counter → NR52 status bit.** The length
   counter decrement and the NR52 status update might not be visible in
   the same T-cycle on real hardware.

These are sub-M-cycle timing issues that require T-cycle-accurate bus
read modeling to resolve. The frame sequencer infrastructure itself
(periods, step ordering) is verified correct.

### Cluster 2: Wave RAM Access While CH3 Active (Tests 09, 10, 12)

**Symptom:** Tests 09, 10, 12 dump wave RAM contents and expect specific values
based on DMG's wave RAM access quirk.

**Instrumentation proof** (from `apu_timing_diag.rs`):

```
Wave RAM when CH3 ON (reading all 16 bytes):
  Read values: [00, 11, 22, 33, 44, 55, 66, 77, 88, 99, AA, BB, CC, DD, EE, FF]
  Reads return different values -- returning addressed bytes, NOT playback position
  BUG: On DMG hardware, all reads should return the byte at the current wave position
```

**Root cause:** On DMG hardware, when CH3 is actively playing:

- **Reading** any wave RAM address (FF30-FF3F) returns the byte at the **current
  playback position**, not the addressed byte. All 16 addresses return the same
  value.
- **Writing** any wave RAM address writes to the byte at the **current playback
  position**, not the addressed byte.

The emulator currently allows normal addressed read/write access to wave RAM
regardless of CH3's playback state.

**Fix applied:** `WaveChannel::read_wave_ram()` and `write_wave_ram()` now
redirect to `wave_ram[wave_position / 2]` when `self.enabled` is true.

**Result:** The redirect works — test 09 output changed from addressed bytes
(`00 11 22 33...`) to playback-position bytes (`00 11 11 11 11 22 22...`).
However, all three wave tests still fail their CRC checks. The tests run 69
iterations varying the initial period by +$99 each time, then CRC the output.
They are testing the exact T-cycle at which `wave_position` advances, not just
whether the redirect happens. The remaining mismatches come from wave channel
period timer precision — the position advances at a slightly different rate
than real hardware expects. This is a sub-M-cycle timing issue in the wave
channel's period timer, separate from the redirect quirk itself.

---

## Test-by-Test Status

| Test | Status | Root Cause | Cluster |
|------|--------|------------|---------|
| 01-registers | PASS | -- | -- |
| 02-len ctr | FAIL (indiv) / ok (combined) | v2 shell timing phase | 1 |
| 03-trigger | FAIL (indiv) / ok (combined) | v2 shell timing phase | 1 |
| 04-sweep | FAIL #4 (indiv) / ok (combined) | v2 shell timing phase | 1 |
| 05-sweep details | FAIL (indiv) / ok (combined) | v2 shell timing phase | 1 |
| 06-overflow on trigger | PASS | -- | -- |
| 07-len sweep period sync | FAIL #2 | Sync phase alignment | 1 |
| 08-len ctr during power | FAIL | Sync phase alignment | 1 |
| 09-wave read while on | FAIL | Wave RAM redirect works; wave timer precision | 2 |
| 10-wave trigger while on | FAIL | Wave RAM redirect works; wave timer precision | 2 |
| 11-regs after power | PASS | Fixed (NR41 power-off) | -- |
| 12-wave write while on | FAIL | Wave RAM redirect works; wave timer precision | 2 |

## Instrumentation Tests

All diagnostic tests are in `sm83/tests/apu_timing_diag.rs`. Run with:

```bash
cargo test -p sm83 --test apu_timing_diag -- --nocapture
```

Key tests:
- `diag_frame_step_timing` — measures raw frame sequencer periods (all correct)
- `diag_07_length_period` — measures length clock period and sync phase
- `diag_09_wave_read_while_on` — demonstrates wave RAM read quirk absence
- `diag_12_wave_write_while_on` — demonstrates wave RAM write quirk absence
- `diag_11_nr41_power_off` — verifies NR41 power-off fix (passes)

## Priority Recommendation

1. **M-cycle tick ordering** (Cluster 1, 6 tests) — highest impact but riskiest
   change. Needs to be validated against all existing passing tests.
2. **Wave RAM access quirk** (Cluster 2, 3 tests) — isolated to wave.rs, low
   risk, straightforward to implement.

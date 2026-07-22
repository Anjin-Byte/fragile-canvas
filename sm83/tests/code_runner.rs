//! Integration tests for the assemble→run engine seam (Slice 2): the
//! `Session::load_code` / `run_code` snippet runner and the header-free
//! `Cartridge::rom_only` machine it builds on. Snippets are hand-assembled so
//! these exercise the emulator core directly, independent of sm83-isa.

use sm83::session::{Session, StopReason};

/// A convenient load origin clear of the header/interrupt-vector region.
const ORG: u16 = 0x0150;

// ── 1. A snippet runs to HALT with the right final register state. ──────────
#[test]
fn runs_to_halt_with_final_register_state() {
    // LD A,$41 ; LD B,$01 ; ADD A,B ; LD C,A ; HALT
    let code = [0x3E, 0x41, 0x06, 0x01, 0x80, 0x4F, 0x76];
    let mut s = Session::new();
    s.load_code(ORG, &code, ORG).unwrap();
    let r = s.run_code(1000, &[], ORG, ORG + code.len() as u16).unwrap();
    assert_eq!(r.reason, StopReason::Halt);
    assert_eq!(r.snapshot.af >> 8, 0x42); // A = $41 + 1
    assert_eq!(r.snapshot.bc & 0xFF, 0x42); // C = A
    assert_eq!(r.steps, 5);
    assert!(r.snapshot.halted);
}

// ── 2. The instruction budget is an exact ceiling and reports the PC. ───────
#[test]
fn budget_is_exact_and_reports_pc() {
    let code = [0x00u8; 300]; // NOP sled
    let mut s = Session::new();
    s.load_code(ORG, &code, ORG).unwrap();
    let r = s.run_code(100, &[], ORG, ORG + code.len() as u16).unwrap();
    assert_eq!(r.reason, StopReason::Budget);
    assert_eq!(r.steps, 100);
    assert_eq!(r.snapshot.pc, ORG + 100);
}

// ── 3. `JP $` (3-byte self-jump) is caught as a terminal loop, not spun. ────
#[test]
fn jp_to_self_is_a_terminal_loop() {
    let code = [0xC3, 0x50, 0x01]; // JP $0150 at $0150
    let mut s = Session::new();
    s.load_code(ORG, &code, ORG).unwrap();
    let r = s.run_code(1_000_000, &[], ORG, ORG + code.len() as u16).unwrap();
    assert_eq!(r.reason, StopReason::SelfLoop);
    assert!(r.steps <= 1, "steps={}", r.steps);
    assert_eq!(r.snapshot.pc, 0x0150);
}

// ── 4. Running off the end of the loaded range stops at LeftRange. ──────────
#[test]
fn falling_off_the_end_stops_at_left_range() {
    let code = [0x00, 0x00]; // two NOPs, no HALT
    let mut s = Session::new();
    s.load_code(ORG, &code, ORG).unwrap();
    let hi = ORG + code.len() as u16;
    let r = s.run_code(1000, &[], ORG, hi).unwrap();
    assert_eq!(r.reason, StopReason::LeftRange);
    assert_eq!(r.snapshot.pc, hi);
    assert_eq!(r.steps, 2);
}

// ── 5. A breakpoint stops, and re-running resumes past it (entry exempt). ───
#[test]
fn breakpoint_stops_then_resumes_past_it() {
    let code = [0x00, 0x00, 0x00, 0x76]; // NOP ; NOP ; NOP ; HALT
    let mut s = Session::new();
    s.load_code(ORG, &code, ORG).unwrap();
    let hi = ORG + code.len() as u16;

    let r1 = s.run_code(1000, &[0x0151], ORG, hi).unwrap();
    assert_eq!(r1.reason, StopReason::Breakpoint);
    assert_eq!(r1.snapshot.pc, 0x0151); // stops BEFORE the bp instruction

    // The instruction at the resume PC (the bp itself) is exempt, so a resume
    // makes progress instead of instantly re-tripping.
    let r2 = s.run_code(1000, &[0x0151], ORG, hi).unwrap();
    assert_eq!(r2.reason, StopReason::Halt);
    assert!(r2.snapshot.halted);
}

// ── 6. load_code fills exactly 32 KiB but rejects one byte past the window. ─
#[test]
fn load_code_fills_but_never_overflows_the_rom_window() {
    let mut s = Session::new();
    assert!(s.load_code(0x0000, &vec![0x00; 0x8000], 0x0000).is_ok());
    assert!(s.load_code(0x7FFE, &[0x00, 0x00, 0x00], 0x7FFE).is_err());
}

// ── 7. The rom_only machine is unbanked: every address reads back verbatim,
//       including the header bytes it must NOT interpret. ────────────────────
#[test]
fn rom_only_reads_back_verbatim_across_both_banks() {
    let mut code = vec![0xFFu8; 0x8000];
    code[0x0000] = 0x11;
    code[0x0147] = 0x19; // cart-type byte — must NOT switch the MBC
    code[0x0148] = 0x08; // rom-size byte — must NOT resize
    code[0x3FFF] = 0x22; // end of bank 0
    code[0x4000] = 0x33; // start of bank 1
    code[0x7FFF] = 0x44; // end of bank 1
    let mut s = Session::new();
    s.load_code(0x0000, &code, 0x0000).unwrap();
    assert_eq!(s.read_memory(0x0000, 1).unwrap(), [0x11]);
    assert_eq!(s.read_memory(0x0147, 1).unwrap(), [0x19]);
    assert_eq!(s.read_memory(0x3FFF, 1).unwrap(), [0x22]);
    assert_eq!(s.read_memory(0x4000, 1).unwrap(), [0x33]);
    assert_eq!(s.read_memory(0x7FFF, 1).unwrap(), [0x44]);
}

// ── 8. A snippet that runs longer than a frame leaves a frame for the UI. ───
//       (Directly guards the "nothing reaches the screen" path: the boot-
//        skipped machine comes up with the LCD on, so >1 frame of cycles must
//        produce a drainable 160×144 frame.)
#[test]
fn a_long_render_loop_completes_a_ppu_frame() {
    // LD BC,$2000 ; loop: DEC BC ; LD A,B ; OR C ; JR NZ,loop ; HALT
    let code = [0x01, 0x00, 0x20, 0x0B, 0x78, 0xB1, 0x20, 0xFB, 0x76];
    let mut s = Session::new();
    s.load_code(ORG, &code, ORG).unwrap();
    let r = s.run_code(1_000_000, &[], ORG, ORG + code.len() as u16).unwrap();
    assert_eq!(r.reason, StopReason::Halt);
    let frame = s
        .drain_frame()
        .expect("a completed frame should be ready after >1 frame of cycles");
    assert_eq!(frame.len(), 160 * 144);
}

// ── 9. The entry point may differ from the load origin. ─────────────────────
#[test]
fn entry_point_can_differ_from_origin() {
    // $0150: HALT (would stop at once) — entry jumps past it to INC A ; HALT.
    let code = [0x76, 0x3C, 0x76];
    let mut s = Session::new();
    s.load_code(ORG, &code, ORG + 1).unwrap(); // entry = $0151
    let r = s.run_code(1000, &[], ORG, ORG + code.len() as u16).unwrap();
    assert_eq!(r.reason, StopReason::Halt);
    assert_eq!(r.snapshot.af >> 8, 0x02); // post-boot A ($01) + 1 → leading HALT skipped
    assert_eq!(r.steps, 2); // INC A, HALT
}

// ── 10. A zero budget executes nothing and leaves the machine untouched. ────
#[test]
fn zero_budget_runs_nothing() {
    let code = [0x3C, 0x76]; // INC A ; HALT
    let mut s = Session::new();
    s.load_code(ORG, &code, ORG).unwrap();
    let r = s.run_code(0, &[], ORG, ORG + code.len() as u16).unwrap();
    assert_eq!(r.reason, StopReason::Budget);
    assert_eq!(r.steps, 0);
    assert_eq!(r.snapshot.pc, ORG); // never advanced
    assert_eq!(r.snapshot.af >> 8, 0x01); // A still the post-boot value
}

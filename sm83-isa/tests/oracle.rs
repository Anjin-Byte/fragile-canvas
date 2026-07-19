//! The emulator as a length oracle.
//!
//! For every opcode whose PC advance is architectural (i.e. everything
//! except taken control flow and halt states), execute it on the real
//! `sm83` core and assert that `pc_after - pc` equals this crate's decoded
//! length. Conditional branches are forced not-taken via the flags, so
//! they participate too. This pins the ISA table to the emulator without
//! any production dependency between the crates.

use sm83::cpu::registers::{Flag, Reg16};
use sm83::session::Session;
use sm83_isa::{decode, instruction_len, Cond, Mnemonic, Operand, ILLEGAL_OPCODES};

const TEST_PC: u16 = 0xC100;

/// Opcodes whose PC advance is not `len` even when conditions fail.
fn excluded(op: u8) -> bool {
    match op {
        0x18 => true,                                     // JR (unconditional)
        0xC3 | 0xE9 => true,                              // JP / JP HL
        0xCD => true,                                     // CALL
        0xC9 | 0xD9 => true,                              // RET / RETI
        0xC7 | 0xCF | 0xD7 | 0xDF | 0xE7 | 0xEF | 0xF7 | 0xFF => true, // RST
        0x76 | 0x10 => true,                              // HALT / STOP
        _ => ILLEGAL_OPCODES.contains(&op),
    }
}

fn session_with_rom() -> Session {
    let rom = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../sm83/roms/tobudx.gb"))
        .expect("tobudx.gb present");
    let mut session = Session::new();
    session.load_rom_no_boot(&rom);
    session
}

/// Prepare deterministic state: instruction bytes at TEST_PC, indirect
/// targets pointed at safe WRAM, interrupts off, condition (if any)
/// forced FALSE so conditional branches fall through.
fn arm(session: &mut Session, bytes: &[u8], cond: Option<Cond>) {
    let gb = session.gameboy_mut().expect("rom loaded");

    for (i, &b) in bytes.iter().enumerate() {
        gb.bus.write(TEST_PC + i as u16, b);
    }

    let rf = &mut gb.cpu.register_file;
    rf.set_16bit(Reg16::PC, TEST_PC);
    rf.set_16bit(Reg16::SP, 0xDFF0);
    rf.set_16bit(Reg16::HL, 0xC800);
    rf.set_16bit(Reg16::BC, 0xC810);
    rf.set_16bit(Reg16::DE, 0xC820);

    match cond {
        Some(Cond::NZ) => rf.set_flag(Flag::Zero),
        Some(Cond::Z) => rf.reset_flag(Flag::Zero),
        Some(Cond::NC) => rf.set_flag(Flag::Carry),
        Some(Cond::C) => rf.reset_flag(Flag::Carry),
        None => {}
    }

    gb.cpu.ime = false;
    gb.cpu.ime_defer = false;
    gb.cpu.halted = false;
}

fn cond_of(op: u8) -> Option<Cond> {
    let d = decode(&[op, 0, 0]).unwrap();
    let cond = d.instr.operands().find_map(|o| match o {
        Operand::Cond(c) => Some(*c),
        _ => None,
    });
    cond
}

#[test]
fn instruction_lengths_match_emulator() {
    let mut session = session_with_rom();
    let mut checked = 0;

    for op in 0u16..=0xFF {
        let op = op as u8;
        if excluded(op) {
            continue;
        }

        // Operand bytes: zeros. Indirect targets resolve to ROM writes
        // (inert MBC registers) or the joypad register — both harmless.
        let len = instruction_len(op);
        let bytes = [op, 0x00, 0x00];
        let bytes = &bytes[..len as usize];

        arm(&mut session, bytes, cond_of(op));
        let trace = session.step_traced().expect("step");

        assert_eq!(
            trace.pc_after.wrapping_sub(trace.pc),
            len as u16,
            "op {op:02X}: emulator advanced PC by {} but decode says len {}",
            trace.pc_after.wrapping_sub(trace.pc),
            len
        );
        checked += 1;
    }

    // 256 - 11 illegal - 16 excluded control flow (JR, JP, JP HL, CALL,
    // RET, RETI, 8×RST, HALT, STOP) = 229
    assert_eq!(checked, 229, "unexpected oracle coverage");
}

#[test]
fn cb_instruction_lengths_match_emulator() {
    let mut session = session_with_rom();

    for cb in 0u16..=0xFF {
        let bytes = [0xCB, cb as u8];
        arm(&mut session, &bytes, None);
        let trace = session.step_traced().expect("step");
        assert_eq!(
            trace.pc_after.wrapping_sub(trace.pc),
            2,
            "CB {cb:02X}: emulator advanced PC by {}",
            trace.pc_after.wrapping_sub(trace.pc)
        );
    }
}

/// Data mnemonic appears exactly for the illegal set — cross-checked with
/// the emulator's own T-cycle table convention (zeros for illegals is a
/// separate concern; here we only pin OUR classification).
#[test]
fn illegal_classification() {
    for op in 0u16..=0xFF {
        let op = op as u8;
        let d = decode(&[op, 0, 0]).unwrap();
        assert_eq!(
            d.instr.mnemonic == Mnemonic::Data,
            ILLEGAL_OPCODES.contains(&op),
            "op {op:02X}"
        );
    }
}

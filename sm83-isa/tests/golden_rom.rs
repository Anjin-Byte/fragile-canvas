//! Golden-ROM roundtrips on real cartridge data (Tobu Tobu Girl DX).
//!
//! Linear disassembly of ROM data produces garbage-but-valid instructions
//! in data regions — which is exactly the point: the text produced for
//! EVERY byte sequence must reassemble to the identical bytes.

use sm83_isa::asm::assemble;
use sm83_isa::{decode, encode, format_simple, parse_instruction};

fn rom() -> Vec<u8> {
    std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../sm83/roms/tobudx.gb"))
        .expect("tobudx.gb present")
}

/// Whole ROM through decode → format → parse → encode, byte-identical.
#[test]
fn whole_rom_line_roundtrip() {
    let rom = rom();
    let mut out = Vec::with_capacity(rom.len());
    let mut offset = 0;
    while offset < rom.len() {
        let d = decode(&rom[offset..]).unwrap();
        let text = format_simple(&d.instr);
        let reparsed = parse_instruction(&text)
            .unwrap_or_else(|e| panic!("offset {offset:#X}: '{text}': {e}"));
        let enc = encode(&reparsed)
            .unwrap_or_else(|e| panic!("offset {offset:#X}: '{text}': {e}"));
        out.extend_from_slice(enc.as_slice());
        offset += d.len as usize;
    }
    assert_eq!(out.len(), rom.len());
    assert_eq!(out, rom, "reassembled ROM differs");
}

/// Bank 0 as a full assembler source file (ORG + one instruction per
/// line) reassembles byte-identically through the two-pass path.
#[test]
fn bank0_through_full_assembler() {
    let rom = rom();
    let bank0 = &rom[..0x4000.min(rom.len())];

    let mut src = String::from("ORG $0000\n");
    let mut offset = 0;
    while offset < bank0.len() {
        let d = decode(&bank0[offset..]).unwrap();
        // Truncation can't happen inside a bank except at the very end.
        src.push_str("    ");
        src.push_str(&format_simple(&d.instr));
        src.push('\n');
        offset += d.len as usize;
    }

    let a = assemble(&src);
    assert!(a.ok(), "diagnostics: {:?}", &a.diagnostics[..a.diagnostics.len().min(5)]);
    let (origin, bytes) = a.flatten(0x00).unwrap();
    assert_eq!(origin, 0);
    assert_eq!(bytes.len(), bank0.len());
    assert_eq!(bytes, bank0, "bank 0 reassembly differs");
}

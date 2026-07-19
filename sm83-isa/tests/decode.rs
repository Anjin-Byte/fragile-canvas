//! Decode/format/cycles properties over the whole opcode space.

use sm83_isa::{
    cycles, decode, format_simple, instruction_len, Cycles, Mnemonic, ILLEGAL_OPCODES,
};

/// Every base opcode decodes, its length matches `instruction_len`, and
/// only the 11 illegal opcodes produce `Data`.
#[test]
fn all_base_opcodes_decode() {
    for op in 0u16..=0xFF {
        let op = op as u8;
        let bytes = [op, 0x00, 0x00];
        let d = decode(&bytes).unwrap();
        let expected_len = instruction_len(op);

        if ILLEGAL_OPCODES.contains(&op) {
            assert_eq!(d.instr.mnemonic, Mnemonic::Data, "op {op:02X} should be Data");
            assert_eq!(d.len, 1);
        } else {
            assert_ne!(d.instr.mnemonic, Mnemonic::Data, "op {op:02X} decoded as Data");
            assert_eq!(d.len, expected_len, "op {op:02X} length mismatch");
        }
    }
}

/// Every CB opcode decodes to a CB-class mnemonic with length 2.
#[test]
fn all_cb_opcodes_decode() {
    use Mnemonic::*;
    for cb in 0u16..=0xFF {
        let bytes = [0xCB, cb as u8];
        let d = decode(&bytes).unwrap();
        assert_eq!(d.len, 2, "CB {cb:02X}");
        assert!(
            matches!(d.instr.mnemonic, Rlc | Rrc | Rl | Rr | Sla | Sra | Swap | Srl | Bit | Res | Set),
            "CB {cb:02X} decoded to {:?}",
            d.instr.mnemonic
        );
    }
}

/// Truncated multi-byte instructions at a buffer end fall back to Data.
#[test]
fn truncation_falls_back_to_data() {
    // LD BC, nn (3 bytes) with only 1 and 2 bytes available.
    for bytes in [&[0x01u8][..], &[0x01, 0x34][..]] {
        let d = decode(bytes).unwrap();
        assert_eq!(d.instr.mnemonic, Mnemonic::Data);
        assert_eq!(d.len, 1);
    }
    // CB with no second byte.
    let d = decode(&[0xCB]).unwrap();
    assert_eq!(d.instr.mnemonic, Mnemonic::Data);
    assert_eq!(d.len, 1);
    // Empty input.
    assert!(decode(&[]).is_none());
}

/// Canonical formatting snapshots for representative encodings.
#[test]
fn format_snapshots() {
    let cases: &[(&[u8], &str)] = &[
        (&[0x00], "NOP"),
        (&[0x10, 0x00], "STOP"),
        (&[0x76], "HALT"),
        (&[0x01, 0x34, 0x12], "LD BC, $1234"),
        (&[0x08, 0x00, 0xC0], "LD ($C000), SP"),
        (&[0x2A], "LD A, (HL+)"),
        (&[0x32], "LD (HL-), A"),
        (&[0x36, 0x7F], "LD (HL), $7F"),
        (&[0x18, 0xFE], "JR @-$02"),
        (&[0x20, 0x05], "JR NZ, @+$05"),
        (&[0xC3, 0x50, 0x01], "JP $0150"),
        (&[0xE9], "JP HL"),
        (&[0xC4, 0x00, 0x20], "CALL NZ, $2000"),
        (&[0xC9], "RET"),
        (&[0xD9], "RETI"),
        (&[0xD8], "RET C"),
        (&[0xE0, 0x44], "LDH ($FF44), A"),
        (&[0xF0, 0x00], "LDH A, ($FF00)"),
        (&[0xE2], "LDH (C), A"),
        (&[0xF2], "LDH A, (C)"),
        (&[0xEA, 0x00, 0x80], "LD ($8000), A"),
        (&[0xFA, 0x44, 0xFF], "LD A, ($FF44)"),
        (&[0xE8, 0xFB], "ADD SP, -$05"),
        (&[0xE8, 0x05], "ADD SP, $05"),
        (&[0xF8, 0xFE], "LD HL, SP-$02"),
        (&[0xF8, 0x02], "LD HL, SP+$02"),
        (&[0xF9], "LD SP, HL"),
        (&[0x80], "ADD A, B"),
        (&[0x8E], "ADC A, (HL)"),
        (&[0x97], "SUB A"),
        (&[0xA1], "AND C"),
        (&[0xEE, 0x0F], "XOR $0F"),
        (&[0xFE, 0x90], "CP $90"),
        (&[0x09], "ADD HL, BC"),
        (&[0x33], "INC SP"),
        (&[0x35], "DEC (HL)"),
        (&[0xC5], "PUSH BC"),
        (&[0xF1], "POP AF"),
        (&[0xC7], "RST $00"),
        (&[0xFF], "RST $38"),
        (&[0xF3], "DI"),
        (&[0xFB], "EI"),
        (&[0x27], "DAA"),
        (&[0x37], "SCF"),
        (&[0xCB, 0x37], "SWAP A"),
        (&[0xCB, 0x7E], "BIT 7, (HL)"),
        (&[0xCB, 0x86], "RES 0, (HL)"),
        (&[0xCB, 0xDA], "SET 3, D"),
        (&[0xCB, 0x3F], "SRL A"),
        (&[0xD3], "DB $D3"),
    ];

    for (bytes, expected) in cases {
        let d = decode(bytes).unwrap();
        assert_eq!(&format_simple(&d.instr), expected, "bytes {bytes:02X?}");
        assert_eq!(d.len as usize, bytes.len(), "len for {bytes:02X?}");
    }
}

/// JR renders an absolute target when the instruction address is known.
#[test]
fn jr_absolute_target_with_addr() {
    use sm83_isa::{format_instruction, FormatOptions};
    // JR -2 at 0x0150 → target 0x0150 (self-loop)
    let d = decode(&[0x18, 0xFE]).unwrap();
    let text = format_instruction(&d.instr, &FormatOptions { addr: Some(0x0150), symbols: None });
    assert_eq!(text, "JR $0150");
    // JR NZ, +5 at 0x0200 → 0x0207
    let d = decode(&[0x20, 0x05]).unwrap();
    let text = format_instruction(&d.instr, &FormatOptions { addr: Some(0x0200), symbols: None });
    assert_eq!(text, "JR NZ, $0207");
}

/// Cycle metadata spot checks against Pandocs.
#[test]
fn cycles_match_pandocs() {
    let both = |t| Cycles { taken: t, not_taken: t };
    let cases: &[(u8, Option<u8>, Cycles)] = &[
        (0x00, None, both(4)),                              // NOP
        (0x01, None, both(12)),                             // LD BC,nn
        (0x08, None, both(20)),                             // LD (a16),SP
        (0x09, None, both(8)),                              // ADD HL,BC
        (0x18, None, both(12)),                             // JR
        (0x20, None, Cycles { taken: 12, not_taken: 8 }),   // JR NZ
        (0x34, None, both(12)),                             // INC (HL)
        (0x36, None, both(12)),                             // LD (HL),n
        (0x46, None, both(8)),                              // LD B,(HL)
        (0x76, None, both(4)),                              // HALT
        (0x86, None, both(8)),                              // ADD A,(HL)
        (0xC0, None, Cycles { taken: 20, not_taken: 8 }),   // RET NZ
        (0xC1, None, both(12)),                             // POP BC
        (0xC3, None, both(16)),                             // JP
        (0xC2, None, Cycles { taken: 16, not_taken: 12 }),  // JP NZ
        (0xC5, None, both(16)),                             // PUSH BC
        (0xC9, None, both(16)),                             // RET
        (0xCD, None, both(24)),                             // CALL
        (0xC4, None, Cycles { taken: 24, not_taken: 12 }),  // CALL NZ
        (0xC7, None, both(16)),                             // RST
        (0xE0, None, both(12)),                             // LDH (a8),A
        (0xE2, None, both(8)),                              // LDH (C),A
        (0xE8, None, both(16)),                             // ADD SP,e8
        (0xE9, None, both(4)),                              // JP HL
        (0xEA, None, both(16)),                             // LD (a16),A
        (0xF8, None, both(12)),                             // LD HL,SP+e8
        (0xF9, None, both(8)),                              // LD SP,HL
        (0xCB, Some(0x37), both(8)),                        // SWAP A
        (0xCB, Some(0x36), both(16)),                       // SWAP (HL)
        (0xCB, Some(0x7E), both(12)),                       // BIT 7,(HL)
        (0xCB, Some(0xC6), both(16)),                       // SET 0,(HL)
        (0xD3, None, both(0)),                              // illegal
    ];
    for &(op, cb, expected) in cases {
        assert_eq!(cycles(op, cb), expected, "op {op:02X} cb {cb:02X?}");
    }
}

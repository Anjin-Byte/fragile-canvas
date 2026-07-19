//! The crate's central contract: decode/encode and format/parse are true
//! inverses, proven by enumerating the entire opcode space and by fuzzing
//! byte streams end-to-end (bytes → text → bytes).

use sm83_isa::{decode, encode, format_simple, instruction_len, parse_instruction};

/// Operand byte patterns covering boundary values.
const PATTERNS: [[u8; 2]; 5] = [
    [0x00, 0x00],
    [0x7F, 0x80],
    [0x80, 0x7F],
    [0xFF, 0xFF],
    [0x34, 0x12],
];

/// encode(decode(bytes)) == bytes for every base opcode × operand pattern.
#[test]
fn byte_roundtrip_base() {
    for op in 0u16..=0xFF {
        let op = op as u8;
        if op == 0xCB {
            continue; // covered by byte_roundtrip_cb
        }
        for pat in PATTERNS {
            let full = [op, pat[0], pat[1]];
            let bytes = &full[..instruction_len(op) as usize];
            let d = decode(bytes).unwrap();
            let e = encode(&d.instr).unwrap_or_else(|err| {
                panic!("op {op:02X} {pat:02X?} decoded to {:?} but won't encode: {err}", d.instr)
            });
            assert_eq!(e.as_slice(), bytes, "op {op:02X} {pat:02X?} → {:?}", d.instr);
        }
    }
}

/// encode(decode([CB xx])) == [CB xx] for every CB opcode.
#[test]
fn byte_roundtrip_cb() {
    for cb in 0u16..=0xFF {
        let bytes = [0xCB, cb as u8];
        let d = decode(&bytes).unwrap();
        let e = encode(&d.instr).unwrap();
        assert_eq!(e.as_slice(), &bytes, "CB {cb:02X} → {:?}", d.instr);
    }
}

/// parse(format(instr)) == instr across the whole opcode space.
#[test]
fn text_roundtrip() {
    for op in 0u16..=0xFF {
        let op = op as u8;
        for pat in PATTERNS {
            let full = [op, pat[0], pat[1]];
            let bytes = &full[..instruction_len(op) as usize];
            let d = decode(bytes).unwrap();
            let text = format_simple(&d.instr);
            let reparsed = parse_instruction(&text).unwrap_or_else(|e| {
                panic!("op {op:02X} {pat:02X?}: '{text}' failed to parse: {e}")
            });
            assert_eq!(reparsed, d.instr, "op {op:02X} {pat:02X?} via '{text}'");
        }
    }
}

/// Alternative surface syntax parses to the same instructions.
#[test]
fn parser_accepts_variants() {
    let same = |a: &str, b: &str| {
        let pa = parse_instruction(a).unwrap_or_else(|e| panic!("'{a}': {e}"));
        let pb = parse_instruction(b).unwrap_or_else(|e| panic!("'{b}': {e}"));
        assert_eq!(pa, pb, "'{a}' vs '{b}'");
    };

    same("ld a, [hl+]", "LD A, (HL+)");
    same("LD A, (HLI)", "LD A, (HL+)");
    same("ld [hld], a", "LD (HL-), A");
    same("sub a, b", "SUB B");
    same("add b", "ADD A, B");
    same("adc $10", "ADC A, $10");
    same("cp a, $90", "CP $90");
    same("ldh [$FF00+$44], a", "LDH ($FF44), A");
    same("ldh ($44), a", "LDH ($FF44), A");
    same("ldh a, [$ff00+c]", "LDH A, (C)");
    same("jp 0x0150", "JP $0150");
    same("jr c, @-2", "JR C, @-$02");
    same("bit 7, [hl]", "BIT 7, (HL)");
    same("xor 255", "XOR $FF");
    same("ld hl, sp + $08", "LD HL, SP+$08");
    same("stop", "STOP $00");
}

/// Distinguishes reg C from cond C by grammar position.
#[test]
fn c_register_vs_condition() {
    use sm83_isa::{Cond, Mnemonic, Operand, Reg8};
    let jr = parse_instruction("JR C, @+$04").unwrap();
    assert_eq!(jr.ops[0], Some(Operand::Cond(Cond::C)));
    let ret = parse_instruction("RET C").unwrap();
    assert_eq!(ret.ops[0], Some(Operand::Cond(Cond::C)));
    let ld = parse_instruction("LD C, $12").unwrap();
    assert_eq!(ld.mnemonic, Mnemonic::Ld);
    assert_eq!(ld.ops[0], Some(Operand::R8(Reg8::C)));
    let inc = parse_instruction("INC C").unwrap();
    assert_eq!(inc.ops[0], Some(Operand::R8(Reg8::C)));
}

/// Invalid inputs produce errors, not panics or silent nonsense.
#[test]
fn parser_rejects_invalid() {
    for bad in [
        "FOO A, B",
        "LD (HL), (HL)",
        "LD AF, $1234",
        "PUSH SP",
        "POP SP",
        "BIT 8, A",
        "RST $17",
        "ADD SP, $200",
        "LDH ($C000), A",
        "JR $0150",
        "LD A, B, C",
    ] {
        let parsed = parse_instruction(bad).and_then(|i| {
            encode(&i).map_err(|e| sm83_isa::ParseError { msg: e.to_string() })
        });
        assert!(parsed.is_err(), "'{bad}' should not assemble, got {parsed:?}");
    }
}

/// Fuzz: random byte streams survive bytes → text → bytes intact.
#[test]
fn stream_roundtrip_fuzz() {
    let mut seed = 0x5eed_cafeu32;
    let mut rand = || {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (seed >> 24) as u8
    };

    for round in 0..64 {
        let stream: Vec<u8> = (0..512).map(|_| rand()).collect();

        // Disassemble linearly to text.
        let mut lines = Vec::new();
        let mut offset = 0;
        while offset < stream.len() {
            let d = decode(&stream[offset..]).unwrap();
            lines.push(format_simple(&d.instr));
            offset += d.len as usize;
        }

        // Reassemble and compare.
        let mut out = Vec::new();
        for line in &lines {
            let instr = parse_instruction(line)
                .unwrap_or_else(|e| panic!("round {round}: '{line}': {e}"));
            let enc = encode(&instr)
                .unwrap_or_else(|e| panic!("round {round}: '{line}': {e}"));
            out.extend_from_slice(enc.as_slice());
        }
        assert_eq!(out, stream, "round {round}");
    }
}

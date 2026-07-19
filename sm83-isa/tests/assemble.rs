//! Two-pass assembler behavior: labels, forward references, EQU, ORG
//! segments, data directives, expressions, and diagnostics.

use sm83_isa::asm::assemble;

fn bytes_of(src: &str) -> Vec<u8> {
    let a = assemble(src);
    assert!(a.ok(), "diagnostics: {:?}", a.diagnostics);
    a.flatten(0x00).expect("emitted something").1
}

fn errors_of(src: &str) -> Vec<String> {
    assemble(src).diagnostics.into_iter().map(|d| d.msg).collect()
}

#[test]
fn basic_program_with_forward_reference() {
    let src = "
        ORG $150
start:  LD A, $01
        LDH ($44), A
loop:   CALL helper      ; forward reference
        JR loop
helper: XOR A
        RET
    ";
    let bytes = bytes_of(src);
    assert_eq!(
        bytes,
        vec![
            0x3E, 0x01,        // $0150 LD A, $01
            0xE0, 0x44,        // $0152 LDH ($44), A
            0xCD, 0x59, 0x01,  // $0154 CALL $0159 (helper)
            0x18, 0xFB,        // $0157 JR loop ($0154, disp -5)
            0xAF,              // $0159 XOR A
            0xC9,              // $015A RET
        ]
    );
}

#[test]
fn jr_displacement_math() {
    // Self-loop: JR to its own address = disp -2.
    let bytes = bytes_of("ORG $200\nspin: JR spin\n");
    assert_eq!(bytes, vec![0x18, 0xFE]);

    // Forward: JR over a NOP.
    let bytes = bytes_of("ORG 0\n JR skip\n NOP\nskip: NOP\n");
    assert_eq!(bytes, vec![0x18, 0x01, 0x00, 0x00]);
}

#[test]
fn jr_out_of_range_is_reported() {
    let errs = errors_of("ORG 0\n JR far\n DS 300\nfar: NOP\n");
    assert!(
        errs.iter().any(|e| e.contains("JR target out of range")),
        "{errs:?}"
    );
}

#[test]
fn equ_and_expressions() {
    let src = "
LCDC    EQU $FF40
BASE    = $C000
        LD A, LOW(BASE + 5)
        LD B, HIGH(BASE + 5)
        LDH (LCDC), A
        LD HL, BASE + 2 * 8
        DB %1010 | %0101, 1 + 2 * 3, LOW($1234)
    ";
    let bytes = bytes_of(src);
    assert_eq!(
        bytes,
        vec![
            0x3E, 0x05,        // LOW($C005)
            0x06, 0xC0,        // HIGH($C005)
            0xE0, 0x40,        // LDH ($FF40) → a8 = $40
            0x21, 0x10, 0xC0,  // LD HL, $C010
            0x0F, 0x07, 0x34,  // DB items
        ]
    );
}

#[test]
fn org_segments_and_flatten_fill() {
    let src = "
        ORG $00
        DB $AA
        ORG $04
        DB $BB
    ";
    let a = assemble(src);
    assert!(a.ok(), "{:?}", a.diagnostics);
    assert_eq!(a.segments.len(), 2);
    let (origin, bytes) = a.flatten(0xFF).unwrap();
    assert_eq!(origin, 0);
    assert_eq!(bytes, vec![0xAA, 0xFF, 0xFF, 0xFF, 0xBB]);
}

#[test]
fn db_strings_and_dw() {
    let bytes = bytes_of("ORG 0\n DB \"GB!\", 0\n DW $1234, start\nstart: NOP\n");
    assert_eq!(
        bytes,
        vec![b'G', b'B', b'!', 0x00, 0x34, 0x12, 0x08, 0x00, 0x00]
    );
}

#[test]
fn ds_reserves_with_fill() {
    let bytes = bytes_of("ORG 0\n DB $11\n DS 3, $EE\n DB $22\n");
    assert_eq!(bytes, vec![0x11, 0xEE, 0xEE, 0xEE, 0x22]);
}

#[test]
fn current_address_symbol() {
    // DW @ stores the directive's own address.
    let bytes = bytes_of("ORG $1234\n DW @\n");
    assert_eq!(bytes, vec![0x34, 0x12]);
}

#[test]
fn label_on_org_line_binds_to_new_address() {
    let a = assemble("here: ORG $4000\n NOP\n");
    assert!(a.ok(), "{:?}", a.diagnostics);
    assert_eq!(a.symbols.get("here"), Some(&0x4000));
}

#[test]
fn diagnostics_carry_line_numbers_and_continue() {
    let src = "
        NOP
        FOO A, B
        LD AF, $1234
        BIT 9, A
dup:    NOP
dup:    NOP
        JP undefined_label
    ";
    let a = assemble(src);
    let lines: Vec<usize> = a.diagnostics.iter().map(|d| d.line).collect();
    assert_eq!(a.diagnostics.len(), 5, "{:?}", a.diagnostics);
    assert_eq!(lines, vec![3, 4, 5, 7, 8]);
    assert!(a.diagnostics[0].msg.contains("unknown mnemonic"));
    assert!(a.diagnostics[1].msg.contains("LD AF"));
    assert!(a.diagnostics[2].msg.contains("bit index"));
    assert!(a.diagnostics[3].msg.contains("duplicate symbol"));
    assert!(a.diagnostics[4].msg.contains("undefined symbol"));
}

#[test]
fn equ_cannot_forward_reference() {
    let errs = errors_of("X EQU Y + 1\nY EQU 2\n");
    assert!(errs.iter().any(|e| e.contains("forward-reference")), "{errs:?}");
}

#[test]
fn overlap_detection() {
    let a = assemble("ORG $10\n DS 8\n ORG $12\n DB 1\n");
    assert!(
        a.diagnostics.iter().any(|d| d.msg.contains("overlaps")),
        "{:?}",
        a.diagnostics
    );
}

#[test]
fn comments_and_case_insensitivity() {
    // Mnemonics/directives/registers are case-insensitive; LABELS are
    // case-sensitive (RGBDS/ca65 convention).
    let bytes = bytes_of("org 0 ; set origin\nstart: nOp ; comment ; nested\n jr start\n");
    assert_eq!(bytes, vec![0x00, 0x18, 0xFD]);

    let a = assemble("StArT: NOP\n JR start\n");
    assert!(
        a.diagnostics.iter().any(|d| d.msg.contains("undefined symbol")),
        "labels must be case-sensitive: {:?}",
        a.diagnostics
    );
}

#[test]
fn semicolon_inside_db_string_is_not_a_comment() {
    let bytes = bytes_of("ORG 0\n DB \"a;b\"\n");
    assert_eq!(bytes, vec![b'a', b';', b'b']);
}

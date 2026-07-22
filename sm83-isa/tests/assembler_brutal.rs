//! Brutal integration tests for the two-pass assembler.
//!
//! These deliberately target the sharp edges the happy-path suite in
//! `assemble.rs` doesn't: exact JR displacement boundaries, signed/unsigned
//! immediate folding at both widths, restart/bit-op validation, the
//! SP-relative and 16-bit-SP-store forms, expression precedence and
//! associativity, the source map, location-counter overflow, bare stacked
//! labels, whitespace/CRLF tolerance, and STOP's two-byte width.

use sm83_isa::asm::{assemble, Assembled, SrcSpan};

fn asm(src: &str) -> Assembled {
    assemble(src)
}

fn bytes_of(src: &str) -> Vec<u8> {
    let a = assemble(src);
    assert!(a.ok(), "unexpected diagnostics for {src:?}: {:?}", a.diagnostics);
    a.flatten(0x00).expect("emitted nothing").1
}

fn has_err(src: &str, needle: &str) -> bool {
    assemble(src).diagnostics.iter().any(|d| d.msg.contains(needle))
}

// ── 1. JR displacement is measured from the *following* byte, and the
//       ±range is inclusive at exactly +127 / -128. ─────────────────────────
#[test]
fn jr_hits_exact_displacement_boundaries() {
    // +127 is the largest forward jump (disp = target - (jr_addr + 2)).
    let a = asm("ORG 0\n JR far\n DS 127\nfar: NOP\n");
    assert!(a.ok(), "+127 JR should assemble: {:?}", a.diagnostics);
    let (_, b) = a.flatten(0x00).unwrap();
    assert_eq!(&b[..2], &[0x18, 0x7F], "forward disp should be +127");

    // One byte further overflows.
    assert!(has_err("ORG 0\n JR far\n DS 128\nfar: NOP\n", "JR target out of range"));

    // -128 is the largest backward jump.
    let a = asm("ORG 0\nback: NOP\n DS 125\n JR back\n");
    assert!(a.ok(), "-128 JR should assemble: {:?}", a.diagnostics);
    let (_, b) = a.flatten(0x00).unwrap();
    assert_eq!(&b[b.len() - 2..], &[0x18, 0x80], "backward disp should be -128");

    // One byte further overflows.
    assert!(has_err("ORG 0\nback: NOP\n DS 126\n JR back\n", "JR target out of range"));
}

// ── 2. Immediates fold two's-complement negatives the SAME way at 8 and 16
//       bits; over-range values are rejected, never silently truncated. ─────
#[test]
fn immediate_range_and_sign_across_widths() {
    // 8-bit: unsigned and signed both land.
    assert_eq!(bytes_of("LD A, $FF\n"), vec![0x3E, 0xFF]);
    assert_eq!(bytes_of("LD A, -1\n"), vec![0x3E, 0xFF]); // -1 → $FF
    assert_eq!(bytes_of("LD A, -128\n"), vec![0x3E, 0x80]);
    assert!(has_err("LD A, $100\n", "8-bit"));
    assert!(has_err("LD A, -129\n", "8-bit"));

    // 16-bit: a negative immediate must fold the same way, not be rejected.
    assert_eq!(bytes_of("LD BC, $1234\n"), vec![0x01, 0x34, 0x12]);
    assert_eq!(bytes_of("LD BC, -1\n"), vec![0x01, 0xFF, 0xFF]); // -1 → $FFFF
    assert_eq!(bytes_of("LD SP, -2\n"), vec![0x31, 0xFE, 0xFF]); // -2 → $FFFE
    assert_eq!(bytes_of("LD HL, -32768\n"), vec![0x21, 0x00, 0x80]); // i16::MIN
    assert!(has_err("LD BC, $10000\n", "out of range"));
    assert!(has_err("LD BC, -32769\n", "16-bit"));
}

// ── 3. RST only accepts the eight vectors; CB-prefixed bit ops enforce the
//       0-7 index and put the register in the low nibble. ──────────────────
#[test]
fn rst_bit_and_cb_encodings() {
    assert_eq!(bytes_of("RST $00\n"), vec![0xC7]);
    assert_eq!(bytes_of("RST $28\n"), vec![0xEF]);
    assert_eq!(bytes_of("RST $38\n"), vec![0xFF]);
    assert!(has_err("RST $05\n", "RST target")); // not a multiple of 8
    assert!(has_err("RST $40\n", "RST target")); // past $38

    assert_eq!(bytes_of("BIT 7, A\n"), vec![0xCB, 0x7F]);
    assert_eq!(bytes_of("SET 0, B\n"), vec![0xCB, 0xC0]);
    assert_eq!(bytes_of("RES 7, (HL)\n"), vec![0xCB, 0xBE]);
    assert_eq!(bytes_of("SWAP A\n"), vec![0xCB, 0x37]);
    assert!(has_err("BIT 8, A\n", "bit index"));
}

// ── 4. The stack surface: the lone 16-bit SP store, SP-relative load/add
//       (both signed 8-bit), push/pop, and out-of-range rejection. ─────────
#[test]
fn stack_and_sp_relative_forms() {
    assert_eq!(bytes_of("LD ($C000), SP\n"), vec![0x08, 0x00, 0xC0]);
    assert_eq!(bytes_of("LD HL, SP+2\n"), vec![0xF8, 0x02]);
    assert_eq!(bytes_of("LD HL, SP-1\n"), vec![0xF8, 0xFF]);
    assert_eq!(bytes_of("ADD SP, -1\n"), vec![0xE8, 0xFF]);
    assert_eq!(bytes_of("ADD SP, 16\n"), vec![0xE8, 0x10]);
    assert_eq!(bytes_of("PUSH BC\n POP HL\n"), vec![0xC5, 0xE1]);
    assert!(has_err("ADD SP, 200\n", "range"));
    assert!(has_err("LD HL, SP+200\n", "range"));
}

// ── 5. Expression precedence, associativity, unary ops, and safe div-by-0. ─
#[test]
fn expression_operators_and_precedence() {
    assert_eq!(bytes_of("DB ~0\n"), vec![0xFF]); // ~0 = -1 → $FF
    assert_eq!(bytes_of("DB -5\n"), vec![0xFB]);
    assert_eq!(bytes_of("DB 1 << 4\n"), vec![0x10]);
    assert_eq!(bytes_of("DB $FF00 >> 8\n"), vec![0xFF]);
    assert_eq!(bytes_of("DB %1111 & %1010\n"), vec![0x0A]);
    assert_eq!(bytes_of("DB 7 ^ 5\n"), vec![0x02]);
    assert_eq!(bytes_of("DB 2 * 3 + 4\n"), vec![0x0A]); // * binds tighter than +
    assert_eq!(bytes_of("DB (2 + 3) * 2\n"), vec![0x0A]); // parens override
    assert_eq!(bytes_of("DB 8 / 2 / 2\n"), vec![0x02]); // left-associative
    assert_eq!(bytes_of("DB 17 % 5\n"), vec![0x02]);
    assert!(has_err("DB 1 / 0\n", "division by zero")); // diagnostic, not a panic
}

// ── 6. The source map records one span per emitting line (line/addr/len),
//       and nothing for ORG or comment-only lines. ─────────────────────────
#[test]
fn source_map_spans_track_every_emitting_line() {
    let src = "\
ORG $100
start: LD HL, $C000
LD A, $42
; comment only — emits nothing
LD (HL), A
data: DB $01, $02, $03
DW $ABCD
DS 4
";
    let a = asm(src);
    assert!(a.ok(), "{:?}", a.diagnostics);
    assert_eq!(
        a.map,
        vec![
            SrcSpan { line: 2, addr: 0x100, len: 3 }, // LD HL, $C000
            SrcSpan { line: 3, addr: 0x103, len: 2 }, // LD A, $42
            SrcSpan { line: 5, addr: 0x105, len: 1 }, // LD (HL), A
            SrcSpan { line: 6, addr: 0x106, len: 3 }, // DB three bytes
            SrcSpan { line: 7, addr: 0x109, len: 2 }, // DW one word
            SrcSpan { line: 8, addr: 0x10B, len: 4 }, // DS four bytes
        ],
    );
    assert!(a.map.iter().all(|s| s.line != 1 && s.line != 4), "no span for ORG/comment");
    assert_eq!(a.symbols.get("start"), Some(&0x100));
    assert_eq!(a.symbols.get("data"), Some(&0x106));
}

// ── 7. The location counter refuses to silently wrap past $FFFF. ───────────
#[test]
fn location_counter_overflow_is_diagnosed() {
    // A three-byte instruction based at $FFFE runs the counter to $10001.
    assert!(has_err("ORG $FFFE\n LD HL, $1234\n", "past $FFFF"));
    // Exactly filling the final byte is legal.
    let a = asm("ORG $FFFF\n NOP\n");
    assert!(a.ok(), "{:?}", a.diagnostics);
    assert_eq!(a.flatten(0x00).unwrap(), (0xFFFF, vec![0x00]));
    // ORG past the address space is rejected up front.
    assert!(has_err("ORG $10000\n", "ORG out of range"));
}

// ── 8. Bare labels (label-only lines) stack and bind to the next emitting
//       address; a trailing label binds to the end address. ────────────────
#[test]
fn stacked_labels_bind_to_next_emitting_line() {
    let a = asm("ORG $200\nalpha:\nbeta:\n NOP\n");
    assert!(a.ok(), "{:?}", a.diagnostics);
    assert_eq!(a.symbols.get("alpha"), Some(&0x200));
    assert_eq!(a.symbols.get("beta"), Some(&0x200));
    assert_eq!(a.flatten(0x00).unwrap(), (0x200, vec![0x00]));

    let a = asm("ORG $10\n DB $AA\ntail:\n");
    assert!(a.ok(), "{:?}", a.diagnostics);
    assert_eq!(a.symbols.get("tail"), Some(&0x11));
}

// ── 9. Tabs, CRLF line endings, and blank lines are all tolerated, and
//       diagnostics still report the correct physical line. ────────────────
#[test]
fn tabs_crlf_and_blank_lines() {
    let src = "\tORG\t0\r\n\tLD\tA,\t$05\r\n\r\n\tNOP\r\n";
    let a = asm(src);
    assert!(a.ok(), "{:?}", a.diagnostics);
    assert_eq!(a.flatten(0x00).unwrap().1, vec![0x3E, 0x05, 0x00]);

    let src = "\tORG 0\r\n\tBOGUS\r\n\r\n\tNOP\r\n";
    let a = asm(src);
    assert_eq!(a.diagnostics.len(), 1, "{:?}", a.diagnostics);
    assert_eq!(a.diagnostics[0].line, 2);
}

// ── 10. STOP assembles to the canonical two-byte form, so it shifts the
//        layout of everything after it — unlike single-byte HALT. ──────────
#[test]
fn stop_occupies_two_bytes_and_shifts_layout() {
    assert_eq!(bytes_of("STOP\n"), vec![0x10, 0x00]);
    assert_eq!(bytes_of("STOP $00\n"), vec![0x10, 0x00]);

    let a = asm("ORG 0\n STOP\nafter: NOP\n");
    assert!(a.ok(), "{:?}", a.diagnostics);
    assert_eq!(a.symbols.get("after"), Some(&0x0002), "STOP is two bytes wide");

    let a = asm("ORG 0\n HALT\nafter: NOP\n");
    assert_eq!(a.symbols.get("after"), Some(&0x0001), "HALT is one byte wide");

    assert_eq!(bytes_of("DI\n EI\n RETI\n HALT\n"), vec![0xF3, 0xFB, 0xD9, 0x76]);
}

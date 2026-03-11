#[derive(Debug, Clone, Copy)]
pub struct ConditionCodes {
    pub z: bool,
    pub n: bool,
    pub h: bool,
    pub c: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct AluResult {
    pub value: u8,
    pub code: ConditionCodes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AluOpKind {
    Add,
    Adc,
    Sub,
    Sbc,
    Xor,
    And,
    Or,
    Cp,
}

/*
This function is (potentially) being underutilized by 
microcode logic. If it is, I suspect that a lot of 
apparent complexity in microcode could be offloaded
more effectively here. Food for thought.
*/
pub fn alu(op: AluOpKind, a: u8, b: u8, carry_in: bool) -> AluResult {
    match op {
        AluOpKind::Add => {
            let (value, carry) = a.overflowing_add(b);
            let half = ((a & 0xF) + (b & 0xF)) > 0xF;

            AluResult {
                value,
                code: ConditionCodes {
                    z: value == 0,
                    n: false,
                    h: half,
                    c: carry,
                },
            }
        }
        AluOpKind::Adc => {
            let c = carry_in as u8;
            let full = a as u16 + b as u16 + c as u16;
            let value = full as u8;
            let half = ((a & 0xF) + (b & 0xF) + c) > 0xF;

            AluResult {
                value,
                code: ConditionCodes {
                    z: value == 0,
                    n: false,
                    h: half,
                    c: full > 0xFF,
                },
            }
        }
        AluOpKind::Sub | AluOpKind::Cp => {
            let (value, carry) = a.overflowing_sub(b);
            let half = (a & 0xF) < (b & 0xF);

            AluResult {
                value,
                code: ConditionCodes {
                    z: value == 0,
                    n: true,
                    h: half,
                    c: carry,
                }
            }
        }
        AluOpKind::Sbc => {
            let c = carry_in as u8;
            let full = (a as u16).wrapping_sub(b as u16).wrapping_sub(c as u16);
            let value = full as u8;
            let half = (a & 0xF) < (b & 0xF) + c;

            AluResult {
                value,
                code: ConditionCodes {
                    z: value == 0,
                    n: true,
                    h: half,
                    c: full > 0xFF,
                }
            }
        }
        AluOpKind::And => {
            let value = a & b;

            AluResult {
                value,
                code: ConditionCodes {
                    z: value == 0,
                    n: false,
                    h: true,
                    c: false,
                }
            }
        }
        AluOpKind::Or => {
            let value = a | b;

            AluResult {
                value,
                code: ConditionCodes {
                    z: value == 0,
                    n: false,
                    h: false,
                    c: false,
                }
            }
        }
        AluOpKind::Xor => {
            let value = a ^ b;

            AluResult {
                value,
                code: ConditionCodes {
                    z: value == 0,
                    n: false,
                    h: false,
                    c: false,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flags(r: &AluResult) -> (bool, bool, bool, bool) {
        (r.code.z, r.code.n, r.code.h, r.code.c)
    }

    // ── ADD ─────────────────────────────────────────────────────────

    #[test]
    fn add_simple() {
        let r = alu(AluOpKind::Add, 0x12, 0x34, false);
        assert_eq!(r.value, 0x46);
        assert_eq!(flags(&r), (false, false, false, false));
    }

    #[test]
    fn add_zero_result() {
        let r = alu(AluOpKind::Add, 0x00, 0x00, false);
        assert_eq!(r.value, 0x00);
        assert!(r.code.z);
    }

    #[test]
    fn add_half_carry() {
        // 0x0F + 0x01: low nibble 0xF + 0x1 = 0x10, half carry
        let r = alu(AluOpKind::Add, 0x0F, 0x01, false);
        assert_eq!(r.value, 0x10);
        assert_eq!(flags(&r), (false, false, true, false));
    }

    #[test]
    fn add_full_carry() {
        let r = alu(AluOpKind::Add, 0xFF, 0x01, false);
        assert_eq!(r.value, 0x00);
        assert_eq!(flags(&r), (true, false, true, true));
    }

    #[test]
    fn add_both_carries_no_zero() {
        // 0xF0 + 0x20 = 0x10 with carry, no half carry
        let r = alu(AluOpKind::Add, 0xF0, 0x20, false);
        assert_eq!(r.value, 0x10);
        assert_eq!(flags(&r), (false, false, false, true));
    }

    #[test]
    fn add_ff_plus_ff() {
        let r = alu(AluOpKind::Add, 0xFF, 0xFF, false);
        assert_eq!(r.value, 0xFE);
        assert_eq!(flags(&r), (false, false, true, true));
    }

    #[test]
    fn add_half_carry_boundary() {
        // 0x08 + 0x08 = 0x10, half carry (8+8=16 > 15)
        let r = alu(AluOpKind::Add, 0x08, 0x08, false);
        assert!(r.code.h);
        // 0x07 + 0x08 = 0x0F, no half carry
        let r = alu(AluOpKind::Add, 0x07, 0x08, false);
        assert!(!r.code.h);
    }

    #[test]
    fn add_ignores_carry_in() {
        let r = alu(AluOpKind::Add, 0x10, 0x20, true);
        assert_eq!(r.value, 0x30);
    }

    // ── ADC ─────────────────────────────────────────────────────────

    #[test]
    fn adc_without_carry() {
        let r = alu(AluOpKind::Adc, 0x12, 0x34, false);
        assert_eq!(r.value, 0x46);
    }

    #[test]
    fn adc_with_carry() {
        let r = alu(AluOpKind::Adc, 0x12, 0x34, true);
        assert_eq!(r.value, 0x47);
    }

    #[test]
    fn adc_carry_in_causes_half_carry() {
        // 0x0F + 0x00 + 1 = 0x10, half carry from the carry-in
        let r = alu(AluOpKind::Adc, 0x0F, 0x00, true);
        assert_eq!(r.value, 0x10);
        assert!(r.code.h);
    }

    #[test]
    fn adc_carry_in_causes_full_carry() {
        // 0xFF + 0x00 + 1 = 0x00 with carry
        let r = alu(AluOpKind::Adc, 0xFF, 0x00, true);
        assert_eq!(r.value, 0x00);
        assert_eq!(flags(&r), (true, false, true, true));
    }

    #[test]
    fn adc_ff_ff_carry() {
        // 0xFF + 0xFF + 1 = 0x1FF = 0xFF with carry
        let r = alu(AluOpKind::Adc, 0xFF, 0xFF, true);
        assert_eq!(r.value, 0xFF);
        assert_eq!(flags(&r), (false, false, true, true));
    }

    #[test]
    fn adc_half_carry_exact_threshold() {
        // (0x0E & 0xF) + (0x01 & 0xF) + 1 = 0x10 > 0xF → half carry
        let r = alu(AluOpKind::Adc, 0x0E, 0x01, true);
        assert!(r.code.h);
        // (0x0E & 0xF) + (0x00 & 0xF) + 1 = 0x0F → no half carry
        let r = alu(AluOpKind::Adc, 0x0E, 0x00, true);
        assert!(!r.code.h);
    }

    // ── SUB ─────────────────────────────────────────────────────────

    #[test]
    fn sub_simple() {
        let r = alu(AluOpKind::Sub, 0x50, 0x20, false);
        assert_eq!(r.value, 0x30);
        assert_eq!(flags(&r), (false, true, false, false));
    }

    #[test]
    fn sub_zero_result() {
        let r = alu(AluOpKind::Sub, 0x42, 0x42, false);
        assert_eq!(r.value, 0x00);
        assert!(r.code.z);
        assert!(r.code.n);
    }

    #[test]
    fn sub_borrow() {
        // 0x00 - 0x01 = 0xFF with carry (borrow)
        let r = alu(AluOpKind::Sub, 0x00, 0x01, false);
        assert_eq!(r.value, 0xFF);
        assert_eq!(flags(&r), (false, true, true, true));
    }

    #[test]
    fn sub_half_borrow() {
        // 0x10 - 0x01: low nibble 0x0 < 0x1 → half carry
        let r = alu(AluOpKind::Sub, 0x10, 0x01, false);
        assert_eq!(r.value, 0x0F);
        assert!(r.code.h);
        assert!(!r.code.c);
    }

    #[test]
    fn sub_no_half_borrow() {
        // 0x1F - 0x01: low nibble 0xF >= 0x1 → no half carry
        let r = alu(AluOpKind::Sub, 0x1F, 0x01, false);
        assert!(!r.code.h);
    }

    #[test]
    fn sub_n_flag_always_set() {
        let r = alu(AluOpKind::Sub, 0xFF, 0x00, false);
        assert!(r.code.n);
    }

    // ── SBC ─────────────────────────────────────────────────────────

    #[test]
    fn sbc_without_carry() {
        let r = alu(AluOpKind::Sbc, 0x50, 0x20, false);
        assert_eq!(r.value, 0x30);
    }

    #[test]
    fn sbc_with_carry() {
        let r = alu(AluOpKind::Sbc, 0x50, 0x20, true);
        assert_eq!(r.value, 0x2F);
    }

    #[test]
    fn sbc_carry_in_causes_half_borrow() {
        // 0x10 - 0x00 - 1: low nibble 0x0 < 0x0 + 1 → half carry
        let r = alu(AluOpKind::Sbc, 0x10, 0x00, true);
        assert_eq!(r.value, 0x0F);
        assert!(r.code.h);
    }

    #[test]
    fn sbc_carry_in_causes_full_borrow() {
        // 0x00 - 0x00 - 1 = 0xFF with borrow
        let r = alu(AluOpKind::Sbc, 0x00, 0x00, true);
        assert_eq!(r.value, 0xFF);
        assert!(r.code.c);
        assert!(r.code.h);
    }

    #[test]
    fn sbc_zero_result_with_carry() {
        // 0x01 - 0x00 - 1 = 0x00
        let r = alu(AluOpKind::Sbc, 0x01, 0x00, true);
        assert_eq!(r.value, 0x00);
        assert!(r.code.z);
    }

    #[test]
    fn sbc_double_borrow() {
        // 0x00 - 0xFF - 1 = 0x00 with carry
        let r = alu(AluOpKind::Sbc, 0x00, 0xFF, true);
        assert_eq!(r.value, 0x00);
        assert_eq!(flags(&r), (true, true, true, true));
    }

    // ── CP (compare — same as SUB but result discarded) ─────────────

    #[test]
    fn cp_flags_match_sub() {
        // CP should produce identical flags to SUB for any inputs
        for a in (0..=255u8).step_by(17) {
            for b in (0..=255u8).step_by(19) {
                let sub = alu(AluOpKind::Sub, a, b, false);
                let cp = alu(AluOpKind::Cp, a, b, false);
                assert_eq!(
                    flags(&sub), flags(&cp),
                    "CP flags differ from SUB for a={:#04X} b={:#04X}", a, b
                );
                assert_eq!(sub.value, cp.value);
            }
        }
    }

    // ── AND ─────────────────────────────────────────────────────────

    #[test]
    fn and_basic() {
        let r = alu(AluOpKind::And, 0xF0, 0x0F, false);
        assert_eq!(r.value, 0x00);
        assert_eq!(flags(&r), (true, false, true, false));
    }

    #[test]
    fn and_preserves_common_bits() {
        let r = alu(AluOpKind::And, 0xFF, 0xAA, false);
        assert_eq!(r.value, 0xAA);
    }

    #[test]
    fn and_h_flag_always_set() {
        let r = alu(AluOpKind::And, 0xFF, 0xFF, false);
        assert!(r.code.h);
        assert!(!r.code.n);
        assert!(!r.code.c);
    }

    // ── OR ──────────────────────────────────────────────────────────

    #[test]
    fn or_basic() {
        let r = alu(AluOpKind::Or, 0xF0, 0x0F, false);
        assert_eq!(r.value, 0xFF);
        assert_eq!(flags(&r), (false, false, false, false));
    }

    #[test]
    fn or_zero_with_zero() {
        let r = alu(AluOpKind::Or, 0x00, 0x00, false);
        assert_eq!(r.value, 0x00);
        assert!(r.code.z);
    }

    #[test]
    fn or_clears_all_auxiliary_flags() {
        let r = alu(AluOpKind::Or, 0x12, 0x34, false);
        assert!(!r.code.n);
        assert!(!r.code.h);
        assert!(!r.code.c);
    }

    // ── XOR ─────────────────────────────────────────────────────────

    #[test]
    fn xor_self_is_zero() {
        let r = alu(AluOpKind::Xor, 0xAB, 0xAB, false);
        assert_eq!(r.value, 0x00);
        assert!(r.code.z);
    }

    #[test]
    fn xor_with_ff_inverts() {
        let r = alu(AluOpKind::Xor, 0x55, 0xFF, false);
        assert_eq!(r.value, 0xAA);
    }

    #[test]
    fn xor_clears_all_auxiliary_flags() {
        let r = alu(AluOpKind::Xor, 0x12, 0x34, false);
        assert!(!r.code.n);
        assert!(!r.code.h);
        assert!(!r.code.c);
    }

    #[test]
    fn xor_is_commutative() {
        let r1 = alu(AluOpKind::Xor, 0xAB, 0xCD, false);
        let r2 = alu(AluOpKind::Xor, 0xCD, 0xAB, false);
        assert_eq!(r1.value, r2.value);
    }

    // ── Commutativity / identity properties ─────────────────────────

    #[test]
    fn add_is_commutative() {
        for a in (0..=255u8).step_by(13) {
            for b in (0..=255u8).step_by(17) {
                let r1 = alu(AluOpKind::Add, a, b, false);
                let r2 = alu(AluOpKind::Add, b, a, false);
                assert_eq!(r1.value, r2.value,
                    "ADD not commutative for {:#04X}, {:#04X}", a, b);
                assert_eq!(flags(&r1), flags(&r2));
            }
        }
    }

    #[test]
    fn add_zero_is_identity() {
        for a in (0..=255u8).step_by(7) {
            let r = alu(AluOpKind::Add, a, 0, false);
            assert_eq!(r.value, a);
            assert!(!r.code.c);
            assert!(!r.code.h);
        }
    }

    #[test]
    fn sub_self_is_always_zero() {
        for a in 0..=255u8 {
            let r = alu(AluOpKind::Sub, a, a, false);
            assert_eq!(r.value, 0);
            assert!(r.code.z);
            assert!(!r.code.c);
            assert!(!r.code.h);
        }
    }

    #[test]
    fn and_with_ff_is_identity() {
        for a in 0..=255u8 {
            let r = alu(AluOpKind::And, a, 0xFF, false);
            assert_eq!(r.value, a);
        }
    }

    #[test]
    fn or_with_zero_is_identity() {
        for a in 0..=255u8 {
            let r = alu(AluOpKind::Or, a, 0x00, false);
            assert_eq!(r.value, a);
        }
    }

    #[test]
    fn xor_with_zero_is_identity() {
        for a in 0..=255u8 {
            let r = alu(AluOpKind::Xor, a, 0x00, false);
            assert_eq!(r.value, a);
        }
    }

    // ── Exhaustive flag correctness for ADD/SUB ─────────────────────
    // Brute-force the entire 8-bit input space. If this passes,
    // the flag logic is correct — not merely plausible.

    #[test]
    fn add_exhaustive_flags() {
        for a in 0..=255u16 {
            for b in 0..=255u16 {
                let r = alu(AluOpKind::Add, a as u8, b as u8, false);
                let sum = a + b;

                assert_eq!(r.value, sum as u8);
                assert_eq!(r.code.z, r.value == 0,
                    "Z flag wrong for ADD {:#04X}+{:#04X}", a, b);
                assert_eq!(r.code.c, sum > 0xFF,
                    "C flag wrong for ADD {:#04X}+{:#04X}", a, b);
                assert_eq!(r.code.h, (a & 0xF) + (b & 0xF) > 0xF,
                    "H flag wrong for ADD {:#04X}+{:#04X}", a, b);
                assert!(!r.code.n);
            }
        }
    }

    #[test]
    fn sub_exhaustive_flags() {
        for a in 0..=255u16 {
            for b in 0..=255u16 {
                let r = alu(AluOpKind::Sub, a as u8, b as u8, false);
                let diff = a.wrapping_sub(b);

                assert_eq!(r.value, diff as u8);
                assert_eq!(r.code.z, r.value == 0,
                    "Z flag wrong for SUB {:#04X}-{:#04X}", a, b);
                assert_eq!(r.code.c, a < b,
                    "C flag wrong for SUB {:#04X}-{:#04X}", a, b);
                assert_eq!(r.code.h, (a & 0xF) < (b & 0xF),
                    "H flag wrong for SUB {:#04X}-{:#04X}", a, b);
                assert!(r.code.n);
            }
        }
    }

    #[test]
    fn adc_exhaustive_flags() {
        for carry in [false, true] {
            let c = carry as u16;
            for a in 0..=255u16 {
                for b in 0..=255u16 {
                    let r = alu(AluOpKind::Adc, a as u8, b as u8, carry);
                    let sum = a + b + c;

                    assert_eq!(r.value, sum as u8,
                        "value wrong for ADC {:#04X}+{:#04X}+{}", a, b, c);
                    assert_eq!(r.code.z, r.value == 0);
                    assert_eq!(r.code.c, sum > 0xFF,
                        "C flag wrong for ADC {:#04X}+{:#04X}+{}", a, b, c);
                    assert_eq!(r.code.h, (a & 0xF) + (b & 0xF) + c > 0xF,
                        "H flag wrong for ADC {:#04X}+{:#04X}+{}", a, b, c);
                    assert!(!r.code.n);
                }
            }
        }
    }

    #[test]
    fn sbc_exhaustive_flags() {
        for carry in [false, true] {
            let c = carry as u16;
            for a in 0..=255u16 {
                for b in 0..=255u16 {
                    let r = alu(AluOpKind::Sbc, a as u8, b as u8, carry);
                    let diff = a.wrapping_sub(b).wrapping_sub(c);

                    assert_eq!(r.value, diff as u8,
                        "value wrong for SBC {:#04X}-{:#04X}-{}", a, b, c);
                    assert_eq!(r.code.z, r.value == 0);
                    assert_eq!(r.code.c, a < b + c,
                        "C flag wrong for SBC {:#04X}-{:#04X}-{}", a, b, c);
                    assert_eq!(r.code.h, (a & 0xF) < (b & 0xF) + c,
                        "H flag wrong for SBC {:#04X}-{:#04X}-{}", a, b, c);
                    assert!(r.code.n);
                }
            }
        }
    }
}

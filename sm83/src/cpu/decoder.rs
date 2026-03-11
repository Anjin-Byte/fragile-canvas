use crate::cpu::microcode::*;
use crate::cpu::registers::{Reg16, Reg8};
use crate::cpu::alu::AluOpKind;

use arrayvec::ArrayVec;
pub type MicrocodeQueue = ArrayVec<MicroOp, 8>;

// Helper: map 3-bit register index to Reg8 (used in opcode decoding)
// 0=B, 1=C, 2=D, 3=E, 4=H, 5=L, 6=(HL) sentinel, 7=A
fn reg8_from_bits(bits: u8) -> Reg8 {
    match bits & 0x07 {
        0 => Reg8::B,
        1 => Reg8::C,
        2 => Reg8::D,
        3 => Reg8::E,
        4 => Reg8::H,
        5 => Reg8::L,
        6 => panic!("reg8_from_bits(6) is (HL) — handle separately"),
        7 => Reg8::A,
        _ => unreachable!(),
    }
}

// Helper: map 2-bit register pair index to Reg16
// 0=BC, 1=DE, 2=HL, 3=SP
fn reg16_from_bits(bits: u8) -> Reg16 {
    match bits & 0x03 {
        0 => Reg16::BC,
        1 => Reg16::DE,
        2 => Reg16::HL,
        3 => Reg16::SP,
        _ => unreachable!(),
    }
}

// Helper: map 2-bit register pair index for PUSH/POP (AF instead of SP)
fn reg16_push_pop(bits: u8) -> Reg16 {
    match bits & 0x03 {
        0 => Reg16::BC,
        1 => Reg16::DE,
        2 => Reg16::HL,
        3 => Reg16::AF,
        _ => unreachable!(),
    }
}

// Helper: map 3-bit ALU operation to AluOpKind
fn alu_op_from_bits(bits: u8) -> AluOpKind {
    match bits & 0x07 {
        0 => AluOpKind::Add,
        1 => AluOpKind::Adc,
        2 => AluOpKind::Sub,
        3 => AluOpKind::Sbc,
        4 => AluOpKind::And,
        5 => AluOpKind::Xor,
        6 => AluOpKind::Or,
        7 => AluOpKind::Cp,
        _ => unreachable!(),
    }
}

// Helper: map 2-bit condition code (available for future use)
#[allow(dead_code)]
fn cond_from_bits(bits: u8) -> Condition {
    match bits & 0x03 {
        0 => Condition::NZ,
        1 => Condition::Z,
        2 => Condition::NC,
        3 => Condition::C,
        _ => unreachable!(),
    }
}

const T_CYCLES: [u8; 256] = [
    // x0  x1  x2  x3  x4  x5  x6  x7  x8  x9  xA  xB  xC  xD  xE  xF
        4, 12,  8,  8,  4,  4,  8,  4, 20,  8,  8,  8,  4,  4,  8,  4,  // 0x
        4, 12,  8,  8,  4,  4,  8,  4,  8,  8,  8,  8,  4,  4,  8,  4,  // 1x
        8, 12,  8,  8,  4,  4,  8,  4,  8,  8,  8,  8,  4,  4,  8,  4,  // 2x
        8, 12,  8,  8, 12, 12, 12,  4,  8,  8,  8,  8,  4,  4,  8,  4,  // 3x
        4,  4,  4,  4,  4,  4,  8,  4,  4,  4,  4,  4,  4,  4,  8,  4,  // 4x
        4,  4,  4,  4,  4,  4,  8,  4,  4,  4,  4,  4,  4,  4,  8,  4,  // 5x
        4,  4,  4,  4,  4,  4,  8,  4,  4,  4,  4,  4,  4,  4,  8,  4,  // 6x
        8,  8,  8,  8,  8,  8,  4,  8,  4,  4,  4,  4,  4,  4,  8,  4,  // 7x
        4,  4,  4,  4,  4,  4,  8,  4,  4,  4,  4,  4,  4,  4,  8,  4,  // 8x
        4,  4,  4,  4,  4,  4,  8,  4,  4,  4,  4,  4,  4,  4,  8,  4,  // 9x
        4,  4,  4,  4,  4,  4,  8,  4,  4,  4,  4,  4,  4,  4,  8,  4,  // Ax
        4,  4,  4,  4,  4,  4,  8,  4,  4,  4,  4,  4,  4,  4,  8,  4,  // Bx
        8, 12, 12, 12, 12, 16,  8, 32,  8,  8, 12,  8, 12, 12,  8, 32,  // Cx
        8, 12, 12,  0, 12, 16,  8, 32,  8,  8, 12,  0, 12,  0,  8, 32,  // Dx
        12, 12,  8,  0,  0, 16,  8, 32, 16,  4, 16,  0,  0,  0,  8, 32, // Ex
        12, 12,  8,  4,  0, 16,  8, 32, 12,  8, 16,  4,  0,  0,  8, 32, // Fx
    ];

const CB_T_CYCLES: [u8; 256] = [
    // x0  x1  x2  x3  x4  x5  x6  x7  x8  x9  xA  xB  xC  xD  xE  xF
        8,  8,  8,  8,  8,  8, 16,  8,  8,  8,  8,  8,  8,  8, 16,  8,  // 0x
        8,  8,  8,  8,  8,  8, 16,  8,  8,  8,  8,  8,  8,  8, 16,  8,  // 1x
        8,  8,  8,  8,  8,  8, 16,  8,  8,  8,  8,  8,  8,  8, 16,  8,  // 2x
        8,  8,  8,  8,  8,  8, 16,  8,  8,  8,  8,  8,  8,  8, 16,  8,  // 3x
        8,  8,  8,  8,  8,  8, 12,  8,  8,  8,  8,  8,  8,  8, 12,  8,  // 4x
        8,  8,  8,  8,  8,  8, 12,  8,  8,  8,  8,  8,  8,  8, 12,  8,  // 5x
        8,  8,  8,  8,  8,  8, 12,  8,  8,  8,  8,  8,  8,  8, 12,  8,  // 6x
        8,  8,  8,  8,  8,  8, 12,  8,  8,  8,  8,  8,  8,  8, 12,  8,  // 7x
        8,  8,  8,  8,  8,  8, 16,  8,  8,  8,  8,  8,  8,  8, 16,  8,  // 8x
        8,  8,  8,  8,  8,  8, 16,  8,  8,  8,  8,  8,  8,  8, 16,  8,  // 9x
        8,  8,  8,  8,  8,  8, 16,  8,  8,  8,  8,  8,  8,  8, 16,  8,  // Ax
        8,  8,  8,  8,  8,  8, 16,  8,  8,  8,  8,  8,  8,  8, 16,  8,  // Bx
        8,  8,  8,  8,  8,  8, 16,  8,  8,  8,  8,  8,  8,  8, 16,  8,  // Cx
        8,  8,  8,  8,  8,  8, 16,  8,  8,  8,  8,  8,  8,  8, 16,  8,  // Dx
        8,  8,  8,  8,  8,  8, 16,  8,  8,  8,  8,  8,  8,  8, 16,  8,  // Ex
        8,  8,  8,  8,  8,  8, 16,  8,  8,  8,  8,  8,  8,  8, 16,  8,  // Fx
    ];

macro_rules! gen_m_cycles {
    ($name:ident, $t_table:ident) => {
        pub const $name: [u8; 256] = {
            let mut m = [0u8; 256];
            let mut i = 0;

            while i < 256 {
                m[i] = $t_table[i] >> 2;
                i += 1;
            }

            m
        };
    };
}

gen_m_cycles!(M_CYCLES, T_CYCLES);
gen_m_cycles!(CB_M_CYCLES, CB_T_CYCLES);

/// T-cycle counts for conditional instructions when the branch IS taken.
/// Non-conditional opcodes have the same value as T_CYCLES.
const TAKEN_T_CYCLES: [u8; 256] = {
    let mut t = T_CYCLES;
    // JR cc (taken = 12, not-taken = 8)
    t[0x20] = 12; t[0x28] = 12; t[0x30] = 12; t[0x38] = 12;
    // RET cc (taken = 20, not-taken = 8)
    t[0xC0] = 20; t[0xC8] = 20; t[0xD0] = 20; t[0xD8] = 20;
    // JP cc (taken = 16, not-taken = 12)
    t[0xC2] = 16; t[0xCA] = 16; t[0xD2] = 16; t[0xDA] = 16;
    // CALL cc (taken = 24, not-taken = 12)
    t[0xC4] = 24; t[0xCC] = 24; t[0xD4] = 24; t[0xDC] = 24;
    t
};

macro_rules! gen_cycles_lookup {
    ($fn_name:ident, $table:ident) => {
        #[inline(always)]
        pub fn $fn_name(opcode: u8) -> u8 {
            unsafe { *$table.get_unchecked(opcode as usize) }
        }
    };
}

gen_cycles_lookup!(t_cycles,       T_CYCLES);
gen_cycles_lookup!(taken_t_cycles, TAKEN_T_CYCLES);
gen_cycles_lookup!(cb_t_cycles,    CB_T_CYCLES);
gen_cycles_lookup!(m_cycles,       M_CYCLES);
gen_cycles_lookup!(cb_m_cycles,    CB_M_CYCLES);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::CPU;
    use crate::memory::bus::Bus;

    // =====================================================================
    //  Cycle table sanity
    // =====================================================================

    #[test]
    fn test_vs_raw_lookup() {
        for opcode in 0u8..=255 {
            let idx = opcode as usize;
            assert_eq!(t_cycles(opcode), T_CYCLES[idx],
                "t_cycles(0x{:02X})", opcode);
            assert_eq!(cb_t_cycles(opcode), CB_T_CYCLES[idx],
                "cb_t_cycles(0x{:02X})", opcode);
        }
    }

    #[test]
    fn test_m_lookup() {
        for opcode in 0u8..=255 {
            let idx = opcode as usize;
            assert_eq!(m_cycles(opcode), T_CYCLES[idx] / 4,
                "m_cycles(0x{:02X})", opcode);
            assert_eq!(cb_m_cycles(opcode), CB_T_CYCLES[idx] / 4,
                "cb_m_cycles(0x{:02X})", opcode);
        }
    }

    // =====================================================================
    //  Coverage: every opcode decodes without panic
    // =====================================================================

    const UNDEFINED_OPCODES: [u8; 11] = [
        0xD3, 0xDB, 0xDD, 0xE3, 0xE4, 0xEB, 0xEC, 0xED, 0xF4, 0xFC, 0xFD,
    ];

    #[test]
    fn decode_all_base_opcodes() {
        for opcode in 0u8..=255 {
            if opcode == 0xCB { continue; }
            let _ops = decode_instruction(opcode);
        }
    }

    #[test]
    fn decode_all_cb_opcodes() {
        for opcode in 0u8..=255 {
            let _ops = decode_cb_instruction(opcode);
        }
    }

    #[test]
    #[should_panic]
    fn decode_cb_from_base_panics() {
        let _ = decode_instruction(0xCB);
    }

    #[test]
    fn decode_nop_returns_empty() {
        assert!(decode_instruction(0x00).is_empty());
    }

    #[test]
    fn undefined_opcodes_lock_cpu() {
        use crate::cpu::microcode::MicroOp;
        for &op in &UNDEFINED_OPCODES {
            let ops = decode_instruction(op);
            assert!(
                ops.iter().any(|o| matches!(o, MicroOp::TriggerHalt)),
                "0x{:02X} should emit TriggerHalt",
                op
            );
        }
    }

    // =====================================================================
    //  Helper function correctness
    // =====================================================================

    #[test]
    fn reg8_from_bits_mapping() {
        assert_eq!(reg8_from_bits(0), Reg8::B);
        assert_eq!(reg8_from_bits(1), Reg8::C);
        assert_eq!(reg8_from_bits(2), Reg8::D);
        assert_eq!(reg8_from_bits(3), Reg8::E);
        assert_eq!(reg8_from_bits(4), Reg8::H);
        assert_eq!(reg8_from_bits(5), Reg8::L);
        assert_eq!(reg8_from_bits(7), Reg8::A);
    }

    #[test]
    #[should_panic]
    fn reg8_from_bits_6_panics() {
        let _ = reg8_from_bits(6);
    }

    #[test]
    fn reg16_from_bits_mapping() {
        assert_eq!(reg16_from_bits(0), Reg16::BC);
        assert_eq!(reg16_from_bits(1), Reg16::DE);
        assert_eq!(reg16_from_bits(2), Reg16::HL);
        assert_eq!(reg16_from_bits(3), Reg16::SP);
    }

    #[test]
    fn reg16_push_pop_mapping() {
        assert_eq!(reg16_push_pop(0), Reg16::BC);
        assert_eq!(reg16_push_pop(1), Reg16::DE);
        assert_eq!(reg16_push_pop(2), Reg16::HL);
        assert_eq!(reg16_push_pop(3), Reg16::AF);
    }

    #[test]
    fn alu_op_from_bits_mapping() {
        assert_eq!(alu_op_from_bits(0), AluOpKind::Add);
        assert_eq!(alu_op_from_bits(1), AluOpKind::Adc);
        assert_eq!(alu_op_from_bits(2), AluOpKind::Sub);
        assert_eq!(alu_op_from_bits(3), AluOpKind::Sbc);
        assert_eq!(alu_op_from_bits(4), AluOpKind::And);
        assert_eq!(alu_op_from_bits(5), AluOpKind::Xor);
        assert_eq!(alu_op_from_bits(6), AluOpKind::Or);
        assert_eq!(alu_op_from_bits(7), AluOpKind::Cp);
    }

    #[test]
    fn cond_from_bits_mapping() {
        assert_eq!(cond_from_bits(0), Condition::NZ);
        assert_eq!(cond_from_bits(1), Condition::Z);
        assert_eq!(cond_from_bits(2), Condition::NC);
        assert_eq!(cond_from_bits(3), Condition::C);
    }

    // =====================================================================
    //  LD r, r' block (0x40-0x7F, excluding 0x76 and (HL) variants)
    // =====================================================================

    #[test]
    fn ld_r_r_all_49_register_to_register() {
        // 7 dst regs × 7 src regs = 49 LD r, r' (excluding (HL) cases)
        let reg_map: [(u8, Reg8); 7] = [
            (0, Reg8::B), (1, Reg8::C), (2, Reg8::D), (3, Reg8::E),
            (4, Reg8::H), (5, Reg8::L), (7, Reg8::A),
        ];

        for &(dst_bits, dst_reg) in &reg_map {
            for &(src_bits, src_reg) in &reg_map {
                let opcode = 0x40 | (dst_bits << 3) | src_bits;
                let ops = decode_instruction(opcode);
                assert_eq!(ops.len(), 1,
                    "LD {:?},{:?} (0x{:02X}) should be 1 micro-op", dst_reg, src_reg, opcode);
                assert_eq!(ops[0], MicroOp::LoadReg8 { dst: dst_reg, src: src_reg },
                    "LD {:?},{:?} (0x{:02X}) wrong micro-op", dst_reg, src_reg, opcode);
            }
        }
    }

    #[test]
    fn ld_r_hl_indirect_all_7() {
        let expected: [(u8, Reg8); 7] = [
            (0x46, Reg8::B), (0x4E, Reg8::C), (0x56, Reg8::D), (0x5E, Reg8::E),
            (0x66, Reg8::H), (0x6E, Reg8::L), (0x7E, Reg8::A),
        ];
        for &(opcode, dst) in &expected {
            let ops = decode_instruction(opcode);
            assert_eq!(ops.len(), 1, "LD {:?},(HL) should be 1 op", dst);
            assert_eq!(ops[0],
                MicroOp::ReadMemReg8 { addr_reg: Reg16::HL, into: dst },
                "LD {:?},(HL) (0x{:02X})", dst, opcode);
        }
    }

    #[test]
    fn ld_hl_r_indirect_all_7() {
        let expected: [(u8, Reg8); 7] = [
            (0x70, Reg8::B), (0x71, Reg8::C), (0x72, Reg8::D), (0x73, Reg8::E),
            (0x74, Reg8::H), (0x75, Reg8::L), (0x77, Reg8::A),
        ];
        for &(opcode, src) in &expected {
            let ops = decode_instruction(opcode);
            assert_eq!(ops.len(), 1, "LD (HL),{:?} should be 1 op", src);
            assert_eq!(ops[0],
                MicroOp::WriteMemReg8 { addr_reg: Reg16::HL, src },
                "LD (HL),{:?} (0x{:02X})", src, opcode);
        }
    }

    #[test]
    fn halt_is_in_ld_block() {
        let ops = decode_instruction(0x76);
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0], MicroOp::TriggerHalt);
    }

    // =====================================================================
    //  LD rr, nn (16-bit immediate loads)
    // =====================================================================

    #[test]
    fn ld_rr_nn_all_4() {
        let cases: [(u8, Reg16); 4] = [
            (0x01, Reg16::BC), (0x11, Reg16::DE),
            (0x21, Reg16::HL), (0x31, Reg16::SP),
        ];
        for &(opcode, rr) in &cases {
            let ops = decode_instruction(opcode);
            assert_eq!(ops, MicrocodeQueue::from_iter([MicroOp::ReadImmediate16 { into: rr }]),
                "LD {:?},nn (0x{:02X})", rr, opcode);
        }
    }

    // =====================================================================
    //  LD r, n (8-bit immediate loads)
    // =====================================================================

    #[test]
    fn ld_r_n_all_7() {
        let cases: [(u8, Reg8); 7] = [
            (0x06, Reg8::B), (0x0E, Reg8::C), (0x16, Reg8::D), (0x1E, Reg8::E),
            (0x26, Reg8::H), (0x2E, Reg8::L), (0x3E, Reg8::A),
        ];
        for &(opcode, r) in &cases {
            let ops = decode_instruction(opcode);
            assert_eq!(ops, MicrocodeQueue::from_iter([MicroOp::ReadImmediate8 { into: r }]),
                "LD {:?},n (0x{:02X})", r, opcode);
        }
    }

    #[test]
    fn ld_hl_n_uses_ir_temp() {
        let ops = decode_instruction(0x36);
        assert_eq!(ops.len(), 2);
        assert_eq!(ops[0], MicroOp::ReadImmediate8 { into: Reg8::IR });
        assert_eq!(ops[1], MicroOp::WriteMemReg8 { addr_reg: Reg16::HL, src: Reg8::IR });
    }

    // =====================================================================
    //  INC / DEC (8-bit)
    // =====================================================================

    #[test]
    fn inc8_all_registers() {
        let cases: [(u8, Reg8); 7] = [
            (0x04, Reg8::B), (0x0C, Reg8::C), (0x14, Reg8::D), (0x1C, Reg8::E),
            (0x24, Reg8::H), (0x2C, Reg8::L), (0x3C, Reg8::A),
        ];
        for &(opcode, r) in &cases {
            let ops = decode_instruction(opcode);
            assert_eq!(ops, MicrocodeQueue::from_iter([MicroOp::Inc8 { target: Operand8::Reg(r) }]),
                "INC {:?} (0x{:02X})", r, opcode);
        }
    }

    #[test]
    fn dec8_all_registers() {
        let cases: [(u8, Reg8); 7] = [
            (0x05, Reg8::B), (0x0D, Reg8::C), (0x15, Reg8::D), (0x1D, Reg8::E),
            (0x25, Reg8::H), (0x2D, Reg8::L), (0x3D, Reg8::A),
        ];
        for &(opcode, r) in &cases {
            let ops = decode_instruction(opcode);
            assert_eq!(ops, MicrocodeQueue::from_iter([MicroOp::Dec8 { target: Operand8::Reg(r) }]),
                "DEC {:?} (0x{:02X})", r, opcode);
        }
    }

    #[test]
    fn inc_hl_indirect_read_modify_write() {
        let ops = decode_instruction(0x34);
        assert_eq!(ops.len(), 3);
        assert_eq!(ops[0], MicroOp::ReadMemReg8 { addr_reg: Reg16::HL, into: Reg8::IR });
        assert_eq!(ops[1], MicroOp::Inc8 { target: Operand8::Reg(Reg8::IR) });
        assert_eq!(ops[2], MicroOp::WriteMemReg8 { addr_reg: Reg16::HL, src: Reg8::IR });
    }

    #[test]
    fn dec_hl_indirect_read_modify_write() {
        let ops = decode_instruction(0x35);
        assert_eq!(ops.len(), 3);
        assert_eq!(ops[0], MicroOp::ReadMemReg8 { addr_reg: Reg16::HL, into: Reg8::IR });
        assert_eq!(ops[1], MicroOp::Dec8 { target: Operand8::Reg(Reg8::IR) });
        assert_eq!(ops[2], MicroOp::WriteMemReg8 { addr_reg: Reg16::HL, src: Reg8::IR });
    }

    // =====================================================================
    //  INC / DEC (16-bit)
    // =====================================================================

    #[test]
    fn inc16_all_4() {
        let cases: [(u8, Reg16); 4] = [
            (0x03, Reg16::BC), (0x13, Reg16::DE),
            (0x23, Reg16::HL), (0x33, Reg16::SP),
        ];
        for &(opcode, rr) in &cases {
            let ops = decode_instruction(opcode);
            assert_eq!(ops, MicrocodeQueue::from_iter([MicroOp::Inc16 { reg: rr }]),
                "INC {:?} (0x{:02X})", rr, opcode);
        }
    }

    #[test]
    fn dec16_all_4() {
        let cases: [(u8, Reg16); 4] = [
            (0x0B, Reg16::BC), (0x1B, Reg16::DE),
            (0x2B, Reg16::HL), (0x3B, Reg16::SP),
        ];
        for &(opcode, rr) in &cases {
            let ops = decode_instruction(opcode);
            assert_eq!(ops, MicrocodeQueue::from_iter([MicroOp::Dec16 { reg: rr }]),
                "DEC {:?} (0x{:02X})", rr, opcode);
        }
    }

    // =====================================================================
    //  ALU A, r  (0x80-0xBF): all 8 ops × 8 sources
    // =====================================================================

    #[test]
    fn alu_a_r_all_56_register_variants() {
        // 8 ALU ops × 7 register sources (excl. (HL))
        let ops_map: [(u8, AluOpKind); 8] = [
            (0, AluOpKind::Add), (1, AluOpKind::Adc),
            (2, AluOpKind::Sub), (3, AluOpKind::Sbc),
            (4, AluOpKind::And), (5, AluOpKind::Xor),
            (6, AluOpKind::Or),  (7, AluOpKind::Cp),
        ];
        let regs: [(u8, Reg8); 7] = [
            (0, Reg8::B), (1, Reg8::C), (2, Reg8::D), (3, Reg8::E),
            (4, Reg8::H), (5, Reg8::L), (7, Reg8::A),
        ];

        for &(op_bits, ref kind) in &ops_map {
            for &(reg_bits, ref r) in &regs {
                let opcode = 0x80 | (op_bits << 3) | reg_bits;
                let ops = decode_instruction(opcode);
                assert_eq!(ops.len(), 1,
                    "ALU {:?},{:?} (0x{:02X}) should be 1 op", kind, r, opcode);
                assert_eq!(ops[0],
                    MicroOp::Alu8 { kind: *kind, dest: Reg8::A, src: Operand8::Reg(*r) },
                    "ALU {:?},{:?} (0x{:02X})", kind, r, opcode);
            }
        }
    }

    #[test]
    fn alu_a_hl_indirect_all_8() {
        let cases: [(u8, AluOpKind); 8] = [
            (0x86, AluOpKind::Add), (0x8E, AluOpKind::Adc),
            (0x96, AluOpKind::Sub), (0x9E, AluOpKind::Sbc),
            (0xA6, AluOpKind::And), (0xAE, AluOpKind::Xor),
            (0xB6, AluOpKind::Or),  (0xBE, AluOpKind::Cp),
        ];
        for &(opcode, ref kind) in &cases {
            let ops = decode_instruction(opcode);
            assert_eq!(ops.len(), 2, "ALU {:?},(HL) (0x{:02X})", kind, opcode);
            assert_eq!(ops[0],
                MicroOp::ReadMemReg8 { addr_reg: Reg16::HL, into: Reg8::IR });
            assert_eq!(ops[1],
                MicroOp::Alu8 { kind: *kind, dest: Reg8::A, src: Operand8::Reg(Reg8::IR) });
        }
    }

    #[test]
    fn alu_a_imm_all_8() {
        let cases: [(u8, AluOpKind); 8] = [
            (0xC6, AluOpKind::Add), (0xCE, AluOpKind::Adc),
            (0xD6, AluOpKind::Sub), (0xDE, AluOpKind::Sbc),
            (0xE6, AluOpKind::And), (0xEE, AluOpKind::Xor),
            (0xF6, AluOpKind::Or),  (0xFE, AluOpKind::Cp),
        ];
        for &(opcode, ref kind) in &cases {
            let ops = decode_instruction(opcode);
            assert_eq!(ops.len(), 2, "ALU {:?},n (0x{:02X})", kind, opcode);
            assert_eq!(ops[0], MicroOp::ReadImmediate8 { into: Reg8::IR });
            assert_eq!(ops[1],
                MicroOp::Alu8 { kind: *kind, dest: Reg8::A, src: Operand8::Reg(Reg8::IR) });
        }
    }

    // =====================================================================
    //  Rotates on A (non-CB — Z always cleared)
    // =====================================================================

    #[test]
    fn rlca_uses_dedicated_op() {
        assert_eq!(decode_instruction(0x07), MicrocodeQueue::from_iter([MicroOp::RlcA]));
    }

    #[test]
    fn rrca_uses_dedicated_op() {
        assert_eq!(decode_instruction(0x0F), MicrocodeQueue::from_iter([MicroOp::RrcA]));
    }

    #[test]
    fn rla_uses_dedicated_op() {
        assert_eq!(decode_instruction(0x17), MicrocodeQueue::from_iter([MicroOp::RlA]));
    }

    #[test]
    fn rra_uses_dedicated_op() {
        assert_eq!(decode_instruction(0x1F), MicrocodeQueue::from_iter([MicroOp::RrA]));
    }

    // =====================================================================
    //  Conditional ops: correct condition codes
    // =====================================================================

    #[test]
    fn jr_cc_correct_conditions() {
        let cases: [(u8, Condition); 4] = [
            (0x20, Condition::NZ), (0x28, Condition::Z),
            (0x30, Condition::NC), (0x38, Condition::C),
        ];
        for &(opcode, cond) in &cases {
            let ops = decode_instruction(opcode);
            assert_eq!(ops.len(), 2);
            assert_eq!(ops[0], MicroOp::CheckCond { cond });
            assert_eq!(ops[1], MicroOp::JumpRelImm);
        }
    }

    #[test]
    fn ret_cc_correct_conditions() {
        let cases: [(u8, Condition); 4] = [
            (0xC0, Condition::NZ), (0xC8, Condition::Z),
            (0xD0, Condition::NC), (0xD8, Condition::C),
        ];
        for &(opcode, cond) in &cases {
            let ops = decode_instruction(opcode);
            assert_eq!(ops[0], MicroOp::CheckCond { cond },
                "RET {:?} (0x{:02X})", cond, opcode);
            assert_eq!(ops[1], MicroOp::Ret);
        }
    }

    #[test]
    fn jp_cc_nn_correct_conditions() {
        let cases: [(u8, Condition); 4] = [
            (0xC2, Condition::NZ), (0xCA, Condition::Z),
            (0xD2, Condition::NC), (0xDA, Condition::C),
        ];
        for &(opcode, cond) in &cases {
            let ops = decode_instruction(opcode);
            assert_eq!(ops[0], MicroOp::CheckCond { cond });
            assert_eq!(ops[1], MicroOp::JumpAbsImm);
        }
    }

    #[test]
    fn call_cc_nn_correct_conditions() {
        let cases: [(u8, Condition); 4] = [
            (0xC4, Condition::NZ), (0xCC, Condition::Z),
            (0xD4, Condition::NC), (0xDC, Condition::C),
        ];
        for &(opcode, cond) in &cases {
            let ops = decode_instruction(opcode);
            assert_eq!(ops[0], MicroOp::CheckCond { cond });
            assert_eq!(ops[1], MicroOp::CallImm);
        }
    }

    // =====================================================================
    //  RST vectors
    // =====================================================================

    #[test]
    fn rst_all_8_vectors() {
        let cases: [(u8, u16); 8] = [
            (0xC7, 0x00), (0xCF, 0x08), (0xD7, 0x10), (0xDF, 0x18),
            (0xE7, 0x20), (0xEF, 0x28), (0xF7, 0x30), (0xFF, 0x38),
        ];
        for &(opcode, addr) in &cases {
            let ops = decode_instruction(opcode);
            assert_eq!(ops, MicrocodeQueue::from_iter([MicroOp::Rst { addr }]),
                "RST 0x{:02X} (opcode 0x{:02X})", addr, opcode);
        }
    }

    // =====================================================================
    //  PUSH / POP: correct register pairs (AF not SP)
    // =====================================================================

    #[test]
    fn push_pop_register_pairs() {
        let push_cases: [(u8, Reg16); 4] = [
            (0xC5, Reg16::BC), (0xD5, Reg16::DE),
            (0xE5, Reg16::HL), (0xF5, Reg16::AF),
        ];
        let pop_cases: [(u8, Reg16); 4] = [
            (0xC1, Reg16::BC), (0xD1, Reg16::DE),
            (0xE1, Reg16::HL), (0xF1, Reg16::AF),
        ];
        for &(opcode, rr) in &push_cases {
            assert_eq!(decode_instruction(opcode),
                MicrocodeQueue::from_iter([MicroOp::Push { src: rr }]),
                "PUSH {:?} (0x{:02X})", rr, opcode);
        }
        for &(opcode, rr) in &pop_cases {
            assert_eq!(decode_instruction(opcode),
                MicrocodeQueue::from_iter([MicroOp::Pop { dst: rr }]),
                "POP {:?} (0x{:02X})", rr, opcode);
        }
    }

    // =====================================================================
    //  ADD HL, rr
    // =====================================================================

    #[test]
    fn add_hl_rr_all_4() {
        let cases: [(u8, Reg16); 4] = [
            (0x09, Reg16::BC), (0x19, Reg16::DE),
            (0x29, Reg16::HL), (0x39, Reg16::SP),
        ];
        for &(opcode, rr) in &cases {
            assert_eq!(decode_instruction(opcode),
                MicrocodeQueue::from_iter([MicroOp::Add16 { dest: Reg16::HL, src: rr }]),
                "ADD HL,{:?} (0x{:02X})", rr, opcode);
        }
    }

    // =====================================================================
    //  Indirect loads with auto-increment/decrement
    // =====================================================================

    #[test]
    fn ld_hl_plus_minus_a() {
        // LD (HL+), A
        let ops = decode_instruction(0x22);
        assert_eq!(ops[0], MicroOp::WriteMemReg8 { addr_reg: Reg16::HL, src: Reg8::A });
        assert_eq!(ops[1], MicroOp::Inc16 { reg: Reg16::HL });

        // LD (HL-), A
        let ops = decode_instruction(0x32);
        assert_eq!(ops[0], MicroOp::WriteMemReg8 { addr_reg: Reg16::HL, src: Reg8::A });
        assert_eq!(ops[1], MicroOp::Dec16 { reg: Reg16::HL });

        // LD A, (HL+)
        let ops = decode_instruction(0x2A);
        assert_eq!(ops[0], MicroOp::ReadMemReg8 { addr_reg: Reg16::HL, into: Reg8::A });
        assert_eq!(ops[1], MicroOp::Inc16 { reg: Reg16::HL });

        // LD A, (HL-)
        let ops = decode_instruction(0x3A);
        assert_eq!(ops[0], MicroOp::ReadMemReg8 { addr_reg: Reg16::HL, into: Reg8::A });
        assert_eq!(ops[1], MicroOp::Dec16 { reg: Reg16::HL });
    }

    // =====================================================================
    //  Misc single-opcode instructions
    // =====================================================================

    #[test]
    fn misc_instructions() {
        assert_eq!(decode_instruction(0x27), MicrocodeQueue::from_iter([MicroOp::Daa]));
        assert_eq!(decode_instruction(0x2F), MicrocodeQueue::from_iter([MicroOp::Cpl]));
        assert_eq!(decode_instruction(0x37), MicrocodeQueue::from_iter([MicroOp::Scf]));
        assert_eq!(decode_instruction(0x3F), MicrocodeQueue::from_iter([MicroOp::Ccf]));
        assert_eq!(decode_instruction(0x10), MicrocodeQueue::from_iter([
            MicroOp::ReadImmediate8 { into: Reg8::IR },
            MicroOp::TriggerStop,
        ]));
        assert_eq!(decode_instruction(0xF3), MicrocodeQueue::from_iter([MicroOp::SetIme { value: false }]));
        assert_eq!(decode_instruction(0xFB), MicrocodeQueue::from_iter([MicroOp::DeferImeEnable]));
        assert_eq!(decode_instruction(0xC9), MicrocodeQueue::from_iter([MicroOp::Ret]));
        assert_eq!(decode_instruction(0xD9), MicrocodeQueue::from_iter([MicroOp::RetI]));
        assert_eq!(decode_instruction(0xC3), MicrocodeQueue::from_iter([MicroOp::JumpAbsImm]));
        assert_eq!(decode_instruction(0x18), MicrocodeQueue::from_iter([MicroOp::JumpRelImm]));
        assert_eq!(decode_instruction(0xCD), MicrocodeQueue::from_iter([MicroOp::CallImm]));
        assert_eq!(decode_instruction(0xE9), MicrocodeQueue::from_iter([MicroOp::JumpHL]));
        assert_eq!(decode_instruction(0xF9),
            MicrocodeQueue::from_iter([MicroOp::LoadReg16 { dst: Reg16::SP, src: Reg16::HL }]));
        assert_eq!(decode_instruction(0xE8), MicrocodeQueue::from_iter([MicroOp::AddSpImm]));
        assert_eq!(decode_instruction(0xF8), MicrocodeQueue::from_iter([MicroOp::LoadHlSpImm]));
        assert_eq!(decode_instruction(0x08), MicrocodeQueue::from_iter([MicroOp::WriteSp16BitAddr]));
    }

    #[test]
    fn ldh_and_indirect_c() {
        // LDH (n), A
        let ops = decode_instruction(0xE0);
        assert_eq!(ops[0], MicroOp::ReadImmediate8 { into: Reg8::IR });
        assert_eq!(ops[1], MicroOp::WriteHighPage { offset: Reg8::IR, src: Reg8::A });
        // LDH A, (n)
        let ops = decode_instruction(0xF0);
        assert_eq!(ops[0], MicroOp::ReadImmediate8 { into: Reg8::IR });
        assert_eq!(ops[1], MicroOp::ReadHighPage { offset: Reg8::IR, into: Reg8::A });
        // LD (C), A
        assert_eq!(decode_instruction(0xE2),
            MicrocodeQueue::from_iter([MicroOp::WriteHighPage { offset: Reg8::C, src: Reg8::A }]));
        // LD A, (C)
        assert_eq!(decode_instruction(0xF2),
            MicrocodeQueue::from_iter([MicroOp::ReadHighPage { offset: Reg8::C, into: Reg8::A }]));
    }

    #[test]
    fn ld_nn_a_and_ld_a_nn() {
        assert_eq!(decode_instruction(0xEA),
            MicrocodeQueue::from_iter([MicroOp::WriteMem16BitAddr { src: Reg8::A }]));
        assert_eq!(decode_instruction(0xFA),
            MicrocodeQueue::from_iter([MicroOp::ReadMem16BitAddr { into: Reg8::A }]));
    }

    // =====================================================================
    //  CB prefix: systematic correctness
    // =====================================================================

    #[test]
    fn cb_rlc_all_registers() {
        let regs: [(u8, Reg8); 7] = [
            (0, Reg8::B), (1, Reg8::C), (2, Reg8::D), (3, Reg8::E),
            (4, Reg8::H), (5, Reg8::L), (7, Reg8::A),
        ];
        for &(bits, r) in &regs {
            let ops = decode_cb_instruction(0x00 | bits);
            assert_eq!(ops, MicrocodeQueue::from_iter([MicroOp::Rlc { dst: r, src: r }]),
                "RLC {:?}", r);
        }
    }

    #[test]
    fn cb_rlc_hl_is_read_modify_write() {
        let ops = decode_cb_instruction(0x06);
        assert_eq!(ops.len(), 3);
        assert_eq!(ops[0], MicroOp::ReadMemReg8 { addr_reg: Reg16::HL, into: Reg8::IR });
        assert_eq!(ops[1], MicroOp::Rlc { dst: Reg8::IR, src: Reg8::IR });
        assert_eq!(ops[2], MicroOp::WriteMemReg8 { addr_reg: Reg16::HL, src: Reg8::IR });
    }

    #[test]
    fn cb_all_shift_rotate_ops_correct_type() {
        // Each group of 8 opcodes uses the same operation type
        // Check the first register (B) of each group
        let expected: [(u8, MicroOp); 8] = [
            (0x00, MicroOp::Rlc  { dst: Reg8::B, src: Reg8::B }),
            (0x08, MicroOp::Rrc  { dst: Reg8::B, src: Reg8::B }),
            (0x10, MicroOp::Rl   { dst: Reg8::B, src: Reg8::B }),
            (0x18, MicroOp::Rr   { dst: Reg8::B, src: Reg8::B }),
            (0x20, MicroOp::Sla  { dst: Reg8::B, src: Reg8::B }),
            (0x28, MicroOp::Sra  { dst: Reg8::B, src: Reg8::B }),
            (0x30, MicroOp::Swap { dst: Reg8::B, src: Reg8::B }),
            (0x38, MicroOp::Srl  { dst: Reg8::B, src: Reg8::B }),
        ];
        for &(opcode, ref expected_op) in &expected {
            let ops = decode_cb_instruction(opcode);
            assert_eq!(ops[0], *expected_op, "CB 0x{:02X}", opcode);
        }
    }

    #[test]
    fn cb_bit_all_64_combinations() {
        // BIT b, r: 8 bits × 8 registers (0x40-0x7F)
        let regs: [(u8, Reg8); 7] = [
            (0, Reg8::B), (1, Reg8::C), (2, Reg8::D), (3, Reg8::E),
            (4, Reg8::H), (5, Reg8::L), (7, Reg8::A),
        ];
        for bit in 0..8u8 {
            for &(reg_bits, r) in &regs {
                let opcode = 0x40 | (bit << 3) | reg_bits;
                let ops = decode_cb_instruction(opcode);
                assert_eq!(ops, MicrocodeQueue::from_iter([MicroOp::BitTest { bit, reg: r }]),
                    "BIT {},{:?} (CB 0x{:02X})", bit, r, opcode);
            }
            // (HL) variant
            let opcode = 0x40 | (bit << 3) | 6;
            let ops = decode_cb_instruction(opcode);
            assert_eq!(ops.len(), 2, "BIT {},(HL)", bit);
            assert_eq!(ops[1], MicroOp::BitTest { bit, reg: Reg8::IR });
        }
    }

    #[test]
    fn cb_res_all_64_combinations() {
        let regs: [(u8, Reg8); 7] = [
            (0, Reg8::B), (1, Reg8::C), (2, Reg8::D), (3, Reg8::E),
            (4, Reg8::H), (5, Reg8::L), (7, Reg8::A),
        ];
        for bit in 0..8u8 {
            for &(reg_bits, r) in &regs {
                let opcode = 0x80 | (bit << 3) | reg_bits;
                let ops = decode_cb_instruction(opcode);
                assert_eq!(ops, MicrocodeQueue::from_iter([MicroOp::ResetBit { bit, reg: r }]),
                    "RES {},{:?} (CB 0x{:02X})", bit, r, opcode);
            }
            // (HL) variant
            let opcode = 0x80 | (bit << 3) | 6;
            let ops = decode_cb_instruction(opcode);
            assert_eq!(ops.len(), 3, "RES {},(HL)", bit);
            assert_eq!(ops[1], MicroOp::ResetBit { bit, reg: Reg8::IR });
        }
    }

    #[test]
    fn cb_set_all_64_combinations() {
        let regs: [(u8, Reg8); 7] = [
            (0, Reg8::B), (1, Reg8::C), (2, Reg8::D), (3, Reg8::E),
            (4, Reg8::H), (5, Reg8::L), (7, Reg8::A),
        ];
        for bit in 0..8u8 {
            for &(reg_bits, r) in &regs {
                let opcode = 0xC0 | (bit << 3) | reg_bits;
                let ops = decode_cb_instruction(opcode);
                assert_eq!(ops, MicrocodeQueue::from_iter([MicroOp::SetBit { bit, reg: r }]),
                    "SET {},{:?} (CB 0x{:02X})", bit, r, opcode);
            }
            // (HL) variant
            let opcode = 0xC0 | (bit << 3) | 6;
            let ops = decode_cb_instruction(opcode);
            assert_eq!(ops.len(), 3, "SET {},(HL)", bit);
            assert_eq!(ops[1], MicroOp::SetBit { bit, reg: Reg8::IR });
        }
    }

    // =====================================================================
    //  End-to-end: compound MicroOps via CPU + Bus
    // =====================================================================

    fn make_cpu() -> (CPU, Bus) {
        (CPU::new(crate::trace::Tracer::off()), Bus::new())
    }

    use crate::cpu::microcode::execute;

    fn write_wram(bus: &mut Bus, addr: u16, val: u8) {
        bus.write(addr, val);
    }

    #[test]
    fn e2e_jump_rel_imm_forward() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::PC, 0xC000);
        write_wram(&mut bus, 0xC000, 0x05); // +5 signed
        execute(&mut cpu, &mut bus, MicroOp::JumpRelImm);
        // PC = 0xC001 (past offset byte) + 5 = 0xC006
        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0xC006);
    }

    #[test]
    fn e2e_jump_rel_imm_backward() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::PC, 0xC010);
        write_wram(&mut bus, 0xC010, 0xFB); // -5 signed
        execute(&mut cpu, &mut bus, MicroOp::JumpRelImm);
        // PC = 0xC011 + (-5) = 0xC00C
        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0xC00C);
    }

    #[test]
    fn e2e_jump_abs_imm() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::PC, 0xC000);
        write_wram(&mut bus, 0xC000, 0x50); // lo
        write_wram(&mut bus, 0xC001, 0xC1); // hi → target = 0xC150
        execute(&mut cpu, &mut bus, MicroOp::JumpAbsImm);
        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0xC150);
    }

    #[test]
    fn e2e_jump_hl() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::HL, 0xBEEF);
        execute(&mut cpu, &mut bus, MicroOp::JumpHL);
        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0xBEEF);
    }

    #[test]
    fn e2e_call_imm_pushes_return_addr() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::PC, 0xC000);
        cpu.register_file.set_16bit(Reg16::SP, 0xDFFE);
        write_wram(&mut bus, 0xC000, 0x00); // lo
        write_wram(&mut bus, 0xC001, 0xC1); // hi → target = 0xC100
        execute(&mut cpu, &mut bus, MicroOp::CallImm);
        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0xC100);
        assert_eq!(cpu.register_file.get_16bit(Reg16::SP), 0xDFFC);
        // Return addr (0xC002) on stack
        let sp = cpu.register_file.get_16bit(Reg16::SP);
        let ret_lo = bus.read(sp);
        let ret_hi = bus.read(sp + 1);
        let ret_addr = u16::from_le_bytes([ret_lo, ret_hi]);
        assert_eq!(ret_addr, 0xC002, "return address should be past the 2-byte operand");
    }

    #[test]
    fn e2e_call_then_ret_roundtrip() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::PC, 0xC000);
        cpu.register_file.set_16bit(Reg16::SP, 0xDFFE);
        write_wram(&mut bus, 0xC000, 0x00);
        write_wram(&mut bus, 0xC001, 0xC4); // target = 0xC400
        execute(&mut cpu, &mut bus, MicroOp::CallImm);
        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0xC400);

        execute(&mut cpu, &mut bus, MicroOp::Ret);
        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0xC002);
        assert_eq!(cpu.register_file.get_16bit(Reg16::SP), 0xDFFE);
    }

    #[test]
    fn e2e_add_sp_imm_positive() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::SP, 0xDFF0);
        cpu.register_file.set_16bit(Reg16::PC, 0xC000);
        write_wram(&mut bus, 0xC000, 0x05); // +5
        execute(&mut cpu, &mut bus, MicroOp::AddSpImm);
        assert_eq!(cpu.register_file.get_16bit(Reg16::SP), 0xDFF5);
        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0xC001);
    }

    #[test]
    fn e2e_add_sp_imm_negative() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::SP, 0xDFF0);
        cpu.register_file.set_16bit(Reg16::PC, 0xC000);
        write_wram(&mut bus, 0xC000, 0xFE); // -2
        execute(&mut cpu, &mut bus, MicroOp::AddSpImm);
        assert_eq!(cpu.register_file.get_16bit(Reg16::SP), 0xDFEE);
    }

    #[test]
    fn e2e_load_hl_sp_imm() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::SP, 0xD000);
        cpu.register_file.set_16bit(Reg16::PC, 0xC000);
        write_wram(&mut bus, 0xC000, 0x10); // +16
        execute(&mut cpu, &mut bus, MicroOp::LoadHlSpImm);
        assert_eq!(cpu.register_file.get_16bit(Reg16::HL), 0xD010);
        assert_eq!(cpu.register_file.get_16bit(Reg16::SP), 0xD000);
    }

    #[test]
    fn e2e_write_sp_16bit_addr() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::SP, 0xABCD);
        cpu.register_file.set_16bit(Reg16::PC, 0xC000);
        write_wram(&mut bus, 0xC000, 0x00); // addr lo
        write_wram(&mut bus, 0xC001, 0xC1); // addr hi → 0xC100
        execute(&mut cpu, &mut bus, MicroOp::WriteSp16BitAddr);
        assert_eq!(bus.read(0xC100), 0xCD); // SP lo
        assert_eq!(bus.read(0xC101), 0xAB); // SP hi
        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0xC002);
    }

    #[test]
    fn e2e_read_mem_16bit_addr() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::PC, 0xC000);
        write_wram(&mut bus, 0xC000, 0x50); // addr lo
        write_wram(&mut bus, 0xC001, 0xC0); // addr hi → 0xC050
        write_wram(&mut bus, 0xC050, 0x42); // data
        execute(&mut cpu, &mut bus, MicroOp::ReadMem16BitAddr { into: Reg8::A });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x42);
        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0xC002);
    }

    #[test]
    fn e2e_write_mem_16bit_addr() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x99);
        cpu.register_file.set_16bit(Reg16::PC, 0xC000);
        write_wram(&mut bus, 0xC000, 0x20); // addr lo
        write_wram(&mut bus, 0xC001, 0xC0); // addr hi → 0xC020
        execute(&mut cpu, &mut bus, MicroOp::WriteMem16BitAddr { src: Reg8::A });
        assert_eq!(bus.read(0xC020), 0x99);
        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0xC002);
    }

    // =====================================================================
    //  End-to-end: ALU new ops (ADC, SBC, CP)
    // =====================================================================

    #[test]
    fn e2e_adc_with_carry_set() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x10);
        cpu.register_file.set_8bit(Reg8::F, FLAG_C); // carry = 1
        execute(&mut cpu, &mut bus, MicroOp::Alu8 {
            kind: AluOpKind::Adc, dest: Reg8::A, src: Operand8::Imm(0x05),
        });
        // 0x10 + 0x05 + 1 = 0x16
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x16);
    }

    #[test]
    fn e2e_adc_without_carry() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x10);
        cpu.register_file.set_8bit(Reg8::F, 0); // carry = 0
        execute(&mut cpu, &mut bus, MicroOp::Alu8 {
            kind: AluOpKind::Adc, dest: Reg8::A, src: Operand8::Imm(0x05),
        });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x15);
    }

    #[test]
    fn e2e_adc_overflow_sets_carry() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0xFF);
        cpu.register_file.set_8bit(Reg8::F, FLAG_C);
        execute(&mut cpu, &mut bus, MicroOp::Alu8 {
            kind: AluOpKind::Adc, dest: Reg8::A, src: Operand8::Imm(0x00),
        });
        // 0xFF + 0 + 1 = 0x100 → A=0x00, C=1, Z=1
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x00);
        let f = cpu.register_file.get_8bit(Reg8::F);
        assert_ne!(f & FLAG_Z, 0, "Z should be set");
        assert_ne!(f & FLAG_C, 0, "C should be set");
    }

    #[test]
    fn e2e_adc_half_carry() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x0F);
        cpu.register_file.set_8bit(Reg8::F, FLAG_C);
        execute(&mut cpu, &mut bus, MicroOp::Alu8 {
            kind: AluOpKind::Adc, dest: Reg8::A, src: Operand8::Imm(0x00),
        });
        // 0x0F + 0 + 1 = 0x10, H should be set
        let f = cpu.register_file.get_8bit(Reg8::F);
        assert_ne!(f & FLAG_H, 0, "H should be set");
    }

    #[test]
    fn e2e_sbc_with_carry() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x10);
        cpu.register_file.set_8bit(Reg8::F, FLAG_C);
        execute(&mut cpu, &mut bus, MicroOp::Alu8 {
            kind: AluOpKind::Sbc, dest: Reg8::A, src: Operand8::Imm(0x05),
        });
        // 0x10 - 0x05 - 1 = 0x0A
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x0A);
    }

    #[test]
    fn e2e_sbc_underflow() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x00);
        cpu.register_file.set_8bit(Reg8::F, FLAG_C);
        execute(&mut cpu, &mut bus, MicroOp::Alu8 {
            kind: AluOpKind::Sbc, dest: Reg8::A, src: Operand8::Imm(0x00),
        });
        // 0x00 - 0x00 - 1 = 0xFF, C=1, H=1
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0xFF);
        let f = cpu.register_file.get_8bit(Reg8::F);
        assert_ne!(f & FLAG_C, 0);
        assert_ne!(f & FLAG_H, 0);
    }

    #[test]
    fn e2e_cp_does_not_modify_a() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x42);
        execute(&mut cpu, &mut bus, MicroOp::Alu8 {
            kind: AluOpKind::Cp, dest: Reg8::A, src: Operand8::Imm(0x42),
        });
        // A should remain 0x42 (CP doesn't store)
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x42);
        // But Z should be set (equal)
        let f = cpu.register_file.get_8bit(Reg8::F);
        assert_ne!(f & FLAG_Z, 0, "Z should be set for equal values");
        assert_ne!(f & FLAG_N, 0, "N always set for CP");
    }

    #[test]
    fn e2e_cp_sets_carry_when_b_greater() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_8bit(Reg8::A, 0x10);
        execute(&mut cpu, &mut bus, MicroOp::Alu8 {
            kind: AluOpKind::Cp, dest: Reg8::A, src: Operand8::Imm(0x20),
        });
        assert_eq!(cpu.register_file.get_8bit(Reg8::A), 0x10); // unchanged
        let f = cpu.register_file.get_8bit(Reg8::F);
        assert_ne!(f & FLAG_C, 0, "C should be set when b > a");
        assert_eq!(f & FLAG_Z, 0, "Z should not be set");
    }

    #[test]
    fn e2e_stop_consumes_second_byte() {
        let (mut cpu, mut bus) = make_cpu();
        cpu.register_file.set_16bit(Reg16::PC, 0xC000);
        // Place STOP's second byte (0x00) at 0xC000
        write_wram(&mut bus, 0xC000, 0x00);
        // Execute both micro-ops: ReadImmediate8 (consumes 0x00), TriggerStop
        let ops = decode_instruction(0x10);
        assert_eq!(ops.len(), 2);
        for op in ops {
            execute(&mut cpu, &mut bus, op);
        }
        // PC should have advanced past the second byte
        assert_eq!(cpu.register_file.get_16bit(Reg16::PC), 0xC001);
        // CPU should be halted
        assert!(cpu.halted);
    }
}

pub fn decode_instruction(opcode: u8) -> MicrocodeQueue {
    // Extract common bit fields
    let y = (opcode >> 3) & 0x07; // bits 5-3
    let z = opcode & 0x07;        // bits 2-0
    let p = (y >> 1) & 0x03;      // bits 5-4

    match opcode {
        // ==================================================================
        // 0x00-0x3F: Misc / Loads / Inc-Dec / Rotates A
        // ==================================================================
        0x00 => MicrocodeQueue::new(), // NOP

        // LD rr, nn (16-bit immediate load)
        0x01 | 0x11 | 0x21 | 0x31 => {
            let rr = reg16_from_bits(p);
            MicrocodeQueue::from_iter([MicroOp::ReadImmediate16 { into: rr }])
        }

        // LD (BC), A
        0x02 => MicrocodeQueue::from_iter([MicroOp::WriteMemReg8 { addr_reg: Reg16::BC, src: Reg8::A }]),
        // LD (DE), A
        0x12 => MicrocodeQueue::from_iter([MicroOp::WriteMemReg8 { addr_reg: Reg16::DE, src: Reg8::A }]),
        // LD (HL+), A
        0x22 => MicrocodeQueue::from_iter([
            MicroOp::WriteMemReg8 { addr_reg: Reg16::HL, src: Reg8::A },
            MicroOp::Inc16 { reg: Reg16::HL },
        ]),
        // LD (HL-), A
        0x32 => MicrocodeQueue::from_iter([
            MicroOp::WriteMemReg8 { addr_reg: Reg16::HL, src: Reg8::A },
            MicroOp::Dec16 { reg: Reg16::HL },
        ]),

        // INC rr (16-bit)
        0x03 | 0x13 | 0x23 | 0x33 => {
            let rr = reg16_from_bits(p);
            MicrocodeQueue::from_iter([MicroOp::Inc16 { reg: rr }])
        }

        // DEC rr (16-bit)
        0x0B | 0x1B | 0x2B | 0x3B => {
            let rr = reg16_from_bits(p);
            MicrocodeQueue::from_iter([MicroOp::Dec16 { reg: rr }])
        }

        // INC r (8-bit) — y encodes the register
        0x04 | 0x0C | 0x14 | 0x1C | 0x24 | 0x2C | 0x3C => {
            let r = reg8_from_bits(y);
            MicrocodeQueue::from_iter([MicroOp::Inc8 { target: Operand8::Reg(r) }])
        }
        // INC (HL)
        0x34 => MicrocodeQueue::from_iter([
            MicroOp::ReadMemReg8 { addr_reg: Reg16::HL, into: Reg8::IR },
            MicroOp::Inc8 { target: Operand8::Reg(Reg8::IR) },
            MicroOp::WriteMemReg8 { addr_reg: Reg16::HL, src: Reg8::IR },
        ]),

        // DEC r (8-bit)
        0x05 | 0x0D | 0x15 | 0x1D | 0x25 | 0x2D | 0x3D => {
            let r = reg8_from_bits(y);
            MicrocodeQueue::from_iter([MicroOp::Dec8 { target: Operand8::Reg(r) }])
        }
        // DEC (HL)
        0x35 => MicrocodeQueue::from_iter([
            MicroOp::ReadMemReg8 { addr_reg: Reg16::HL, into: Reg8::IR },
            MicroOp::Dec8 { target: Operand8::Reg(Reg8::IR) },
            MicroOp::WriteMemReg8 { addr_reg: Reg16::HL, src: Reg8::IR },
        ]),

        // LD r, n (8-bit immediate load)
        0x06 | 0x0E | 0x16 | 0x1E | 0x26 | 0x2E | 0x3E => {
            let r = reg8_from_bits(y);
            MicrocodeQueue::from_iter([MicroOp::ReadImmediate8 { into: r }])
        }
        // LD (HL), n
        0x36 => MicrocodeQueue::from_iter([
            MicroOp::ReadImmediate8 { into: Reg8::IR },
            MicroOp::WriteMemReg8 { addr_reg: Reg16::HL, src: Reg8::IR },
        ]),

        // Rotates on A (non-CB versions — always Z=0, N=0, H=0, C=result)
        0x07 => MicrocodeQueue::from_iter([MicroOp::RlcA]),  // RLCA
        0x0F => MicrocodeQueue::from_iter([MicroOp::RrcA]),  // RRCA
        0x17 => MicrocodeQueue::from_iter([MicroOp::RlA]),   // RLA
        0x1F => MicrocodeQueue::from_iter([MicroOp::RrA]),   // RRA

        // LD (nn), SP
        0x08 => MicrocodeQueue::from_iter([MicroOp::WriteSp16BitAddr]),

        // ADD HL, rr
        0x09 | 0x19 | 0x29 | 0x39 => {
            let rr = reg16_from_bits(p);
            MicrocodeQueue::from_iter([MicroOp::Add16 { dest: Reg16::HL, src: rr }])
        }

        // LD A, (BC)
        0x0A => MicrocodeQueue::from_iter([MicroOp::ReadMemReg8 { addr_reg: Reg16::BC, into: Reg8::A }]),
        // LD A, (DE)
        0x1A => MicrocodeQueue::from_iter([MicroOp::ReadMemReg8 { addr_reg: Reg16::DE, into: Reg8::A }]),
        // LD A, (HL+)
        0x2A => MicrocodeQueue::from_iter([
            MicroOp::ReadMemReg8 { addr_reg: Reg16::HL, into: Reg8::A },
            MicroOp::Inc16 { reg: Reg16::HL },
        ]),
        // LD A, (HL-)
        0x3A => MicrocodeQueue::from_iter([
            MicroOp::ReadMemReg8 { addr_reg: Reg16::HL, into: Reg8::A },
            MicroOp::Dec16 { reg: Reg16::HL },
        ]),

        // JR e (unconditional relative jump)
        0x18 => MicrocodeQueue::from_iter([MicroOp::JumpRelImm]),

        // JR cc, e (conditional relative jump)
        0x20 => MicrocodeQueue::from_iter([MicroOp::CheckCond { cond: Condition::NZ }, MicroOp::JumpRelImm]),
        0x28 => MicrocodeQueue::from_iter([MicroOp::CheckCond { cond: Condition::Z  }, MicroOp::JumpRelImm]),
        0x30 => MicrocodeQueue::from_iter([MicroOp::CheckCond { cond: Condition::NC }, MicroOp::JumpRelImm]),
        0x38 => MicrocodeQueue::from_iter([MicroOp::CheckCond { cond: Condition::C  }, MicroOp::JumpRelImm]),

        // DAA
        0x27 => MicrocodeQueue::from_iter([MicroOp::Daa]),
        // CPL
        0x2F => MicrocodeQueue::from_iter([MicroOp::Cpl]),
        // SCF
        0x37 => MicrocodeQueue::from_iter([MicroOp::Scf]),
        // CCF
        0x3F => MicrocodeQueue::from_iter([MicroOp::Ccf]),

        // STOP (2-byte instruction: 0x10 0x00 — consume the second byte)
        0x10 => MicrocodeQueue::from_iter([
            MicroOp::ReadImmediate8 { into: Reg8::IR },
            MicroOp::TriggerStop,
        ]),

        // ==================================================================
        // 0x40-0x7F: LD r, r' block (and HALT)
        // ==================================================================
        0x76 => MicrocodeQueue::from_iter([MicroOp::TriggerHalt]), // HALT

        // LD r, (HL) — dest from y bits, src is memory at HL
        0x46 | 0x4E | 0x56 | 0x5E | 0x66 | 0x6E | 0x7E => {
            let dst = reg8_from_bits(y);
            MicrocodeQueue::from_iter([MicroOp::ReadMemReg8 { addr_reg: Reg16::HL, into: dst }])
        }

        // LD (HL), r — src from z bits, dest is memory at HL
        0x70 | 0x71 | 0x72 | 0x73 | 0x74 | 0x75 | 0x77 => {
            let src = reg8_from_bits(z);
            MicrocodeQueue::from_iter([MicroOp::WriteMemReg8 { addr_reg: Reg16::HL, src }])
        }

        // LD r, r' — both are register-to-register (x=1, z!=6, y!=6)
        0x40..=0x7F => {
            let dst = reg8_from_bits(y);
            let src = reg8_from_bits(z);
            MicrocodeQueue::from_iter([MicroOp::LoadReg8 { dst, src }])
        }

        // ==================================================================
        // 0x80-0xBF: ALU A, r block
        // ==================================================================

        // ALU A, (HL) — z=6 means memory operand
        0x86 | 0x8E | 0x96 | 0x9E | 0xA6 | 0xAE | 0xB6 | 0xBE => {
            let kind = alu_op_from_bits(y);
            MicrocodeQueue::from_iter([
                MicroOp::ReadMemReg8 { addr_reg: Reg16::HL, into: Reg8::IR },
                MicroOp::Alu8 { kind, dest: Reg8::A, src: Operand8::Reg(Reg8::IR) },
            ])
        }

        // ALU A, r — register operand
        0x80..=0xBF => {
            let kind = alu_op_from_bits(y);
            let r = reg8_from_bits(z);
            MicrocodeQueue::from_iter([MicroOp::Alu8 { kind, dest: Reg8::A, src: Operand8::Reg(r) }])
        }

        // ==================================================================
        // 0xC0-0xFF: Control flow, stack, misc
        // ==================================================================

        // RET cc
        0xC0 => MicrocodeQueue::from_iter([MicroOp::CheckCond { cond: Condition::NZ }, MicroOp::Ret]),
        0xC8 => MicrocodeQueue::from_iter([MicroOp::CheckCond { cond: Condition::Z  }, MicroOp::Ret]),
        0xD0 => MicrocodeQueue::from_iter([MicroOp::CheckCond { cond: Condition::NC }, MicroOp::Ret]),
        0xD8 => MicrocodeQueue::from_iter([MicroOp::CheckCond { cond: Condition::C  }, MicroOp::Ret]),

        // POP rr
        0xC1 | 0xD1 | 0xE1 | 0xF1 => {
            let rr = reg16_push_pop(p);
            MicrocodeQueue::from_iter([MicroOp::Pop { dst: rr }])
        }

        // JP cc, nn
        0xC2 => MicrocodeQueue::from_iter([MicroOp::CheckCond { cond: Condition::NZ }, MicroOp::JumpAbsImm]),
        0xCA => MicrocodeQueue::from_iter([MicroOp::CheckCond { cond: Condition::Z  }, MicroOp::JumpAbsImm]),
        0xD2 => MicrocodeQueue::from_iter([MicroOp::CheckCond { cond: Condition::NC }, MicroOp::JumpAbsImm]),
        0xDA => MicrocodeQueue::from_iter([MicroOp::CheckCond { cond: Condition::C  }, MicroOp::JumpAbsImm]),

        // JP nn (unconditional)
        0xC3 => MicrocodeQueue::from_iter([MicroOp::JumpAbsImm]),

        // CALL cc, nn
        0xC4 => MicrocodeQueue::from_iter([MicroOp::CheckCond { cond: Condition::NZ }, MicroOp::CallImm]),
        0xCC => MicrocodeQueue::from_iter([MicroOp::CheckCond { cond: Condition::Z  }, MicroOp::CallImm]),
        0xD4 => MicrocodeQueue::from_iter([MicroOp::CheckCond { cond: Condition::NC }, MicroOp::CallImm]),
        0xDC => MicrocodeQueue::from_iter([MicroOp::CheckCond { cond: Condition::C  }, MicroOp::CallImm]),

        // PUSH rr
        0xC5 | 0xD5 | 0xE5 | 0xF5 => {
            let rr = reg16_push_pop(p);
            MicrocodeQueue::from_iter([MicroOp::Push { src: rr }])
        }

        // ALU A, n (immediate operand)
        0xC6 => MicrocodeQueue::from_iter([
            MicroOp::ReadImmediate8 { into: Reg8::IR },
            MicroOp::Alu8 { kind: AluOpKind::Add, dest: Reg8::A, src: Operand8::Reg(Reg8::IR) },
        ]),
        0xCE => MicrocodeQueue::from_iter([
            MicroOp::ReadImmediate8 { into: Reg8::IR },
            MicroOp::Alu8 { kind: AluOpKind::Adc, dest: Reg8::A, src: Operand8::Reg(Reg8::IR) },
        ]),
        0xD6 => MicrocodeQueue::from_iter([
            MicroOp::ReadImmediate8 { into: Reg8::IR },
            MicroOp::Alu8 { kind: AluOpKind::Sub, dest: Reg8::A, src: Operand8::Reg(Reg8::IR) },
        ]),
        0xDE => MicrocodeQueue::from_iter([
            MicroOp::ReadImmediate8 { into: Reg8::IR },
            MicroOp::Alu8 { kind: AluOpKind::Sbc, dest: Reg8::A, src: Operand8::Reg(Reg8::IR) },
        ]),
        0xE6 => MicrocodeQueue::from_iter([
            MicroOp::ReadImmediate8 { into: Reg8::IR },
            MicroOp::Alu8 { kind: AluOpKind::And, dest: Reg8::A, src: Operand8::Reg(Reg8::IR) },
        ]),
        0xEE => MicrocodeQueue::from_iter([
            MicroOp::ReadImmediate8 { into: Reg8::IR },
            MicroOp::Alu8 { kind: AluOpKind::Xor, dest: Reg8::A, src: Operand8::Reg(Reg8::IR) },
        ]),
        0xF6 => MicrocodeQueue::from_iter([
            MicroOp::ReadImmediate8 { into: Reg8::IR },
            MicroOp::Alu8 { kind: AluOpKind::Or, dest: Reg8::A, src: Operand8::Reg(Reg8::IR) },
        ]),
        0xFE => MicrocodeQueue::from_iter([
            MicroOp::ReadImmediate8 { into: Reg8::IR },
            MicroOp::Alu8 { kind: AluOpKind::Cp, dest: Reg8::A, src: Operand8::Reg(Reg8::IR) },
        ]),

        // RST vectors
        0xC7 => MicrocodeQueue::from_iter([MicroOp::Rst { addr: 0x00 }]),
        0xCF => MicrocodeQueue::from_iter([MicroOp::Rst { addr: 0x08 }]),
        0xD7 => MicrocodeQueue::from_iter([MicroOp::Rst { addr: 0x10 }]),
        0xDF => MicrocodeQueue::from_iter([MicroOp::Rst { addr: 0x18 }]),
        0xE7 => MicrocodeQueue::from_iter([MicroOp::Rst { addr: 0x20 }]),
        0xEF => MicrocodeQueue::from_iter([MicroOp::Rst { addr: 0x28 }]),
        0xF7 => MicrocodeQueue::from_iter([MicroOp::Rst { addr: 0x30 }]),
        0xFF => MicrocodeQueue::from_iter([MicroOp::Rst { addr: 0x38 }]),

        // RET
        0xC9 => MicrocodeQueue::from_iter([MicroOp::Ret]),
        // RETI
        0xD9 => MicrocodeQueue::from_iter([MicroOp::RetI]),

        // CALL nn
        0xCD => MicrocodeQueue::from_iter([MicroOp::CallImm]),

        // CB prefix — handled by pipeline, should never arrive here
        0xCB => panic!("CB prefix should be handled by pipeline, not base decoder"),

        // LDH (n), A — write A to 0xFF00+n
        0xE0 => MicrocodeQueue::from_iter([
            MicroOp::ReadImmediate8 { into: Reg8::IR },
            MicroOp::WriteHighPage { offset: Reg8::IR, src: Reg8::A },
        ]),
        // LDH A, (n) — read from 0xFF00+n into A
        0xF0 => MicrocodeQueue::from_iter([
            MicroOp::ReadImmediate8 { into: Reg8::IR },
            MicroOp::ReadHighPage { offset: Reg8::IR, into: Reg8::A },
        ]),

        // LD (C), A — write A to 0xFF00+C
        0xE2 => MicrocodeQueue::from_iter([MicroOp::WriteHighPage { offset: Reg8::C, src: Reg8::A }]),
        // LD A, (C) — read from 0xFF00+C into A
        0xF2 => MicrocodeQueue::from_iter([MicroOp::ReadHighPage { offset: Reg8::C, into: Reg8::A }]),

        // LD (nn), A
        0xEA => MicrocodeQueue::from_iter([MicroOp::WriteMem16BitAddr { src: Reg8::A }]),
        // LD A, (nn)
        0xFA => MicrocodeQueue::from_iter([MicroOp::ReadMem16BitAddr { into: Reg8::A }]),

        // ADD SP, e
        0xE8 => MicrocodeQueue::from_iter([MicroOp::AddSpImm]),
        // LD HL, SP+e
        0xF8 => MicrocodeQueue::from_iter([MicroOp::LoadHlSpImm]),

        // JP HL
        0xE9 => MicrocodeQueue::from_iter([MicroOp::JumpHL]),

        // LD SP, HL
        0xF9 => MicrocodeQueue::from_iter([MicroOp::LoadReg16 { dst: Reg16::SP, src: Reg16::HL }]),

        // DI — disable interrupts
        0xF3 => MicrocodeQueue::from_iter([MicroOp::SetIme { value: false }]),
        // EI — enable interrupts (deferred)
        0xFB => MicrocodeQueue::from_iter([MicroOp::DeferImeEnable]),

        // Undefined opcodes (0xD3, 0xDB, 0xDD, 0xE3, 0xE4, 0xEB, 0xEC, 0xED, 0xF4, 0xFC, 0xFD)
        // Real SM83 locks up on these. Lock the CPU rather than panicking.
        0xD3 | 0xDB | 0xDD | 0xE3 | 0xE4 | 0xEB | 0xEC | 0xED | 0xF4 | 0xFC | 0xFD =>
            MicrocodeQueue::from_iter([MicroOp::TriggerHalt]),
    }
}

pub fn decode_cb_instruction(opcode: u8) -> MicrocodeQueue {
    let reg_bits = opcode & 0x07;
    let is_hl = reg_bits == 6;

    match opcode {
        // ==================================================================
        // 0x00-0x07: RLC r
        // ==================================================================
        0x00..=0x05 | 0x07 => {
            let r = reg8_from_bits(reg_bits);
            MicrocodeQueue::from_iter([MicroOp::Rlc { dst: r, src: r }])
        }
        0x06 => MicrocodeQueue::from_iter([
            MicroOp::ReadMemReg8 { addr_reg: Reg16::HL, into: Reg8::IR },
            MicroOp::Rlc { dst: Reg8::IR, src: Reg8::IR },
            MicroOp::WriteMemReg8 { addr_reg: Reg16::HL, src: Reg8::IR },
        ]),

        // 0x08-0x0F: RRC r
        0x08..=0x0D | 0x0F => {
            let r = reg8_from_bits(reg_bits);
            MicrocodeQueue::from_iter([MicroOp::Rrc { dst: r, src: r }])
        }
        0x0E => MicrocodeQueue::from_iter([
            MicroOp::ReadMemReg8 { addr_reg: Reg16::HL, into: Reg8::IR },
            MicroOp::Rrc { dst: Reg8::IR, src: Reg8::IR },
            MicroOp::WriteMemReg8 { addr_reg: Reg16::HL, src: Reg8::IR },
        ]),

        // 0x10-0x17: RL r
        0x10..=0x15 | 0x17 => {
            let r = reg8_from_bits(reg_bits);
            MicrocodeQueue::from_iter([MicroOp::Rl { dst: r, src: r }])
        }
        0x16 => MicrocodeQueue::from_iter([
            MicroOp::ReadMemReg8 { addr_reg: Reg16::HL, into: Reg8::IR },
            MicroOp::Rl { dst: Reg8::IR, src: Reg8::IR },
            MicroOp::WriteMemReg8 { addr_reg: Reg16::HL, src: Reg8::IR },
        ]),

        // 0x18-0x1F: RR r
        0x18..=0x1D | 0x1F => {
            let r = reg8_from_bits(reg_bits);
            MicrocodeQueue::from_iter([MicroOp::Rr { dst: r, src: r }])
        }
        0x1E => MicrocodeQueue::from_iter([
            MicroOp::ReadMemReg8 { addr_reg: Reg16::HL, into: Reg8::IR },
            MicroOp::Rr { dst: Reg8::IR, src: Reg8::IR },
            MicroOp::WriteMemReg8 { addr_reg: Reg16::HL, src: Reg8::IR },
        ]),

        // 0x20-0x27: SLA r
        0x20..=0x25 | 0x27 => {
            let r = reg8_from_bits(reg_bits);
            MicrocodeQueue::from_iter([MicroOp::Sla { dst: r, src: r }])
        }
        0x26 => MicrocodeQueue::from_iter([
            MicroOp::ReadMemReg8 { addr_reg: Reg16::HL, into: Reg8::IR },
            MicroOp::Sla { dst: Reg8::IR, src: Reg8::IR },
            MicroOp::WriteMemReg8 { addr_reg: Reg16::HL, src: Reg8::IR },
        ]),

        // 0x28-0x2F: SRA r
        0x28..=0x2D | 0x2F => {
            let r = reg8_from_bits(reg_bits);
            MicrocodeQueue::from_iter([MicroOp::Sra { dst: r, src: r }])
        }
        0x2E => MicrocodeQueue::from_iter([
            MicroOp::ReadMemReg8 { addr_reg: Reg16::HL, into: Reg8::IR },
            MicroOp::Sra { dst: Reg8::IR, src: Reg8::IR },
            MicroOp::WriteMemReg8 { addr_reg: Reg16::HL, src: Reg8::IR },
        ]),

        // 0x30-0x37: SWAP r
        0x30..=0x35 | 0x37 => {
            let r = reg8_from_bits(reg_bits);
            MicrocodeQueue::from_iter([MicroOp::Swap { dst: r, src: r }])
        }
        0x36 => MicrocodeQueue::from_iter([
            MicroOp::ReadMemReg8 { addr_reg: Reg16::HL, into: Reg8::IR },
            MicroOp::Swap { dst: Reg8::IR, src: Reg8::IR },
            MicroOp::WriteMemReg8 { addr_reg: Reg16::HL, src: Reg8::IR },
        ]),

        // 0x38-0x3F: SRL r
        0x38..=0x3D | 0x3F => {
            let r = reg8_from_bits(reg_bits);
            MicrocodeQueue::from_iter([MicroOp::Srl { dst: r, src: r }])
        }
        0x3E => MicrocodeQueue::from_iter([
            MicroOp::ReadMemReg8 { addr_reg: Reg16::HL, into: Reg8::IR },
            MicroOp::Srl { dst: Reg8::IR, src: Reg8::IR },
            MicroOp::WriteMemReg8 { addr_reg: Reg16::HL, src: Reg8::IR },
        ]),

        // 0x40-0x7F: BIT b, r
        0x40..=0x7F => {
            let bit = (opcode >> 3) & 0x07;
            if is_hl {
                MicrocodeQueue::from_iter([
                    MicroOp::ReadMemReg8 { addr_reg: Reg16::HL, into: Reg8::IR },
                    MicroOp::BitTest { bit, reg: Reg8::IR },
                ])
            } else {
                let r = reg8_from_bits(reg_bits);
                MicrocodeQueue::from_iter([MicroOp::BitTest { bit, reg: r }])
            }
        }

        // 0x80-0xBF: RES b, r
        0x80..=0xBF => {
            let bit = (opcode >> 3) & 0x07;
            if is_hl {
                MicrocodeQueue::from_iter([
                    MicroOp::ReadMemReg8 { addr_reg: Reg16::HL, into: Reg8::IR },
                    MicroOp::ResetBit { bit, reg: Reg8::IR },
                    MicroOp::WriteMemReg8 { addr_reg: Reg16::HL, src: Reg8::IR },
                ])
            } else {
                let r = reg8_from_bits(reg_bits);
                MicrocodeQueue::from_iter([MicroOp::ResetBit { bit, reg: r }])
            }
        }

        // 0xC0-0xFF: SET b, r
        0xC0..=0xFF => {
            let bit = (opcode >> 3) & 0x07;
            if is_hl {
                MicrocodeQueue::from_iter([
                    MicroOp::ReadMemReg8 { addr_reg: Reg16::HL, into: Reg8::IR },
                    MicroOp::SetBit { bit, reg: Reg8::IR },
                    MicroOp::WriteMemReg8 { addr_reg: Reg16::HL, src: Reg8::IR },
                ])
            } else {
                let r = reg8_from_bits(reg_bits);
                MicrocodeQueue::from_iter([MicroOp::SetBit { bit, reg: r }])
            }
        }
    }
}
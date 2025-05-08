use crate::cpu::microcode::*;
use crate::cpu::registers::{ Reg16, Reg8};
use crate::cpu::alu::AluOpKind;

use std::collections::VecDeque;
type MicrocodeQueue = VecDeque<MicroOp>;

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

macro_rules! gen_cycles_lookup {
    ($fn_name:ident, $table:ident) => {
        #[inline(always)]
        pub fn $fn_name(opcode: u8) -> u8 {
            unsafe { *$table.get_unchecked(opcode as usize) }
        }
    };
}

gen_cycles_lookup!(t_cycles,    T_CYCLES);
gen_cycles_lookup!(cb_t_cycles, CB_T_CYCLES);
gen_cycles_lookup!(m_cycles,    M_CYCLES);
gen_cycles_lookup!(cb_m_cycles, CB_M_CYCLES);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vs_raw_lookup() {
        for opcode in 0u8..=255 {
            let idx = opcode as usize;

            assert_eq!(
                t_cycles(opcode),
                T_CYCLES[idx],
                "t_cycles(0x{:02X}) != T_CYCLES[{}]",
                opcode,
                idx
            );
            assert_eq!(
                cb_t_cycles(opcode),
                CB_T_CYCLES[idx],
                "cb_t_cycles(0x{:02X}) != CB_T_CYCLES[{}]",
                opcode,
                idx
            );
        }
    }

    #[test]
    fn test_m_lookup() {
        for opcode in 0u8..=255 {
            let idx = opcode as usize;
            
            assert_eq!(
                m_cycles(opcode),
                T_CYCLES[idx] / 4,
                "m_cycles(0x{:02X}) != T_CYCLES[{}]/4",
                opcode,
                idx
            );
            assert_eq!(
                cb_m_cycles(opcode),
                CB_T_CYCLES[idx] / 4,
                "cb_m_cycles(0x{:02X}) != CB_T_CYCLES[{}]/4",
                opcode,
                idx
            );
        }
    }
}

/// PANICS
pub fn decode_instruction(opcode: u8) -> MicrocodeQueue {
    match opcode {
        0x00 => VecDeque::new(), // NOP
        0x21 => VecDeque::from([ // LD HL, nn16
            MicroOp::ReadImmediate16 { into: Reg16::HL },
        ]),
        0x31 => VecDeque::from([ // LD SP, nn16
            MicroOp::ReadImmediate16 { into: Reg16::SP },
        ]),
        0x32 => VecDeque::from([ // LD (HL-),A
            MicroOp::WriteMemReg8 { addr_reg: Reg16::HL, src: Reg8::A },
            MicroOp::Dec16        { reg: Reg16::HL },
        ]),
        0x3E => VecDeque::from([ // LD A, n8
            MicroOp::ReadImmediate8 { into: Reg8::A },
        ]),
        0xAF => VecDeque::from([ // XOR A
            MicroOp::Alu8 {
                kind: AluOpKind::Xor,
                dest: Reg8::A,
                src:  Operand8::Reg(Reg8::A),
            },
        ]),
        0xCB => panic!("CB should be matched from here!"),
        _ => panic!("Unimplemented opcode: {:02X}", opcode),
    }
}

/// PANICS
pub fn decode_cb_instruction(opcode: u8) -> MicrocodeQueue {
    match opcode {
        0x7C => VecDeque::from([ // BIT 7,H
            MicroOp::BitTest { bit: 7, reg: Reg8::H },
        ]),
        _ => panic!("Unimplemented CB prefixed opcode: {:02X}", opcode),
    }
}
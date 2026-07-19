//! Canonical T-cycle counts per instruction (Pandocs values).
//!
//! Note: these deliberately do NOT mirror `sm83`'s internal `T_CYCLES`
//! tables, which carry pipeline bookkeeping values that differ from the
//! architectural counts (e.g. JP/CALL/RST). This table is the
//! hardware-documented cost, for display and analysis.

/// T-cycles for one instruction. `taken == not_taken` for everything
/// except conditional JR/JP/CALL/RET.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cycles {
    pub taken: u8,
    pub not_taken: u8,
}

impl Cycles {
    const fn both(t: u8) -> Self {
        Self { taken: t, not_taken: t }
    }
    const fn cond(taken: u8, not_taken: u8) -> Self {
        Self { taken, not_taken }
    }
}

/// Cycle count for the instruction starting with opcode `op`; `cb` is the
/// second byte when `op == 0xCB`. Illegal opcodes return 0/0.
pub fn cycles(op: u8, cb: Option<u8>) -> Cycles {
    if op == 0xCB {
        return cb_cycles(cb.unwrap_or(0));
    }
    if crate::decode::ILLEGAL_OPCODES.contains(&op) {
        return Cycles::both(0);
    }

    let x = op >> 6;
    let y = (op >> 3) & 7;
    let z = op & 7;
    let q = y & 1;

    match (x, z) {
        (0, 0) => match y {
            0 | 2 => Cycles::both(4),            // NOP, STOP
            1 => Cycles::both(20),               // LD (a16),SP
            3 => Cycles::both(12),               // JR
            _ => Cycles::cond(12, 8),            // JR cc
        },
        (0, 1) => {
            if q == 0 { Cycles::both(12) } else { Cycles::both(8) } // LD rr,nn / ADD HL,rr
        }
        (0, 2) => Cycles::both(8),               // LD (rr),A / LD A,(rr)
        (0, 3) => Cycles::both(8),               // INC/DEC rr
        (0, 4) | (0, 5) => {
            if y == 6 { Cycles::both(12) } else { Cycles::both(4) } // INC/DEC r / (HL)
        }
        (0, 6) => {
            if y == 6 { Cycles::both(12) } else { Cycles::both(8) } // LD r,n / LD (HL),n
        }
        (0, 7) => Cycles::both(4),               // rotates A / DAA / CPL / SCF / CCF
        (1, _) => {
            if y == 6 && z == 6 {
                Cycles::both(4)                  // HALT
            } else if y == 6 || z == 6 {
                Cycles::both(8)                  // LD r,(HL) / LD (HL),r
            } else {
                Cycles::both(4)                  // LD r,r
            }
        }
        (2, _) => {
            if z == 6 { Cycles::both(8) } else { Cycles::both(4) } // ALU r / (HL)
        }
        (3, 0) => match y {
            0..=3 => Cycles::cond(20, 8),        // RET cc
            4 | 6 => Cycles::both(12),           // LDH (a8),A / LDH A,(a8)
            5 => Cycles::both(16),               // ADD SP,e8
            _ => Cycles::both(12),               // LD HL,SP+e8
        },
        (3, 1) => {
            if q == 0 {
                Cycles::both(12)                 // POP
            } else {
                match y >> 1 {
                    0 | 1 => Cycles::both(16),   // RET / RETI
                    2 => Cycles::both(4),        // JP HL
                    _ => Cycles::both(8),        // LD SP,HL
                }
            }
        }
        (3, 2) => match y {
            0..=3 => Cycles::cond(16, 12),       // JP cc
            4 | 6 => Cycles::both(8),            // LDH (C),A / LDH A,(C)
            _ => Cycles::both(16),               // LD (a16),A / LD A,(a16)
        },
        (3, 3) => match y {
            0 => Cycles::both(16),               // JP a16
            _ => Cycles::both(4),                // DI / EI (CB handled above)
        },
        (3, 4) => Cycles::cond(24, 12),          // CALL cc
        (3, 5) => {
            if q == 0 { Cycles::both(16) } else { Cycles::both(24) } // PUSH / CALL
        }
        (3, 6) => Cycles::both(8),               // ALU n
        (3, 7) => Cycles::both(16),              // RST
        _ => unreachable!(),
    }
}

fn cb_cycles(cb: u8) -> Cycles {
    let x = cb >> 6;
    let z = cb & 7;
    if z == 6 {
        // (HL) operand: BIT reads only (12); RLC..SRL, RES, SET read-modify-write (16)
        if x == 1 { Cycles::both(12) } else { Cycles::both(16) }
    } else {
        Cycles::both(8)
    }
}

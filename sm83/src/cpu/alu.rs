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

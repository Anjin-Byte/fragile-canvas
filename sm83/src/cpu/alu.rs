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

#[derive(Debug)]
pub enum AluOpKind {
    Add,
    Sub,
    Xor,
    And,
    Or,
}

/*
This function is (potentially) being underutilized by 
microcode logic. If it is, I suspect that a lot of 
apparent complexity in microcode could be offloaded
more effectively here. Food for thought.
*/
pub fn alu(op: AluOpKind, a: u8, b: u8) -> AluResult {
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
        AluOpKind::Sub => {
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

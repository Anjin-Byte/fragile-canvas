use super::microcode::AluOpKind;

#[derive(Debug, Clone, Copy)]
pub struct AluResult {
    pub value: u8,
    pub z: bool,
    pub n: bool,
    pub h: bool,
    pub c: bool,
}

pub fn alu(op: AluOpKind, a: u8, b: u8) -> AluResult {
    match op {
        AluOpKind::Add => {
            let (value, carry) = a.overflowing_add(b);
            let half = ((a & 0xF) + (b & 0xF)) > 0xF;
            AluResult {
                value,
                z: value == 0,
                n: false,
                h: half,
                c: carry,
            }
        }
        AluOpKind::Sub => {
            let (value, carry) = a.overflowing_sub(b);
            let half = (a & 0xF) < (b & 0xF);
            AluResult {
                value,
                z: value == 0,
                n: true,
                h: half,
                c: carry,
            }
        }
        AluOpKind::And => {
            let value = a & b;
            AluResult {
                value,
                z: value == 0,
                n: false,
                h: true,
                c: false,
            }
        }
        AluOpKind::Or => {
            let value = a | b;
            AluResult {
                value,
                z: value == 0,
                n: false,
                h: false,
                c: false,
            }
        }
        AluOpKind::Xor => {
            let value = a ^ b;
            AluResult {
                value,
                z: value == 0,
                n: false,
                h: false,
                c: false,
            }
        }
    }
}

pub fn set_bit(value: u8, bit: u8) -> u8 {
    value | (1 << bit)
}

pub fn reset_bit(value: u8, bit: u8) -> u8 {
    value & !(1 << bit)
}

pub fn toggle_bit(value: u8, bit: u8) -> u8 {
    value ^ (1 << bit)
}

pub fn is_bit_set(value: u8, bit: u8) -> bool {
    (value & (1 << bit)) != 0
}
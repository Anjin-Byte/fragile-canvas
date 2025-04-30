#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interrupt {
    VBlank,     // Vertical blanking interrupt
    LCDStat,    // LCD status interrupt
    Timer,      // Timer overflow interrupt
    Serial,     // Serial link interrupt
    Joypad,     // Joypad input interrupt
}

impl Interrupt {
    pub fn vector(self) -> u16 {
        match self {
            Interrupt::VBlank  => 0x0040,
            Interrupt::LCDStat => 0x0048,
            Interrupt::Timer   => 0x0050,
            Interrupt::Serial  => 0x0058,
            Interrupt::Joypad  => 0x0060,
        }
    }

    pub fn bit_index(self) -> u8 {
        match self {
            Interrupt::VBlank  => 0,
            Interrupt::LCDStat => 1,
            Interrupt::Timer   => 2,
            Interrupt::Serial  => 3,
            Interrupt::Joypad  => 4,
        }
    }
}
/// Joypad controller (0xFF00 — P1/JOYP).
///
/// The Game Boy joypad has 8 buttons arranged in two groups selected
/// by bits 4-5 of P1.  Buttons are active-low (0 = pressed).
///
/// Bit layout of P1 (0xFF00):
///   Bit 5: Select action buttons    (0 = selected) — write
///   Bit 4: Select direction buttons (0 = selected) — write
///   Bit 3: Down  / Start   (0 = pressed) — read
///   Bit 2: Up    / Select  (0 = pressed) — read
///   Bit 1: Left  / B       (0 = pressed) — read
///   Bit 0: Right / A       (0 = pressed) — read
///
/// The frontend sets button state via `set_state()`.  When the game
/// reads P1, the Joypad returns the appropriate group based on the
/// select bits.

/// Button bit masks for the action group (active-low).
pub const BTN_A: u8      = 1 << 0;
pub const BTN_B: u8      = 1 << 1;
pub const BTN_SELECT: u8 = 1 << 2;
pub const BTN_START: u8  = 1 << 3;

/// Button bit masks for the direction group (active-low).
pub const BTN_RIGHT: u8 = 1 << 0;
pub const BTN_LEFT: u8  = 1 << 1;
pub const BTN_UP: u8    = 1 << 2;
pub const BTN_DOWN: u8  = 1 << 3;

#[derive(Clone)]
pub struct Joypad {
    /// Bits 4-5: select lines written by the CPU.
    select: u8,
    /// Action button state (Start, Select, B, A).
    /// Bits set = button pressed (we invert on read to match active-low).
    action: u8,
    /// Direction pad state (Down, Up, Left, Right).
    /// Bits set = button pressed (inverted on read).
    direction: u8,
    /// Joypad interrupt requested (any button press transition 1→0).
    pub interrupt_pending: bool,
}

impl Joypad {
    pub fn new() -> Self {
        Self {
            select: 0x30, // both groups deselected
            action: 0,
            direction: 0,
            interrupt_pending: false,
        }
    }

    /// Set button state from the frontend.
    ///
    /// `action`: bitmask of pressed action buttons (BTN_A | BTN_B | ...).
    /// `direction`: bitmask of pressed direction buttons (BTN_UP | ...).
    ///
    /// A transition from released to pressed on any button fires the
    /// Joypad interrupt (IF bit 4).
    pub fn set_state(&mut self, action: u8, direction: u8) {
        let old_action = self.action;
        let old_direction = self.direction;
        self.action = action & 0x0F;
        self.direction = direction & 0x0F;

        // Interrupt on any button press transition (was 0, now 1)
        let new_presses = (self.action & !old_action) | (self.direction & !old_direction);
        if new_presses != 0 {
            self.interrupt_pending = true;
        }
    }

    /// CPU read of P1 (0xFF00).
    pub fn read(&self) -> u8 {
        let mut lower = 0x0F; // default: all buttons released (active-low)

        // If action buttons selected (bit 5 = 0), mix in action state
        if self.select & 0x20 == 0 {
            lower &= !(self.action & 0x0F); // pressed buttons → 0
        }
        // If direction buttons selected (bit 4 = 0), mix in direction state
        if self.select & 0x10 == 0 {
            lower &= !(self.direction & 0x0F);
        }

        0xC0 | self.select | lower
    }

    /// CPU write to P1 (0xFF00). Only bits 4-5 are writable.
    pub fn write(&mut self, value: u8) {
        self.select = value & 0x30;
    }

    /// Reset all state.
    pub fn power_off(&mut self) {
        self.select = 0x30;
        self.action = 0;
        self.direction = 0;
        self.interrupt_pending = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_buttons_pressed() {
        let jp = Joypad::new();
        // Both groups deselected → lower nibble = 0xF
        assert_eq!(jp.read() & 0x0F, 0x0F);
    }

    #[test]
    fn action_buttons() {
        let mut jp = Joypad::new();
        jp.set_state(BTN_A | BTN_START, 0);
        jp.write(0x20); // select action (bit 5 = 1 means DEselect, 0 = select)
        // Wait — bit 5 = 0 selects action buttons
        jp.write(0x10); // bit 5=0 (action selected), bit 4=1 (dpad deselected)
        // A (bit 0) and Start (bit 3) pressed → bits 0,3 = 0
        assert_eq!(jp.read() & 0x0F, 0x0F & !(BTN_A | BTN_START));
        assert_eq!(jp.read() & 0x0F, 0b0110); // bits 1,2 high (B,Select released)
    }

    #[test]
    fn direction_buttons() {
        let mut jp = Joypad::new();
        jp.set_state(0, BTN_UP | BTN_LEFT);
        jp.write(0x20); // bit 4=0 (dpad selected), bit 5=1 (action deselected)
        // Up (bit 2) and Left (bit 1) pressed → bits 1,2 = 0
        assert_eq!(jp.read() & 0x0F, 0x0F & !(BTN_UP | BTN_LEFT));
        assert_eq!(jp.read() & 0x0F, 0b1001); // bits 0,3 high
    }

    #[test]
    fn both_groups_selected() {
        let mut jp = Joypad::new();
        jp.set_state(BTN_A, BTN_RIGHT); // A and Right
        jp.write(0x00); // both selected
        // Both A (action bit 0) and Right (dpad bit 0) → bit 0 = 0
        assert_eq!(jp.read() & 0x01, 0);
    }

    #[test]
    fn interrupt_on_press() {
        let mut jp = Joypad::new();
        assert!(!jp.interrupt_pending);
        jp.set_state(BTN_A, 0);
        assert!(jp.interrupt_pending);

        jp.interrupt_pending = false;
        // Same state, no new press → no interrupt
        jp.set_state(BTN_A, 0);
        assert!(!jp.interrupt_pending);

        // Release A, press B → new press → interrupt
        jp.set_state(BTN_B, 0);
        assert!(jp.interrupt_pending);
    }

    #[test]
    fn upper_bits_always_set() {
        let jp = Joypad::new();
        assert_eq!(jp.read() & 0xC0, 0xC0);
    }
}

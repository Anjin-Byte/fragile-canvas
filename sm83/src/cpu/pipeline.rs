/// M-cycle pipeline types for the SM83 CPU state machine.
///
/// The CPU executes one M-cycle (4 T-cycles) per `step_m()` call.
/// `MCycleResult` tells the system loop whether the current instruction
/// is still in progress, just completed, or the CPU is halted.

/// Result returned by `CPU::step_m()` after executing one M-cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MCycleResult {
    /// The M-cycle executed but the instruction is not yet complete.
    Continue,
    /// The instruction (or interrupt dispatch) completed on this M-cycle.
    /// `opcode` is `0x000-0x0FF` for base instructions, `0x100-0x1FF`
    /// for CB-prefixed, or `0xFFFF` for interrupt dispatch.
    InstructionComplete { opcode: u16 },
    /// The CPU is halted and burned 1 idle M-cycle.
    HaltBurn,
}

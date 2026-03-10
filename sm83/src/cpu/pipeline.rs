use crate::cpu::decoder::MicrocodeQueue;

#[derive(Debug)]
pub enum PipelineState {
    Fetch,
    Decode(u8),
    Execute(MicrocodeQueue),
    Halted,
    InterruptService(MicrocodeQueue),
}

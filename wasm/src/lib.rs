use serde::Serialize;
use sm83::cpu::registers::{Reg8, Reg16};
use sm83::cpu::CPU;
use sm83::memory::bus::Bus;
use sm83::memory::mmu::MMU;
use sm83::trace::Tracer;
use wasm_bindgen::prelude::*;

#[derive(Serialize)]
struct CpuState {
    pc: u16,
    sp: u16,
    af: u16,
    bc: u16,
    de: u16,
    hl: u16,
    ir: u8,
    ie: u8,
    halted: bool,
}

fn read_state(cpu: &CPU<MMU>) -> CpuState {
    let regs = &cpu.register_file;
    CpuState {
        pc: regs.get_16bit(Reg16::PC),
        sp: regs.get_16bit(Reg16::SP),
        af: regs.get_16bit(Reg16::AF),
        bc: regs.get_16bit(Reg16::BC),
        de: regs.get_16bit(Reg16::DE),
        hl: regs.get_16bit(Reg16::HL),
        ir: regs.get_8bit(Reg8::IR),
        ie: regs.get_8bit(Reg8::IE),
        halted: matches!(cpu.state, sm83::cpu::pipeline::PipelineState::Halted),
    }
}

#[wasm_bindgen]
pub struct EmulatorWasm {
    cpu: Option<CPU<MMU>>,
}

#[wasm_bindgen]
impl EmulatorWasm {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self { cpu: None }
    }

    #[wasm_bindgen(js_name = loadRom)]
    pub fn load_rom(&mut self, boot_rom: &[u8], cart_rom: &[u8]) -> Result<JsValue, JsValue> {
        let mut mmu = MMU::new();
        mmu.load_boot_rom(boot_rom).map_err(|e| JsValue::from_str(&e))?;
        mmu.load_cartridge(cart_rom);
        let cpu = CPU::new(mmu, Tracer::off());
        let state = read_state(&cpu);
        self.cpu = Some(cpu);
        serde_wasm_bindgen::to_value(&state).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    pub fn step(&mut self, ticks: u32) -> Result<JsValue, JsValue> {
        let cpu = self.cpu.as_mut().ok_or_else(|| JsValue::from_str("no ROM loaded"))?;
        for _ in 0..ticks {
            cpu.tick();
        }
        let state = read_state(cpu);
        serde_wasm_bindgen::to_value(&state).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    #[wasm_bindgen(js_name = getState)]
    pub fn get_state(&self) -> Result<JsValue, JsValue> {
        let cpu = self.cpu.as_ref().ok_or_else(|| JsValue::from_str("no ROM loaded"))?;
        let state = read_state(cpu);
        serde_wasm_bindgen::to_value(&state).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    #[wasm_bindgen(js_name = readMemory)]
    pub fn read_memory(&self, addr: u16, length: u16) -> Result<JsValue, JsValue> {
        let cpu = self.cpu.as_ref().ok_or_else(|| JsValue::from_str("no ROM loaded"))?;
        let end = addr.saturating_add(length);
        let data: Vec<u8> = (addr..end).map(|a| cpu.bus.read(a)).collect();
        serde_wasm_bindgen::to_value(&data).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    pub fn reset(&mut self) {
        self.cpu = None;
    }
}

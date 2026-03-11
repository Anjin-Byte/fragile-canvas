use serde::Serialize;
use sm83::cpu::registers::{Reg8, Reg16};
use sm83::memory::bus::Bus;
use sm83::memory::mmu::MMU;
use sm83::system::GameBoy;
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

fn read_state(gb: &GameBoy) -> CpuState {
    let regs = &gb.cpu.register_file;
    CpuState {
        pc: regs.get_16bit(Reg16::PC),
        sp: regs.get_16bit(Reg16::SP),
        af: regs.get_16bit(Reg16::AF),
        bc: regs.get_16bit(Reg16::BC),
        de: regs.get_16bit(Reg16::DE),
        hl: regs.get_16bit(Reg16::HL),
        ir: regs.get_8bit(Reg8::IR),
        ie: regs.get_8bit(Reg8::IE),
        halted: matches!(gb.cpu.state, sm83::cpu::pipeline::PipelineState::Halted),
    }
}

#[wasm_bindgen]
pub struct EmulatorWasm {
    gb: Option<GameBoy>,
}

#[wasm_bindgen]
impl EmulatorWasm {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self { gb: None }
    }

    #[wasm_bindgen(js_name = loadRom)]
    pub fn load_rom(&mut self, cart_rom: &[u8]) -> Result<JsValue, JsValue> {
        let mut mmu = MMU::new();
        mmu.load_boot_rom(sm83::BOOT_ROM).unwrap();
        mmu.load_cartridge(cart_rom);
        let gb = GameBoy::new(mmu, Tracer::off());
        let state = read_state(&gb);
        self.gb = Some(gb);
        serde_wasm_bindgen::to_value(&state).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    #[wasm_bindgen(js_name = loadDefaultRom)]
    pub fn load_default_rom(&mut self) -> Result<JsValue, JsValue> {
        self.load_rom(sm83::DEFAULT_ROM)
    }

    pub fn step(&mut self, ticks: u32) -> Result<JsValue, JsValue> {
        let gb = self.gb.as_mut().ok_or_else(|| JsValue::from_str("no ROM loaded"))?;
        gb.tick_n(ticks);
        let state = read_state(gb);
        serde_wasm_bindgen::to_value(&state).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    #[wasm_bindgen(js_name = getState)]
    pub fn get_state(&self) -> Result<JsValue, JsValue> {
        let gb = self.gb.as_ref().ok_or_else(|| JsValue::from_str("no ROM loaded"))?;
        let state = read_state(gb);
        serde_wasm_bindgen::to_value(&state).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    #[wasm_bindgen(js_name = readMemory)]
    pub fn read_memory(&self, addr: u16, length: u16) -> Result<JsValue, JsValue> {
        let gb = self.gb.as_ref().ok_or_else(|| JsValue::from_str("no ROM loaded"))?;
        let end = addr.saturating_add(length);
        let data: Vec<u8> = (addr..end).map(|a| gb.cpu.bus.read(a)).collect();
        serde_wasm_bindgen::to_value(&data).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    pub fn reset(&mut self) {
        self.gb = None;
    }
}

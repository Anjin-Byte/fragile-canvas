use serde::Serialize;
use sm83::session::{CpuSnapshot, Session};
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

impl From<CpuSnapshot> for CpuState {
    fn from(s: CpuSnapshot) -> Self {
        Self {
            pc: s.pc,
            sp: s.sp,
            af: s.af,
            bc: s.bc,
            de: s.de,
            hl: s.hl,
            ir: s.ir,
            ie: s.ie,
            halted: s.halted,
        }
    }
}

fn to_js(snap: CpuSnapshot) -> Result<JsValue, JsValue> {
    let state: CpuState = snap.into();
    serde_wasm_bindgen::to_value(&state).map_err(|e| JsValue::from_str(&e.to_string()))
}

fn to_js_result(r: Result<CpuSnapshot, &str>) -> Result<JsValue, JsValue> {
    to_js(r.map_err(|e| JsValue::from_str(e))?)
}

#[wasm_bindgen]
pub struct EmulatorWasm {
    session: Session,
}

#[wasm_bindgen]
impl EmulatorWasm {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self { session: Session::new() }
    }

    #[wasm_bindgen(js_name = loadRom)]
    pub fn load_rom(&mut self, cart_rom: &[u8]) -> Result<JsValue, JsValue> {
        to_js(self.session.load_rom(cart_rom))
    }

    #[wasm_bindgen(js_name = loadDefaultRom)]
    pub fn load_default_rom(&mut self) -> Result<JsValue, JsValue> {
        to_js(self.session.load_default_rom())
    }

    pub fn step(&mut self, ticks: u32) -> Result<JsValue, JsValue> {
        to_js_result(self.session.step(ticks))
    }

    #[wasm_bindgen(js_name = getState)]
    pub fn get_state(&self) -> Result<JsValue, JsValue> {
        to_js_result(self.session.cpu_snapshot())
    }

    #[wasm_bindgen(js_name = readMemory)]
    pub fn read_memory(&self, addr: u16, length: u16) -> Result<JsValue, JsValue> {
        let data = self.session.read_memory(addr, length)
            .map_err(|e| JsValue::from_str(e))?;
        serde_wasm_bindgen::to_value(&data).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    #[wasm_bindgen(js_name = tickFrame)]
    pub fn tick_frame(&mut self, elapsed_ns: u64) -> Result<JsValue, JsValue> {
        to_js_result(self.session.tick_frame(elapsed_ns))
    }

    #[wasm_bindgen(js_name = resetGovernor)]
    pub fn reset_governor(&mut self) {
        self.session.reset_governor();
    }

    pub fn reset(&mut self) {
        self.session.reset();
    }
}

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

#[derive(Serialize)]
struct BundledRom {
    id: String,
    title: String,
    author: String,
}

#[derive(Serialize)]
struct DisasmLine {
    addr: u16,
    bytes: String,
    text: String,
    len: u8,
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

    /// Load a ROM skipping the boot ROM (starts at PC=0x0100 with
    /// correct post-boot DMG register and I/O state).
    #[wasm_bindgen(js_name = loadRomNoBoot)]
    pub fn load_rom_no_boot(&mut self, cart_rom: &[u8]) -> Result<JsValue, JsValue> {
        to_js(self.session.load_rom_no_boot(cart_rom))
    }

    #[wasm_bindgen(js_name = loadDefaultRom)]
    pub fn load_default_rom(&mut self) -> Result<JsValue, JsValue> {
        to_js(self.session.load_default_rom())
    }

    /// List all bundled ROMs. Returns an array of {id, title, author} objects.
    #[wasm_bindgen(js_name = listBundledRoms)]
    pub fn list_bundled_roms(&self) -> Result<JsValue, JsValue> {
        let roms: Vec<BundledRom> = Session::list_bundled_roms()
            .into_iter()
            .map(|(id, title, author)| BundledRom {
                id: id.to_string(),
                title: title.to_string(),
                author: author.to_string(),
            })
            .collect();
        serde_wasm_bindgen::to_value(&roms).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Load a bundled ROM by id.
    #[wasm_bindgen(js_name = loadBundledRom)]
    pub fn load_bundled_rom(&mut self, id: &str) -> Result<JsValue, JsValue> {
        let snap = self.session.load_bundled_rom(id)
            .map_err(|e| JsValue::from_str(e))?;
        to_js(snap)
    }

    /// Set joypad button state.
    /// `action`: A=1, B=2, Select=4, Start=8.
    /// `direction`: Right=1, Left=2, Up=4, Down=8.
    #[wasm_bindgen(js_name = setButtons)]
    pub fn set_buttons(&mut self, action: u8, direction: u8) {
        self.session.set_buttons(action, direction);
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

    /// Disassemble `count` instructions starting at `addr`, reading through
    /// the bus. Returns an array of { addr, bytes, text, len } objects.
    #[wasm_bindgen(js_name = disassemble)]
    pub fn disassemble(&self, addr: u16, count: u16) -> Result<JsValue, JsValue> {
        let mut lines: Vec<DisasmLine> = Vec::with_capacity(count as usize);
        let mut cur = addr as u32;

        for _ in 0..count {
            if cur > 0xFFFF {
                break;
            }
            let avail = (0x10000 - cur).min(3) as u16;
            let bytes = self
                .session
                .read_memory(cur as u16, avail)
                .map_err(|e| JsValue::from_str(e))?;
            let Some(decoded) = sm83_isa::decode(&bytes) else { break };

            let opts = sm83_isa::FormatOptions { addr: Some(cur as u16), symbols: None };
            let raw: Vec<String> = bytes[..decoded.len as usize]
                .iter()
                .map(|b| format!("{b:02X}"))
                .collect();
            lines.push(DisasmLine {
                addr: cur as u16,
                bytes: raw.join(" "),
                text: sm83_isa::format_instruction(&decoded.instr, &opts),
                len: decoded.len,
            });
            cur += decoded.len as u32;
        }

        serde_wasm_bindgen::to_value(&lines).map_err(|e| JsValue::from_str(&e.to_string()))
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

    /// Drain buffered audio samples as interleaved f32 (L,R,L,R...).
    /// Returns a Float32Array for direct use with Web Audio API.
    #[wasm_bindgen(js_name = drainAudioSamples)]
    pub fn drain_audio_samples(&mut self) -> Vec<f32> {
        self.session.drain_audio_samples()
    }

    /// Drain the latest completed PPU frame.
    ///
    /// Returns a `Uint8Array` of 160×144 shade indices (0-3) when a new frame
    /// is ready, or `undefined` if no frame has completed since the last call.
    #[wasm_bindgen(js_name = getFrame)]
    pub fn get_frame(&mut self) -> Option<Vec<u8>> {
        self.session.drain_frame()
    }
}

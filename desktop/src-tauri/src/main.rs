#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;
use sm83::session::{CpuSnapshot, Session};
use sm83::trace::Tracer;
use std::sync::Mutex;
use std::time::SystemTime;
use tauri::State;

struct Emulator(Mutex<Session>);

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

fn ok(snap: CpuSnapshot) -> Result<CpuState, String> {
    Ok(snap.into())
}

fn ok_result(r: Result<CpuSnapshot, &str>) -> Result<CpuState, String> {
    Ok(r.map_err(|e| e.to_string())?.into())
}

#[tauri::command]
fn load_rom(emu: State<Emulator>, cart_rom: Vec<u8>) -> Result<CpuState, String> {
    let mut session = emu.0.lock().unwrap();
    ok(session.load_rom(&cart_rom))
}

#[tauri::command]
fn load_rom_no_boot(emu: State<Emulator>, cart_rom: Vec<u8>) -> Result<CpuState, String> {
    let mut session = emu.0.lock().unwrap();
    ok(session.load_rom_no_boot(&cart_rom))
}

#[tauri::command]
fn load_default_rom(emu: State<Emulator>) -> Result<CpuState, String> {
    let mut session = emu.0.lock().unwrap();
    ok(session.load_default_rom())
}

#[tauri::command]
fn set_buttons(emu: State<Emulator>, action: u8, direction: u8) {
    let mut session = emu.0.lock().unwrap();
    session.set_buttons(action, direction);
}

#[tauri::command]
fn step(emu: State<Emulator>, ticks: u32) -> Result<CpuState, String> {
    let mut session = emu.0.lock().unwrap();
    ok_result(session.step(ticks))
}

#[tauri::command]
fn tick_frame(emu: State<Emulator>, elapsed_ns: u64) -> Result<CpuState, String> {
    let mut session = emu.0.lock().unwrap();
    ok_result(session.tick_frame(elapsed_ns))
}

#[tauri::command]
fn reset_governor(emu: State<Emulator>) -> Result<(), String> {
    let mut session = emu.0.lock().unwrap();
    session.reset_governor();
    Ok(())
}

#[tauri::command]
fn get_state(emu: State<Emulator>) -> Result<CpuState, String> {
    let session = emu.0.lock().unwrap();
    ok_result(session.cpu_snapshot())
}

#[tauri::command]
fn read_memory(emu: State<Emulator>, addr: u16, length: u16) -> Result<Vec<u8>, String> {
    let session = emu.0.lock().unwrap();
    session.read_memory(addr, length).map_err(|e| e.to_string())
}

#[tauri::command]
fn toggle_trace(emu: State<Emulator>) -> Result<String, String> {
    let mut session = emu.0.lock().unwrap();
    let gb = session.gameboy_mut().ok_or("no ROM loaded")?;

    if gb.cpu.tracer.enabled() {
        gb.cpu.tracer.flush();
        gb.cpu.tracer = Tracer::off();
        Ok("off".to_string())
    } else {
        std::fs::create_dir_all("logs").map_err(|e| format!("couldn't create logs dir: {e}"))?;
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let log_path = format!("logs/{timestamp}.log");
        let file = std::fs::File::create(&log_path)
            .map_err(|e| format!("couldn't create {log_path}: {e}"))?;
        gb.cpu.tracer = Tracer::to_file(file);
        Ok(log_path)
    }
}

#[tauri::command]
fn get_frame(emu: State<Emulator>) -> Option<Vec<u8>> {
    let mut session = emu.0.lock().unwrap();
    session.drain_frame()
}

#[tauri::command]
fn drain_audio_samples(emu: State<Emulator>) -> Vec<f32> {
    let mut session = emu.0.lock().unwrap();
    session.drain_audio_samples()
}

#[tauri::command]
fn reset(emu: State<Emulator>) {
    let mut session = emu.0.lock().unwrap();
    if let Some(gb) = session.gameboy_mut() {
        gb.cpu.tracer.flush();
    }
    session.reset();
}

fn main() {
    tauri::Builder::default()
        .manage(Emulator(Mutex::new(Session::new())))
        .invoke_handler(tauri::generate_handler![
            load_rom,
            load_rom_no_boot,
            load_default_rom,
            set_buttons,
            step,
            tick_frame,
            reset_governor,
            get_state,
            read_memory,
            toggle_trace,
            get_frame,
            drain_audio_samples,
            reset,
        ])
        .run(tauri::generate_context!())
        .expect("error running tauri application");
}

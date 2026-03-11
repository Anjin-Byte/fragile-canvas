#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;
use sm83::cpu::registers::{Reg8, Reg16};
use sm83::memory::bus::Bus;
use sm83::memory::mmu::MMU;
use sm83::system::GameBoy;
use sm83::trace::Tracer;
use std::sync::Mutex;
use std::time::SystemTime;
use tauri::State;

struct Emulator(Mutex<Option<GameBoy>>);

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

#[tauri::command]
fn load_rom(emu: State<Emulator>, cart_rom: Vec<u8>) -> Result<CpuState, String> {
    let mut mmu = MMU::new();
    mmu.load_boot_rom(sm83::BOOT_ROM).unwrap();
    mmu.load_cartridge(&cart_rom);
    let gb = GameBoy::new(mmu, Tracer::off());
    let state = read_state(&gb);
    *emu.0.lock().unwrap() = Some(gb);
    Ok(state)
}

#[tauri::command]
fn load_default_rom(emu: State<Emulator>) -> Result<CpuState, String> {
    load_rom(emu, sm83::DEFAULT_ROM.to_vec())
}

#[tauri::command]
fn step(emu: State<Emulator>, ticks: u32) -> Result<CpuState, String> {
    let mut guard = emu.0.lock().unwrap();
    let gb = guard.as_mut().ok_or("no ROM loaded")?;
    gb.tick_n(ticks);
    Ok(read_state(gb))
}

#[tauri::command]
fn get_state(emu: State<Emulator>) -> Result<CpuState, String> {
    let guard = emu.0.lock().unwrap();
    let gb = guard.as_ref().ok_or("no ROM loaded")?;
    Ok(read_state(gb))
}

#[tauri::command]
fn read_memory(emu: State<Emulator>, addr: u16, length: u16) -> Result<Vec<u8>, String> {
    let guard = emu.0.lock().unwrap();
    let gb = guard.as_ref().ok_or("no ROM loaded")?;
    let end = addr.saturating_add(length);
    Ok((addr..end).map(|a| gb.cpu.bus.read(a)).collect())
}

#[tauri::command]
fn toggle_trace(emu: State<Emulator>) -> Result<String, String> {
    let mut guard = emu.0.lock().unwrap();
    let gb = guard.as_mut().ok_or("no ROM loaded")?;

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
fn reset(emu: State<Emulator>) {
    let mut guard = emu.0.lock().unwrap();
    if let Some(gb) = guard.as_mut() {
        gb.cpu.tracer.flush();
    }
    *guard = None;
}

fn main() {
    tauri::Builder::default()
        .manage(Emulator(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![
            load_rom,
            load_default_rom,
            step,
            get_state,
            read_memory,
            toggle_trace,
            reset,
        ])
        .run(tauri::generate_context!())
        .expect("error running tauri application");
}

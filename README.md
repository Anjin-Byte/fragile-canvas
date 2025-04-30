# SM83 Core Emulator

This project is a work-in-progress **Game Boy CPU emulator**, written in **pure Rust** with modular components for decoding, microcode execution, memory mapping, and register management.

---

## 🛠️ Project Structure

- `cpu/` — Core CPU logic
  - `pipeline.rs` – Implements a fetch-decode-execute state machine
  - `decoder.rs` – Converts opcodes into micro-operations (`MicroOp`s)
  - `microcode.rs` – (WIP) Logic to interpret and execute individual micro-ops
  - `registers.rs` – Emulates 8-bit and 16-bit CPU registers, flags, and bitwise operations
  - `interrupts.rs` – Basic interrupt vector table and helpers

- `memory/`
  - `mmu.rs` – Memory Management Unit: models Game Boy memory layout (ROM, VRAM, RAM, I/O)
  - `bus.rs` – Abstract memory bus trait used by the CPU

- `utils/`
  - `bit_twiddling.rs` – Helpers for working with individual bits

---

## 🎯 Goals

- 🧩 **Modular CPU core**: Structured around micro-ops and pipelines, to support flexible instruction modeling
- 🧠 **Educational architecture**: Build understanding of low-level CPU execution (microcode, flags, interrupt handling)
---

## 📦 Getting Started

To run the CPU loop with a sample ROM:

```bash
cargo run
```

The entry point is `main.rs`, which loads a small program into memory and ticks the CPU.

---

## ✨ License

MIT

---

*Built as a learning project to better understand CPU design, emulation, and systems programming in Rust.*
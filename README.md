# Okra ([demo](https://anjin-byte.github.io/okra-emu/))

> Formerly codenamed *fragile-canvas* — the legacy name still appears in package/crate identifiers.

Game Boy (DMG) emulator. The SM83 CPU is modeled as a microcode engine:
each opcode is decoded into a sequence of primitive `MicroOp`s
(register loads, ALU operations, memory reads/writes, flag updates)
which execute one per cycle through a fetch-decode-execute pipeline.
This decomposes the full ISA into a small set of reusable building blocks
rather than implementing each instruction as a monolithic handler.

This project distributes the Tobu Tobu Girl ROM as an example / demo game. If you enjoy the bundled demo, consider supporting [Simon Larsen](https://github.com/SimonLarsen) and the creators at Tangram Games.

---

## Structure

```
sm83/        emulator library (CPU, MMU, timer, clock governor, tracer)
cli/         headless binary (ROM loading, CLI, config, debug tracing)
desktop/     Tauri + React native desktop app
ui/          shared React components and styles
wasm/        wasm-bindgen crate (compiles sm83 to WASM)
web/         Vite + React browser app (WASM backend)
```

---

## Usage

Requires a DMG boot ROM and a cartridge ROM. Paths are set in `config.yaml`:

```yaml
boot_rom: path/to/boot_rom.bin
cart_rom: path/to/cartridge.gb
```

```
make build     release build
make run       run with config defaults
make debug     run with instruction trace -> logs/<timestamp>.log
make test      run sm83 tests
make desktop   launch Tauri desktop app
make web       build WASM and start browser dev server
```

Override the cartridge from the command line:

```
cargo run --release -p fragile-canvas -- path/to/rom.gb
```

---

## Timing

The DMG master clock is exactly 2^22 Hz (4,194,304 Hz). Every timing divider in the system is a power-of-two bit shift from this single crystal — nothing is an arbitrary frequency.

The emulator ticks at T-cycle (dot) resolution. A 16-bit system counter increments every T-cycle, and all subsystem timing derives from it:

```
1 T-cycle  = 1 dot  = 1 master clock tick   (2^22 Hz)
4 T-cycles = 1 M-cycle                       (2^20 Hz)
```

By default the CLI runs governed at real-time speed. A clock governor tracks fractional T-cycle debt with an integer accumulator — zero algorithmic drift. Measured accuracy is ~26 ppm, lost to OS scheduling jitter (the governor math is exact). The actual DMG crystal has a ±30–50 ppm manufacturing tolerance that drifts in both directions.

```
--uncapped   run as fast as hardware allows
--bench      measure governor accuracy for 5 seconds
```

---

## License

MIT

---

## Third-Party Licenses

Tobu Tobu Girl Deluxe
Copyright © Tangram Games

Source code licensed under the MIT License.
Game assets (graphics, music, sound, text) licensed under
Creative Commons Attribution 4.0 International (CC BY 4.0).

Original project:
https://github.com/SimonLarsen/tobutobugirl

---

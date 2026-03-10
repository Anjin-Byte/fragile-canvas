# fragile-canvas ([demo](https://anjin-byte.github.io/fragile-canvas/))

Game Boy (DMG) emulator. The SM83 CPU is modeled as a microcode engine:
each opcode is decoded into a sequence of primitive `MicroOp`s
(register loads, ALU operations, memory reads/writes, flag updates)
which execute one per cycle through a fetch-decode-execute pipeline.
This decomposes the full ISA into a small set of reusable building blocks
rather than implementing each instruction as a monolithic handler.

---

## Structure

```
sm83/        emulator library (CPU, MMU, decoder, microcode, tracer)
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
boot_rom: assets/ROMs/DMG_ROM.bin
cart_rom: assets/ROMs/Tetris.gb
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

If you enjoy the bundled demo, consider supporting [Simon Larsen](https://github.com/SimonLarsen) and the creators at Tangram Games.
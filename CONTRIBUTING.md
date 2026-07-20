# Contributing to Okra

Thanks for your interest! Okra is a Game Boy (DMG) emulator with a built-in debugger workbench. Bug reports, issues, and pull requests are all welcome.

By participating, you agree to abide by our [Code of Conduct](CODE_OF_CONDUCT.md).

> The project was codenamed **fragile-canvas**, and that name still appears in some crate/package identifiers (`@fragile-canvas/ui`, `fragile-canvas-wasm`, the `cli` crate, the Tauri bundle id). That's expected — there's no need to rename them.

## Prerequisites

- [Rust](https://rustup.rs/) (stable) with the WASM target: `rustup target add wasm32-unknown-unknown`
- [`wasm-pack`](https://rustwasm.github.io/wasm-pack/)
- [Node](https://nodejs.org/) 20+ and npm

## Workspace layout

Rust workspace crates: `sm83` (core), `sm83-isa` (assembler/disassembler), `cli`, `wasm`, `desktop/src-tauri`.
Front-end packages: `ui` (shared Svelte 5 UI + the Workbench), `phi` (component toolkit), `web` (Vite/WASM), `desktop` (Tauri). The full map is in the [README](README.md#project-structure).

`web` and `desktop` consume `ui` and `phi` from source (via `file:` links), so each has its own `node_modules`.

## Building & running

```bash
make web        # build the WASM core + start the browser dev server
make desktop    # launch the Tauri desktop app
make test       # run the sm83 test suites
make build      # release build
```

Because the front-ends bundle `ui`/`phi` from source, after a **core (Rust) change** you must rebuild the WASM before the browser picks it up:

```bash
make wasm       # or: cd wasm && wasm-pack build --target web
```

### Running the CLI

```bash
cargo run --release -p fragile-canvas -- path/to/rom.gb
# or set boot_rom / cart_rom in config.yaml and: make run
```

## Testing

Hardware-accuracy tests use [Blargg's test ROMs](https://github.com/c-sp/game-boy-test-roms) and live in `sm83/tests/`:

```bash
cargo test -p sm83                                                     # unit + always-on integration tests
cargo test -p sm83 --test blargg report_all -- --ignored --nocapture  # full suite report tables
```

Please keep the `cpu_instrs` and timing suites green. If you're improving accuracy, reference the relevant Blargg suite in your PR.

## Conventions

**Rust**

- The CPU is a **microcode engine** — new or edited instructions decode into `MicroOp` sequences that execute one per cycle. Don't add monolithic per-opcode handlers; follow the existing decomposition so memory/interrupt timing stays correct.
- Cross-reference hardware behavior against the [Pandocs](https://gbdev.io/pandocs/) and cite the section in a comment where it isn't obvious.
- Keep `cargo fmt` and `cargo clippy` clean.

**Svelte 5 / UI**

- Runes only: `$state`, `$derived`, `$effect`, `$props()`. No Svelte 4 stores or `$:` reactive statements.
- UI panels are built from the `phi` components in this repo.

## Pull requests

1. Fork and branch off `main`.
2. Keep changes focused; describe what changed and why.
3. Make sure `make test` passes and the web build is clean (`cd web && npm run build`).
4. For UI changes, a screenshot or short clip helps a lot.

Not sure where to start? The [roadmap](README.md#roadmap) lists open areas — breakpoint enforcement, the two failing Blargg timing tests (`halt_bug`, `interrupt_time`), the PPU tile/OAM views, and save states are all good first issues.

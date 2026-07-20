# ROMs

Bundled ROM files for fragile-canvas. Only DMG-compatible ROMs can be
included in the binary via `include_bytes!` — the emulator does not
implement CGB hardware (double-speed CPU, VRAM banking, color palettes).

## Bundled (compiled into binary)

| File | Title | MBC | Size | License |
|------|-------|-----|------|---------|
| tobudx.gb | Tobu Tobu Girl DX | MBC1+RAM+BAT | 256 KiB | CC BY 4.0 (Tangram Games) |

## Available but not yet bundled

| File | Title | MBC | Size | Status |
|------|-------|-----|------|--------|
| Opossum Country.gbc | Opossum Country | MBC5+RAM+BAT | 512 KiB | CGB-enhanced, DMG-compatible — should work |

## Not supported (CGB-only)

These ROMs require CGB hardware (0xC0 cartridge flag) and will not run
correctly on the current DMG core.

| File | Title | MBC | Size | Reason |
|------|-------|-----|------|--------|
| BMOv3.1.gb | BMO v3.1 | MBC5+RAM+BAT | 512 KiB | CGB-only (0xC0) |
| Capybara-Village-Update1.gb | Capybara Village | MBC5+RAM+BAT | 512 KiB | CGB-only (0xC0) |
| Machine DEMO v1_1.gb | Machine DEMO | MBC5+RAM+BAT | 2 MiB | CGB-only (0xC0) |

## Other

| File | Description |
|------|-------------|
| bootrom.bin | DMG boot ROM (256 bytes) — used for boot sequence emulation |

## CGB flag reference

- `0x00` — DMG only
- `0x80` — CGB-enhanced, backwards-compatible with DMG
- `0xC0` — CGB-only, will not run on DMG hardware

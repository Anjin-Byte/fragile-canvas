// The editor's default program: a self-contained Okra demo. It paints a
// four-shade pattern to the screen with no cartridge, so "Assemble & Run" shows
// a picture immediately. Exercises labels, EQU, LDH I/O, and counted loops, and
// teaches the core idiom: turn the LCD off before writing VRAM.
// Source of truth for assets/ROMs/okra_demo.asm.

export const STARTER_SOURCE = `; ─── Okra demo — a self-contained picture, no cartridge needed ───────────────
; Turn the LCD off, build one background tile that ramps through all four DMG
; shades, tile it across the screen, turn the LCD back on, then hold the frame.
; Assemble & Run to see it.

rLCDC   EQU $40          ; LCD control   (LDH offset → $FF40)
rBGP    EQU $47          ; BG palette    (→ $FF47)

        ORG $0150
main:
        ; LCD OFF first — VRAM writes are ignored while the PPU is drawing.
        XOR  A
        LDH  (rLCDC), A

        ; Palette: color 0→white, 1→light, 2→dark, 3→black.
        LD   A, $E4
        LDH  (rBGP), A

        ; Tile #1 at $8010 — each 8-px row ramps white|light|dark|black.
        ;   plane 0 = %00110011 ($33),  plane 1 = %00001111 ($0F)
        LD   HL, $8010
        LD   B, 8                ; 8 rows
tile:   LD   A, $33
        LD   (HL+), A            ; low bit-plane
        LD   A, $0F
        LD   (HL+), A            ; high bit-plane
        DEC  B
        JR   NZ, tile

        ; Paint the whole 32×32 background map with tile #1.
        LD   HL, $9800
        LD   BC, $0400           ; 1024 map entries
fill:   LD   A, $01
        LD   (HL+), A
        DEC  BC
        LD   A, B
        OR   C
        JR   NZ, fill

        ; LCD on, background on.
        LD   A, $91
        LDH  (rLCDC), A

        ; Give the PPU a few frames to draw, then hold the picture.
        LD   DE, $8000
wait:   DEC  DE
        LD   A, D
        OR   E
        JR   NZ, wait

        HALT
`;

// The editor's default program: a self-contained, animated Okra demo. It spells
// OKRA in tiles across the background and marquees it sideways forever (no
// cartridge), so "Launch" shows motion immediately and never halts. Exercises
// labels, EQU, DB tables, LDH I/O, indexed copies, and counted loops, and — by
// scrolling in sync with VBlank — doubles as a live test of the PPU's timing.
// Source of truth for assets/ROMs/okra_demo.asm.

export const STARTER_SOURCE = `; ─── Okra ──────────────────────────────
rLCDC   EQU $40          ; LCD control
rSCY    EQU $42          ; BG scroll Y
rSCX    EQU $43          ; BG scroll X
rLY     EQU $44          ; LCD Y — current scanline
rBGP    EQU $47          ; BG palette

        ORG $0150
main:
        ; LCD off so VRAM is free to write; zero the scroll registers.
        XOR  A
        LDH  (rLCDC), A
        LDH  (rSCX), A
        LDH  (rSCY), A

        ; Palette: 0→white, 1→light, 2→dark, 3→black.
        LD   A, $E4
        LDH  (rBGP), A

        ; Copy the four letter tiles (O K R A) to VRAM at $8010, duplicating
        ; each row into both bit-planes so the letters draw in shade 3.
        LD   HL, tiles
        LD   DE, $8010
        LD   B, 32               ; 4 letters × 8 rows
copytile:
        LD   A, (HL+)
        LD   (DE), A             ; bit-plane 0
        INC  DE
        LD   (DE), A             ; bit-plane 1
        INC  DE
        DEC  B
        JR   NZ, copytile

        ; Fill the 32×32 map with tiles 1..4 on repeat. 32 is a multiple of 4,
        ; so every row reads "OKRAOKRA...", perfectly aligned.
        LD   HL, $9800
        LD   BC, $0400           ; 1024 map entries
        LD   D, $01              ; O,K,R,A = tiles 1..4
mapfill:
        LD   A, D
        LD   (HL+), A
        INC  D
        LD   A, D
        CP   $05                 ; past A? wrap to O
        JR   NZ, nowrap
        LD   D, $01
nowrap:
        DEC  BC
        LD   A, B
        OR   C
        JR   NZ, mapfill

        ; LCD on, background on.
        LD   A, $91
        LDH  (rLCDC), A

        ; ── Forever: marquee one pixel per frame, synced to VBlank ──
vsync:  LDH  A, (rLY)            ; wait for the start of VBlank (LY = 144)
        CP   $90
        JR   NZ, vsync

        LDH  A, (rSCX)           ; SCX + 1  → the word drifts left
        INC  A
        LDH  (rSCX), A

hold:   LDH  A, (rLY)            ; wait out VBlank → one step per frame
        CP   $90
        JR   Z, hold
        JR   vsync

        ; ── Letter tiles: 8 rows each, bit7 = leftmost pixel ──
tiles:
        DB   $70,$88,$88,$88,$88,$88,$70,$00   ; O
        DB   $88,$90,$A0,$C0,$A0,$90,$88,$00   ; K
        DB   $F0,$88,$88,$F0,$A0,$90,$88,$00   ; R
        DB   $70,$88,$88,$F8,$88,$88,$88,$00   ; A
`;

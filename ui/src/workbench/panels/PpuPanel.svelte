<script lang="ts">
  /**
   * PPU viewer (spec §5). The Registers tab is live today via bus reads
   * of FF40-FF4B. Tiles / BG Map / Sprites are blocked out as placeholder
   * canvases until VRAM/OAM decoding lands (follow-up).
   */
  import { ToggleGroup, PropRow, BitField } from "@gestalt/phi";
  import { hex8, type EmuController } from "../emu.svelte.js";

  let { emu }: { emu: EmuController } = $props();

  let tab = $state("regs");
  let regs = $state<number[]>([]);

  // FF40..FF4B: LCDC STAT SCY SCX LY LYC DMA BGP OBP0 OBP1 WY WX
  $effect(() => {
    const f = emu.frameCount;
    if (!emu.romLoaded || tab !== "regs") return;
    if (emu.running && f % 6 !== 0) return;
    emu.read(0xff40, 12).then((v) => (regs = v));
  });

  const lcdc = $derived(regs[0]);
  const stat = $derived(regs[1]);

  function bits(value: number | undefined, names: string[], short: string[]) {
    return names.map((name, i) => ({
      label: short[i],
      value: value === undefined ? undefined : (value & (1 << i)) !== 0,
      title: name,
    }));
  }

  const LCDC_NAMES = [
    "BG/Win enable", "OBJ enable", "OBJ size 8x16", "BG map 9C00",
    "Tile data 8000", "Window enable", "Win map 9C00", "LCD enable",
  ];
  const LCDC_SHORT = ["BG", "OBJ", "SZ", "BGM", "TD", "WIN", "WM", "LCD"];
  const STAT_NAMES = [
    "Mode bit 0", "Mode bit 1", "LYC == LY", "Mode 0 int",
    "Mode 1 int", "Mode 2 int", "LYC int",
  ];
  const STAT_SHORT = ["M0", "M1", "LYC=", "H", "V", "O", "LY"];

  const MODE_NAMES = ["HBlank", "VBlank", "OAM scan", "Drawing"];

  /** BGP/OBP palette byte → four shade indices (0 = lightest). */
  function paletteShades(value: number | undefined): number[] {
    if (value === undefined) return [0, 1, 2, 3];
    return [0, 1, 2, 3].map((i) => (value >> (i * 2)) & 3);
  }

  const SHADE_COLORS = ["#9BBC0F", "#8BAC0F", "#306230", "#0F380F"];

  const TABS = [
    { value: "tiles", label: "Tiles" },
    { value: "map", label: "Map" },
    { value: "sprites", label: "Sprites" },
    { value: "regs", label: "Registers" },
  ];
</script>

<div class="ppu-panel">
  <div class="ppu-tabs">
    <ToggleGroup options={TABS} value={tab} onValueChange={(v) => (tab = v)} label="PPU view" />
  </div>

  {#if tab === "regs"}
    {#if regs.length === 12}
      <div class="ppu-regs">
        <BitField label="LCDC" flags={bits(lcdc, LCDC_NAMES, LCDC_SHORT)} />
        <BitField label="STAT" flags={bits(stat, STAT_NAMES, STAT_SHORT)} />
        <PropRow label="Mode" value={MODE_NAMES[stat & 3]} />
        <div class="divider"></div>
        <PropRow label="LY / LYC" value={`${hex8(regs[4])} / ${hex8(regs[5])}`} />
        <PropRow label="SCX / SCY" value={`${hex8(regs[3])} / ${hex8(regs[2])}`} />
        <PropRow label="WX / WY" value={`${hex8(regs[11])} / ${hex8(regs[10])}`} />
        <div class="divider"></div>
        {#each [["BGP", 7], ["OBP0", 8], ["OBP1", 9]] as [name, idx] (name)}
          <div class="palette-row">
            <span class="palette-label">{name}</span>
            <span class="palette-swatches">
              {#each paletteShades(regs[idx as number]) as shade, i (i)}
                <span
                  class="palette-swatch"
                  style:background={SHADE_COLORS[shade]}
                  title="color {i} → shade {shade}"
                ></span>
              {/each}
            </span>
            <span class="palette-value">{hex8(regs[idx as number])}</span>
          </div>
        {/each}
      </div>
    {:else}
      <div class="empty-hint">Load a ROM to inspect PPU registers</div>
    {/if}
  {:else}
    <div class="ppu-placeholder">
      <div class="placeholder-canvas">
        {#if tab === "tiles"}384 tiles · 3 blocks · 2× zoom{/if}
        {#if tab === "map"}32×32 BG map · viewport overlay{/if}
        {#if tab === "sprites"}40 OAM entries · previews{/if}
      </div>
      <span class="placeholder-note">Needs VRAM/OAM decoding (planned)</span>
    </div>
  {/if}
</div>

<style>
  .ppu-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .ppu-tabs {
    padding: 6px 8px;
    border-bottom: 1px solid var(--stroke-lo);
  }

  .ppu-regs {
    padding: 8px 10px;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .divider {
    height: 1px;
    margin: 6px 0;
    background: var(--stroke-lo);
  }

  .palette-row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 20px;
    padding: 2px 4px;
  }

  .palette-label {
    font-family: var(--font-sans);
    font-size: 11px;
    color: var(--text-subtle);
    width: 34px;
  }

  .palette-swatches {
    display: flex;
    gap: 2px;
    flex: 1;
  }

  .palette-swatch {
    width: 14px;
    height: 12px;
    border-radius: 2px;
    border: 1px solid oklch(0 0 0 / 30%);
  }

  .palette-value {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-mid);
  }

  .ppu-placeholder {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 12px;
  }

  .placeholder-canvas {
    width: 80%;
    max-width: 260px;
    aspect-ratio: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-faint);
    text-align: center;
    background: var(--fill-inset);
    border: 1px dashed var(--stroke-mid);
    border-radius: var(--radius-sm);
    box-shadow: var(--shadow-inset);
  }

  .placeholder-note {
    font-family: var(--font-sans);
    font-size: 10px;
    color: var(--text-faint);
  }

  .empty-hint {
    padding: 16px 8px;
    font-family: var(--font-sans);
    font-size: 11px;
    color: var(--text-faint);
    text-align: center;
  }
</style>

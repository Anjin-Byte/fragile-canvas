<script lang="ts">
  /**
   * I/O register panel (spec §7) — live raw values for the memory-mapped
   * registers, grouped by subsystem, with bit decode for the flag-heavy
   * ones. One 128-byte bus read per refresh.
   */
  import { Section, PropRow, BitField } from "@gestalt/phi";
  import { hex8, type EmuController } from "../emu.svelte.js";

  let { emu }: { emu: EmuController } = $props();

  let io = $state<number[]>([]);
  let ieVal = $state<number | null>(null);

  $effect(() => {
    const f = emu.frameCount;
    if (!emu.romLoaded) return;
    if (emu.running && f % 6 !== 0) return;
    emu.read(0xff00, 0x80).then((v) => (io = v));
    emu.read(0xffff, 1).then(([v]) => (ieVal = v));
  });

  /** io[] is indexed from FF00. */
  const reg = $derived((addr: number): number | undefined => io[addr - 0xff00]);

  function fmt(addr: number): string {
    const v = io[addr - 0xff00];
    return v === undefined ? "--" : hex8(v);
  }

  function bitFlags(value: number | undefined, names: string[]) {
    return names.map((name, i) => ({
      label: name,
      value: value === undefined ? undefined : (value & (1 << i)) !== 0,
      title: `bit ${i}`,
    }));
  }

  const INT_SHORT = ["VBl", "STA", "Tim", "Ser", "Joy"];

  const TIMER_CLOCKS = ["4096 Hz", "262144 Hz", "65536 Hz", "16384 Hz"];
</script>

<div class="io-panel">
  {#if io.length > 0}
    <Section sectionId="io-interrupt" title="Interrupts">
      <BitField label="IF FF0F" flags={bitFlags(reg(0xff0f), INT_SHORT)} />
      <BitField label="IE FFFF" flags={bitFlags(ieVal ?? undefined, INT_SHORT)} />
    </Section>

    <Section sectionId="io-joypad-serial" title="Joypad / Serial">
      <PropRow label="P1 FF00" value={fmt(0xff00)} />
      <PropRow label="SB FF01" value={fmt(0xff01)} />
      <PropRow label="SC FF02" value={fmt(0xff02)} />
    </Section>

    <Section sectionId="io-timer" title="Timer">
      <PropRow label="DIV FF04" value={fmt(0xff04)} />
      <PropRow label="TIMA FF05" value={fmt(0xff05)} />
      <PropRow label="TMA FF06" value={fmt(0xff06)} />
      <PropRow label="TAC FF07" value={fmt(0xff07)} />
      <BitField
        label="TAC bits"
        flags={[
          ...bitFlags(reg(0xff07), ["S0", "S1"]),
          { label: "EN", value: reg(0xff07) === undefined ? undefined : (reg(0xff07)! & 4) !== 0, title: "Timer enable" },
        ]}
      />
      <PropRow
        label="Clock"
        value={reg(0xff07) === undefined ? "--" : TIMER_CLOCKS[reg(0xff07)! & 3]}
      />
    </Section>

    <Section sectionId="io-lcd" title="LCD">
      <PropRow label="LCDC FF40" value={fmt(0xff40)} />
      <PropRow label="STAT FF41" value={fmt(0xff41)} />
      <PropRow label="SCY/SCX" value={`${fmt(0xff42)} ${fmt(0xff43)}`} />
      <PropRow label="LY/LYC" value={`${fmt(0xff44)} ${fmt(0xff45)}`} />
      <PropRow label="DMA FF46" value={fmt(0xff46)} />
      <PropRow label="BGP FF47" value={fmt(0xff47)} />
      <PropRow label="OBP0/1" value={`${fmt(0xff48)} ${fmt(0xff49)}`} />
      <PropRow label="WY/WX" value={`${fmt(0xff4a)} ${fmt(0xff4b)}`} />
    </Section>

    <Section sectionId="io-sound" title="Sound">
      <PropRow label="NR50 FF24" value={fmt(0xff24)} />
      <PropRow label="NR51 FF25" value={fmt(0xff25)} />
      <PropRow label="NR52 FF26" value={fmt(0xff26)} />
    </Section>
  {:else}
    <div class="empty-hint">Load a ROM to inspect I/O registers</div>
  {/if}
</div>

<style>
  .io-panel {
    height: 100%;
    overflow: auto;
    padding: 4px 10px 10px;
  }

  .empty-hint {
    padding: 16px 8px;
    font-family: var(--font-sans);
    font-size: 11px;
    color: var(--text-faint);
    text-align: center;
  }
</style>

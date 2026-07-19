<script lang="ts">
  /**
   * CPU panel — live registers, flags, and interrupt state (spec §2).
   * PC/SP/pairs come free with every frame (CpuState). IF is fetched via
   * readMemory on the heartbeat; IME needs the io_snapshot wasm export
   * (follow-up) and shows as unknown until then.
   */
  import { PropRow, BitField } from "@gestalt/phi";
  import { hex16, hex8, type EmuController } from "../emu.svelte.js";

  let { emu }: { emu: EmuController } = $props();

  let ifVal = $state<number | null>(null);

  // Heartbeat refresh: ~10 Hz while running, immediate on step/pause.
  $effect(() => {
    const f = emu.frameCount;
    if (!emu.romLoaded) return;
    if (emu.running && f % 6 !== 0) return;
    emu.read(0xff0f, 1).then(([v]) => (ifVal = v));
  });

  const cpu = $derived(emu.cpu);
  const flags = $derived.by(() => {
    const f = cpu ? cpu.af & 0xff : 0;
    const known = cpu !== null;
    return [
      { label: "Z", value: known ? (f & 0x80) !== 0 : undefined, title: "Zero" },
      { label: "N", value: known ? (f & 0x40) !== 0 : undefined, title: "Subtract" },
      { label: "H", value: known ? (f & 0x20) !== 0 : undefined, title: "Half-carry" },
      { label: "C", value: known ? (f & 0x10) !== 0 : undefined, title: "Carry" },
    ];
  });

  const INT_BITS = ["VBlank", "STAT", "Timer", "Serial", "Joypad"];

  function intFlags(value: number | null | undefined) {
    return INT_BITS.map((name, i) => ({
      label: name.slice(0, 3),
      value: value === null || value === undefined ? undefined : (value & (1 << i)) !== 0,
      title: name,
    }));
  }

  function pair(v: number): string {
    return `${hex8(v >> 8)} ${hex8(v & 0xff)}`;
  }
</script>

<div class="cpu-panel">
  {#if cpu}
    <PropRow label="PC" value={hex16(cpu.pc)} />
    <PropRow label="SP" value={hex16(cpu.sp)} />
    <PropRow label="AF" value={pair(cpu.af)} />
    <PropRow label="BC" value={pair(cpu.bc)} />
    <PropRow label="DE" value={pair(cpu.de)} />
    <PropRow label="HL" value={pair(cpu.hl)} />

    <div class="divider"></div>

    <BitField label="Flags" flags={flags} />
    <BitField label="IE" flags={intFlags(cpu.ie)} />
    <BitField label="IF" flags={intFlags(ifVal)} />

    <div class="divider"></div>

    <PropRow label="IR" value={hex8(cpu.ir)} />
    <PropRow label="Halted" value={cpu.halted ? "yes" : "no"} />
    <div class="ime-row" title="IME needs the io_snapshot wasm export (planned)">
      <span class="ime-label">IME</span>
      <span class="ime-value">—</span>
    </div>
  {:else}
    <div class="empty-hint">Load a ROM to inspect CPU state</div>
  {/if}
</div>

<style>
  .cpu-panel {
    padding: 8px 10px;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .divider {
    height: 1px;
    margin: 6px 0;
    background: var(--stroke-lo);
  }

  .ime-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    min-height: 20px;
    padding: 2px 4px;
  }

  .ime-label {
    font-family: var(--font-sans);
    font-size: 11px;
    color: var(--text-subtle);
  }

  .ime-value {
    font-family: var(--font-mono);
    font-size: 11px;
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

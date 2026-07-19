<script lang="ts">
  /**
   * The status sliver (spec §9): PC · LY/mode · frame · fps · state.
   * LY/STAT come from a 2-byte heartbeat read; the rest is free CpuState.
   */
  import { hex16, hex8, type EmuController } from "./emu.svelte.js";

  let { emu }: { emu: EmuController } = $props();

  let ly = $state<number | null>(null);
  let stat = $state<number | null>(null);

  const MODE_NAMES = ["HBlank", "VBlank", "OAM", "Draw"];

  $effect(() => {
    const f = emu.frameCount;
    if (!emu.romLoaded) return;
    if (emu.running && f % 6 !== 0) return;
    emu.read(0xff41, 4).then((v) => {
      stat = v[0];
      ly = v[3]; // FF44
    });
  });

  const stateLabel = $derived(
    emu.running ? "▶ Running" : emu.paused ? "⏸ Paused" : emu.romLoaded ? "Ready" : "No ROM",
  );
</script>

<footer class="status-bar">
  {#if emu.cpu}
    <span class="seg">PC:{hex16(emu.cpu.pc)}</span>
    <span class="sep">│</span>
    <span class="seg">
      LY:{ly === null ? "--" : hex8(ly)}
      {stat === null ? "" : MODE_NAMES[stat & 3]}
    </span>
    <span class="sep">│</span>
    <span class="seg">Frame {emu.frameCount}</span>
    <span class="sep">│</span>
    <span class="seg">{emu.fps}fps</span>
    <span class="sep">│</span>
  {/if}
  <span class="seg" class:running={emu.running}>{stateLabel}</span>
</footer>

<style>
  .status-bar {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 24px;
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-subtle);
    background: var(--surface-0);
    border-top: 1px solid var(--stroke-lo);
    flex-shrink: 0;
    user-select: none;
  }

  .seg {
    padding: 0 10px;
    white-space: nowrap;
  }

  .seg.running {
    color: var(--interactive);
  }

  .sep {
    color: var(--text-faint);
  }
</style>

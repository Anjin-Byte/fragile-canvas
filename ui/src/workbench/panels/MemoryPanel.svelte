<script lang="ts">
  /**
   * Memory inspector — live hex dump with region quick-jumps (spec §4).
   * Reads a 256-byte page through the bus; heartbeat-throttled while
   * running, immediate on step. Editing comes with the memory-poke API.
   */
  import { hex8, hex16, type EmuController } from "../emu.svelte.js";

  let { emu }: { emu: EmuController } = $props();

  const REGIONS: { label: string; addr: number; title: string }[] = [
    { label: "ROM0", addr: 0x0000, title: "ROM bank 0 (0000-3FFF)" },
    { label: "ROMN", addr: 0x4000, title: "ROM bank N (4000-7FFF)" },
    { label: "VRAM", addr: 0x8000, title: "Video RAM (8000-9FFF)" },
    { label: "ERAM", addr: 0xa000, title: "External RAM (A000-BFFF)" },
    { label: "WRAM", addr: 0xc000, title: "Work RAM (C000-DFFF)" },
    { label: "OAM", addr: 0xfe00, title: "Object attributes (FE00-FE9F)" },
    { label: "I/O", addr: 0xff00, title: "I/O registers (FF00-FF7F)" },
    { label: "HRAM", addr: 0xff80, title: "High RAM (FF80-FFFE)" },
  ];

  const PAGE = 256;

  let base = $state(0x0000);
  let addrInput = $state("0000");
  let bytes = $state<number[]>([]);

  function jumpTo(addr: number) {
    base = Math.max(0, Math.min(0xffff - PAGE + 1, addr)) & 0xfff0;
    addrInput = hex16(base);
    void refresh();
  }

  function onAddrSubmit(e: Event) {
    e.preventDefault();
    const parsed = parseInt(addrInput.replace(/^0x/i, ""), 16);
    if (!Number.isNaN(parsed)) jumpTo(parsed);
  }

  async function refresh() {
    if (!emu.romLoaded) return;
    bytes = await emu.read(base, PAGE);
  }

  // Heartbeat refresh: ~10 Hz while running, immediate on step/pause.
  $effect(() => {
    const f = emu.frameCount;
    void base;
    if (!emu.romLoaded) return;
    if (emu.running && f % 6 !== 0) return;
    void refresh();
  });

  const rows = $derived.by(() => {
    const out: { addr: number; hex: string[]; ascii: string }[] = [];
    for (let r = 0; r < PAGE / 16; r++) {
      const rowBytes = bytes.slice(r * 16, r * 16 + 16);
      out.push({
        addr: base + r * 16,
        hex: rowBytes.map(hex8),
        ascii: rowBytes
          .map((b) => (b >= 0x20 && b < 0x7f ? String.fromCharCode(b) : "."))
          .join(""),
      });
    }
    return out;
  });
</script>

<div class="memory-panel">
  <div class="mem-controls">
    <form class="mem-addr" onsubmit={onAddrSubmit}>
      <span class="mem-addr-prefix">$</span>
      <input
        class="mem-addr-input"
        bind:value={addrInput}
        spellcheck="false"
        aria-label="Go to address"
      />
    </form>
    <div class="mem-regions">
      {#each REGIONS as region (region.label)}
        <button
          class="mem-region"
          class:active={base === region.addr}
          title={region.title}
          onclick={() => jumpTo(region.addr)}
        >{region.label}</button>
      {/each}
    </div>
  </div>

  <div class="mem-grid">
    {#if bytes.length > 0}
      {#each rows as row (row.addr)}
        <div class="mem-row">
          <span class="mem-row-addr">{hex16(row.addr)}</span>
          <span class="mem-row-hex">
            {#each row.hex as byte, i}
              <span class="mem-byte" class:mem-byte-gap={i === 8}>{byte}</span>
            {/each}
          </span>
          <span class="mem-row-ascii">{row.ascii}</span>
        </div>
      {/each}
    {:else}
      <div class="empty-hint">Load a ROM to inspect memory</div>
    {/if}
  </div>
</div>

<style>
  .memory-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .mem-controls {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border-bottom: 1px solid var(--stroke-lo);
    flex-wrap: wrap;
  }

  .mem-addr {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 0 6px;
    background: var(--fill-lo);
    border: 1px solid var(--stroke-mid);
    border-radius: var(--radius-sm);
  }

  .mem-addr-prefix {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-faint);
  }

  .mem-addr-input {
    width: 44px;
    padding: 3px 0;
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-hi);
    background: none;
    border: none;
    outline: none;
    text-transform: uppercase;
  }

  .mem-regions {
    display: flex;
    gap: 2px;
    flex-wrap: wrap;
  }

  .mem-region {
    padding: 3px 7px;
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-subtle);
    background: none;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: color 0.1s ease, background 0.1s ease;
  }

  .mem-region:hover {
    color: var(--text-mid);
    background: var(--fill-lo);
  }

  .mem-region.active {
    color: var(--interactive);
    background: var(--interactive-fill);
  }

  .mem-grid {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 6px 8px;
    font-family: var(--font-mono);
    font-size: 11px;
    line-height: 1.6;
  }

  .mem-row {
    display: flex;
    gap: 10px;
    white-space: nowrap;
  }

  .mem-row-addr {
    color: var(--text-subtle);
  }

  .mem-row-hex {
    display: flex;
    gap: 5px;
    color: var(--text-mid);
  }

  .mem-byte-gap {
    margin-left: 6px;
  }

  .mem-row-ascii {
    color: var(--text-faint);
    letter-spacing: 0.05em;
  }

  .empty-hint {
    padding: 16px 8px;
    font-family: var(--font-sans);
    font-size: 11px;
    color: var(--text-faint);
    text-align: center;
  }
</style>

<script lang="ts">
  /**
   * Disassembly panel blockout (spec §3). Structure only: gutter (future
   * breakpoint dots), address, raw bytes, mnemonic, comment. Live rows
   * need a decoder in sm83 (no disassembler exists yet) — the current PC
   * row is highlighted against real CPU state to prove the wiring.
   */
  import { hex16, type EmuController } from "../emu.svelte.js";

  let { emu }: { emu: EmuController } = $props();

  // Static sample rows (DMG cartridge entry sequence) purely for layout.
  const SAMPLE = [
    { addr: 0x0100, raw: "00", mnemonic: "NOP", comment: "" },
    { addr: 0x0101, raw: "C3 50 01", mnemonic: "JP $0150", comment: "" },
    { addr: 0x0150, raw: "F3", mnemonic: "DI", comment: "" },
    { addr: 0x0151, raw: "3E 01", mnemonic: "LD A, $01", comment: "" },
    { addr: 0x0153, raw: "E0 FF", mnemonic: "LDH ($FF), A", comment: "IE = $01" },
    { addr: 0x0155, raw: "31 FE FF", mnemonic: "LD SP, $FFFE", comment: "" },
    { addr: 0x0158, raw: "CD 00 20", mnemonic: "CALL $2000", comment: "" },
    { addr: 0x015b, raw: "18 FE", mnemonic: "JR -2", comment: "spin" },
  ];

  const pc = $derived(emu.cpu?.pc ?? null);
</script>

<div class="disasm-panel">
  <div class="disasm-rows">
    {#each SAMPLE as row (row.addr)}
      <div class="disasm-row" class:current={pc === row.addr}>
        <span class="disasm-gutter"></span>
        <span class="disasm-addr">{hex16(row.addr)}</span>
        <span class="disasm-raw">{row.raw}</span>
        <span class="disasm-mnemonic">{row.mnemonic}</span>
        {#if row.comment}
          <span class="disasm-comment">; {row.comment}</span>
        {/if}
      </div>
    {/each}
  </div>
  <div class="disasm-note">
    Sample rows — live decoding needs an sm83 disassembler (planned).
    {#if pc !== null}<br />PC is currently {hex16(pc)}.{/if}
  </div>
</div>

<style>
  .disasm-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .disasm-rows {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 6px 0;
    font-family: var(--font-mono);
    font-size: 11px;
    line-height: 1.7;
  }

  .disasm-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 8px;
    white-space: nowrap;
  }

  .disasm-row.current {
    background: var(--interactive-fill);
    box-shadow: inset 2px 0 0 var(--interactive);
  }

  .disasm-gutter {
    width: 10px;
    flex-shrink: 0;
    /* future breakpoint dot target */
  }

  .disasm-addr {
    color: var(--text-subtle);
  }

  .disasm-raw {
    color: var(--text-faint);
    width: 64px;
  }

  .disasm-mnemonic {
    color: var(--text-mid);
  }

  .disasm-comment {
    color: var(--text-faint);
  }

  .disasm-note {
    padding: 8px;
    font-family: var(--font-sans);
    font-size: 10px;
    color: var(--text-faint);
    border-top: 1px solid var(--stroke-lo);
  }
</style>

<script lang="ts">
  /**
   * Disassembly panel (spec §3) — live instruction stream from the
   * sm83-isa decoder via the backend's `disassemble` export. Follows PC
   * while running/stepping; the follow lock can be released by entering
   * an address, and re-engaged with the ⌖ button.
   *
   * Gutter (future breakpoint dots) / address / raw bytes / mnemonic.
   * Falls back to a placeholder note on backends without the export.
   */
  import type { DisasmLine } from "../../types";
  import { hex16, type EmuController } from "../emu.svelte.js";

  let { emu }: { emu: EmuController } = $props();

  const WINDOW = 32;

  let lines = $state<DisasmLine[]>([]);
  let follow = $state(true);
  let addrInput = $state("");
  let viewAddr = $state(0);

  async function refresh(base: number) {
    lines = await emu.disasm(base, WINDOW);
  }

  // Heartbeat: follow PC (~10 Hz while running, immediate on step/pause).
  $effect(() => {
    const f = emu.frameCount;
    if (!emu.romLoaded || !emu.canDisasm) return;
    if (emu.running && f % 6 !== 0) return;
    const base = follow ? (emu.cpu?.pc ?? 0) : viewAddr;
    void refresh(base);
  });

  function onAddrSubmit(e: Event) {
    e.preventDefault();
    const parsed = parseInt(addrInput.replace(/^0x/i, "").replace(/^\$/, ""), 16);
    if (!Number.isNaN(parsed)) {
      follow = false;
      viewAddr = parsed & 0xffff;
      void refresh(viewAddr);
    }
  }

  function refollow() {
    follow = true;
    addrInput = "";
    void refresh(emu.cpu?.pc ?? 0);
  }
</script>

<div class="disasm-panel">
  {#if emu.canDisasm}
    <div class="disasm-controls">
      <form class="disasm-addr" onsubmit={onAddrSubmit}>
        <span class="disasm-addr-prefix">$</span>
        <input
          class="disasm-addr-input"
          bind:value={addrInput}
          placeholder={emu.cpu ? hex16(emu.cpu.pc) : "0000"}
          spellcheck="false"
          aria-label="Go to address"
        />
      </form>
      <button
        class="disasm-follow"
        class:active={follow}
        title={follow ? "Following PC" : "Follow PC"}
        onclick={refollow}
      >⌖ PC</button>
    </div>

    <div class="disasm-rows">
      {#if lines.length > 0}
        {#each lines as row (row.addr)}
          <div class="disasm-row" class:current={emu.cpu?.pc === row.addr}>
            <span class="disasm-gutter"></span>
            <span class="disasm-addr-col">{hex16(row.addr)}</span>
            <span class="disasm-raw">{row.bytes}</span>
            <span class="disasm-mnemonic">{row.text}</span>
          </div>
        {/each}
      {:else}
        <div class="empty-hint">Load a ROM to disassemble</div>
      {/if}
    </div>
  {:else}
    <div class="empty-hint">
      Disassembly needs a backend with the sm83-isa export
      (rebuild the WASM package).
    </div>
  {/if}
</div>

<style>
  .disasm-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .disasm-controls {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border-bottom: 1px solid var(--stroke-lo);
  }

  .disasm-addr {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 0 6px;
    background: var(--fill-lo);
    border: 1px solid var(--stroke-mid);
    border-radius: var(--radius-sm);
  }

  .disasm-addr-prefix {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-faint);
  }

  .disasm-addr-input {
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

  .disasm-follow {
    padding: 3px 8px;
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-subtle);
    background: none;
    border: 1px solid var(--stroke-mid);
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: color 0.1s ease, border-color 0.1s ease;
  }

  .disasm-follow:hover {
    color: var(--text-mid);
  }

  .disasm-follow.active {
    color: var(--interactive);
    border-color: var(--interactive-ring);
    background: var(--interactive-fill);
  }

  .disasm-rows {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 4px 0;
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

  .disasm-addr-col {
    color: var(--text-subtle);
  }

  .disasm-raw {
    color: var(--text-faint);
    width: 66px;
    overflow: hidden;
  }

  .disasm-mnemonic {
    color: var(--text-mid);
  }

  .empty-hint {
    padding: 16px 8px;
    font-family: var(--font-sans);
    font-size: 11px;
    color: var(--text-faint);
    text-align: center;
  }
</style>

<script lang="ts">
  /**
   * APU panel blockout (spec §6). Master/NR52 status is live via bus
   * reads; per-channel detail (envelope, sweep, wave RAM, oscilloscopes)
   * needs a structured APU snapshot from the core (follow-up).
   */
  import { Section, PropRow, BitField, StatusIndicator } from "@gestalt/phi";
  import { hex8, type EmuController } from "../emu.svelte.js";

  let { emu }: { emu: EmuController } = $props();

  let nr52 = $state<number | null>(null);
  let nr50 = $state<number | null>(null);
  let nr51 = $state<number | null>(null);

  $effect(() => {
    const f = emu.frameCount;
    if (!emu.romLoaded) return;
    if (emu.running && f % 6 !== 0) return;
    emu.read(0xff24, 3).then(([a, b, c]) => {
      nr50 = a;
      nr51 = b;
      nr52 = c;
    });
  });

  const powered = $derived(nr52 === null ? undefined : (nr52 & 0x80) !== 0);

  const CHANNELS = [
    { id: 1, name: "CH1 Pulse + sweep" },
    { id: 2, name: "CH2 Pulse" },
    { id: 3, name: "CH3 Wave" },
    { id: 4, name: "CH4 Noise" },
  ];

  function channelOn(id: number): boolean | undefined {
    return nr52 === null ? undefined : (nr52 & (1 << (id - 1))) !== 0;
  }

  function panFlags() {
    return CHANNELS.map((ch) => ({
      label: `${ch.id}`,
      value: nr51 === null ? undefined : (nr51 & (1 << (ch.id - 1))) !== 0,
      title: `CH${ch.id} → right`,
    })).concat(
      CHANNELS.map((ch) => ({
        label: `${ch.id}`,
        value: nr51 === null ? undefined : (nr51 & (1 << (ch.id + 3))) !== 0,
        title: `CH${ch.id} → left`,
      })),
    );
  }
</script>

<div class="apu-panel">
  <Section sectionId="apu-master" title="Master">
    <div class="power-row">
      <StatusIndicator status={powered ? "ok" : "idle"} label={powered ? "Powered" : "Off"} />
      <span class="power-value">{nr52 === null ? "--" : hex8(nr52)}</span>
    </div>
    <PropRow label="NR50 volume" value={nr50 === null ? "--" : hex8(nr50)} />
    <BitField label="NR51 pan R/L" flags={panFlags()} />
  </Section>

  {#each CHANNELS as ch (ch.id)}
    <Section sectionId="apu-ch{ch.id}" title={ch.name}>
      <div class="ch-status">
        <StatusIndicator status={channelOn(ch.id) ? "ok" : "idle"} label={channelOn(ch.id) ? "active" : "silent"} pulse={false} />
      </div>
      <div class="ch-placeholder">
        freq · volume · envelope{ch.id === 1 ? " · sweep" : ""}{ch.id === 3 ? " · wave RAM" : ""}{ch.id === 4 ? " · LFSR" : ""}
      </div>
    </Section>
  {/each}

  <span class="placeholder-note">Channel detail needs an APU snapshot export (planned)</span>
</div>

<style>
  .apu-panel {
    height: 100%;
    overflow: auto;
    padding: 4px 10px 10px;
    display: flex;
    flex-direction: column;
  }

  .power-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    min-height: 22px;
    padding: 2px 4px;
  }

  .power-value {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-mid);
  }

  .ch-status {
    padding: 2px 4px;
  }

  .ch-placeholder {
    margin: 4px;
    padding: 10px;
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-faint);
    text-align: center;
    background: var(--fill-inset);
    border: 1px dashed var(--stroke-mid);
    border-radius: var(--radius-sm);
  }

  .placeholder-note {
    padding: 8px 4px;
    font-family: var(--font-sans);
    font-size: 10px;
    color: var(--text-faint);
    text-align: center;
  }
</style>

<script lang="ts">
  /**
   * Preferences — a modal settings dialog. It is a *view* of the command
   * registry: each control reads a command's current value and dispatches back
   * through it, so the toolbar, palette and this dialog all stay in sync.
   * Built from phi's form primitives (Section / Slider / ToggleGroup /
   * CheckboxRow).
   */
  import { Section, Slider, ToggleGroup, CheckboxRow } from "@gestalt/phi";
  import { X } from "lucide-svelte";
  import { commandsById, type Command } from "./commands.svelte.js";

  let {
    commands,
    open,
    onclose,
  }: {
    commands: Command[];
    open: boolean;
    onclose: () => void;
  } = $props();

  const byId = $derived(commandsById(commands));
  const volCmd = $derived(byId.get("audio.volume") as Extract<Command, { kind: "value" }> | undefined);
  const muteCmd = $derived(byId.get("audio.mute") as Extract<Command, { kind: "toggle" }> | undefined);
  const speedCmd = $derived(byId.get("machine.speed") as Extract<Command, { kind: "choice" }> | undefined);
  const bootCmd = $derived(byId.get("machine.skipBoot") as Extract<Command, { kind: "toggle" }> | undefined);
  const lcdCmd = $derived(byId.get("display.lcdEffect") as Extract<Command, { kind: "toggle" }> | undefined);

  const audioEnabled = $derived(!!volCmd && (volCmd.enabled ? volCmd.enabled() : true));

  /** Drive a toggle command toward `v` (its run() flips the current value). */
  function setToggle(cmd: Extract<Command, { kind: "toggle" }> | undefined, v: boolean) {
    if (cmd && v !== cmd.checked()) cmd.run();
  }

  // Close on Escape while open.
  $effect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onclose();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });
</script>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="pf-backdrop" onclick={onclose}>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="pf-panel" onclick={(e) => e.stopPropagation()}>
      <header class="pf-header">
        <span class="pf-title">Preferences</span>
        <button class="pf-close" title="Close" onclick={onclose}><X size={15} /></button>
      </header>

      <div class="pf-body">
        {#if audioEnabled && volCmd && muteCmd}
          <Section sectionId="prefs-audio" title="Audio">
            <div class="pf-controls">
              <Slider
                id="prefs-volume"
                label="Master volume"
                min={0}
                max={100}
                step={1}
                decimals={0}
                value={volCmd.value() * 100}
                onValueChange={(v) => volCmd.set(v / 100)}
              />
              <CheckboxRow
                label="Mute"
                checked={muteCmd.checked()}
                onchange={(v) => setToggle(muteCmd, v)}
              />
            </div>
          </Section>
        {/if}

        {#if speedCmd && bootCmd}
          <Section sectionId="prefs-emulation" title="Emulation">
            <div class="pf-controls">
              <div class="pf-row">
                <span class="pf-label">Speed</span>
                <ToggleGroup
                  label="Emulation speed"
                  options={speedCmd.options}
                  value={speedCmd.value()}
                  onValueChange={(v) => speedCmd.select(v)}
                />
              </div>
              <CheckboxRow
                label="Skip boot ROM"
                checked={bootCmd.checked()}
                onchange={(v) => setToggle(bootCmd, v)}
              />
            </div>
          </Section>
        {/if}

        {#if lcdCmd}
          <Section sectionId="prefs-display" title="Display">
            <div class="pf-controls">
              <CheckboxRow
                label="LCD screen effect"
                checked={lcdCmd.checked()}
                onchange={(v) => setToggle(lcdCmd, v)}
              />
            </div>
          </Section>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .pf-backdrop {
    position: fixed;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: oklch(0 0 0 / 45%);
    backdrop-filter: blur(3px);
    z-index: 400;
  }

  .pf-panel {
    width: min(440px, 92vw);
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    background: var(--surface-3);
    border: 1px solid var(--stroke-mid);
    border-radius: var(--radius-lg, 10px);
    box-shadow: var(--shadow-overlay);
    overflow: hidden;
    /* Align native form-control accents (Slider thumb) with the app. */
    accent-color: var(--interactive);
  }

  /* phi's ToggleGroup defaults to a blue active tint — match the DMG accent. */
  .pf-body :global(.tg-btn[aria-checked="true"]) {
    background: var(--interactive-fill);
  }

  .pf-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 11px 12px 11px 16px;
    border-bottom: 1px solid var(--stroke-lo);
  }

  .pf-title {
    font-family: var(--font-sans);
    font-size: 13px;
    font-weight: 600;
    color: var(--text-hi);
  }

  .pf-close {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    color: var(--text-subtle);
    background: none;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: color 0.1s ease, background 0.1s ease;
  }

  .pf-close:hover {
    color: var(--text-hi);
    background: var(--fill-mid);
  }

  .pf-body {
    overflow-y: auto;
    padding: 4px 16px 12px;
  }

  .pf-controls {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 8px 0 4px;
  }

  .pf-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .pf-label {
    font-family: var(--font-sans);
    font-size: 12px;
    color: var(--text-mid);
  }
</style>

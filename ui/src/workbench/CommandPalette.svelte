<script lang="ts">
  /**
   * Command palette — a searchable view of the command registry (Cmd/Ctrl+K).
   * Renders every action / toggle / choice command; choices expand into one
   * row per option ("Speed → 2×"). Executing dispatches back through the same
   * registry the toolbar and preferences use, so state stays consistent.
   */
  import { Check } from "lucide-svelte";
  import { GROUP_ORDER, type Command, type CommandGroup } from "./commands.svelte.js";

  let {
    commands,
    open,
    onclose,
  }: {
    commands: Command[];
    open: boolean;
    onclose: () => void;
  } = $props();

  interface Entry {
    key: string;
    group: CommandGroup;
    label: string;
    /** Checked toggle or current choice option. */
    selected: boolean;
    disabled: boolean;
    shortcut?: string;
    act: () => void;
  }

  let query = $state("");
  let selectedIdx = $state(0);
  let inputEl = $state<HTMLInputElement>();

  const groupRank = (g: CommandGroup) => {
    const i = GROUP_ORDER.indexOf(g);
    return i < 0 ? GROUP_ORDER.length : i;
  };

  // Flatten the registry into palette rows. Thunks (checked/value/enabled) are
  // read here so the list reflects live state.
  const entries = $derived.by<Entry[]>(() => {
    const out: Entry[] = [];
    for (const c of commands) {
      const enabled = c.enabled ? c.enabled() : true;
      if (c.kind === "action") {
        out.push({
          key: c.id,
          group: c.group,
          label: c.label,
          selected: false,
          disabled: !enabled,
          shortcut: c.shortcut,
          act: () => c.run(),
        });
      } else if (c.kind === "toggle") {
        out.push({
          key: c.id,
          group: c.group,
          label: c.label,
          selected: c.checked(),
          disabled: !enabled,
          act: () => c.run(),
        });
      } else if (c.kind === "choice") {
        const current = c.value();
        for (const opt of c.options) {
          out.push({
            key: `${c.id}:${opt.value}`,
            group: c.group,
            label: `${c.label} → ${opt.label}`,
            selected: opt.value === current,
            disabled: !enabled,
            act: () => c.select(opt.value),
          });
        }
      }
      // value / submenu kinds have no useful palette row.
    }
    return out.sort((a, b) => groupRank(a.group) - groupRank(b.group));
  });

  const filtered = $derived.by<Entry[]>(() => {
    const q = query.trim().toLowerCase();
    if (!q) return entries;
    return entries.filter((e) => `${e.label} ${e.group}`.toLowerCase().includes(q));
  });

  // Keep the selection in range as the filtered set changes.
  $effect(() => {
    if (selectedIdx >= filtered.length) selectedIdx = Math.max(0, filtered.length - 1);
  });

  // Focus the input and reset when the palette opens.
  $effect(() => {
    if (open) {
      query = "";
      selectedIdx = 0;
      queueMicrotask(() => inputEl?.focus());
    }
  });

  function run(entry: Entry) {
    if (entry.disabled) return;
    entry.act();
    onclose();
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      onclose();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      if (filtered.length) selectedIdx = (selectedIdx + 1) % filtered.length;
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      if (filtered.length) selectedIdx = (selectedIdx - 1 + filtered.length) % filtered.length;
    } else if (e.key === "Enter") {
      e.preventDefault();
      const entry = filtered[selectedIdx];
      if (entry) run(entry);
    }
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="cp-backdrop" onclick={onclose}>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="cp-panel" onclick={(e) => e.stopPropagation()} onkeydown={onKeydown}>
      <input
        bind:this={inputEl}
        class="cp-input"
        type="text"
        placeholder="Search commands…"
        bind:value={query}
        spellcheck="false"
        autocomplete="off"
      />
      <div class="cp-list" role="listbox" aria-label="Commands">
        {#each filtered as entry, i (entry.key)}
          <button
            class="cp-row"
            class:active={i === selectedIdx}
            class:disabled={entry.disabled}
            role="option"
            aria-selected={i === selectedIdx}
            disabled={entry.disabled}
            onmousemove={() => (selectedIdx = i)}
            onclick={() => run(entry)}
          >
            <span class="cp-check">{#if entry.selected}<Check size={13} />{/if}</span>
            <span class="cp-label">{entry.label}</span>
            {#if entry.shortcut}
              <span class="cp-shortcut">{entry.shortcut}</span>
            {:else}
              <span class="cp-group">{entry.group}</span>
            {/if}
          </button>
        {:else}
          <div class="cp-empty">No matching commands</div>
        {/each}
      </div>
    </div>
  </div>
{/if}

<style>
  .cp-backdrop {
    position: fixed;
    inset: 0;
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 12vh;
    background: oklch(0 0 0 / 45%);
    backdrop-filter: blur(3px);
    z-index: 400;
  }

  .cp-panel {
    width: min(560px, 92vw);
    max-height: 62vh;
    display: flex;
    flex-direction: column;
    background: var(--surface-3);
    border: 1px solid var(--stroke-mid);
    border-radius: var(--radius-lg, 10px);
    box-shadow: var(--shadow-overlay);
    overflow: hidden;
  }

  .cp-input {
    flex-shrink: 0;
    padding: 12px 14px;
    font-family: var(--font-sans);
    font-size: 14px;
    color: var(--text-hi);
    background: none;
    border: none;
    border-bottom: 1px solid var(--stroke-lo);
    outline: none;
  }

  .cp-input::placeholder {
    color: var(--text-faint);
  }

  .cp-list {
    overflow-y: auto;
    padding: 4px;
  }

  .cp-row {
    display: flex;
    align-items: center;
    gap: 9px;
    width: 100%;
    padding: 7px 9px;
    background: none;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    text-align: left;
    color: var(--text-mid);
    font-family: var(--font-sans);
    font-size: 12.5px;
  }

  .cp-row.active:not(.disabled) {
    background: var(--fill-mid);
    color: var(--text-hi);
  }

  .cp-row.disabled {
    opacity: 0.4;
    cursor: default;
  }

  .cp-check {
    display: inline-flex;
    justify-content: center;
    width: 14px;
    flex-shrink: 0;
    color: var(--interactive);
  }

  .cp-label {
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .cp-shortcut {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-subtle);
  }

  .cp-group {
    font-family: var(--font-mono);
    font-size: 9.5px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-faint);
  }

  .cp-empty {
    padding: 18px;
    text-align: center;
    font-family: var(--font-sans);
    font-size: 12px;
    color: var(--text-faint);
  }
</style>

<script lang="ts">
  /**
   * DockLayout playground — manual verification surface for the dock.
   *
   * Exercises: splits on all 4 edges, tabify, tab reorder, sash drags
   * (incl. nested), close-to-collapse, maximize (dbl-click a tab bar),
   * keep-alive panel state (counter/textarea/scroll survive moves), and
   * persistence across reloads.
   */
  import DockLayout from "../src/dock/DockLayout.svelte";
  import {
    DockModel,
    type PanelDef,
    type SerializedNode,
  } from "../src/dock/model.svelte.js";

  const DEFAULT_LAYOUT: SerializedNode = {
    type: "branch",
    orientation: "row",
    children: [
      { type: "leaf", id: "side", panels: ["Registers", "Notes"] },
      {
        type: "branch",
        orientation: "column",
        fraction: 2.5,
        children: [
          { type: "leaf", id: "main", panels: ["Counter", "About"] },
          { type: "leaf", id: "bottom", panels: ["Log"], fraction: 0.6 },
        ],
      },
    ],
  };

  const PANEL_DEFS: PanelDef[] = [
    { id: "Registers", title: "CPU Registers", closable: false, minWidth: 160 },
    { id: "Counter", title: "Counter Demo" },
    { id: "Notes", title: "Scratch Notes" },
    { id: "Log", title: "Event Log", minHeight: 100 },
    { id: "About", title: "About" },
  ];

  const KEY = "phi-dock-playground";
  let model = $state(DockModel.fromStorage(KEY, DEFAULT_LAYOUT));

  function reset() {
    localStorage.removeItem(KEY);
    model = new DockModel(DEFAULT_LAYOUT);
  }

  function reopenAbout() {
    model.openPanel("About", { near: "Counter" });
  }

  // Demo panel state — proves keep-alive across tab switches and moves.
  let count = $state(0);
  let seconds = $state(0);
  $effect(() => {
    const t = setInterval(() => (seconds += 1), 1000);
    return () => clearInterval(t);
  });

  const logLines = Array.from(
    { length: 300 },
    (_, i) => `[${String(i).padStart(4, "0")}] sample event — scroll me, then move the panel`,
  );

  const regs: [string, string][] = [
    ["AF", "0x01B0"],
    ["BC", "0x0013"],
    ["DE", "0x00D8"],
    ["HL", "0x014D"],
    ["SP", "0xFFFE"],
    ["PC", "0x0100"],
  ];
</script>

<div class="shell">
  <header>
    <span class="title">Phi DockLayout</span>
    <span class="hint">
      drag tabs: edges split · center tabifies · strips reorder · container
      edges root-split · Alt+drop floats · ⧉ floats a group · dbl-click strip
      maximizes · dbl-click sash equalizes · Alt+W/[/]/Enter keys
    </span>
    <button class="reset" onclick={reopenAbout}>Open About</button>
    <button class="reset" onclick={reset}>Reset layout</button>
  </header>
  <main>
    <DockLayout {model} persistKey={KEY} panelDefs={PANEL_DEFS}>
      {#snippet panel(id: string)}
        {#if id === "Counter"}
          <div class="pad">
            <p>Panel state survives tab switches and moves between groups:</p>
            <button class="demo-btn" onclick={() => count++}>
              clicked {count} times
            </button>
            <p class="dim">mounted for {seconds}s</p>
          </div>
        {:else if id === "Notes"}
          <div class="pad fill">
            <textarea
              class="notes"
              placeholder="Type here, then drag this panel somewhere else — the text persists."
            ></textarea>
          </div>
        {:else if id === "Log"}
          <div class="mono log">
            {#each logLines as line (line)}
              <div>{line}</div>
            {/each}
          </div>
        {:else if id === "Registers"}
          <div class="pad mono">
            {#each regs as [name, value] (name)}
              <div class="reg">
                <span class="dim">{name}</span>
                <span>{value}</span>
              </div>
            {/each}
          </div>
        {:else}
          <div class="pad">
            <p>Reactive dock rewrite: fractions in <code>$state</code>,
            rects <code>$derived</code> — no invalidation bugs by construction.</p>
          </div>
        {/if}
      {/snippet}
    </DockLayout>
  </main>
</div>

<style>
  .shell {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  header {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 12px;
    background: var(--surface-0);
    border-bottom: 1px solid var(--stroke-lo);
    flex-shrink: 0;
  }

  .title {
    color: var(--text-hi);
    font-weight: 600;
    font-size: 12px;
  }

  .hint {
    color: var(--text-subtle);
    font-size: 11px;
    flex: 1;
  }

  .reset {
    padding: 4px 10px;
    font-size: 11px;
    font-family: inherit;
    color: var(--text-hi);
    background: var(--fill-mid);
    border: 1px solid var(--stroke-mid);
    border-radius: 4px;
    cursor: pointer;
  }

  .reset:hover {
    border-color: var(--interactive);
  }

  main {
    flex: 1;
    min-height: 0;
    padding: 8px;
  }

  .pad {
    padding: 12px;
  }

  .fill {
    height: 100%;
  }

  .dim {
    color: var(--text-subtle);
  }

  .mono {
    font-family: ui-monospace, "SF Mono", Menlo, monospace;
    font-size: 11px;
  }

  .log {
    padding: 8px 12px;
    color: var(--text-lo);
    white-space: nowrap;
  }

  .reg {
    display: flex;
    justify-content: space-between;
    max-width: 160px;
    padding: 2px 0;
  }

  .notes {
    width: 100%;
    height: calc(100% - 8px);
    resize: none;
    background: var(--fill-lo);
    border: 1px solid var(--stroke-mid);
    border-radius: 4px;
    color: var(--text-mid);
    font-family: inherit;
    font-size: 12px;
    padding: 8px;
    outline: none;
  }

  .notes:focus {
    box-shadow: 0 0 0 2px var(--interactive-ring);
  }

  .demo-btn {
    padding: 5px 12px;
    font-size: 12px;
    font-family: inherit;
    color: var(--text-hi);
    background: var(--fill-mid);
    border: 1px solid var(--stroke-mid);
    border-radius: 4px;
    cursor: pointer;
  }

  .demo-btn:hover {
    border-color: var(--interactive);
  }
</style>

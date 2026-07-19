<script lang="ts" module>
  /** Registry resolution consumed by the tab strip. */
  export interface ResolvedPanelDef {
    title: string;
    closable: boolean;
  }
</script>

<script lang="ts">
  /**
   * Tab bar for a dock group. Presentational: renders the tab strip from
   * a panel list and reports interactions upward. Each tab is draggable —
   * DockLayout owns the drag orchestration and passes `dropIndex` back
   * down to render the insertion caret during a drag over this strip.
   *
   * Right-side controls: an overflow menu (») when the strip scrolls, and
   * a float/dock toggle (⧉/⇱) when `onfloat` is provided. Tab labels and
   * per-tab close visibility resolve through `getDef` (the panel registry).
   */

  let {
    panels,
    activePanel,
    onactivate,
    onclose,
    ondragstartpanel,
    ondragendpanel,
    dropIndex = null,
    getDef = (id: string) => ({ title: id, closable: true }),
    floating = false,
    onfloat,
  }: {
    panels: string[];
    activePanel: string;
    onactivate: (panelId: string) => void;
    onclose?: (panelId: string) => void;
    /** Fired when a tab drag begins. */
    ondragstartpanel?: (panelId: string, event: DragEvent) => void;
    /** Fired when a tab drag ends (regardless of drop). */
    ondragendpanel?: () => void;
    /** Insertion caret position during a drag over this strip, or null. */
    dropIndex?: number | null;
    /** Panel registry resolver: tab title + close-button visibility. */
    getDef?: (id: string) => ResolvedPanelDef;
    /** Whether this group is a floating window (flips the float glyph). */
    floating?: boolean;
    /** Float/dock toggle for the whole group. Button hidden when absent. */
    onfloat?: () => void;
  } = $props();

  function handleDragStart(panelId: string, event: DragEvent) {
    if (!event.dataTransfer) return;
    const target = event.currentTarget as HTMLElement;
    event.dataTransfer.setDragImage?.(target, 20, 13);
    ondragstartpanel?.(panelId, event);
  }

  // ─── Overflow menu ─────────────────────────────────────────────────────

  let stripEl = $state<HTMLElement | null>(null);
  let barEl = $state<HTMLElement | null>(null);
  let overflowing = $state(false);
  let menuOpen = $state(false);

  function checkOverflow() {
    if (stripEl) overflowing = stripEl.scrollWidth > stripEl.clientWidth + 1;
  }

  $effect(() => {
    if (!stripEl) return;
    const ro = new ResizeObserver(checkOverflow);
    ro.observe(stripEl);
    checkOverflow();
    return () => ro.disconnect();
  });

  $effect(() => {
    void panels.length; // re-measure whenever the tab set changes
    checkOverflow();
  });

  $effect(() => {
    if (!menuOpen) return;
    const onOutside = (e: PointerEvent) => {
      if (barEl && !barEl.contains(e.target as Node)) menuOpen = false;
    };
    window.addEventListener("pointerdown", onOutside, true);
    return () => window.removeEventListener("pointerdown", onOutside, true);
  });
</script>

<div class="dock-tabs-bar" bind:this={barEl}>
  <div class="dock-tabs" role="tablist" bind:this={stripEl}>
    {#each panels as panel, i (panel)}
      {#if dropIndex === i}
        <div class="dock-tab-caret"></div>
      {/if}
      <button
        class="dock-tab"
        class:active={panel === activePanel}
        role="tab"
        aria-selected={panel === activePanel}
        draggable="true"
        onclick={() => onactivate(panel)}
        ondragstart={(e) => handleDragStart(panel, e)}
        ondragend={() => ondragendpanel?.()}
      >
        <span class="dock-tab-label">{getDef(panel).title}</span>
        {#if onclose && getDef(panel).closable}
          <!-- svelte-ignore node_invalid_placement_ssr -->
          <span
            class="dock-tab-close"
            role="button"
            tabindex="-1"
            aria-label="Close {getDef(panel).title}"
            onclick={(e) => {
              e.stopPropagation();
              onclose?.(panel);
            }}
            onkeydown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.stopPropagation();
                onclose?.(panel);
              }
            }}
          >&times;</span>
        {/if}
      </button>
    {/each}
    {#if dropIndex !== null && dropIndex >= panels.length}
      <div class="dock-tab-caret"></div>
    {/if}
  </div>

  <div class="dock-tabs-controls">
    {#if overflowing}
      <button
        class="dock-tabs-btn"
        aria-label="All tabs"
        aria-expanded={menuOpen}
        onclick={() => (menuOpen = !menuOpen)}
      >&raquo;</button>
    {/if}
    {#if onfloat}
      <button
        class="dock-tabs-btn"
        aria-label={floating ? "Dock group" : "Float group"}
        title={floating ? "Dock group" : "Float group"}
        onclick={() => onfloat?.()}
      >{floating ? "⇱" : "⧉"}</button>
    {/if}
  </div>

  {#if menuOpen}
    <div class="dock-tabs-menu" role="menu">
      {#each panels as panel (panel)}
        <button
          class="dock-tabs-menu-item"
          class:active={panel === activePanel}
          role="menuitem"
          onclick={() => {
            onactivate(panel);
            menuOpen = false;
          }}
        >{getDef(panel).title}</button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .dock-tabs-bar {
    position: relative;
    display: flex;
    align-items: stretch;
    height: 26px;
    background: var(--fill-lo, oklch(1 0 0 / 0.05));
    border-bottom: 1px solid var(--stroke-lo, oklch(1 0 0 / 0.06));
    flex-shrink: 0;
    user-select: none;
  }

  .dock-tabs {
    display: flex;
    align-items: stretch;
    flex: 1;
    min-width: 0;
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: none;
  }

  .dock-tab {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 0 10px;
    border: none;
    border-right: 1px solid var(--stroke-lo, oklch(1 0 0 / 0.06));
    background: none;
    color: var(--text-subtle, #999);
    font-size: 11px;
    font-weight: 500;
    font-family: inherit;
    cursor: grab;
    white-space: nowrap;
    transition: color 0.1s ease, background 0.1s ease;
  }

  .dock-tab:hover {
    color: var(--text-mid, #ccc);
    background: var(--fill-mid, oklch(1 0 0 / 0.08));
  }

  .dock-tab.active {
    color: var(--text-hi, #eee);
    background: var(--fill-mid, oklch(1 0 0 / 0.08));
    box-shadow: inset 0 -2px 0 var(--interactive, oklch(0.80 0.16 250));
  }

  .dock-tab:focus-visible {
    outline: none;
    box-shadow: inset 0 0 0 2px var(--interactive-ring, oklch(0.80 0.16 250 / 30%));
  }

  .dock-tab:active {
    cursor: grabbing;
  }

  .dock-tab-label {
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 140px;
  }

  .dock-tab-close {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 14px;
    height: 14px;
    padding: 0;
    border: none;
    border-radius: 2px;
    background: none;
    color: var(--text-faint, #555);
    font-size: 12px;
    line-height: 1;
    cursor: pointer;
  }

  .dock-tab-close:hover {
    color: var(--text-mid, #ccc);
    background: oklch(1 0 0 / 0.1);
  }

  .dock-tab-caret {
    flex-shrink: 0;
    width: 2px;
    margin: 3px 0;
    background: var(--interactive, oklch(0.80 0.16 250));
    border-radius: 1px;
  }

  .dock-tabs-controls {
    display: flex;
    align-items: center;
    flex-shrink: 0;
    padding: 0 2px;
  }

  .dock-tabs-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    padding: 0;
    border: none;
    border-radius: 3px;
    background: none;
    color: var(--text-faint, #555);
    font-size: 12px;
    line-height: 1;
    font-family: inherit;
    cursor: pointer;
  }

  .dock-tabs-btn:hover {
    color: var(--text-mid, #ccc);
    background: var(--fill-mid, oklch(1 0 0 / 0.08));
  }

  .dock-tabs-menu {
    position: absolute;
    top: 100%;
    right: 0;
    z-index: 60;
    min-width: 120px;
    max-height: 240px;
    overflow-y: auto;
    padding: 3px;
    background: var(--surface-5, oklch(0.22 0.015 250));
    border: 1px solid var(--stroke-mid, oklch(1 0 0 / 0.12));
    border-radius: 4px;
    box-shadow: 0 4px 16px oklch(0 0 0 / 0.4);
  }

  .dock-tabs-menu-item {
    display: block;
    width: 100%;
    padding: 4px 8px;
    border: none;
    border-radius: 3px;
    background: none;
    color: var(--text-mid, #ccc);
    font-size: 11px;
    font-family: inherit;
    text-align: left;
    cursor: pointer;
  }

  .dock-tabs-menu-item:hover {
    background: var(--fill-mid, oklch(1 0 0 / 0.08));
  }

  .dock-tabs-menu-item.active {
    color: var(--text-hi, #eee);
  }
</style>

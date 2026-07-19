<script lang="ts">
  /**
   * DockLayout — reactive 2D docking container.
   *
   * One $derived solve() maps (model tree, container size) → pixel rects.
   * There is no cached layout state to invalidate: sash drags and
   * structural operations mutate the DockModel and every rect re-derives.
   *
   * Rendering layers (absolute, bottom to top):
   *   1. group chrome (tab bars + backdrops), one per docked leaf
   *   2. keep-alive panel layer — every panel of every group (docked AND
   *      floating) stays mounted, keyed by panel id, positioned into its
   *      group's content area, display:none unless it is the active tab.
   *      Panels survive tab switches, moves, and floating without
   *      unmounting.
   *   3. sashes
   *   4. floating windows (chrome + their panels above)
   *   5. drop overlays during tab drags
   *
   * DnD is a single geometric hit-test at this level against the solved
   * rects. Priority: Alt (float at cursor) → floating windows → container
   * edges (root splits) → tab strips (insert/reorder) → group zones
   * (tabify/split).
   *
   * Keyboard (while focus is inside the layout): Alt+W close active tab,
   * Alt+[ / Alt+] cycle tabs, Alt+Enter maximize toggle.
   */
  import type { Snippet } from "svelte";
  import {
    DockModel,
    type Orientation,
    type Direction,
    type PanelDef,
    type FloatingGroup,
  } from "./model.svelte.js";
  import {
    solve,
    minSize,
    distribute,
    resizeSash,
    equalizeAtSash,
    TAB_BAR_HEIGHT,
    type SolveOptions,
    type SashLayout,
    type Rect,
  } from "./solve.js";
  import {
    detectZone,
    zoneToDirection,
    rectContains,
    insertionIndex,
    insertionToReorderIndex,
    setDragPayload,
    getDragPayload,
    hasPanelDrag,
    type PanelDragPayload,
    type DropZonePosition,
  } from "./dnd.js";
  import type { ResolvedPanelDef } from "./DockTabs.svelte";
  import DockGroup from "./DockGroup.svelte";
  import DropOverlay from "./DropOverlay.svelte";

  let {
    model,
    panel,
    panelDefs,
    empty,
    minPanelWidth = 120,
    minPanelHeight = 80,
    persistKey,
  }: {
    model: DockModel;
    /** Renders one panel's content by panel id. Mounted once per panel. */
    panel: Snippet<[string]>;
    /** Panel registry: tab titles, close visibility, per-panel minimums. */
    panelDefs?: PanelDef[];
    /** Rendered when no docked groups exist (floating windows may remain). */
    empty?: Snippet;
    minPanelWidth?: number;
    minPanelHeight?: number;
    /**
     * When set, the layout is saved (debounced) to localStorage under this
     * key on every change. Restore at construction time with
     * `DockModel.fromStorage(key, fallbackSpec)`.
     */
    persistKey?: string;
  } = $props();

  // ─── Panel registry ────────────────────────────────────────────────────

  const defs = $derived(new Map((panelDefs ?? []).map((d) => [d.id, d])));

  function getDef(id: string): ResolvedPanelDef {
    const def = defs.get(id);
    return { title: def?.title ?? id, closable: def?.closable ?? true };
  }

  const panelMins = $derived.by(() => {
    const map = new Map<string, { width?: number; height?: number }>();
    for (const def of defs.values()) {
      if (def.minWidth !== undefined || def.minHeight !== undefined) {
        map.set(def.id, { width: def.minWidth, height: def.minHeight });
      }
    }
    return map;
  });

  // ─── Container size ────────────────────────────────────────────────────

  let containerEl = $state<HTMLElement | null>(null);
  let containerW = $state(0);
  let containerH = $state(0);

  $effect(() => {
    if (!containerEl) return;
    const ro = new ResizeObserver((entries) => {
      const { width, height } = entries[0].contentRect;
      containerW = width;
      containerH = height;
    });
    ro.observe(containerEl);
    const rect = containerEl.getBoundingClientRect();
    containerW = rect.width;
    containerH = rect.height;
    return () => ro.disconnect();
  });

  // ─── Persistence ───────────────────────────────────────────────────────

  $effect(() => {
    if (!persistKey) return;
    const key = persistKey;
    // serialize() reads every reactive field, so this effect re-runs on
    // any layout change; the teardown timer makes it a trailing debounce.
    const data = JSON.stringify(model.serialize());
    const timer = setTimeout(() => {
      try {
        localStorage.setItem(key, data);
      } catch {
        /* storage full or unavailable — persistence is best-effort */
      }
    }, 200);
    return () => clearTimeout(timer);
  });

  // ─── Derived layout ────────────────────────────────────────────────────

  const opts = $derived<SolveOptions>({
    minWidth: minPanelWidth,
    minHeight: minPanelHeight,
    sashSize: 8,
    panelMins,
  });

  const solution = $derived(
    solve(model.root, containerW, containerH, model.maximizedLeafId, opts),
  );

  interface PanelLayout {
    panelId: string;
    groupId: string;
    rect: Rect;
    visible: boolean;
    z: number;
  }

  function contentRect(outer: Rect): Rect {
    // Group rect minus tab bar, inset for the 1px border.
    return {
      left: outer.left + 1,
      top: outer.top + TAB_BAR_HEIGHT,
      width: Math.max(0, outer.width - 2),
      height: Math.max(0, outer.height - TAB_BAR_HEIGHT - 1),
    };
  }

  const panelLayouts = $derived.by(() => {
    const out: PanelLayout[] = [];
    for (const item of solution.leaves) {
      const rect = contentRect(item.rect);
      for (const panelId of item.leaf.panels) {
        out.push({
          panelId,
          groupId: item.leaf.id,
          rect,
          visible: !item.hidden && item.leaf.activePanel === panelId,
          z: 1,
        });
      }
    }
    for (let i = 0; i < model.floating.length; i++) {
      const fg = model.floating[i];
      const rect = contentRect(fg.rect);
      for (const panelId of fg.leaf.panels) {
        out.push({
          panelId,
          groupId: fg.leaf.id,
          rect,
          visible: fg.leaf.activePanel === panelId,
          z: 41 + i * 2,
        });
      }
    }
    // Stable order so DOM nodes never re-order (keyed by panel id).
    out.sort((a, b) => (a.panelId < b.panelId ? -1 : a.panelId > b.panelId ? 1 : 0));
    return out;
  });

  /** Set the active group; floating groups also raise to the front. */
  function focusGroup(groupId: string) {
    if (model.floatingOf(groupId)) model.bringToFront(groupId);
    else model.setActiveLeaf(groupId);
  }

  // ─── Keyboard ──────────────────────────────────────────────────────────

  function handleContainerPointerDown(e: PointerEvent) {
    // Keep keyboard shortcuts alive when clicking chrome, without
    // stealing focus from panel content.
    if (!(e.target as HTMLElement).closest(".dock-panel")) {
      containerEl?.focus({ preventScroll: true });
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    // Alt-based (e.code, so macOS Option-glyphs don't interfere); the
    // browser owns Ctrl/Cmd+W and Ctrl+Tab.
    if (!e.altKey || e.ctrlKey || e.metaKey || e.shiftKey) return;
    const leaf = model.activeLeaf;
    if (!leaf) return;

    if (e.code === "KeyW") {
      if (getDef(leaf.activePanel).closable) {
        model.closePanel(leaf.id, leaf.activePanel);
      }
      e.preventDefault();
    } else if (e.code === "BracketRight" || e.code === "BracketLeft") {
      const n = leaf.panels.length;
      if (n > 1) {
        const idx = leaf.panels.indexOf(leaf.activePanel);
        const step = e.code === "BracketRight" ? 1 : -1;
        leaf.activePanel = leaf.panels[(idx + step + n) % n];
      }
      e.preventDefault();
    } else if (e.code === "Enter") {
      model.toggleMaximize(leaf.id);
      e.preventDefault();
    }
  }

  // ─── Sash dragging ─────────────────────────────────────────────────────

  let sashDragOrientation = $state<Orientation | null>(null);

  function handleSashDown(sash: SashLayout, event: PointerEvent) {
    if (event.button !== 0) return;
    event.preventDefault();
    const el = event.currentTarget as HTMLElement;
    el.setPointerCapture?.(event.pointerId);

    const { branch, index } = sash;
    const isRow = sash.orientation === "row";
    const extent = isRow ? sash.branchRect.width : sash.branchRect.height;

    // Snapshot child px sizes and minimums at drag start — the same
    // distribution the current frame was drawn with, so drags are
    // drift-free relative to what the user sees.
    const mins = branch.children.map((c) => {
      const m = minSize(c, opts);
      return isRow ? m.width : m.height;
    });
    const snapshot = distribute(
      branch.children.map((c) => c.fraction),
      mins,
      extent,
    );
    const startPos = isRow ? event.clientX : event.clientY;

    const onMove = (e: PointerEvent) => {
      const delta = (isRow ? e.clientX : e.clientY) - startPos;
      resizeSash(branch, index, delta, snapshot, mins);
    };
    const onUp = (e: PointerEvent) => {
      el.releasePointerCapture?.(e.pointerId);
      el.removeEventListener("pointermove", onMove);
      el.removeEventListener("pointerup", onUp);
      el.removeEventListener("pointercancel", onUp);
      sashDragOrientation = null;
    };

    el.addEventListener("pointermove", onMove);
    el.addEventListener("pointerup", onUp);
    el.addEventListener("pointercancel", onUp);
    sashDragOrientation = sash.orientation;
  }

  // ─── Floating window move/resize ───────────────────────────────────────

  const FLOAT_MIN_W = 160;
  const FLOAT_MIN_H = 120;

  function handleFloatHeaderDown(fg: FloatingGroup, event: PointerEvent) {
    const target = event.target as HTMLElement;
    // Drag only from the strip background — not tabs, buttons, or menus.
    if (!target.closest(".dock-tabs") || target.closest("button")) return;
    if (event.button !== 0) return;
    event.preventDefault();
    startFloatDrag(fg, event, "move");
  }

  function startFloatDrag(fg: FloatingGroup, event: PointerEvent, mode: "move" | "resize") {
    const el = event.currentTarget as HTMLElement;
    el.setPointerCapture?.(event.pointerId);
    const startX = event.clientX;
    const startY = event.clientY;
    const start = { ...fg.rect };

    const onMove = (e: PointerEvent) => {
      const dx = e.clientX - startX;
      const dy = e.clientY - startY;
      if (mode === "move") {
        fg.rect = {
          ...fg.rect,
          left: Math.min(Math.max(start.left + dx, 80 - fg.rect.width), Math.max(0, containerW - 80)),
          top: Math.min(Math.max(start.top + dy, 0), Math.max(0, containerH - TAB_BAR_HEIGHT)),
        };
      } else {
        fg.rect = {
          ...fg.rect,
          width: Math.max(FLOAT_MIN_W, start.width + dx),
          height: Math.max(FLOAT_MIN_H, start.height + dy),
        };
      }
    };
    const onUp = (e: PointerEvent) => {
      el.releasePointerCapture?.(e.pointerId);
      el.removeEventListener("pointermove", onMove);
      el.removeEventListener("pointerup", onUp);
      el.removeEventListener("pointercancel", onUp);
    };
    el.addEventListener("pointermove", onMove);
    el.addEventListener("pointerup", onUp);
    el.addEventListener("pointercancel", onUp);
  }

  // ─── Panel drag-and-drop ───────────────────────────────────────────────

  const ROOT_EDGE_BAND = 16;

  let dragPayload = $state<PanelDragPayload | null>(null);
  let dropTarget = $state<{ leafId: string; zone: DropZonePosition } | null>(null);
  let tabDrop = $state<{ leafId: string; index: number } | null>(null);
  let rootDrop = $state<Direction | null>(null);
  let floatPreview = $state<Rect | null>(null);

  function handleTabDragStart(leafId: string, panelId: string, event: DragEvent) {
    if (!event.dataTransfer) return;
    setDragPayload(event.dataTransfer, { panelId, sourceGroupId: leafId });
    dragPayload = { panelId, sourceGroupId: leafId };
  }

  function clearDragState() {
    dragPayload = null;
    dropTarget = null;
    tabDrop = null;
    rootDrop = null;
    floatPreview = null;
  }

  function clearDropTargets() {
    dropTarget = null;
    tabDrop = null;
    rootDrop = null;
    floatPreview = null;
  }

  function tabInsertionIndexFor(groupId: string, clientX: number): number {
    const tabs = containerEl?.querySelectorAll(
      `[data-group-id="${groupId}"] .dock-tab`,
    );
    if (!tabs || tabs.length === 0) return 0;
    const midpoints = [...tabs].map((tab) => {
      const r = tab.getBoundingClientRect();
      return r.left + r.width / 2;
    });
    return insertionIndex(midpoints, clientX);
  }

  function handleDragOver(event: DragEvent) {
    if (!event.dataTransfer || !hasPanelDrag(event.dataTransfer) || !containerEl) return;
    const origin = containerEl.getBoundingClientRect();
    const x = event.clientX - origin.left;
    const y = event.clientY - origin.top;

    const accept = () => {
      event.preventDefault(); // required for drop to fire
      event.dataTransfer!.dropEffect = "move";
    };

    // Alt: float the panel at the cursor.
    if (event.altKey) {
      accept();
      clearDropTargets();
      floatPreview = {
        left: Math.min(Math.max(0, x - 40), Math.max(0, containerW - 120)),
        top: Math.min(Math.max(0, y - 13), Math.max(0, containerH - 60)),
        width: 360,
        height: 260,
      };
      return;
    }

    // Floating windows, topmost first: tab strip inserts, body tabifies.
    for (let i = model.floating.length - 1; i >= 0; i--) {
      const fg = model.floating[i];
      if (!rectContains(fg.rect, x, y)) continue;
      accept();
      if (y < fg.rect.top + TAB_BAR_HEIGHT) {
        tabDrop = { leafId: fg.leaf.id, index: tabInsertionIndexFor(fg.leaf.id, event.clientX) };
        dropTarget = null;
      } else {
        dropTarget = { leafId: fg.leaf.id, zone: "center" };
        tabDrop = null;
      }
      rootDrop = null;
      floatPreview = null;
      return;
    }

    // No docked layout: the whole container docks the panel as root.
    if (model.root === null) {
      accept();
      clearDropTargets();
      rootDrop = "right";
      return;
    }

    const hit = solution.leaves.find((l) => !l.hidden && rectContains(l.rect, x, y));

    // Tab strips are precise targets — they beat the container-edge band.
    if (hit && y < hit.rect.top + TAB_BAR_HEIGHT) {
      accept();
      clearDropTargets();
      tabDrop = {
        leafId: hit.leaf.id,
        index: tabInsertionIndexFor(hit.leaf.id, event.clientX),
      };
      return;
    }

    // Container edges: root-level splits spanning the full side.
    const edge: Direction | null =
      x < ROOT_EDGE_BAND ? "left"
      : x > containerW - ROOT_EDGE_BAND ? "right"
      : y < ROOT_EDGE_BAND ? "up"
      : y > containerH - ROOT_EDGE_BAND ? "down"
      : null;
    if (edge) {
      accept();
      clearDropTargets();
      rootDrop = edge;
      return;
    }

    if (!hit) {
      clearDropTargets();
      return;
    }

    accept();
    clearDropTargets();
    dropTarget = { leafId: hit.leaf.id, zone: detectZone(hit.rect, x, y) };
  }

  function handleDragLeave(event: DragEvent) {
    // Only clear when the drag leaves the layout entirely (relatedTarget
    // is outside the container, or null when leaving the window).
    const related = event.relatedTarget as Node | null;
    if (!related || !containerEl?.contains(related)) {
      clearDropTargets();
    }
  }

  function handleDrop(event: DragEvent) {
    event.preventDefault();
    const payload =
      dragPayload ?? (event.dataTransfer ? getDragPayload(event.dataTransfer) : null);
    if (!payload) {
      clearDragState();
      return;
    }
    const { panelId, sourceGroupId } = payload;

    if (floatPreview) {
      model.floatPanel(panelId, sourceGroupId, floatPreview);
    } else if (rootDrop) {
      model.splitRootWithPanel(panelId, sourceGroupId, rootDrop);
    } else if (tabDrop) {
      const { leafId, index } = tabDrop;
      if (leafId === sourceGroupId) {
        const leaf = model.findLeaf(leafId);
        const from = leaf?.panels.indexOf(panelId) ?? -1;
        if (leaf && from !== -1) {
          model.reorderPanel(leafId, from, insertionToReorderIndex(index, from));
          model.activatePanel(leafId, panelId);
        }
      } else {
        model.movePanel(panelId, sourceGroupId, leafId, index);
      }
    } else if (dropTarget) {
      const { leafId, zone } = dropTarget;
      const direction = zoneToDirection(zone);
      if (direction === null) {
        // Center drop: tabify. On the source group it's a no-op.
        if (leafId !== sourceGroupId) {
          model.movePanel(panelId, sourceGroupId, leafId);
        }
      } else {
        model.splitWithPanel(panelId, sourceGroupId, leafId, direction);
      }
    }
    clearDragState();
  }

  // ─── Drop overlay rects ────────────────────────────────────────────────

  const dropOverlayRect = $derived.by((): Rect | null => {
    if (!dropTarget) return null;
    const docked = solution.leaves.find((l) => l.leaf.id === dropTarget!.leafId);
    if (docked) return docked.rect;
    return model.floatingOf(dropTarget.leafId)?.rect ?? null;
  });

  const rootOverlayRect = $derived.by((): Rect | null => {
    if (!rootDrop) return null;
    if (model.root === null) {
      return { left: 0, top: 0, width: containerW, height: containerH };
    }
    const w = Math.round(containerW * 0.25);
    const h = Math.round(containerH * 0.25);
    switch (rootDrop) {
      case "left": return { left: 0, top: 0, width: w, height: containerH };
      case "right": return { left: containerW - w, top: 0, width: w, height: containerH };
      case "up": return { left: 0, top: 0, width: containerW, height: h };
      case "down": return { left: 0, top: containerH - h, width: containerW, height: h };
    }
  });

  function rectStyle(rect: Rect): string {
    return `left:${rect.left}px;top:${rect.top}px;width:${rect.width}px;height:${rect.height}px;`;
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions, a11y_no_noninteractive_tabindex -->
<div
  class="dock-layout"
  class:resizing-row={sashDragOrientation === "row"}
  class:resizing-column={sashDragOrientation === "column"}
  bind:this={containerEl}
  tabindex="-1"
  ondragover={handleDragOver}
  ondragleave={handleDragLeave}
  ondrop={handleDrop}
  onpointerdown={handleContainerPointerDown}
  onkeydown={handleKeydown}
>
  {#if model.root === null}
    <div class="dock-empty">
      {#if empty}
        {@render empty()}
      {:else}
        <span>No panels docked</span>
      {/if}
    </div>
  {/if}

  {#each solution.leaves as item (item.leaf.id)}
    <div
      class="dock-leaf"
      class:dock-hidden={item.hidden}
      data-leaf-id={item.leaf.id}
      style={rectStyle(item.rect)}
      onpointerdown={() => focusGroup(item.leaf.id)}
    >
      <DockGroup
        {model}
        leaf={item.leaf}
        {getDef}
        ontabdragstart={(panelId, e) => handleTabDragStart(item.leaf.id, panelId, e)}
        ontabdragend={clearDragState}
        tabDropIndex={tabDrop?.leafId === item.leaf.id ? tabDrop.index : null}
      />
    </div>
  {/each}

  {#each panelLayouts as pl (pl.panelId)}
    <div
      class="dock-panel"
      class:dock-hidden={!pl.visible}
      data-panel-id={pl.panelId}
      style="{rectStyle(pl.rect)}z-index:{pl.z};"
      onpointerdown={() => focusGroup(pl.groupId)}
    >
      {@render panel(pl.panelId)}
    </div>
  {/each}

  {#each solution.sashes as sash, i (i)}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="dock-sash"
      class:dock-sash-row={sash.orientation === "row"}
      class:dock-sash-column={sash.orientation === "column"}
      style={rectStyle(sash.rect)}
      onpointerdown={(e) => handleSashDown(sash, e)}
      ondblclick={() => equalizeAtSash(sash.branch, sash.index)}
    >
      <div class="dock-sash-line"></div>
    </div>
  {/each}

  {#each model.floating as fg, i (fg.leaf.id)}
    <div
      class="dock-float"
      data-float-id={fg.leaf.id}
      style="{rectStyle(fg.rect)}z-index:{40 + i * 2};"
      onpointerdowncapture={() => focusGroup(fg.leaf.id)}
      onpointerdown={(e) => handleFloatHeaderDown(fg, e)}
    >
      <DockGroup
        {model}
        leaf={fg.leaf}
        floating
        {getDef}
        ontabdragstart={(panelId, e) => handleTabDragStart(fg.leaf.id, panelId, e)}
        ontabdragend={clearDragState}
        tabDropIndex={tabDrop?.leafId === fg.leaf.id ? tabDrop.index : null}
      />
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="dock-float-grip"
        onpointerdown={(e) => {
          if (e.button !== 0) return;
          e.preventDefault();
          e.stopPropagation();
          startFloatDrag(fg, e, "resize");
        }}
      ></div>
    </div>
  {/each}

  {#if dropOverlayRect && dropTarget}
    <div class="dock-drop-layer" style={rectStyle(dropOverlayRect)}>
      <DropOverlay zone={dropTarget.zone} />
    </div>
  {/if}

  {#if rootOverlayRect}
    <div class="dock-drop-layer dock-drop-root" style={rectStyle(rootOverlayRect)}></div>
  {/if}

  {#if floatPreview}
    <div class="dock-drop-layer dock-float-preview" style={rectStyle(floatPreview)}></div>
  {/if}
</div>

<style>
  .dock-layout {
    position: relative;
    width: 100%;
    height: 100%;
    overflow: hidden;
    outline: none;
  }

  .dock-layout.resizing-row {
    cursor: ew-resize;
    user-select: none;
  }

  .dock-layout.resizing-column {
    cursor: ns-resize;
    user-select: none;
  }

  .dock-empty {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-faint, #555);
    font-size: 12px;
  }

  .dock-leaf {
    position: absolute;
    overflow: hidden;
  }

  .dock-panel {
    position: absolute;
    overflow: auto;
    background: var(--surface-3, oklch(0.18 0.015 250));
  }

  .dock-hidden {
    display: none;
  }

  .dock-sash {
    position: absolute;
    z-index: 10;
    display: flex;
    align-items: center;
    justify-content: center;
    touch-action: none;
  }

  .dock-sash-row {
    cursor: ew-resize;
    flex-direction: row;
  }

  .dock-sash-column {
    cursor: ns-resize;
    flex-direction: column;
  }

  .dock-sash-line {
    background: transparent;
    transition: background 0.1s ease;
  }

  .dock-sash-row .dock-sash-line {
    width: 2px;
    height: 100%;
  }

  .dock-sash-column .dock-sash-line {
    width: 100%;
    height: 2px;
  }

  .dock-sash:hover .dock-sash-line,
  .resizing-row .dock-sash-line,
  .resizing-column .dock-sash-line {
    background: var(--interactive, oklch(0.80 0.16 250));
    opacity: 0.5;
  }

  .dock-float {
    position: absolute;
    border-radius: 6px;
    box-shadow: 0 6px 24px oklch(0 0 0 / 40%), 0 1px 3px oklch(0 0 0 / 30%);
  }

  .dock-float-grip {
    position: absolute;
    right: 0;
    bottom: 0;
    width: 14px;
    height: 14px;
    cursor: nwse-resize;
    touch-action: none;
  }

  .dock-drop-layer {
    position: absolute;
    z-index: 55;
    pointer-events: none;
  }

  .dock-drop-root {
    background: oklch(0.65 0.15 250 / 0.15);
    border: 2px solid oklch(0.65 0.15 250 / 0.5);
    border-radius: 4px;
  }

  .dock-float-preview {
    border: 2px dashed oklch(0.65 0.15 250 / 0.6);
    border-radius: 6px;
    background: oklch(0.65 0.15 250 / 0.08);
  }
</style>

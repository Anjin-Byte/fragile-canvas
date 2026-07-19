<script lang="ts">
  /**
   * Chrome for one dock group (leaf): tab bar + content backdrop.
   *
   * Panel content is NOT rendered here — DockLayout renders every panel in
   * a keep-alive layer above the groups, positioned into this group's
   * content area. That is what lets panels survive tab switches and moves
   * between groups without unmounting.
   *
   * Docked groups: double-clicking the tab strip toggles maximize; the
   * ⧉ control floats the group. Floating groups: ⇱ docks it back.
   */
  import { DockModel, DockLeaf } from "./model.svelte.js";
  import DockTabs, { type ResolvedPanelDef } from "./DockTabs.svelte";

  let {
    model,
    leaf,
    floating = false,
    getDef,
    ontabdragstart,
    ontabdragend,
    tabDropIndex = null,
  }: {
    model: DockModel;
    leaf: DockLeaf;
    /** Whether this group renders inside a floating window. */
    floating?: boolean;
    /** Panel registry resolver (from DockLayout's panelDefs). */
    getDef?: (id: string) => ResolvedPanelDef;
    /** Fired when one of this group's tabs starts dragging. */
    ontabdragstart?: (panelId: string, event: DragEvent) => void;
    /** Fired when a tab drag ends (regardless of drop). */
    ontabdragend?: () => void;
    /** Insertion caret position during a drag over this strip, or null. */
    tabDropIndex?: number | null;
  } = $props();
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="dock-group"
  class:active={model.activeLeafId === leaf.id}
  class:floating
  data-group-id={leaf.id}
  ondblclick={(e) => {
    // Only the tab strip background/tabs toggle maximize, not content.
    if (!floating && (e.target as HTMLElement).closest(".dock-tabs")) {
      model.toggleMaximize(leaf.id);
    }
  }}
>
  <DockTabs
    panels={leaf.panels}
    activePanel={leaf.activePanel}
    {getDef}
    {floating}
    onactivate={(panelId) => model.activatePanel(leaf.id, panelId)}
    onclose={(panelId) => model.closePanel(leaf.id, panelId)}
    onfloat={() => (floating ? model.unfloatLeaf(leaf.id) : model.floatLeaf(leaf.id))}
    ondragstartpanel={ontabdragstart}
    ondragendpanel={ontabdragend}
    dropIndex={tabDropIndex}
  />
  <div class="dock-group-content"></div>
</div>

<style>
  .dock-group {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100%;
    overflow: hidden;
    background: var(--surface-3, oklch(0.18 0.015 250));
    border: 1px solid var(--stroke-lo, oklch(1 0 0 / 0.06));
    box-sizing: border-box;
  }

  .dock-group.active {
    border-color: var(--stroke-mid, oklch(1 0 0 / 0.12));
  }

  .dock-group.floating {
    border-color: var(--stroke-mid, oklch(1 0 0 / 0.12));
    border-radius: 6px;
  }

  .dock-group-content {
    flex: 1;
    min-width: 0;
    min-height: 0;
  }
</style>

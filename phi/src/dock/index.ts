// ─── Dock layout system ─────────────────────────────────────────────────────
// Reactive 2D docking: DockModel ($state tree, fractions as source of
// truth) + solve() (derived pixel rects) + DockLayout (rendering, sash
// drags, dnd, keep-alive panels, maximize, persistence).

export { default as DockLayout } from "./DockLayout.svelte";
export { default as DockGroup } from "./DockGroup.svelte";
export { default as DockTabs } from "./DockTabs.svelte";
export { default as DropOverlay } from "./DropOverlay.svelte";

export {
  DockModel,
  DockLeaf,
  DockBranch,
  FloatingGroup,
  directionOrientation,
} from "./model.svelte.js";
export type {
  DockNode,
  Orientation,
  Direction,
  PanelDef,
  FloatRect,
  SerializedDock,
  SerializedNode,
  SerializedLeaf,
  SerializedBranch,
  SerializedFloating,
} from "./model.svelte.js";
export type { ResolvedPanelDef } from "./DockTabs.svelte";

export {
  solve,
  distribute,
  minSize,
  resizeSash,
  equalizeAtSash,
  DEFAULT_SOLVE_OPTIONS,
  TAB_BAR_HEIGHT,
} from "./solve.js";
export type {
  Rect,
  LeafLayout,
  SashLayout,
  DockSolution,
  SolveOptions,
} from "./solve.js";

export {
  detectZone,
  zoneToDirection,
  rectContains,
  insertionIndex,
  insertionToReorderIndex,
  setDragPayload,
  getDragPayload,
  hasPanelDrag,
  PANEL_DRAG_MIME,
} from "./dnd.js";
export type { DropZonePosition, PanelDragPayload, RectLike } from "./dnd.js";

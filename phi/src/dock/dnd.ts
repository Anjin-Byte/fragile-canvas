/**
 * Dock DnD — drag-and-drop utilities for panel rearrangement.
 *
 * Zone detection, payload types, direction mapping for the 5-zone drop
 * target system (center + 4 edges), and tab-strip insertion indexing.
 *
 * All functions are pure geometry/data helpers; DockLayout owns the event
 * wiring with a single layout-level hit-test against the solved rects (no
 * per-group dragenter/dragleave bookkeeping to get out of sync).
 */

import type { Direction } from "./model.svelte.js";

// ─── Types ──────────────────────────────────────────────────────────────

/** Drop zone position within a dock group's bounding rect. */
export type DropZonePosition = "center" | "top" | "right" | "bottom" | "left";

/** Data transferred during a tab drag operation. */
export interface PanelDragPayload {
  panelId: string;
  sourceGroupId: string;
}

/** MIME type for panel drag data in dataTransfer. */
export const PANEL_DRAG_MIME = "application/x-phi-panel";

/** The minimal rect shape zone detection needs (DOMRect or solver Rect). */
export interface RectLike {
  left: number;
  top: number;
  width: number;
  height: number;
}

// ─── Zone Detection ─────────────────────────────────────────────────────

/**
 * Detect which drop zone a point is in relative to a bounding rect.
 * Point and rect must share a coordinate space.
 *
 * Uses a 20% edge threshold (matching dockview). The point must be within
 * the outer 20% of an edge to trigger that edge zone; otherwise it's "center".
 *
 * Edge priority: left/right checked before top/bottom (horizontal layout bias).
 */
export function detectZone(
  rect: RectLike,
  x: number,
  y: number,
  edgeThreshold = 0.2,
): DropZonePosition {
  const relX = (x - rect.left) / rect.width;
  const relY = (y - rect.top) / rect.height;

  if (relX < edgeThreshold) return "left";
  if (relX > 1 - edgeThreshold) return "right";
  if (relY < edgeThreshold) return "top";
  if (relY > 1 - edgeThreshold) return "bottom";
  return "center";
}

/** Whether a point lies inside a rect. */
export function rectContains(rect: RectLike, x: number, y: number): boolean {
  return (
    x >= rect.left &&
    x < rect.left + rect.width &&
    y >= rect.top &&
    y < rect.top + rect.height
  );
}

// ─── Direction Mapping ──────────────────────────────────────────────────

/**
 * Map a drop zone position to a split direction.
 * "center" has no direction — it means tabify (add to existing group).
 */
export function zoneToDirection(zone: DropZonePosition): Direction | null {
  switch (zone) {
    case "left": return "left";
    case "right": return "right";
    case "top": return "up";
    case "bottom": return "down";
    case "center": return null;
  }
}

// ─── Tab-strip insertion ────────────────────────────────────────────────

/**
 * Given the x-midpoints of the tabs in a strip, the insertion index for a
 * drop at `x`: the number of midpoints left of the pointer.
 */
export function insertionIndex(midpoints: number[], x: number): number {
  let index = 0;
  for (const mid of midpoints) {
    if (x > mid) index++;
  }
  return index;
}

/**
 * Convert a tab-strip insertion index into the final reorder position for
 * a panel already in that strip (removal shifts later indices down one).
 */
export function insertionToReorderIndex(insertion: number, fromIndex: number): number {
  return insertion > fromIndex ? insertion - 1 : insertion;
}

// ─── Payload Helpers ────────────────────────────────────────────────────

/** Encode a panel drag payload into a drag event's dataTransfer. */
export function setDragPayload(dt: DataTransfer, payload: PanelDragPayload): void {
  dt.setData(PANEL_DRAG_MIME, JSON.stringify(payload));
  dt.effectAllowed = "move";
}

/** Decode a panel drag payload from a drag event's dataTransfer. Returns null if not a panel drag. */
export function getDragPayload(dt: DataTransfer): PanelDragPayload | null {
  const raw = dt.getData(PANEL_DRAG_MIME);
  if (!raw) return null;
  try {
    return JSON.parse(raw) as PanelDragPayload;
  } catch {
    return null;
  }
}

/** Check if a drag event contains panel drag data (without reading it — useful for dragover). */
export function hasPanelDrag(dt: DataTransfer): boolean {
  return dt.types.includes(PANEL_DRAG_MIME);
}

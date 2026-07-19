// ─── Dock Layout Solver ─────────────────────────────────────────────────────
// Pure functions from (tree, container size) → pixel rects. No DOM, no
// Svelte. DockLayout wraps solve() in a single $derived, so rects can never
// go stale relative to the model — there is no cached layout to invalidate.

import {
  type DockNode,
  type Orientation,
  DockBranch,
  DockLeaf,
} from "./model.svelte.js";

// ─── Types ──────────────────────────────────────────────────────────────────

export interface Rect {
  left: number;
  top: number;
  width: number;
  height: number;
}

export interface LeafLayout {
  leaf: DockLeaf;
  rect: Rect;
  /** True when another leaf is maximized — keep mounted, display:none. */
  hidden: boolean;
}

export interface SashLayout {
  /** The branch whose children this sash resizes. */
  branch: DockBranch;
  /** Sash i sits between children[i] and children[i+1]. */
  index: number;
  /** The branch's orientation: "row" sashes resize horizontally. */
  orientation: Orientation;
  /** Hit-area rect (wider than the visible line). */
  rect: Rect;
  /** The owning branch's rect — used to snapshot child px sizes at drag start. */
  branchRect: Rect;
}

export interface DockSolution {
  leaves: LeafLayout[];
  sashes: SashLayout[];
}

export interface SolveOptions {
  /** Minimum leaf width in px. */
  minWidth: number;
  /** Minimum leaf height in px. */
  minHeight: number;
  /** Sash hit-area thickness in px. */
  sashSize: number;
  /**
   * Per-panel minimum sizes (from PanelDefs). A leaf's minimum is the
   * global minimum raised by the largest minimum among its panels.
   */
  panelMins?: ReadonlyMap<string, { width?: number; height?: number }>;
}

export const DEFAULT_SOLVE_OPTIONS: SolveOptions = {
  minWidth: 120,
  minHeight: 80,
  sashSize: 8,
};

/** Height of a dock group's tab bar in px (kept in sync with DockTabs css). */
export const TAB_BAR_HEIGHT = 26;

// ─── Minimum sizes ──────────────────────────────────────────────────────────

/** Live minimum size of a subtree. Row: widths add, heights envelope. */
export function minSize(
  node: DockNode,
  opts: SolveOptions,
): { width: number; height: number } {
  if (node.kind === "leaf") {
    let width = opts.minWidth;
    let height = opts.minHeight;
    if (opts.panelMins) {
      for (const panelId of node.panels) {
        const m = opts.panelMins.get(panelId);
        if (m?.width !== undefined) width = Math.max(width, m.width);
        if (m?.height !== undefined) height = Math.max(height, m.height);
      }
    }
    return { width, height };
  }
  const mins = node.children.map((c) => minSize(c, opts));
  if (node.orientation === "row") {
    return {
      width: mins.reduce((s, m) => s + m.width, 0),
      height: mins.reduce((s, m) => Math.max(s, m.height), 0),
    };
  }
  return {
    width: mins.reduce((s, m) => Math.max(s, m.width), 0),
    height: mins.reduce((s, m) => s + m.height, 0),
  };
}

// ─── Distribution ───────────────────────────────────────────────────────────

/**
 * Distribute `total` px among children by fraction, respecting per-child
 * minimums. When the total cannot cover the minimums, minimums are scaled
 * down proportionally (graceful degradation instead of overflow).
 * Returns exact integer sizes summing to round(total).
 */
export function distribute(
  fractions: number[],
  mins: number[],
  total: number,
): number[] {
  const n = fractions.length;
  if (n === 0) return [];

  const minSum = mins.reduce((s, m) => s + m, 0);
  let ideal: number[];

  if (total <= minSum) {
    // Container smaller than the minimums: scale minimums down.
    const scale = minSum > 0 ? total / minSum : 0;
    ideal = mins.map((m) => m * scale);
  } else {
    // Waterfall: pin children that would fall below their minimum, then
    // redistribute the remainder among the rest by fraction. Each pass pins
    // at least one child, so this terminates within n iterations.
    const pinned = new Array<boolean>(n).fill(false);
    ideal = new Array<number>(n).fill(0);
    for (;;) {
      let freeTotal = total;
      let freeFraction = 0;
      for (let i = 0; i < n; i++) {
        if (pinned[i]) freeTotal -= mins[i];
        else freeFraction += fractions[i];
      }
      let changed = false;
      for (let i = 0; i < n; i++) {
        if (pinned[i]) {
          ideal[i] = mins[i];
          continue;
        }
        ideal[i] = freeFraction > 0
          ? (fractions[i] / freeFraction) * freeTotal
          : freeTotal / (n - pinned.filter(Boolean).length);
        if (ideal[i] < mins[i]) {
          pinned[i] = true;
          changed = true;
        }
      }
      if (!changed) break;
    }
  }

  // Rounding: snap cumulative offsets so sizes are integers summing exactly.
  const sizes = new Array<number>(n);
  let cum = 0;
  let prevOffset = 0;
  for (let i = 0; i < n; i++) {
    cum += ideal[i];
    const offset = Math.round(cum);
    sizes[i] = offset - prevOffset;
    prevOffset = offset;
  }
  return sizes;
}

// ─── Solve ──────────────────────────────────────────────────────────────────

export function solve(
  root: DockNode | null,
  width: number,
  height: number,
  maximizedLeafId: string | null,
  opts: SolveOptions = DEFAULT_SOLVE_OPTIONS,
): DockSolution {
  const leaves: LeafLayout[] = [];
  const sashes: SashLayout[] = [];
  if (!root || width <= 0 || height <= 0) {
    return { leaves, sashes };
  }

  const walk = (node: DockNode, rect: Rect) => {
    if (node.kind === "leaf") {
      leaves.push({ leaf: node, rect, hidden: false });
      return;
    }

    const isRow = node.orientation === "row";
    const total = isRow ? rect.width : rect.height;
    const axisMins = node.children.map((c) => {
      const m = minSize(c, opts);
      return isRow ? m.width : m.height;
    });
    const sizes = distribute(
      node.children.map((c) => c.fraction),
      axisMins,
      total,
    );

    let offset = 0;
    for (let i = 0; i < node.children.length; i++) {
      const childRect: Rect = isRow
        ? { left: rect.left + offset, top: rect.top, width: sizes[i], height: rect.height }
        : { left: rect.left, top: rect.top + offset, width: rect.width, height: sizes[i] };
      walk(node.children[i], childRect);
      offset += sizes[i];

      if (i < node.children.length - 1) {
        const half = opts.sashSize / 2;
        sashes.push({
          branch: node,
          index: i,
          orientation: node.orientation,
          rect: isRow
            ? { left: rect.left + offset - half, top: rect.top, width: opts.sashSize, height: rect.height }
            : { left: rect.left, top: rect.top + offset - half, width: rect.width, height: opts.sashSize },
          branchRect: rect,
        });
      }
    }
  };

  walk(root, { left: 0, top: 0, width, height });

  // Maximize: the maximized leaf covers the container; everything else is
  // hidden but stays mounted. No sashes while maximized.
  if (maximizedLeafId && leaves.some((l) => l.leaf.id === maximizedLeafId)) {
    for (const l of leaves) {
      if (l.leaf.id === maximizedLeafId) {
        l.rect = { left: 0, top: 0, width, height };
        l.hidden = false;
      } else {
        l.hidden = true;
      }
    }
    return { leaves, sashes: [] };
  }

  return { leaves, sashes };
}

// ─── Sash resize ────────────────────────────────────────────────────────────

/**
 * Apply a sash drag to a branch's child fractions.
 *
 * The greedy cascade from VS Code's splitview: the child adjacent to the
 * sash absorbs the delta first; when it hits its minimum the remainder
 * cascades outward. Operates on a pixel snapshot taken at drag start so
 * repeated moves within one drag are drift-free, then writes the result
 * back as fractions (the source of truth).
 *
 * @param snapshot Child pixel sizes at drag start.
 * @param mins     Child minimum pixel sizes along the branch axis.
 * @param delta    Pointer delta in px (positive = sash moves right/down).
 */
export function resizeSash(
  branch: DockBranch,
  index: number,
  delta: number,
  snapshot: number[],
  mins: number[],
): void {
  const n = branch.children.length;
  if (index < 0 || index >= n - 1 || snapshot.length !== n) return;

  // Feasibility clamp: each side can shrink only to its minimums.
  let maxGrow = 0; // room for the up-side to grow = down-side shrinkable px
  for (let i = index + 1; i < n; i++) maxGrow += Math.max(0, snapshot[i] - mins[i]);
  let maxShrink = 0; // room for the up-side to shrink
  for (let i = 0; i <= index; i++) maxShrink += Math.max(0, snapshot[i] - mins[i]);
  const clamped = Math.max(-maxShrink, Math.min(maxGrow, delta));

  const next = [...snapshot];

  // Up side (index → 0): adjacent child absorbs first, cascade on shrink.
  let deltaUp = clamped;
  let applied = 0;
  for (let i = index; i >= 0 && deltaUp !== 0; i--) {
    const size = Math.max(mins[i], snapshot[i] + deltaUp);
    const viewDelta = size - snapshot[i];
    applied += viewDelta;
    deltaUp -= viewDelta;
    next[i] = size;
  }

  // Down side mirrors with the opposite sign (conservation of space).
  let deltaDown = applied;
  for (let i = index + 1; i < n && deltaDown !== 0; i++) {
    const size = Math.max(mins[i], snapshot[i] - deltaDown);
    const viewDelta = size - snapshot[i];
    deltaDown += viewDelta;
    next[i] = size;
  }

  const total = next.reduce((s, v) => s + v, 0);
  if (total <= 0) return;
  for (let i = 0; i < n; i++) {
    branch.children[i].fraction = next[i] / total;
  }
}

/**
 * Double-click affordance: give the two children adjacent to a sash equal
 * shares of their combined space.
 */
export function equalizeAtSash(branch: DockBranch, index: number): void {
  const a = branch.children[index];
  const b = branch.children[index + 1];
  if (!a || !b) return;
  const half = (a.fraction + b.fraction) / 2;
  a.fraction = half;
  b.fraction = half;
}

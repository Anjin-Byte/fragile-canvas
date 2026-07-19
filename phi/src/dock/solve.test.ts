// ─── Dock solver tests ──────────────────────────────────────────────────────
// Ports the layout-propagation and min-size scenarios from the retired
// Splitview/Gridview suites: exact tiling, conservation, min enforcement,
// sash cascade behavior, and drift-free snapshot drags.

import { describe, it, expect } from "vitest";
import {
  DockModel,
  DockBranch,
  type SerializedNode,
} from "./model.svelte.js";
import {
  solve,
  distribute,
  minSize,
  resizeSash,
  equalizeAtSash,
  DEFAULT_SOLVE_OPTIONS,
  type SolveOptions,
  type Rect,
} from "./solve.js";

const OPTS: SolveOptions = { minWidth: 100, minHeight: 60, sashSize: 8 };

function leafSpec(id: string, panels: string[] = [id]): SerializedNode {
  return { type: "leaf", id, panels };
}

function rectOf(model: DockModel, w: number, h: number, id: string): Rect {
  const solution = solve(model.root, w, h, model.maximizedLeafId, OPTS);
  const hit = solution.leaves.find((l) => l.leaf.id === id);
  if (!hit) throw new Error(`no leaf ${id}`);
  return hit.rect;
}

// ─── distribute ─────────────────────────────────────────────────────────────

describe("distribute", () => {
  it("splits by fraction into exact integers", () => {
    expect(distribute([0.5, 0.5], [0, 0], 100)).toEqual([50, 50]);
    expect(distribute([0.25, 0.75], [0, 0], 200)).toEqual([50, 150]);
  });

  it("rounds odd totals without gaps (sizes sum exactly)", () => {
    const sizes = distribute([1 / 3, 1 / 3, 1 / 3], [0, 0, 0], 100);
    expect(sizes.reduce((s, v) => s + v, 0)).toBe(100);
    for (const s of sizes) expect(Math.abs(s - 100 / 3)).toBeLessThan(1);
  });

  it("pins children at their minimum and redistributes", () => {
    // 10% fraction of 200px = 20px, below min 50 → pinned, rest split 150.
    const sizes = distribute([0.1, 0.45, 0.45], [50, 50, 50], 200);
    expect(sizes[0]).toBe(50);
    expect(sizes[1]).toBe(75);
    expect(sizes[2]).toBe(75);
  });

  it("cascading pins: two children below minimum", () => {
    const sizes = distribute([0.05, 0.05, 0.9], [40, 40, 0], 200);
    expect(sizes[0]).toBe(40);
    expect(sizes[1]).toBe(40);
    expect(sizes[2]).toBe(120);
  });

  it("scales minimums down when the container cannot fit them", () => {
    const sizes = distribute([0.5, 0.5], [80, 80], 80);
    expect(sizes.reduce((s, v) => s + v, 0)).toBe(80);
    expect(sizes[0]).toBe(40);
    expect(sizes[1]).toBe(40);
  });

  it("handles zero-length input and zero fractions", () => {
    expect(distribute([], [], 100)).toEqual([]);
    const sizes = distribute([0, 0], [0, 0], 100);
    expect(sizes.reduce((s, v) => s + v, 0)).toBe(100);
  });
});

// ─── minSize ────────────────────────────────────────────────────────────────

describe("minSize", () => {
  it("leaf minimum comes from options", () => {
    const m = new DockModel(leafSpec("a"));
    expect(minSize(m.root!, OPTS)).toEqual({ width: 100, height: 60 });
  });

  it("row sums widths and envelopes heights; column mirrors", () => {
    const row = new DockModel({
      type: "branch",
      orientation: "row",
      children: [leafSpec("a"), leafSpec("b")],
    });
    expect(minSize(row.root!, OPTS)).toEqual({ width: 200, height: 60 });

    const column = new DockModel({
      type: "branch",
      orientation: "column",
      children: [leafSpec("a"), leafSpec("b")],
    });
    expect(minSize(column.root!, OPTS)).toEqual({ width: 100, height: 120 });
  });

  it("per-panel minimums raise a leaf's minimum", () => {
    const withMins: SolveOptions = {
      ...OPTS,
      panelMins: new Map([
        ["viewport", { width: 240, height: 160 }],
        ["log", { height: 90 }],
      ]),
    };
    const leaf = new DockModel(leafSpec("a", ["viewport", "log"])).root!;
    expect(minSize(leaf, withMins)).toEqual({ width: 240, height: 160 });

    // Panels without registry mins fall back to the globals.
    const plain = new DockModel(leafSpec("b", ["other"])).root!;
    expect(minSize(plain, withMins)).toEqual({ width: 100, height: 60 });

    // Branch composition sees the raised minimum.
    const row = new DockModel({
      type: "branch",
      orientation: "row",
      children: [leafSpec("a", ["viewport"]), leafSpec("b", ["other"])],
    });
    expect(minSize(row.root!, withMins)).toEqual({ width: 340, height: 160 });
  });

  it("nested minimums compose live from the tree", () => {
    const m = new DockModel({
      type: "branch",
      orientation: "row",
      children: [
        leafSpec("a"),
        { type: "branch", orientation: "column", children: [leafSpec("b"), leafSpec("c")] },
      ],
    });
    expect(minSize(m.root!, OPTS)).toEqual({ width: 200, height: 120 });
    // Splitting grows the subtree minimum without any adapter refresh.
    m.splitLeaf("b", "right", ["p"]);
    expect(minSize(m.root!, OPTS)).toEqual({ width: 300, height: 120 });
  });
});

// ─── solve ──────────────────────────────────────────────────────────────────

describe("solve", () => {
  it("returns empty for a null root or zero-sized container", () => {
    expect(solve(null, 800, 600, null, OPTS).leaves).toEqual([]);
    const m = new DockModel(leafSpec("a"));
    expect(solve(m.root, 0, 600, null, OPTS).leaves).toEqual([]);
  });

  it("a single leaf fills the container", () => {
    const m = new DockModel(leafSpec("a"));
    expect(rectOf(m, 800, 600, "a")).toEqual({ left: 0, top: 0, width: 800, height: 600 });
  });

  it("a 50/50 row splits exactly with one sash on the boundary", () => {
    const m = new DockModel({
      type: "branch",
      orientation: "row",
      children: [leafSpec("a"), leafSpec("b")],
    });
    const solution = solve(m.root, 800, 600, null, OPTS);
    expect(rectOf(m, 800, 600, "a")).toEqual({ left: 0, top: 0, width: 400, height: 600 });
    expect(rectOf(m, 800, 600, "b")).toEqual({ left: 400, top: 0, width: 400, height: 600 });
    expect(solution.sashes).toHaveLength(1);
    expect(solution.sashes[0].rect).toEqual({ left: 396, top: 0, width: 8, height: 600 });
    expect(solution.sashes[0].orientation).toBe("row");
    expect(solution.sashes[0].index).toBe(0);
  });

  it("nested layouts tile the container exactly", () => {
    const m = new DockModel({
      type: "branch",
      orientation: "row",
      children: [
        leafSpec("side"),
        {
          type: "branch",
          orientation: "column",
          fraction: 3,
          children: [leafSpec("main", ["editor"]), leafSpec("term")],
        },
      ],
    });
    const solution = solve(m.root, 801, 601, null, OPTS);
    const area = solution.leaves.reduce((s, l) => s + l.rect.width * l.rect.height, 0);
    expect(area).toBe(801 * 601);
    // Integer rects only.
    for (const l of solution.leaves) {
      for (const v of Object.values(l.rect)) expect(Number.isInteger(v)).toBe(true);
    }
    // The nested column's leaves share the column's left edge and width.
    const main = solution.leaves.find((l) => l.leaf.id === "main")!;
    const term = solution.leaves.find((l) => l.leaf.id === "term")!;
    expect(term.rect.left).toBe(main.rect.left);
    expect(term.rect.width).toBe(main.rect.width);
    expect(term.rect.top).toBe(main.rect.top + main.rect.height);
  });

  it("random trees tile exactly after arbitrary operations", () => {
    let seed = 0xabad1dea;
    const rand = () => {
      seed = (seed * 1664525 + 1013904223) >>> 0;
      return seed / 0x100000000;
    };
    const directions = ["left", "right", "up", "down"] as const;
    const m = new DockModel(leafSpec("seed"));
    for (let i = 0; i < 24; i++) {
      const leaves = m.leaves();
      m.splitLeaf(
        leaves[Math.floor(rand() * leaves.length)].id,
        directions[Math.floor(rand() * 4)],
        [`p${i}`],
      );
    }
    const solution = solve(m.root, 1440, 900, null, { ...OPTS, minWidth: 10, minHeight: 10 });
    const area = solution.leaves.reduce((s, l) => s + l.rect.width * l.rect.height, 0);
    expect(area).toBe(1440 * 900);
  });

  it("enforces minimums when a fraction is squeezed", () => {
    const m = new DockModel({
      type: "branch",
      orientation: "row",
      children: [
        { ...leafSpec("tiny"), fraction: 0.01 } as SerializedNode,
        { ...leafSpec("big"), fraction: 0.99 } as SerializedNode,
      ],
    });
    const rect = rectOf(m, 800, 600, "tiny");
    expect(rect.width).toBe(100); // clamped to minWidth
  });

  it("maximize gives one leaf the full container and hides the rest", () => {
    const m = new DockModel({
      type: "branch",
      orientation: "row",
      children: [leafSpec("a"), leafSpec("b")],
    });
    m.maximizeLeaf("b");
    const solution = solve(m.root, 800, 600, m.maximizedLeafId, OPTS);
    const b = solution.leaves.find((l) => l.leaf.id === "b")!;
    const a = solution.leaves.find((l) => l.leaf.id === "a")!;
    expect(b.rect).toEqual({ left: 0, top: 0, width: 800, height: 600 });
    expect(b.hidden).toBe(false);
    expect(a.hidden).toBe(true);
    expect(solution.sashes).toEqual([]);
  });
});

// ─── resizeSash ─────────────────────────────────────────────────────────────

describe("resizeSash", () => {
  function rowModel(fractions: number[]): DockModel {
    const m = new DockModel({
      type: "branch",
      orientation: "row",
      children: fractions.map((f, i) => ({ ...leafSpec(`v${i}`), fraction: f }) as SerializedNode),
    });
    return m;
  }

  /** Pixel sizes of the root row's children at a given width. */
  function px(m: DockModel, width: number): number[] {
    const root = m.root as DockBranch;
    return root.children.map((c) => c.fraction * width);
  }

  it("basic drag grows the adjacent up child and shrinks the adjacent down child", () => {
    const m = rowModel([0.5, 0.5]);
    const root = m.root as DockBranch;
    resizeSash(root, 0, 100, [400, 400], [100, 100]);
    expect(px(m, 800)).toEqual([500, 300]);
  });

  it("conserves total size across any drag", () => {
    const m = rowModel([0.25, 0.5, 0.25]);
    const root = m.root as DockBranch;
    resizeSash(root, 1, 137, [200, 400, 200], [50, 50, 50]);
    const total = root.children.reduce((s, c) => s + c.fraction, 0);
    expect(total).toBeCloseTo(1, 9);
  });

  it("clamps at the down side's minimum", () => {
    const m = rowModel([0.5, 0.5]);
    const root = m.root as DockBranch;
    resizeSash(root, 0, 1000, [400, 400], [100, 100]);
    expect(px(m, 800)).toEqual([700, 100]);
  });

  it("cascades shrink past the adjacent child into its neighbour", () => {
    const m = rowModel([0.25, 0.25, 0.5]);
    const root = m.root as DockBranch;
    // Sash 1 dragged 250px left: child1 shrinks 200→50 (min), the remaining
    // 100px of shrink cascades into child0 (200→100); child2 takes it all.
    resizeSash(root, 1, -250, [200, 200, 400], [100, 50, 100]);
    expect(px(m, 800)).toEqual([100, 50, 650]);
  });

  it("snapshot-relative drags do not drift", () => {
    const m = rowModel([0.5, 0.5]);
    const root = m.root as DockBranch;
    const snapshot = [400, 400];
    const mins = [100, 100];
    resizeSash(root, 0, 200, snapshot, mins);
    resizeSash(root, 0, -200, snapshot, mins);
    resizeSash(root, 0, 0, snapshot, mins);
    expect(px(m, 800)).toEqual([400, 400]);
  });

  it("drag to extreme then back restores original sizes", () => {
    const m = rowModel([0.25, 0.25, 0.5]);
    const root = m.root as DockBranch;
    const snapshot = [200, 200, 400];
    const mins = [50, 50, 50];
    resizeSash(root, 0, 5000, snapshot, mins);
    resizeSash(root, 0, -5000, snapshot, mins);
    resizeSash(root, 0, 0, snapshot, mins);
    expect(px(m, 800).map((v) => Math.round(v))).toEqual([200, 200, 400]);
  });

  it("ignores invalid sash indices and mismatched snapshots", () => {
    const m = rowModel([0.5, 0.5]);
    const root = m.root as DockBranch;
    resizeSash(root, 5, 100, [400, 400], [0, 0]);
    resizeSash(root, -1, 100, [400, 400], [0, 0]);
    resizeSash(root, 0, 100, [400], [0]);
    expect(px(m, 800)).toEqual([400, 400]);
  });

  it("equalizeAtSash evens out only the two adjacent children", () => {
    const m = rowModel([0.2, 0.6, 0.2]);
    const root = m.root as DockBranch;
    equalizeAtSash(root, 0);
    expect(root.children.map((c) => c.fraction)).toEqual([0.4, 0.4, 0.2]);
    equalizeAtSash(root, 5); // out of range → no-op
    expect(root.children.map((c) => c.fraction)).toEqual([0.4, 0.4, 0.2]);
  });
});

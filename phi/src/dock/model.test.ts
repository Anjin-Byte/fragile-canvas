// ─── DockModel tests ────────────────────────────────────────────────────────
// Ports the structural scenarios from the retired Gridview/Splitview suites
// (collapse rules, all-4-direction splits, serialization roundtrips, stress)
// against the reactive model, plus the invariants the old design could
// violate (normalization after every op, single source of truth for panels).

import { describe, it, expect } from "vitest";
import {
  DockModel,
  DockBranch,
  DockLeaf,
  type DockNode,
  type SerializedNode,
} from "./model.svelte.js";

// ─── Helpers ────────────────────────────────────────────────────────────────

/** Assert every documented tree invariant. */
function assertInvariants(model: DockModel): void {
  const seenIds = new Set<string>();
  const walk = (node: DockNode, parent: DockBranch | null): void => {
    if (node.kind === "leaf") {
      expect(seenIds.has(node.id)).toBe(false);
      seenIds.add(node.id);
      expect(node.panels.length).toBeGreaterThan(0);
      expect(node.panels).toContain(node.activePanel);
    } else {
      expect(node.children.length).toBeGreaterThanOrEqual(2);
      if (parent) expect(node.orientation).not.toBe(parent.orientation);
      const sum = node.children.reduce((s, c) => s + c.fraction, 0);
      expect(sum).toBeCloseTo(1, 6);
      for (const child of node.children) walk(child, node);
    }
  };
  if (model.root) walk(model.root, null);
  if (model.maximizedLeafId !== null) {
    expect(model.findLeaf(model.maximizedLeafId)).not.toBeNull();
  }
}

function leafSpec(id: string, panels: string[] = [id]): SerializedNode {
  return { type: "leaf", id, panels };
}

/** row[a, b] starting layout. */
function twoLeafModel(): DockModel {
  return new DockModel({
    type: "branch",
    orientation: "row",
    children: [leafSpec("a"), leafSpec("b")],
  });
}

// ─── Construction ───────────────────────────────────────────────────────────

describe("DockModel — construction", () => {
  it("starts empty without a spec", () => {
    const m = new DockModel();
    expect(m.isEmpty).toBe(true);
    expect(m.leaves()).toEqual([]);
  });

  it("builds a single leaf as root", () => {
    const m = new DockModel(leafSpec("a", ["p1", "p2"]));
    expect(m.root).toBeInstanceOf(DockLeaf);
    expect(m.findLeaf("a")?.panels).toEqual(["p1", "p2"]);
    expect(m.findLeaf("a")?.activePanel).toBe("p1");
    assertInvariants(m);
  });

  it("respects an explicit active panel", () => {
    const m = new DockModel({ type: "leaf", id: "a", panels: ["p1", "p2"], active: "p2" });
    expect(m.findLeaf("a")?.activePanel).toBe("p2");
  });

  it("repairs an active panel that is not in panels", () => {
    const m = new DockModel({ type: "leaf", id: "a", panels: ["p1"], active: "nope" });
    expect(m.findLeaf("a")?.activePanel).toBe("p1");
  });

  it("auto-generates missing leaf ids without collisions", () => {
    const m = new DockModel({
      type: "branch",
      orientation: "row",
      children: [
        { type: "leaf", panels: ["x"] },
        { type: "leaf", panels: ["y"] },
      ],
    });
    const ids = m.leaves().map((l) => l.id);
    expect(new Set(ids).size).toBe(2);
    assertInvariants(m);
  });

  it("throws on duplicate explicit leaf ids", () => {
    expect(
      () =>
        new DockModel({
          type: "branch",
          orientation: "row",
          children: [leafSpec("dup"), leafSpec("dup")],
        }),
    ).toThrow(/duplicate/);
  });

  it("normalizes equal fractions from a terse spec", () => {
    const m = twoLeafModel();
    const root = m.root as DockBranch;
    expect(root.children.map((c) => c.fraction)).toEqual([0.5, 0.5]);
  });

  it("flattens sloppy same-orientation nesting in a spec", () => {
    const m = new DockModel({
      type: "branch",
      orientation: "row",
      children: [
        leafSpec("a"),
        {
          type: "branch",
          orientation: "row",
          children: [leafSpec("b"), leafSpec("c")],
        },
      ],
    });
    const root = m.root as DockBranch;
    expect(root.children).toHaveLength(3);
    expect(root.children.every((c) => c.kind === "leaf")).toBe(true);
    assertInvariants(m);
  });

  it("drops empty leaves and collapses single-child branches from a spec", () => {
    const m = new DockModel({
      type: "branch",
      orientation: "row",
      children: [
        leafSpec("a"),
        {
          type: "branch",
          orientation: "column",
          children: [{ type: "leaf", id: "empty", panels: [] }, leafSpec("b")],
        },
      ],
    });
    // The empty leaf vanishes, its single-child column collapses to "b".
    expect(m.findLeaf("empty")).toBeNull();
    const root = m.root as DockBranch;
    expect(root.children).toHaveLength(2);
    assertInvariants(m);
  });
});

// ─── splitLeaf ──────────────────────────────────────────────────────────────

describe("DockModel — splitLeaf", () => {
  it("splits a root leaf to the right into a row", () => {
    const m = new DockModel(leafSpec("a"));
    const created = m.splitLeaf("a", "right", ["p"]);
    expect(created).not.toBeNull();
    const root = m.root as DockBranch;
    expect(root.orientation).toBe("row");
    expect(root.children.map((c) => (c as DockLeaf).id)).toEqual(["a", created!.id]);
    expect(root.children.map((c) => c.fraction)).toEqual([0.5, 0.5]);
    assertInvariants(m);
  });

  it("splits to the left placing the new leaf first", () => {
    const m = new DockModel(leafSpec("a"));
    const created = m.splitLeaf("a", "left", ["p"]);
    const root = m.root as DockBranch;
    expect(root.children.map((c) => (c as DockLeaf).id)).toEqual([created!.id, "a"]);
    assertInvariants(m);
  });

  it("splits up/down into a column", () => {
    const m = new DockModel(leafSpec("a"));
    const below = m.splitLeaf("a", "down", ["p"]);
    const root = m.root as DockBranch;
    expect(root.orientation).toBe("column");
    expect(root.children.map((c) => (c as DockLeaf).id)).toEqual(["a", below!.id]);

    const above = m.splitLeaf("a", "up", ["q"]);
    expect(root.children.map((c) => (c as DockLeaf).id)).toEqual([above!.id, "a", below!.id]);
    assertInvariants(m);
  });

  it("same-axis split becomes a sibling dividing the target's share", () => {
    const m = twoLeafModel(); // a: 0.5, b: 0.5
    const created = m.splitLeaf("a", "right", ["p"]);
    const root = m.root as DockBranch;
    expect(root.children).toHaveLength(3);
    expect(root.children.map((c) => (c as DockLeaf).id)).toEqual(["a", created!.id, "b"]);
    expect(root.children.map((c) => c.fraction)).toEqual([0.25, 0.25, 0.5]);
    assertInvariants(m);
  });

  it("orthogonal split nests a branch in place", () => {
    const m = twoLeafModel();
    const created = m.splitLeaf("a", "down", ["p"]);
    const root = m.root as DockBranch;
    expect(root.children).toHaveLength(2);
    const nested = root.children[0] as DockBranch;
    expect(nested.kind).toBe("branch");
    expect(nested.orientation).toBe("column");
    expect(nested.children.map((c) => (c as DockLeaf).id)).toEqual(["a", created!.id]);
    expect(nested.fraction).toBeCloseTo(0.5);
    assertInvariants(m);
  });

  it("can add views in all 4 directions around a center leaf", () => {
    const m = new DockModel(leafSpec("center"));
    m.splitLeaf("center", "left", ["l"]);
    m.splitLeaf("center", "right", ["r"]);
    m.splitLeaf("center", "up", ["u"]);
    m.splitLeaf("center", "down", ["d"]);
    expect(m.leaves()).toHaveLength(5);
    assertInvariants(m);
  });

  it("returns null for unknown targets or empty panel lists", () => {
    const m = new DockModel(leafSpec("a"));
    expect(m.splitLeaf("nope", "right", ["p"])).toBeNull();
    expect(m.splitLeaf("a", "right", [])).toBeNull();
    expect((m.root as DockLeaf).kind).toBe("leaf");
  });
});

// ─── splitWithPanel ─────────────────────────────────────────────────────────

describe("DockModel — splitWithPanel", () => {
  it("moves the panel into a new leaf beside the target", () => {
    const m = new DockModel({
      type: "branch",
      orientation: "row",
      children: [leafSpec("a", ["p1", "p2"]), leafSpec("b", ["q"])],
    });
    const created = m.splitWithPanel("p2", "a", "b", "down");
    expect(created?.panels).toEqual(["p2"]);
    expect(m.findLeaf("a")?.panels).toEqual(["p1"]);
    assertInvariants(m);
  });

  it("removes the source leaf when it empties", () => {
    const m = twoLeafModel();
    m.splitWithPanel("a", "a", "b", "down");
    expect(m.findLeaf("a")).toBeNull();
    const root = m.root as DockBranch;
    expect(root.orientation).toBe("column");
    expect(m.leaves()).toHaveLength(2);
    assertInvariants(m);
  });

  it("is a no-op when dragging a sole panel onto its own edge", () => {
    const m = new DockModel(leafSpec("a"));
    expect(m.splitWithPanel("a", "a", "a", "right")).toBeNull();
    expect((m.root as DockLeaf).id).toBe("a");
  });

  it("splits a leaf's own group when it holds multiple panels", () => {
    const m = new DockModel(leafSpec("a", ["p1", "p2"]));
    const created = m.splitWithPanel("p2", "a", "a", "right");
    expect(created?.panels).toEqual(["p2"]);
    expect(m.findLeaf("a")?.panels).toEqual(["p1"]);
    expect((m.root as DockBranch).orientation).toBe("row");
    assertInvariants(m);
  });
});

// ─── removeLeaf ─────────────────────────────────────────────────────────────

describe("DockModel — removeLeaf", () => {
  it("removing the root leaf empties the model", () => {
    const m = new DockModel(leafSpec("a"));
    m.removeLeaf("a");
    expect(m.isEmpty).toBe(true);
  });

  it("collapses a two-child branch to the surviving child", () => {
    const m = twoLeafModel();
    m.removeLeaf("a");
    expect((m.root as DockLeaf).id).toBe("b");
    expect(m.root!.kind).toBe("leaf");
  });

  it("renormalizes sibling fractions after removal", () => {
    const m = new DockModel({
      type: "branch",
      orientation: "row",
      children: [leafSpec("a"), leafSpec("b"), leafSpec("c")],
    });
    m.removeLeaf("b");
    const root = m.root as DockBranch;
    expect(root.children.map((c) => c.fraction)).toEqual([0.5, 0.5]);
    assertInvariants(m);
  });

  it("flattens same-orientation nesting created by a collapse", () => {
    // row[a, column[row[b, c], d]] — removing d collapses the column to
    // row[b, c], which must merge into the root row: row[a, b, c].
    const m = new DockModel({
      type: "branch",
      orientation: "row",
      children: [
        leafSpec("a"),
        {
          type: "branch",
          orientation: "column",
          children: [
            {
              type: "branch",
              orientation: "row",
              children: [leafSpec("b"), leafSpec("c")],
            },
            leafSpec("d"),
          ],
        },
      ],
    });
    m.removeLeaf("d");
    const root = m.root as DockBranch;
    expect(root.orientation).toBe("row");
    expect(root.children.map((c) => (c as DockLeaf).id)).toEqual(["a", "b", "c"]);
    assertInvariants(m);
  });

  it("removes deeply then keeps collapsing to a single leaf", () => {
    const m = new DockModel(leafSpec("a"));
    m.splitLeaf("a", "right", ["b"]);
    m.splitLeaf("a", "down", ["c"]);
    const ids = m.leaves().map((l) => l.id);
    for (const id of ids.slice(0, -1)) m.removeLeaf(id);
    expect(m.root!.kind).toBe("leaf");
    assertInvariants(m);
  });
});

// ─── Panel operations ───────────────────────────────────────────────────────

describe("DockModel — panel operations", () => {
  it("activatePanel switches the active tab; invalid ids are ignored", () => {
    const m = new DockModel(leafSpec("a", ["p1", "p2"]));
    m.activatePanel("a", "p2");
    expect(m.findLeaf("a")!.activePanel).toBe("p2");
    m.activatePanel("a", "nope");
    expect(m.findLeaf("a")!.activePanel).toBe("p2");
  });

  it("reorderPanel moves a tab to its final position", () => {
    const m = new DockModel(leafSpec("a", ["p1", "p2", "p3"]));
    m.reorderPanel("a", 0, 2);
    expect(m.findLeaf("a")!.panels).toEqual(["p2", "p3", "p1"]);
    m.reorderPanel("a", 2, 0);
    expect(m.findLeaf("a")!.panels).toEqual(["p1", "p2", "p3"]);
  });

  it("reorderPanel clamps out-of-range targets and ignores bad sources", () => {
    const m = new DockModel(leafSpec("a", ["p1", "p2"]));
    m.reorderPanel("a", 0, 99);
    expect(m.findLeaf("a")!.panels).toEqual(["p2", "p1"]);
    m.reorderPanel("a", 99, 0);
    expect(m.findLeaf("a")!.panels).toEqual(["p2", "p1"]);
  });

  it("movePanel tabifies into the target at an index and activates", () => {
    const m = new DockModel({
      type: "branch",
      orientation: "row",
      children: [leafSpec("a", ["p1", "p2"]), leafSpec("b", ["q1", "q2"])],
    });
    m.movePanel("p2", "a", "b", 1);
    expect(m.findLeaf("a")!.panels).toEqual(["p1"]);
    expect(m.findLeaf("b")!.panels).toEqual(["q1", "p2", "q2"]);
    expect(m.findLeaf("b")!.activePanel).toBe("p2");
    assertInvariants(m);
  });

  it("movePanel removes the source leaf when it empties", () => {
    const m = twoLeafModel();
    m.movePanel("a", "a", "b");
    expect(m.findLeaf("a")).toBeNull();
    expect((m.root as DockLeaf).id).toBe("b");
    expect(m.findLeaf("b")!.panels).toEqual(["b", "a"]);
    assertInvariants(m);
  });

  it("movePanel within the same leaf reorders and activates", () => {
    const m = new DockModel(leafSpec("a", ["p1", "p2", "p3"]));
    m.movePanel("p1", "a", "a", 2);
    expect(m.findLeaf("a")!.panels).toEqual(["p2", "p3", "p1"]);
    expect(m.findLeaf("a")!.activePanel).toBe("p1");
  });

  it("closePanel switches active to the neighbouring tab", () => {
    const m = new DockModel({ type: "leaf", id: "a", panels: ["p1", "p2", "p3"], active: "p2" });
    m.closePanel("a", "p2");
    expect(m.findLeaf("a")!.panels).toEqual(["p1", "p3"]);
    expect(m.findLeaf("a")!.activePanel).toBe("p3");
  });

  it("closing the last panel removes the leaf and collapses", () => {
    const m = twoLeafModel();
    m.closePanel("a", "a");
    expect(m.findLeaf("a")).toBeNull();
    expect((m.root as DockLeaf).id).toBe("b");
    assertInvariants(m);
  });

  it("findPanel locates the leaf holding a panel", () => {
    const m = twoLeafModel();
    expect(m.findPanel("b")?.id).toBe("b");
    expect(m.findPanel("nope")).toBeNull();
  });
});

// ─── Maximize ───────────────────────────────────────────────────────────────

describe("DockModel — maximize", () => {
  it("maximize, restore, and toggle", () => {
    const m = twoLeafModel();
    m.maximizeLeaf("a");
    expect(m.maximizedLeafId).toBe("a");
    m.restore();
    expect(m.maximizedLeafId).toBeNull();
    m.toggleMaximize("b");
    expect(m.maximizedLeafId).toBe("b");
    m.toggleMaximize("b");
    expect(m.maximizedLeafId).toBeNull();
  });

  it("ignores unknown leaf ids", () => {
    const m = twoLeafModel();
    m.maximizeLeaf("nope");
    expect(m.maximizedLeafId).toBeNull();
  });

  it("clears when the maximized leaf is removed", () => {
    const m = twoLeafModel();
    m.maximizeLeaf("a");
    m.removeLeaf("a");
    expect(m.maximizedLeafId).toBeNull();
  });

  it("clears when the maximized leaf's last panel closes", () => {
    const m = twoLeafModel();
    m.maximizeLeaf("a");
    m.closePanel("a", "a");
    expect(m.maximizedLeafId).toBeNull();
  });
});

// ─── Serialization ──────────────────────────────────────────────────────────

describe("DockModel — serialization", () => {
  function deepModel(): DockModel {
    const m = new DockModel({
      type: "branch",
      orientation: "row",
      children: [
        { type: "leaf", id: "side", panels: ["tree", "search"], active: "search" },
        {
          type: "branch",
          orientation: "column",
          fraction: 3,
          children: [leafSpec("main", ["editor"]), leafSpec("term", ["shell", "log"])],
        },
      ],
    });
    return m;
  }

  it("roundtrip preserves structure, ids, fractions, and active panels", () => {
    const m = deepModel();
    const restored = DockModel.deserialize(m.serialize());
    expect(restored.serialize()).toEqual(m.serialize());
    expect(restored.findLeaf("side")?.activePanel).toBe("search");
    const root = restored.root as DockBranch;
    expect(root.children[1].fraction).toBeCloseTo(0.75);
    assertInvariants(restored);
  });

  it("roundtrips the maximized state", () => {
    const m = deepModel();
    m.maximizeLeaf("main");
    const restored = DockModel.deserialize(m.serialize());
    expect(restored.maximizedLeafId).toBe("main");
  });

  it("drops a serialized maximized id that no longer resolves", () => {
    const m = deepModel();
    const data = m.serialize();
    data.maximized = "ghost";
    expect(DockModel.deserialize(data).maximizedLeafId).toBeNull();
  });

  it("roundtrip after structural operations", () => {
    const m = deepModel();
    m.splitWithPanel("log", "term", "side", "down");
    m.closePanel("main", "editor");
    const restored = DockModel.deserialize(m.serialize());
    expect(restored.serialize()).toEqual(m.serialize());
    assertInvariants(restored);
  });

  it("rejects malformed input", () => {
    expect(() => DockModel.deserialize(null)).toThrow();
    expect(() => DockModel.deserialize({})).toThrow();
    expect(() => DockModel.deserialize({ v: 2, root: null })).toThrow();
    expect(() => DockModel.deserialize("garbage")).toThrow();
  });

  it("deserializes an empty dock", () => {
    const restored = DockModel.deserialize({ v: 1, root: null });
    expect(restored.isEmpty).toBe(true);
  });
});

// ─── Stress ─────────────────────────────────────────────────────────────────

describe("DockModel — stress", () => {
  it("random splits, moves, and removals keep every invariant", () => {
    // Deterministic LCG so failures reproduce.
    let seed = 0xdecafbad;
    const rand = () => {
      seed = (seed * 1664525 + 1013904223) >>> 0;
      return seed / 0x100000000;
    };
    const pick = <T,>(arr: T[]): T => arr[Math.floor(rand() * arr.length)];
    const directions = ["left", "right", "up", "down"] as const;

    const m = new DockModel(leafSpec("seed", ["seed-panel"]));
    let panelCounter = 0;

    for (let step = 0; step < 200; step++) {
      const leaves = m.leaves();
      if (leaves.length === 0) break;
      const op = rand();
      if (op < 0.45) {
        m.splitLeaf(pick(leaves).id, pick([...directions]), [`p${panelCounter++}`]);
      } else if (op < 0.7 && leaves.length > 1) {
        const from = pick(leaves);
        m.movePanel(pick(from.panels), from.id, pick(leaves).id);
      } else if (op < 0.85) {
        const leaf = pick(leaves);
        m.closePanel(leaf.id, pick(leaf.panels));
      } else {
        m.removeLeaf(pick(leaves).id);
      }
      assertInvariants(m);
    }

    // The tree survived; a final roundtrip also holds.
    if (!m.isEmpty) {
      assertInvariants(DockModel.deserialize(m.serialize()));
    }
  });
});

// ─── DockModel feature tests ────────────────────────────────────────────────
// openPanel/closePanelById, active-leaf tracking, floating groups, and
// container-edge (root) splits.

import { describe, it, expect } from "vitest";
import {
  DockModel,
  DockBranch,
  DockLeaf,
  type SerializedNode,
} from "./model.svelte.js";

function leafSpec(id: string, panels: string[] = [id]): SerializedNode {
  return { type: "leaf", id, panels };
}

function twoLeafModel(): DockModel {
  return new DockModel({
    type: "branch",
    orientation: "row",
    children: [leafSpec("a", ["p1", "p2"]), leafSpec("b", ["q1"])],
  });
}

// ─── openPanel ──────────────────────────────────────────────────────────────

describe("DockModel — openPanel", () => {
  it("activates an already-open panel wherever it lives", () => {
    const m = twoLeafModel();
    const leaf = m.openPanel("p2");
    expect(leaf?.id).toBe("a");
    expect(m.findLeaf("a")!.activePanel).toBe("p2");
    expect(m.activeLeafId).toBe("a");
  });

  it("opens near another panel (tabify)", () => {
    const m = twoLeafModel();
    const leaf = m.openPanel("new", { near: "q1" });
    expect(leaf?.id).toBe("b");
    expect(m.findLeaf("b")!.panels).toEqual(["q1", "new"]);
    expect(m.findLeaf("b")!.activePanel).toBe("new");
  });

  it("opens near a leaf id with a split direction", () => {
    const m = twoLeafModel();
    const leaf = m.openPanel("new", { near: "b", direction: "down" });
    expect(leaf).not.toBeNull();
    expect(leaf!.panels).toEqual(["new"]);
    expect(m.leaves().map((l) => l.panels)).toEqual([["p1", "p2"], ["q1"], ["new"]]);
    expect(m.activeLeafId).toBe(leaf!.id);
  });

  it("defaults to the active leaf", () => {
    const m = twoLeafModel();
    m.setActiveLeaf("b");
    m.openPanel("new");
    expect(m.findLeaf("b")!.panels).toEqual(["q1", "new"]);
  });

  it("falls back to the first docked leaf when nothing is active", () => {
    const m = twoLeafModel();
    expect(m.activeLeafId).toBeNull();
    m.openPanel("new");
    expect(m.findLeaf("a")!.panels).toEqual(["p1", "p2", "new"]);
  });

  it("creates a root leaf in an empty model", () => {
    const m = new DockModel();
    const leaf = m.openPanel("solo");
    expect(leaf).not.toBeNull();
    expect(m.root).toBe(leaf);
    expect(leaf!.panels).toEqual(["solo"]);
    expect(m.activeLeafId).toBe(leaf!.id);
  });

  it("closePanelById closes wherever the panel lives", () => {
    const m = twoLeafModel();
    m.closePanelById("q1");
    expect(m.findLeaf("b")).toBeNull();
    m.closePanelById("nope"); // no-op
    expect(m.leaves()).toHaveLength(1);
  });
});

// ─── Active leaf ────────────────────────────────────────────────────────────

describe("DockModel — active leaf", () => {
  it("tracks activation, moves, and splits", () => {
    const m = twoLeafModel();
    m.activatePanel("b", "q1");
    expect(m.activeLeafId).toBe("b");

    m.movePanel("p2", "a", "b");
    expect(m.activeLeafId).toBe("b");

    const created = m.splitWithPanel("p2", "b", "a", "down");
    expect(m.activeLeafId).toBe(created!.id);
  });

  it("reassigns when the active leaf disappears", () => {
    const m = twoLeafModel();
    m.setActiveLeaf("b");
    m.removeLeaf("b");
    expect(m.activeLeafId).toBe("a");
  });

  it("nulls out when the model empties", () => {
    const m = new DockModel(leafSpec("a"));
    m.setActiveLeaf("a");
    m.removeLeaf("a");
    expect(m.activeLeafId).toBeNull();
  });

  it("setActiveLeaf ignores unknown ids", () => {
    const m = twoLeafModel();
    m.setActiveLeaf("ghost");
    expect(m.activeLeafId).toBeNull();
  });
});

// ─── Floating ───────────────────────────────────────────────────────────────

describe("DockModel — floating", () => {
  it("floatLeaf detaches a docked group into a window", () => {
    const m = twoLeafModel();
    const group = m.floatLeaf("a");
    expect(group).not.toBeNull();
    expect(group!.leaf.panels).toEqual(["p1", "p2"]);
    expect(m.leaves().map((l) => l.id)).toEqual(["b"]);
    expect((m.root as DockLeaf).id).toBe("b"); // tree collapsed
    expect(m.findLeaf("a")).not.toBeNull(); // still findable (floating)
    expect(m.floatingOf("a")).toBe(group);
    expect(m.activeLeafId).toBe("a");
  });

  it("floating the only leaf empties the tree but not the model", () => {
    const m = new DockModel(leafSpec("a"));
    m.floatLeaf("a");
    expect(m.root).toBeNull();
    expect(m.isEmpty).toBe(false);
  });

  it("floatPanel detaches a single panel; source keeps the rest", () => {
    const m = twoLeafModel();
    const group = m.floatPanel("p2", "a", { left: 10, top: 20, width: 200, height: 150 });
    expect(group!.leaf.panels).toEqual(["p2"]);
    expect(group!.rect).toEqual({ left: 10, top: 20, width: 200, height: 150 });
    expect(m.findLeaf("a")!.panels).toEqual(["p1"]);
  });

  it("floatPanel on a sole floating panel just moves the window", () => {
    const m = twoLeafModel();
    const group = m.floatPanel("p2", "a")!;
    const before = m.floating.length;
    const moved = m.floatPanel("p2", group.leaf.id, { left: 5, top: 6, width: 300, height: 200 });
    expect(moved).toBe(group);
    expect(m.floating).toHaveLength(before);
    expect(group.rect.left).toBe(5);
  });

  it("panel operations work on floating leaves unchanged", () => {
    const m = twoLeafModel();
    m.floatLeaf("a");
    m.activatePanel("a", "p2");
    expect(m.findLeaf("a")!.activePanel).toBe("p2");
    m.reorderPanel("a", 0, 1);
    expect(m.findLeaf("a")!.panels).toEqual(["p2", "p1"]);
    m.movePanel("p1", "a", "b");
    expect(m.findLeaf("b")!.panels).toEqual(["q1", "p1"]);
    m.closePanel("a", "p2"); // last panel → window closes
    expect(m.floating).toHaveLength(0);
    expect(m.findLeaf("a")).toBeNull();
  });

  it("splits cannot target a floating leaf; maximize ignores them", () => {
    const m = twoLeafModel();
    m.floatLeaf("a");
    expect(m.splitLeaf("a", "right", ["x"])).toBeNull();
    m.maximizeLeaf("a");
    expect(m.maximizedLeafId).toBeNull();
  });

  it("dragging a panel from a float into the dock re-docks it", () => {
    const m = twoLeafModel();
    m.floatPanel("p2", "a");
    const floatId = m.floating[0].leaf.id;
    const created = m.splitWithPanel("p2", floatId, "b", "down");
    expect(created!.panels).toEqual(["p2"]);
    expect(m.floating).toHaveLength(0); // emptied window removed
  });

  it("unfloatLeaf docks a window back at a container edge", () => {
    const m = twoLeafModel();
    m.floatLeaf("a");
    const leaf = m.unfloatLeaf("a", "right");
    expect(leaf).not.toBeNull();
    expect(m.floating).toHaveLength(0);
    const root = m.root as DockBranch;
    expect(root.orientation).toBe("row");
    expect(root.children[root.children.length - 1]).toBe(leaf);
  });

  it("bringToFront reorders the floating stack and activates", () => {
    const m = twoLeafModel();
    m.floatPanel("p1", "a");
    m.floatPanel("p2", "a"); // a is now empty and removed
    const [first, second] = m.floating;
    m.bringToFront(first.leaf.id);
    expect(m.floating[1]).toBe(first);
    expect(m.floating[0]).toBe(second);
    expect(m.activeLeafId).toBe(first.leaf.id);
  });

  it("floating windows get cascaded default placements", () => {
    const m = new DockModel(leafSpec("a", ["p1", "p2", "p3"]));
    const g1 = m.floatPanel("p1", "a")!;
    const g2 = m.floatPanel("p2", "a")!;
    expect(g2.rect.left).toBeGreaterThan(g1.rect.left);
  });
});

// ─── Root splits ────────────────────────────────────────────────────────────

describe("DockModel — splitRoot", () => {
  it("joins a matching root branch with an equal share", () => {
    const m = twoLeafModel();
    const created = m.splitRoot("right", ["new"]);
    const root = m.root as DockBranch;
    expect(root.children).toHaveLength(3);
    expect(root.children[2]).toBe(created);
    for (const child of root.children) {
      expect(child.fraction).toBeCloseTo(1 / 3);
    }
  });

  it("left/up insert at the start", () => {
    const m = twoLeafModel();
    const created = m.splitRoot("left", ["new"]);
    const root = m.root as DockBranch;
    expect(root.children[0]).toBe(created);
  });

  it("wraps an orthogonal layout, taking a quarter of the axis", () => {
    const m = twoLeafModel();
    const created = m.splitRoot("down", ["new"]);
    const root = m.root as DockBranch;
    expect(root.orientation).toBe("column");
    expect(root.children).toHaveLength(2);
    expect(root.children[1]).toBe(created);
    expect(created!.fraction).toBeCloseTo(0.25);
    expect(root.children[0].fraction).toBeCloseTo(0.75);
  });

  it("becomes the root in an empty tree", () => {
    const m = new DockModel();
    const created = m.splitRoot("down", ["solo"]);
    expect(m.root).toBe(created);
  });

  it("splitRootWithPanel moves a panel to a full-side leaf", () => {
    const m = twoLeafModel();
    m.splitRootWithPanel("p2", "a", "down");
    const root = m.root as DockBranch;
    expect(root.orientation).toBe("column");
    expect(m.findLeaf("a")!.panels).toEqual(["p1"]);
    expect(m.leaves().map((l) => l.panels)).toEqual([["p1"], ["q1"], ["p2"]]);
  });

  it("is a no-op for the sole panel of the only docked leaf", () => {
    const m = new DockModel(leafSpec("a"));
    expect(m.splitRootWithPanel("a", "a", "left")).toBeNull();
    expect(m.root).toBe(m.findLeaf("a"));
  });

  it("docks a floating panel when the tree is empty", () => {
    const m = new DockModel(leafSpec("a"));
    m.floatLeaf("a");
    const created = m.splitRootWithPanel("a", "a", "right");
    expect(created).not.toBeNull();
    expect(m.root).toBe(created);
    expect(m.floating).toHaveLength(0);
  });
});

// ─── Serialization ──────────────────────────────────────────────────────────

describe("DockModel — feature serialization", () => {
  it("roundtrips floating windows and the active leaf", () => {
    const m = twoLeafModel();
    m.floatPanel("p2", "a", { left: 30, top: 40, width: 220, height: 180 });
    m.setActiveLeaf("b");

    const restored = DockModel.deserialize(m.serialize());
    expect(restored.serialize()).toEqual(m.serialize());
    expect(restored.floating).toHaveLength(1);
    expect(restored.floating[0].leaf.panels).toEqual(["p2"]);
    expect(restored.floating[0].rect).toEqual({ left: 30, top: 40, width: 220, height: 180 });
    expect(restored.activeLeafId).toBe("b");
  });

  it("tolerates a floating entry with a malformed rect", () => {
    const m = twoLeafModel();
    m.floatPanel("p2", "a");
    const data = m.serialize();
    (data.floating![0].rect as unknown as { left: unknown }).left = "oops";
    const restored = DockModel.deserialize(data);
    expect(restored.floating).toHaveLength(1);
    expect(typeof restored.floating[0].rect.left).toBe("number");
  });

  it("rejects a floating entry that is not a leaf", () => {
    const m = twoLeafModel();
    m.floatPanel("p2", "a");
    const data = m.serialize();
    (data.floating![0] as unknown as { leaf: { type: string } }).leaf.type = "branch";
    expect(() => DockModel.deserialize(data)).toThrow(/floating/);
  });

  it("drops a serialized active id that no longer resolves", () => {
    const m = twoLeafModel();
    m.setActiveLeaf("a");
    const data = m.serialize();
    data.active = "ghost";
    expect(DockModel.deserialize(data).activeLeafId).toBeNull();
  });
});

// ─── DockLayout drag-and-drop tests ─────────────────────────────────────────
// Full drag flows through the layout-level hit-test: tabify (center),
// split (edges), tab-strip insertion/reorder, overlay lifecycle, and the
// dragleave semantics the old per-group handlers got wrong.

import { render } from "@testing-library/svelte";
import { describe, it, expect } from "vitest";
import { createRawSnippet, tick } from "svelte";
import DockLayout from "./DockLayout.svelte";
import { DockModel, DockBranch, type SerializedNode } from "./model.svelte.js";
import { PANEL_DRAG_MIME } from "./dnd.js";

// ─── Helpers ────────────────────────────────────────────────────────────────

const panelSnippet = createRawSnippet((panelId: () => string) => ({
  render: () => `<div>panel ${panelId()}</div>`,
}));

function leafSpec(id: string, panels: string[] = [id]): SerializedNode {
  return { type: "leaf", id, panels };
}

/** Minimal DataTransfer stand-in (happy-dom lacks a real one). */
function fakeDataTransfer(): DataTransfer {
  const store = new Map<string, string>();
  const types: string[] = [];
  return {
    types,
    setData(type: string, value: string) {
      store.set(type, value);
      if (!types.includes(type)) types.push(type);
    },
    getData(type: string) {
      return store.get(type) ?? "";
    },
    setDragImage() {},
    effectAllowed: "",
    dropEffect: "",
  } as unknown as DataTransfer;
}

function dragEvent(type: string, x: number, y: number, dt: DataTransfer): Event {
  const e = new MouseEvent(type, { bubbles: true, cancelable: true, clientX: x, clientY: y });
  Object.defineProperty(e, "dataTransfer", { value: dt });
  return e;
}

/**
 * Simulate a full tab drag: dragstart on the source tab, dragover at
 * (x, y) on the layout, then (unless aborted) drop at the same point.
 */
async function drag(
  container: HTMLElement,
  sourceLeafId: string,
  sourcePanel: string,
  x: number,
  y: number,
  opts: { drop?: boolean } = {},
): Promise<DataTransfer> {
  const dt = fakeDataTransfer();
  const tabs = container.querySelectorAll<HTMLElement>(
    `[data-leaf-id="${sourceLeafId}"] .dock-tab`,
  );
  const tab = [...tabs].find((t) => t.textContent?.includes(sourcePanel));
  if (!tab) throw new Error(`no tab ${sourcePanel} in ${sourceLeafId}`);
  const layout = container.querySelector<HTMLElement>(".dock-layout")!;

  tab.dispatchEvent(dragEvent("dragstart", 0, 0, dt));
  await tick();
  layout.dispatchEvent(dragEvent("dragover", x, y, dt));
  await tick();
  if (opts.drop !== false) {
    layout.dispatchEvent(dragEvent("drop", x, y, dt));
    await tick();
  }
  return dt;
}

async function mount(model: DockModel, w = 800, h = 600) {
  const result = render(DockLayout, { model, panel: panelSnippet });
  globalThis.triggerResizeObservers(w, h);
  await tick();
  return result;
}

/** row[a, b] at 800x600: a = [0,400), b = [400,800), tab bars y < 26. */
async function twoGroupSetup() {
  const model = new DockModel({
    type: "branch",
    orientation: "row",
    children: [leafSpec("a", ["p1", "p2"]), leafSpec("b", ["q1"])],
  });
  const mounted = await mount(model);
  return { model, ...mounted };
}

// ─── Center drop: tabify ────────────────────────────────────────────────────

describe("DockLayout dnd — tabify (center)", () => {
  it("dropping on another group's center moves the panel there", async () => {
    const { model, container } = await twoGroupSetup();
    await drag(container, "a", "p2", 600, 300); // center of b
    expect(model.findLeaf("a")!.panels).toEqual(["p1"]);
    expect(model.findLeaf("b")!.panels).toEqual(["q1", "p2"]);
    expect(model.findLeaf("b")!.activePanel).toBe("p2");
  });

  it("dropping the source's last panel tabifies and collapses the source", async () => {
    const { model, container } = await twoGroupSetup();
    await drag(container, "b", "q1", 200, 300); // center of a
    expect(model.findLeaf("b")).toBeNull();
    expect(model.findLeaf("a")!.panels).toEqual(["p1", "p2", "q1"]);
    expect(model.leaves()).toHaveLength(1);
  });

  it("dropping on the source group's own center is a no-op", async () => {
    const { model, container } = await twoGroupSetup();
    await drag(container, "a", "p2", 200, 300); // center of a itself
    expect(model.findLeaf("a")!.panels).toEqual(["p1", "p2"]);
    expect(model.findLeaf("b")!.panels).toEqual(["q1"]);
  });
});

// ─── Edge drops: split ──────────────────────────────────────────────────────

describe("DockLayout dnd — split (edges)", () => {
  it("dropping on a left edge splits the target into a row", async () => {
    const { model, container } = await twoGroupSetup();
    await drag(container, "a", "p2", 440, 300); // left 10% of b
    const root = model.root as DockBranch;
    expect(root.orientation).toBe("row");
    expect(model.leaves().map((l) => l.panels)).toEqual([["p1"], ["p2"], ["q1"]]);
  });

  it("dropping on a bottom edge splits the target into a column", async () => {
    const { model, container } = await twoGroupSetup();
    await drag(container, "a", "p2", 600, 550); // bottom 20% of b
    const leaves = model.leaves();
    expect(leaves.map((l) => l.panels)).toEqual([["p1"], ["q1"], ["p2"]]);
    // b's slot is now a column [q1-leaf, p2-leaf].
    const root = model.root as DockBranch;
    const nested = root.children[1] as DockBranch;
    expect(nested.kind).toBe("branch");
    expect(nested.orientation).toBe("column");
  });

  it("moving a sole panel to another group's edge collapses the source", async () => {
    const { model, container } = await twoGroupSetup();
    await drag(container, "b", "q1", 200, 550); // bottom edge of a
    expect(model.findLeaf("b")).toBeNull();
    expect((model.root as DockBranch).orientation).toBe("column");
    expect(model.leaves().map((l) => l.panels)).toEqual([["p1", "p2"], ["q1"]]);
  });
});

// ─── Tab strip drops ────────────────────────────────────────────────────────

describe("DockLayout dnd — tab strip", () => {
  it("dropping on another group's tab strip inserts there", async () => {
    const { model, container } = await twoGroupSetup();
    // y=10 is inside b's tab bar. happy-dom tab rects are all zero-width,
    // so the insertion index resolves to the end of the strip.
    await drag(container, "a", "p1", 600, 10);
    expect(model.findLeaf("b")!.panels).toEqual(["q1", "p1"]);
    expect(model.findLeaf("b")!.activePanel).toBe("p1");
    expect(model.findLeaf("a")!.panels).toEqual(["p2"]);
  });

  it("dropping on the own strip reorders the tab", async () => {
    const { model, container } = await twoGroupSetup();
    await drag(container, "a", "p1", 200, 10); // own strip, insertion at end
    expect(model.findLeaf("a")!.panels).toEqual(["p2", "p1"]);
    expect(model.findLeaf("a")!.activePanel).toBe("p1");
  });

  it("shows the insertion caret during a strip dragover", async () => {
    const { container } = await twoGroupSetup();
    await drag(container, "a", "p1", 600, 10, { drop: false });
    const caret = container.querySelector(`[data-leaf-id="b"] .dock-tab-caret`);
    expect(caret).not.toBeNull();
  });
});

// ─── Overlay lifecycle ──────────────────────────────────────────────────────

describe("DockLayout dnd — overlay", () => {
  it("shows the zone overlay over the hovered group and clears on drop", async () => {
    const { container } = await twoGroupSetup();
    await drag(container, "a", "p2", 600, 300, { drop: false });
    const layer = container.querySelector<HTMLElement>(".dock-drop-layer");
    expect(layer).not.toBeNull();
    expect(layer!.style.left).toBe("400px");
    expect(layer!.querySelector(".dock-drop-center.active")).not.toBeNull();

    const layout = container.querySelector<HTMLElement>(".dock-layout")!;
    layout.dispatchEvent(dragEvent("drop", 600, 300, fakeDataTransfer()));
    await tick();
    expect(container.querySelector(".dock-drop-layer")).toBeNull();
  });

  it("highlights the matching edge zone", async () => {
    const { container } = await twoGroupSetup();
    await drag(container, "a", "p2", 440, 300, { drop: false });
    const layer = container.querySelector<HTMLElement>(".dock-drop-layer");
    expect(layer!.querySelector(".dock-drop-left.active")).not.toBeNull();
  });

  it("clears the overlay when the drag leaves the layout", async () => {
    const { container } = await twoGroupSetup();
    const dt = await drag(container, "a", "p2", 600, 300, { drop: false });
    expect(container.querySelector(".dock-drop-layer")).not.toBeNull();

    const layout = container.querySelector<HTMLElement>(".dock-layout")!;
    // relatedTarget null = left the window entirely.
    layout.dispatchEvent(dragEvent("dragleave", -10, -10, dt));
    await tick();
    expect(container.querySelector(".dock-drop-layer")).toBeNull();
  });

  it("clears the overlay on dragend without a drop", async () => {
    const { container } = await twoGroupSetup();
    const dt = await drag(container, "a", "p2", 600, 300, { drop: false });
    const tab = container.querySelector<HTMLElement>(`[data-leaf-id="a"] .dock-tab`)!;
    tab.dispatchEvent(dragEvent("dragend", 0, 0, dt));
    await tick();
    expect(container.querySelector(".dock-drop-layer")).toBeNull();
  });

  it("ignores drags without panel payload", async () => {
    const { container } = await twoGroupSetup();
    const layout = container.querySelector<HTMLElement>(".dock-layout")!;
    layout.dispatchEvent(dragEvent("dragover", 600, 300, fakeDataTransfer()));
    await tick();
    expect(container.querySelector(".dock-drop-layer")).toBeNull();
  });
});

// ─── Maximize interaction ───────────────────────────────────────────────────

describe("DockLayout — maximize via double-click", () => {
  it("double-clicking a tab strip toggles maximize", async () => {
    const { model, container } = await twoGroupSetup();
    const tabs = container.querySelector<HTMLElement>(`[data-group-id="a"] .dock-tabs`)!;
    tabs.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
    await tick();
    expect(model.maximizedLeafId).toBe("a");
    tabs.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
    await tick();
    expect(model.maximizedLeafId).toBeNull();
  });
});

// ─── Persistence ────────────────────────────────────────────────────────────

describe("DockLayout — persistence", () => {
  it("saves the layout to localStorage (debounced) and restores via fromStorage", async () => {
    localStorage.clear();
    const model = new DockModel({
      type: "branch",
      orientation: "row",
      children: [leafSpec("a"), leafSpec("b")],
    });
    render(DockLayout, { model, panel: panelSnippet, persistKey: "dock-test" });
    globalThis.triggerResizeObservers(800, 600);
    await tick();

    model.movePanel("a", "a", "b");
    await tick();
    await new Promise((r) => setTimeout(r, 300)); // let the debounce flush

    const restored = DockModel.fromStorage("dock-test", leafSpec("fallback"));
    expect(restored.findLeaf("b")!.panels).toEqual(["b", "a"]);
    expect(restored.findLeaf("a")).toBeNull();
  });

  it("fromStorage falls back on missing or corrupt data", () => {
    localStorage.clear();
    const fallback = DockModel.fromStorage("missing-key", leafSpec("fb"));
    expect(fallback.findLeaf("fb")).not.toBeNull();

    localStorage.setItem("bad-key", "not json{");
    const fromBad = DockModel.fromStorage("bad-key", leafSpec("fb"));
    expect(fromBad.findLeaf("fb")).not.toBeNull();
  });
});

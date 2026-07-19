// ─── DockLayout feature tests ───────────────────────────────────────────────
// Panel registry (titles/closable), empty state, container-edge drops,
// floating windows (render, move, resize, stack, float/dock buttons,
// Alt+drop), and keyboard shortcuts.

import { render } from "@testing-library/svelte";
import { describe, it, expect } from "vitest";
import { createRawSnippet, tick } from "svelte";
import DockLayout from "./DockLayout.svelte";
import {
  DockModel,
  DockBranch,
  type PanelDef,
  type SerializedNode,
} from "./model.svelte.js";

// ─── Helpers ────────────────────────────────────────────────────────────────

const panelSnippet = createRawSnippet((panelId: () => string) => ({
  render: () => `<div data-testid="content-${panelId()}">panel ${panelId()}</div>`,
}));

function leafSpec(id: string, panels: string[] = [id]): SerializedNode {
  return { type: "leaf", id, panels };
}

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

function mouse(type: string, init: Record<string, unknown> = {}): MouseEvent {
  return new MouseEvent(type, { bubbles: true, cancelable: true, ...init });
}

function dragEvent(type: string, x: number, y: number, dt: DataTransfer, alt = false): Event {
  const e = mouse(type, { clientX: x, clientY: y, altKey: alt });
  Object.defineProperty(e, "dataTransfer", { value: dt });
  return e;
}

async function drag(
  container: HTMLElement,
  sourceGroupId: string,
  sourcePanel: string,
  x: number,
  y: number,
  opts: { drop?: boolean; alt?: boolean } = {},
): Promise<void> {
  const dt = fakeDataTransfer();
  const tabs = container.querySelectorAll<HTMLElement>(
    `[data-group-id="${sourceGroupId}"] .dock-tab`,
  );
  const tab = [...tabs].find((t) => t.textContent?.includes(sourcePanel));
  if (!tab) throw new Error(`no tab ${sourcePanel} in ${sourceGroupId}`);
  const layout = container.querySelector<HTMLElement>(".dock-layout")!;

  tab.dispatchEvent(dragEvent("dragstart", 0, 0, dt));
  await tick();
  layout.dispatchEvent(dragEvent("dragover", x, y, dt, opts.alt));
  await tick();
  if (opts.drop !== false) {
    layout.dispatchEvent(dragEvent("drop", x, y, dt, opts.alt));
    await tick();
  }
}

async function mount(
  model: DockModel,
  extra: Record<string, unknown> = {},
  w = 800,
  h = 600,
) {
  const result = render(DockLayout, { model, panel: panelSnippet, ...extra });
  globalThis.triggerResizeObservers(w, h);
  await tick();
  return result;
}

async function twoGroupSetup(extra: Record<string, unknown> = {}) {
  const model = new DockModel({
    type: "branch",
    orientation: "row",
    children: [leafSpec("a", ["p1", "p2"]), leafSpec("b", ["q1"])],
  });
  const mounted = await mount(model, extra);
  return { model, ...mounted };
}

// ─── Panel registry ─────────────────────────────────────────────────────────

describe("DockLayout — panel registry", () => {
  const defs: PanelDef[] = [
    { id: "p1", title: "CPU Registers", closable: false },
    { id: "p2", title: "Memory" },
  ];

  it("tabs render registry titles, falling back to ids", async () => {
    const { container } = await twoGroupSetup({ panelDefs: defs });
    const labels = [...container.querySelectorAll(".dock-tab-label")].map(
      (el) => el.textContent,
    );
    expect(labels).toContain("CPU Registers");
    expect(labels).toContain("Memory");
    expect(labels).toContain("q1"); // no def → id
  });

  it("closable: false hides that tab's close button", async () => {
    const { container } = await twoGroupSetup({ panelDefs: defs });
    const tabs = [...container.querySelectorAll<HTMLElement>('[data-group-id="a"] .dock-tab')];
    const p1Tab = tabs.find((t) => t.textContent?.includes("CPU Registers"))!;
    const p2Tab = tabs.find((t) => t.textContent?.includes("Memory"))!;
    expect(p1Tab.querySelector(".dock-tab-close")).toBeNull();
    expect(p2Tab.querySelector(".dock-tab-close")).not.toBeNull();
  });
});

// ─── Empty state ────────────────────────────────────────────────────────────

describe("DockLayout — empty state", () => {
  it("renders the default hint when no groups are docked", async () => {
    const { container } = await mount(new DockModel());
    expect(container.querySelector(".dock-empty")).toHaveTextContent("No panels docked");
  });

  it("renders a custom empty snippet", async () => {
    const emptySnippet = createRawSnippet(() => ({
      render: () => `<div data-testid="custom-empty">nothing here</div>`,
    }));
    const { getByTestId } = await mount(new DockModel(), { empty: emptySnippet });
    expect(getByTestId("custom-empty")).toBeInTheDocument();
  });

  it("disappears once a panel is opened", async () => {
    const model = new DockModel();
    const { container } = await mount(model);
    model.openPanel("solo");
    await tick();
    expect(container.querySelector(".dock-empty")).toBeNull();
    expect(container.querySelector('[data-panel-id="solo"]')).not.toBeNull();
  });
});

// ─── Container-edge drops ───────────────────────────────────────────────────

describe("DockLayout — root edge drops", () => {
  it("shows the root overlay in the edge band", async () => {
    const { container } = await twoGroupSetup();
    await drag(container, "a", "p1", 795, 300, { drop: false });
    const overlay = container.querySelector<HTMLElement>(".dock-drop-root");
    expect(overlay).not.toBeNull();
    expect(overlay!.style.left).toBe("600px"); // right quarter
  });

  it("dropping on the right edge creates a full-side split", async () => {
    const { model, container } = await twoGroupSetup();
    await drag(container, "a", "p1", 795, 300);
    const root = model.root as DockBranch;
    expect(root.children).toHaveLength(3);
    expect(model.leaves().map((l) => l.panels)).toEqual([["p2"], ["q1"], ["p1"]]);
  });

  it("dropping on the bottom edge wraps the layout in a column", async () => {
    const { model, container } = await twoGroupSetup();
    await drag(container, "a", "p1", 400, 595);
    const root = model.root as DockBranch;
    expect(root.orientation).toBe("column");
    expect(model.leaves().map((l) => l.panels)).toEqual([["p2"], ["q1"], ["p1"]]);
  });

  it("tab strips beat the edge band for top-row groups", async () => {
    const { model, container } = await twoGroupSetup();
    await drag(container, "a", "p1", 600, 10); // b's strip, inside top band
    expect(model.findLeaf("b")!.panels).toEqual(["q1", "p1"]);
    expect((model.root as DockBranch).children).toHaveLength(2);
  });

  it("an empty tree accepts a drop anywhere from a floating window", async () => {
    const model = new DockModel(leafSpec("a"));
    const { container } = await mount(model);
    model.floatLeaf("a");
    await tick();
    expect(model.root).toBeNull();
    // Outside the floating window's own rect, in the empty dock area.
    await drag(container, "a", "a", 600, 500);
    expect(model.root).not.toBeNull();
    expect(model.floating).toHaveLength(0);
    expect(model.leaves()[0].panels).toEqual(["a"]);
  });
});

// ─── Floating windows ───────────────────────────────────────────────────────

describe("DockLayout — floating windows", () => {
  it("renders a window with chrome and keeps the panel node alive", async () => {
    const { model, container } = await twoGroupSetup();
    const before = container.querySelector<HTMLElement>('[data-panel-id="p2"]')!;
    model.floatPanel("p2", "a", { left: 100, top: 80, width: 300, height: 200 });
    await tick();

    const win = container.querySelector<HTMLElement>(".dock-float");
    expect(win).not.toBeNull();
    expect(win!.style.left).toBe("100px");
    const after = container.querySelector<HTMLElement>('[data-panel-id="p2"]')!;
    expect(after).toBe(before); // keep-alive across floating
    expect(after).not.toHaveClass("dock-hidden");
    expect(parseInt(after.style.zIndex)).toBeGreaterThan(40);
  });

  it("dragging the strip background moves the window", async () => {
    const { model, container } = await twoGroupSetup();
    const fg = model.floatPanel("p2", "a", { left: 100, top: 80, width: 300, height: 200 })!;
    await tick();
    const win = container.querySelector<HTMLElement>(".dock-float")!;
    const strip = win.querySelector<HTMLElement>(".dock-tabs")!;

    strip.dispatchEvent(mouse("pointerdown", { clientX: 150, clientY: 90, button: 0 }));
    win.dispatchEvent(mouse("pointermove", { clientX: 210, clientY: 130 }));
    await tick();
    expect(fg.rect.left).toBe(160);
    expect(fg.rect.top).toBe(120);
    win.dispatchEvent(mouse("pointerup", { clientX: 210, clientY: 130 }));
  });

  it("the corner grip resizes with a floor", async () => {
    const { model, container } = await twoGroupSetup();
    const fg = model.floatPanel("p2", "a", { left: 100, top: 80, width: 300, height: 200 })!;
    await tick();
    const win = container.querySelector<HTMLElement>(".dock-float")!;
    const grip = win.querySelector<HTMLElement>(".dock-float-grip")!;

    grip.dispatchEvent(mouse("pointerdown", { clientX: 400, clientY: 280, button: 0 }));
    grip.dispatchEvent(mouse("pointermove", { clientX: 460, clientY: 320 }));
    await tick();
    expect(fg.rect.width).toBe(360);
    expect(fg.rect.height).toBe(240);

    grip.dispatchEvent(mouse("pointermove", { clientX: 0, clientY: 0 }));
    await tick();
    expect(fg.rect.width).toBe(160); // FLOAT_MIN_W
    expect(fg.rect.height).toBe(120); // FLOAT_MIN_H
    grip.dispatchEvent(mouse("pointerup", { clientX: 0, clientY: 0 }));
  });

  it("pointerdown raises a window to the top of the stack", async () => {
    const model = new DockModel(leafSpec("a", ["p1", "p2", "p3"]));
    const { container } = await mount(model);
    model.floatPanel("p1", "a");
    model.floatPanel("p2", "a");
    await tick();
    const firstId = model.floating[0].leaf.id;
    const win = container.querySelector<HTMLElement>(`[data-float-id="${firstId}"]`)!;
    win.dispatchEvent(mouse("pointerdown", { button: 0 }));
    await tick();
    expect(model.floating[1].leaf.id).toBe(firstId);
    expect(model.activeLeafId).toBe(firstId);
  });

  it("the ⧉ button floats a docked group; ⇱ docks it back", async () => {
    const { model, container } = await twoGroupSetup();
    const floatBtn = container.querySelector<HTMLElement>(
      '[data-group-id="a"] [aria-label="Float group"]',
    )!;
    floatBtn.click();
    await tick();
    expect(model.floatingOf("a")).not.toBeNull();
    expect(model.leaves().map((l) => l.id)).toEqual(["b"]);

    const dockBtn = container.querySelector<HTMLElement>(
      '[data-group-id="a"] [aria-label="Dock group"]',
    )!;
    dockBtn.click();
    await tick();
    expect(model.floatingOf("a")).toBeNull();
    expect(model.leaves().map((l) => l.id)).toEqual(["b", "a"]);
  });

  it("Alt+drop floats the dragged panel at the cursor", async () => {
    const { model, container } = await twoGroupSetup();
    await drag(container, "a", "p2", 300, 200, { alt: true });
    expect(model.floating).toHaveLength(1);
    expect(model.floating[0].leaf.panels).toEqual(["p2"]);
    expect(model.floating[0].rect.left).toBe(260); // x - 40
  });

  it("Alt dragover shows the dashed float preview", async () => {
    const { container } = await twoGroupSetup();
    await drag(container, "a", "p2", 300, 200, { alt: true, drop: false });
    expect(container.querySelector(".dock-float-preview")).not.toBeNull();
  });

  it("dropping on a float's body tabifies into it", async () => {
    const { model, container } = await twoGroupSetup();
    model.floatPanel("p2", "a", { left: 100, top: 80, width: 300, height: 200 });
    await tick();
    const floatId = model.floating[0].leaf.id;
    await drag(container, "b", "q1", 250, 180); // inside the float body
    expect(model.findLeaf(floatId)!.panels).toEqual(["p2", "q1"]);
    expect(model.leaves().map((l) => l.id)).toEqual(["a"]); // b collapsed
  });
});

// ─── Keyboard ───────────────────────────────────────────────────────────────

describe("DockLayout — keyboard", () => {
  function key(code: string): KeyboardEvent {
    return new KeyboardEvent("keydown", { code, altKey: true, bubbles: true, cancelable: true });
  }

  it("Alt+] and Alt+[ cycle the active group's tabs", async () => {
    const { model, container } = await twoGroupSetup();
    model.setActiveLeaf("a");
    await tick();
    const layout = container.querySelector<HTMLElement>(".dock-layout")!;
    layout.dispatchEvent(key("BracketRight"));
    expect(model.findLeaf("a")!.activePanel).toBe("p2");
    layout.dispatchEvent(key("BracketRight"));
    expect(model.findLeaf("a")!.activePanel).toBe("p1"); // wraps
    layout.dispatchEvent(key("BracketLeft"));
    expect(model.findLeaf("a")!.activePanel).toBe("p2");
  });

  it("Alt+W closes the active tab, respecting closable", async () => {
    const { model, container } = await twoGroupSetup({
      panelDefs: [{ id: "p1", closable: false }] satisfies PanelDef[],
    });
    model.setActiveLeaf("a");
    await tick();
    const layout = container.querySelector<HTMLElement>(".dock-layout")!;
    layout.dispatchEvent(key("KeyW")); // active is p1 — not closable
    expect(model.findLeaf("a")!.panels).toEqual(["p1", "p2"]);

    model.activatePanel("a", "p2");
    await tick();
    layout.dispatchEvent(key("KeyW"));
    expect(model.findLeaf("a")!.panels).toEqual(["p1"]);
  });

  it("Alt+Enter toggles maximize on the active group", async () => {
    const { model, container } = await twoGroupSetup();
    model.setActiveLeaf("b");
    await tick();
    const layout = container.querySelector<HTMLElement>(".dock-layout")!;
    layout.dispatchEvent(key("Enter"));
    expect(model.maximizedLeafId).toBe("b");
    layout.dispatchEvent(key("Enter"));
    expect(model.maximizedLeafId).toBeNull();
  });

  it("clicking a group makes it the active target", async () => {
    const { model, container } = await twoGroupSetup();
    const leafB = container.querySelector<HTMLElement>('[data-leaf-id="b"]')!;
    leafB.dispatchEvent(mouse("pointerdown", { button: 0 }));
    await tick();
    expect(model.activeLeafId).toBe("b");
  });
});

// ─── Sash equalize ──────────────────────────────────────────────────────────

describe("DockLayout — sash double-click", () => {
  it("equalizes the two adjacent groups", async () => {
    const { model, container } = await twoGroupSetup();
    const root = model.root as DockBranch;
    root.children[0].fraction = 0.8;
    root.children[1].fraction = 0.2;
    await tick();
    const sash = container.querySelector<HTMLElement>(".dock-sash")!;
    sash.dispatchEvent(mouse("dblclick"));
    await tick();
    expect(root.children.map((c) => c.fraction)).toEqual([0.5, 0.5]);
    const leafA = container.querySelector<HTMLElement>('[data-leaf-id="a"]')!;
    expect(leafA.style.width).toBe("400px");
  });
});

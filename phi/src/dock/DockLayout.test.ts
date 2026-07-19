// ─── DockLayout component tests ─────────────────────────────────────────────
// The integration layer the old dock never tested: rendering from the model,
// tab interactions, keep-alive panel identity, sash dragging, and layout
// consistency after structural changes (the old design's failure mode).

import { render } from "@testing-library/svelte";
import { describe, it, expect } from "vitest";
import { createRawSnippet, tick } from "svelte";
import DockLayout from "./DockLayout.svelte";
import { DockModel, type SerializedNode } from "./model.svelte.js";

// ─── Helpers ────────────────────────────────────────────────────────────────

const panelSnippet = createRawSnippet((panelId: () => string) => ({
  render: () => `<div data-testid="content-${panelId()}">panel ${panelId()}</div>`,
}));

declare global {
  // Provided by test-setup.ts
  var triggerResizeObservers: (width: number, height: number) => void;
}

function leafSpec(id: string, panels: string[] = [id]): SerializedNode {
  return { type: "leaf", id, panels };
}

async function mount(model: DockModel, w = 800, h = 600) {
  const result = render(DockLayout, { model, panel: panelSnippet });
  globalThis.triggerResizeObservers(w, h);
  await tick();
  return result;
}

function leafRect(container: HTMLElement, id: string): Record<string, string> {
  const el = container.querySelector<HTMLElement>(`[data-leaf-id="${id}"]`);
  if (!el) throw new Error(`no leaf ${id}`);
  return {
    left: el.style.left,
    top: el.style.top,
    width: el.style.width,
    height: el.style.height,
  };
}

function panelEl(container: HTMLElement, id: string): HTMLElement {
  const el = container.querySelector<HTMLElement>(`[data-panel-id="${id}"]`);
  if (!el) throw new Error(`no panel ${id}`);
  return el;
}

function sashEls(container: HTMLElement): HTMLElement[] {
  return [...container.querySelectorAll<HTMLElement>(".dock-sash")];
}

function pointer(type: string, init: Record<string, unknown>): Event {
  // happy-dom may lack PointerEvent; MouseEvent carries everything we read.
  const Ctor = (globalThis as Record<string, unknown>).PointerEvent ?? MouseEvent;
  return new (Ctor as typeof MouseEvent)(type, { bubbles: true, ...init });
}

// ─── Rendering ──────────────────────────────────────────────────────────────

describe("DockLayout — rendering", () => {
  it("renders nothing for an empty model", async () => {
    const { container } = await mount(new DockModel());
    expect(container.querySelectorAll(".dock-leaf")).toHaveLength(0);
    expect(container.querySelectorAll(".dock-sash")).toHaveLength(0);
  });

  it("a single leaf fills the container", async () => {
    const { container } = await mount(new DockModel(leafSpec("a")));
    expect(leafRect(container, "a")).toEqual({
      left: "0px",
      top: "0px",
      width: "800px",
      height: "600px",
    });
  });

  it("a 50/50 row renders two groups and one sash", async () => {
    const model = new DockModel({
      type: "branch",
      orientation: "row",
      children: [leafSpec("a"), leafSpec("b")],
    });
    const { container } = await mount(model);
    expect(leafRect(container, "a").width).toBe("400px");
    expect(leafRect(container, "b").left).toBe("400px");
    expect(sashEls(container)).toHaveLength(1);
  });

  it("renders tabs for each panel and marks the active one", async () => {
    const model = new DockModel({ type: "leaf", id: "a", panels: ["p1", "p2"], active: "p2" });
    const { container, getByRole } = await mount(model);
    expect(container.querySelectorAll('[role="tab"]')).toHaveLength(2);
    expect(getByRole("tab", { selected: true })).toHaveTextContent("p2");
  });

  it("shows only the active panel; inactive panels stay mounted but hidden", async () => {
    const model = new DockModel({ type: "leaf", id: "a", panels: ["p1", "p2"] });
    const { container, getByTestId } = await mount(model);
    expect(panelEl(container, "p1")).not.toHaveClass("dock-hidden");
    expect(panelEl(container, "p2")).toHaveClass("dock-hidden");
    // Content exists in the DOM even while hidden — that is keep-alive.
    expect(getByTestId("content-p2")).toBeInTheDocument();
  });

  it("reacts to container resizes", async () => {
    const model = new DockModel({
      type: "branch",
      orientation: "row",
      children: [leafSpec("a"), leafSpec("b")],
    });
    const { container } = await mount(model);
    globalThis.triggerResizeObservers(1000, 500);
    await tick();
    expect(leafRect(container, "a").width).toBe("500px");
    expect(leafRect(container, "b")).toEqual({
      left: "500px",
      top: "0px",
      width: "500px",
      height: "500px",
    });
  });
});

// ─── Tab interactions ───────────────────────────────────────────────────────

describe("DockLayout — tabs", () => {
  it("clicking a tab activates its panel", async () => {
    const model = new DockModel({ type: "leaf", id: "a", panels: ["p1", "p2"] });
    const { container, getAllByRole } = await mount(model);
    getAllByRole("tab")[1].click();
    await tick();
    expect(model.findLeaf("a")!.activePanel).toBe("p2");
    expect(panelEl(container, "p2")).not.toHaveClass("dock-hidden");
    expect(panelEl(container, "p1")).toHaveClass("dock-hidden");
  });

  it("keep-alive: switching tabs preserves the panel DOM node", async () => {
    const model = new DockModel({ type: "leaf", id: "a", panels: ["p1", "p2"] });
    const { container, getAllByRole } = await mount(model);
    const before = panelEl(container, "p1");
    getAllByRole("tab")[1].click();
    await tick();
    getAllByRole("tab")[0].click();
    await tick();
    expect(panelEl(container, "p1")).toBe(before);
  });

  it("keep-alive: moving a panel between groups preserves its DOM node", async () => {
    const model = new DockModel({
      type: "branch",
      orientation: "row",
      children: [leafSpec("a", ["p1", "p2"]), leafSpec("b", ["q"])],
    });
    const { container } = await mount(model);
    const before = panelEl(container, "p1");
    model.movePanel("p1", "a", "b");
    await tick();
    expect(panelEl(container, "p1")).toBe(before);
    expect(panelEl(container, "p1")).not.toHaveClass("dock-hidden");
  });

  it("closing a tab removes the panel; closing the last collapses the group", async () => {
    const model = new DockModel({
      type: "branch",
      orientation: "row",
      children: [leafSpec("a"), leafSpec("b")],
    });
    const { container } = await mount(model);
    const closeBtn = container.querySelector<HTMLElement>(
      '[data-leaf-id="a"] .dock-tab-close',
    );
    closeBtn!.click();
    await tick();
    // Group a is gone and b expands to fill — the relayout the old
    // implementation missed.
    expect(container.querySelector('[data-leaf-id="a"]')).toBeNull();
    expect(leafRect(container, "b")).toEqual({
      left: "0px",
      top: "0px",
      width: "800px",
      height: "600px",
    });
    expect(sashEls(container)).toHaveLength(0);
  });
});

// ─── Maximize ───────────────────────────────────────────────────────────────

describe("DockLayout — maximize", () => {
  it("maximized leaf covers the container; others hide; sashes vanish", async () => {
    const model = new DockModel({
      type: "branch",
      orientation: "row",
      children: [leafSpec("a"), leafSpec("b")],
    });
    const { container } = await mount(model);
    model.maximizeLeaf("b");
    await tick();
    expect(leafRect(container, "b")).toEqual({
      left: "0px",
      top: "0px",
      width: "800px",
      height: "600px",
    });
    const leafA = container.querySelector<HTMLElement>('[data-leaf-id="a"]');
    expect(leafA).toHaveClass("dock-hidden");
    expect(sashEls(container)).toHaveLength(0);

    model.restore();
    await tick();
    expect(leafRect(container, "a").width).toBe("400px");
    expect(sashEls(container)).toHaveLength(1);
  });
});

// ─── Sash dragging ──────────────────────────────────────────────────────────

describe("DockLayout — sash drag", () => {
  async function twoColumnSetup() {
    const model = new DockModel({
      type: "branch",
      orientation: "row",
      children: [leafSpec("a"), leafSpec("b")],
    });
    const mounted = await mount(model);
    return { model, ...mounted };
  }

  it("dragging the sash resizes both groups", async () => {
    const { container } = await twoColumnSetup();
    const sash = sashEls(container)[0];
    sash.dispatchEvent(pointer("pointerdown", { clientX: 400, clientY: 300, button: 0 }));
    sash.dispatchEvent(pointer("pointermove", { clientX: 500, clientY: 300 }));
    await tick();
    expect(leafRect(container, "a").width).toBe("500px");
    expect(leafRect(container, "b")).toEqual({
      left: "500px",
      top: "0px",
      width: "300px",
      height: "600px",
    });
    sash.dispatchEvent(pointer("pointerup", { clientX: 500, clientY: 300 }));
  });

  it("drag clamps at the neighbour's minimum size", async () => {
    const { container } = await twoColumnSetup();
    const sash = sashEls(container)[0];
    sash.dispatchEvent(pointer("pointerdown", { clientX: 400, clientY: 300, button: 0 }));
    sash.dispatchEvent(pointer("pointermove", { clientX: 3000, clientY: 300 }));
    await tick();
    // minPanelWidth defaults to 120.
    expect(leafRect(container, "a").width).toBe("680px");
    expect(leafRect(container, "b").width).toBe("120px");
    sash.dispatchEvent(pointer("pointerup", { clientX: 3000, clientY: 300 }));
  });

  it("moves within one drag are relative to the drag-start snapshot (no drift)", async () => {
    const { container } = await twoColumnSetup();
    const sash = sashEls(container)[0];
    sash.dispatchEvent(pointer("pointerdown", { clientX: 400, clientY: 300, button: 0 }));
    sash.dispatchEvent(pointer("pointermove", { clientX: 700, clientY: 300 }));
    await tick();
    sash.dispatchEvent(pointer("pointermove", { clientX: 400, clientY: 300 }));
    await tick();
    expect(leafRect(container, "a").width).toBe("400px");
    expect(leafRect(container, "b").width).toBe("400px");
    sash.dispatchEvent(pointer("pointerup", { clientX: 400, clientY: 300 }));
  });

  it("layout stays consistent when the container resizes after a structural change", async () => {
    // The old implementation's killer: stale proportions after add/remove.
    const { model, container } = await twoColumnSetup();
    model.splitWithPanel("b", "b", "a", "down"); // b moves below a; leaf b removed
    await tick();
    globalThis.triggerResizeObservers(1000, 900);
    await tick();
    const leaves = [...container.querySelectorAll<HTMLElement>(".dock-leaf")];
    expect(leaves).toHaveLength(2);
    const heights = leaves.map((el) => parseFloat(el.style.height));
    expect(heights.reduce((s, h) => s + h, 0)).toBe(900);
    const widths = leaves.map((el) => parseFloat(el.style.width));
    for (const w of widths) expect(w).toBe(1000);
  });
});

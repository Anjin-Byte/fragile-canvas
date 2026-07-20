// ─── Workbench layout presets & named-config store ─────────────────────────
// A thin layer over the dock's built-in serialization (DockModel.serialize /
// .deserialize). Named user configs live in one localStorage object; the
// live working layout is persisted separately by DockLayout's persistKey.

import type { SerializedDock, SerializedLeaf, SerializedNode } from "@gestalt/phi";

const STORE_KEY = "fc-workbench-layouts";

// ─── Built-in presets ───────────────────────────────────────────────────────

export interface Preset {
  name: string;
  spec: SerializedNode;
}

/**
 * The default workbench arrangement (also the fromStorage fallback for a
 * fresh install). Inspector tabs (PPU/APU/IO/Memory) left, disassembly
 * center, screen over CPU at right. Captured from a hand-arranged layout.
 */
export const DEFAULT_LAYOUT: SerializedNode = {
  type: "branch",
  orientation: "row",
  children: [
    {
      type: "leaf",
      panels: ["ppu", "apu", "io", "memory"],
      active: "ppu",
      fraction: 0.2958828540558511,
    },
    { type: "leaf", panels: ["disasm"], fraction: 0.37964906083776595 },
    {
      type: "branch",
      orientation: "column",
      fraction: 0.324468085106383,
      children: [
        { type: "leaf", panels: ["screen"], fraction: 0.5941955798479087 },
        { type: "leaf", panels: ["cpu"], fraction: 0.4058044201520912 },
      ],
    },
  ],
};

/** Everything visible at once — four columns of panels. */
const BUSY_LAYOUT: SerializedNode = {
  type: "branch",
  orientation: "row",
  children: [
    { type: "leaf", panels: ["disasm"], fraction: 0.25142857142857145 },
    {
      type: "branch",
      orientation: "column",
      fraction: 0.2857142857142857,
      children: [
        { type: "leaf", panels: ["cpu"], fraction: 0.4717749920785805 },
        { type: "leaf", panels: ["screen"], fraction: 0.5282250079214195 },
      ],
    },
    {
      type: "branch",
      orientation: "column",
      fraction: 0.14285714285714285,
      children: [
        { type: "leaf", panels: ["ppu"], fraction: 0.41816678548795944 },
        { type: "leaf", panels: ["apu"], fraction: 0.5818332145120405 },
      ],
    },
    {
      type: "branch",
      orientation: "column",
      fraction: 0.32,
      children: [
        { type: "leaf", panels: ["io"], fraction: 0.5609107652091255 },
        { type: "leaf", panels: ["memory"], fraction: 0.4390892347908745 },
      ],
    },
  ],
};

/** Just the game — a clean player view. */
const MINIMAL_LAYOUT: SerializedNode = {
  type: "leaf",
  panels: ["screen"],
};

export const BUILTIN_PRESETS: Preset[] = [
  { name: "Default", spec: DEFAULT_LAYOUT },
  { name: "Busy", spec: BUSY_LAYOUT },
  { name: "Minimal", spec: MINIMAL_LAYOUT },
];

// ─── Named-config store ─────────────────────────────────────────────────────

function readStore(): Record<string, SerializedDock> {
  try {
    const raw = localStorage.getItem(STORE_KEY);
    const parsed = raw ? JSON.parse(raw) : {};
    return parsed && typeof parsed === "object" ? parsed : {};
  } catch {
    return {};
  }
}

function writeStore(store: Record<string, SerializedDock>): void {
  try {
    localStorage.setItem(STORE_KEY, JSON.stringify(store));
  } catch {
    /* storage unavailable — best-effort */
  }
}

/** User-saved config names, alphabetized. */
export function listSaved(): string[] {
  return Object.keys(readStore()).sort((a, b) => a.localeCompare(b));
}

export function getSaved(name: string): SerializedDock | null {
  return readStore()[name] ?? null;
}

export function saveLayout(name: string, dock: SerializedDock): void {
  const store = readStore();
  store[name] = dock;
  writeStore(store);
}

export function deleteLayout(name: string): void {
  const store = readStore();
  delete store[name];
  writeStore(store);
}

// ─── Reconciliation ─────────────────────────────────────────────────────────

/**
 * Drop panel ids not in `known` from a serialized dock, so loading an old or
 * imported config can't reference panels that no longer exist. Empty leaves
 * and single-child branches are cleaned up by DockModel.deserialize's
 * normalization pass afterward.
 */
export function reconcile(dock: SerializedDock, known: Set<string>): SerializedDock {
  const clone: SerializedDock = JSON.parse(JSON.stringify(dock));

  const pruneLeaf = (leaf: SerializedLeaf) => {
    leaf.panels = leaf.panels.filter((p) => known.has(p));
    if (leaf.active && !leaf.panels.includes(leaf.active)) {
      leaf.active = leaf.panels[0];
    }
  };
  const walk = (node: SerializedNode | null | undefined) => {
    if (!node) return;
    if (node.type === "leaf") pruneLeaf(node);
    else node.children.forEach(walk);
  };

  walk(clone.root);
  clone.floating?.forEach((f) => pruneLeaf(f.leaf));
  return clone;
}

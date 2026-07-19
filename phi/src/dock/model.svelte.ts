// ─── DockModel: Reactive Dock Layout Tree ───────────────────────────────────
// Svelte-5-native replacement for the imperative Splitview/Gridview port.
//
// Design: fractions in $state are the single source of truth for sizing;
// pixel rects are derived (see solve.ts). Panel lists and active tabs live
// in the tree leaves — there is no separate groups record to keep in sync.
// Every structural operation ends with a normalize pass, so the tree always
// satisfies the invariants:
//   • no branch has fewer than 2 children
//   • no branch contains a child branch with the same orientation
//   • sibling fractions sum to 1
//   • no leaf has an empty panel list
//   • every leaf's activePanel is a member of its panels

export type Orientation = "row" | "column";
export type Direction = "left" | "right" | "up" | "down";

export function directionOrientation(direction: Direction): Orientation {
  return direction === "left" || direction === "right" ? "row" : "column";
}

// ─── Nodes ──────────────────────────────────────────────────────────────────

export class DockLeaf {
  readonly kind = "leaf" as const;
  readonly id: string;
  fraction = $state(1);
  panels = $state<string[]>([]);
  activePanel = $state("");

  constructor(id: string, panels: string[], activePanel?: string, fraction = 1) {
    this.id = id;
    this.panels = [...panels];
    this.activePanel =
      activePanel !== undefined && panels.includes(activePanel)
        ? activePanel
        : (panels[0] ?? "");
    this.fraction = fraction;
  }
}

export class DockBranch {
  readonly kind = "branch" as const;
  readonly orientation: Orientation;
  fraction = $state(1);
  children = $state<DockNode[]>([]);

  constructor(orientation: Orientation, children: DockNode[] = [], fraction = 1) {
    this.orientation = orientation;
    this.children = children;
    this.fraction = fraction;
  }
}

export type DockNode = DockLeaf | DockBranch;

/** A floating window's placement, relative to the dock container. */
export interface FloatRect {
  left: number;
  top: number;
  width: number;
  height: number;
}

/**
 * A group detached from the tree, rendered as a draggable window above the
 * dock. Reuses DockLeaf, so every panel operation (activate, close,
 * reorder, move, drag-out) works on floating groups unchanged.
 * Stacking order = position in DockModel.floating (last = topmost).
 */
export class FloatingGroup {
  readonly leaf: DockLeaf;
  rect = $state<FloatRect>({ left: 48, top: 48, width: 360, height: 260 });

  constructor(leaf: DockLeaf, rect?: FloatRect) {
    this.leaf = leaf;
    if (rect) this.rect = { ...rect };
  }
}

/**
 * Panel metadata registry entry. Panels are still identified by plain
 * string ids everywhere in the model; definitions add presentation and
 * constraint metadata resolved by DockLayout.
 */
export interface PanelDef {
  id: string;
  /** Tab label. Defaults to the id. */
  title?: string;
  /** Whether the tab shows a close button. Default true. UI-level only —
   *  programmatic closePanel always works. */
  closable?: boolean;
  /** Per-panel minimum content size (px); raises the leaf's minimum. */
  minWidth?: number;
  minHeight?: number;
}

// ─── Serialization Types ────────────────────────────────────────────────────
// Also the hand-written layout spec format: `id`, `fraction`, and `active`
// are optional so consumers can describe initial layouts tersely.

export interface SerializedLeaf {
  type: "leaf";
  id?: string;
  fraction?: number;
  panels: string[];
  active?: string;
}

export interface SerializedBranch {
  type: "branch";
  fraction?: number;
  orientation: Orientation;
  children: SerializedNode[];
}

export type SerializedNode = SerializedLeaf | SerializedBranch;

export interface SerializedFloating {
  rect: FloatRect;
  leaf: SerializedLeaf;
}

export interface SerializedDock {
  v: 1;
  maximized?: string | null;
  active?: string | null;
  floating?: SerializedFloating[];
  root: SerializedNode | null;
}

// ─── DockModel ──────────────────────────────────────────────────────────────

export class DockModel {
  root = $state<DockNode | null>(null);
  maximizedLeafId = $state<string | null>(null);
  /** Floating windows, bottom to top. */
  floating = $state<FloatingGroup[]>([]);
  /** The group that owns focus-sensitive actions (keyboard, openPanel). */
  activeLeafId = $state<string | null>(null);
  #idCounter = 0;

  /** Build from an optional layout spec (see SerializedNode). */
  constructor(spec?: SerializedNode | null) {
    if (spec) {
      this.root = this.#buildNode(spec, new Set());
      this.#normalize();
    }
  }

  // ─── Queries ──────────────────────────────────────────────────────────

  get isEmpty(): boolean {
    return this.root === null && this.floating.length === 0;
  }

  /** Docked (tree) leaves in document order. Floating leaves excluded. */
  leaves(): DockLeaf[] {
    const out: DockLeaf[] = [];
    const walk = (node: DockNode | null) => {
      if (!node) return;
      if (node.kind === "leaf") out.push(node);
      else for (const child of node.children) walk(child);
    };
    walk(this.root);
    return out;
  }

  /** Every leaf — docked first, then floating (bottom to top). */
  allLeaves(): DockLeaf[] {
    return [...this.leaves(), ...this.floating.map((f) => f.leaf)];
  }

  /** Find a leaf anywhere — docked or floating. */
  findLeaf(id: string): DockLeaf | null {
    return this.allLeaves().find((l) => l.id === id) ?? null;
  }

  /** The leaf currently containing a panel (docked or floating), if any. */
  findPanel(panelId: string): DockLeaf | null {
    return this.allLeaves().find((l) => l.panels.includes(panelId)) ?? null;
  }

  /** The floating group owning a leaf, if it is floating. */
  floatingOf(leafId: string): FloatingGroup | null {
    return this.floating.find((f) => f.leaf.id === leafId) ?? null;
  }

  /** The active leaf object, if any. */
  get activeLeaf(): DockLeaf | null {
    return this.activeLeafId ? this.findLeaf(this.activeLeafId) : null;
  }

  setActiveLeaf(id: string): void {
    if (this.findLeaf(id)) this.activeLeafId = id;
  }

  #treeLeaf(id: string): DockLeaf | null {
    return this.leaves().find((l) => l.id === id) ?? null;
  }

  // ─── Panel operations ─────────────────────────────────────────────────

  activatePanel(leafId: string, panelId: string): void {
    const leaf = this.findLeaf(leafId);
    if (leaf && leaf.panels.includes(panelId)) {
      leaf.activePanel = panelId;
      this.activeLeafId = leaf.id;
    }
  }

  /**
   * Bring a panel to front wherever it lives, or open it. Resolution
   * order for a closed panel's destination: explicit `near` (a panel or
   * leaf id) → the active leaf → the first docked leaf → a new root leaf.
   * With `direction`, splits the (docked) destination instead of tabifying.
   */
  openPanel(
    panelId: string,
    opts?: { near?: string; direction?: Direction },
  ): DockLeaf | null {
    const existing = this.findPanel(panelId);
    if (existing) {
      existing.activePanel = panelId;
      this.activeLeafId = existing.id;
      return existing;
    }

    let target: DockLeaf | null = null;
    if (opts?.near) {
      target = this.findPanel(opts.near) ?? this.findLeaf(opts.near);
    }
    target ??= this.activeLeaf ?? this.leaves()[0] ?? null;

    if (target && opts?.direction && this.#treeLeaf(target.id)) {
      const created = this.splitLeaf(target.id, opts.direction, [panelId]);
      if (created) {
        this.activeLeafId = created.id;
        return created;
      }
    }
    if (target) {
      target.panels.push(panelId);
      target.activePanel = panelId;
      this.activeLeafId = target.id;
      return target;
    }

    // Nothing to attach to — become the root.
    const leaf = new DockLeaf(this.#newLeafId(), [panelId]);
    this.root = leaf;
    this.activeLeafId = leaf.id;
    return leaf;
  }

  /** Close a panel wherever it lives. */
  closePanelById(panelId: string): void {
    const leaf = this.findPanel(panelId);
    if (leaf) this.closePanel(leaf.id, panelId);
  }

  /**
   * Move a panel within a leaf's tab strip.
   * @param toIndex The panel's position in the final array.
   */
  reorderPanel(leafId: string, fromIndex: number, toIndex: number): void {
    const leaf = this.findLeaf(leafId);
    if (!leaf) return;
    const n = leaf.panels.length;
    if (fromIndex < 0 || fromIndex >= n) return;
    const clampedTo = Math.max(0, Math.min(n - 1, toIndex));
    if (fromIndex === clampedTo) return;
    const [panel] = leaf.panels.splice(fromIndex, 1);
    leaf.panels.splice(clampedTo, 0, panel);
  }

  /**
   * Move a panel to another leaf (tabify). Removes the source leaf if it
   * empties. Activates the panel in its destination.
   * @param index Insertion position in the target's final panel array.
   */
  movePanel(panelId: string, fromLeafId: string, toLeafId: string, index?: number): void {
    const from = this.findLeaf(fromLeafId);
    const to = this.findLeaf(toLeafId);
    if (!from || !to || !from.panels.includes(panelId)) return;

    if (from === to) {
      const cur = from.panels.indexOf(panelId);
      from.panels.splice(cur, 1);
      const at = index === undefined
        ? from.panels.length
        : Math.max(0, Math.min(from.panels.length, index));
      from.panels.splice(at, 0, panelId);
      from.activePanel = panelId;
      return;
    }

    this.#detachPanel(from, panelId);
    const at = index === undefined
      ? to.panels.length
      : Math.max(0, Math.min(to.panels.length, index));
    to.panels.splice(at, 0, panelId);
    to.activePanel = panelId;
    this.activeLeafId = to.id;

    if (from.panels.length === 0) {
      this.removeLeaf(from.id);
    }
  }

  /** Close (remove) a panel. Removes the leaf if it empties. */
  closePanel(leafId: string, panelId: string): void {
    const leaf = this.findLeaf(leafId);
    if (!leaf || !leaf.panels.includes(panelId)) return;
    this.#detachPanel(leaf, panelId);
    if (leaf.panels.length === 0) {
      this.removeLeaf(leaf.id);
    }
  }

  // ─── Structural operations ────────────────────────────────────────────

  /**
   * Split a (docked) leaf, creating a new leaf holding `panels` on the
   * given side. The target's space is divided 50/50 between them.
   */
  splitLeaf(
    targetLeafId: string,
    direction: Direction,
    panels: string[],
    opts?: { id?: string },
  ): DockLeaf | null {
    const target = this.#treeLeaf(targetLeafId);
    if (!target || panels.length === 0) return null;
    const newLeaf = new DockLeaf(opts?.id ?? this.#newLeafId(), panels);
    this.#insertSplit(target, direction, newLeaf);
    this.#normalize();
    return newLeaf;
  }

  /**
   * Take a panel out of one leaf and split another leaf with it — the
   * edge-drop operation. No-op when dragging a leaf's sole panel onto its
   * own edge (the result would collapse back to the starting layout).
   */
  splitWithPanel(
    panelId: string,
    fromLeafId: string,
    targetLeafId: string,
    direction: Direction,
  ): DockLeaf | null {
    const from = this.findLeaf(fromLeafId);
    const target = this.#treeLeaf(targetLeafId);
    if (!from || !target || !from.panels.includes(panelId)) return null;
    if (from === target && from.panels.length === 1) return null;

    this.#detachPanel(from, panelId);
    const newLeaf = this.splitLeaf(target.id, direction, [panelId]);
    if (from.panels.length === 0) {
      this.removeLeaf(from.id);
    }
    if (newLeaf) this.activeLeafId = newLeaf.id;
    return newLeaf;
  }

  /**
   * Split at the container edge: the new leaf spans the full extent of
   * that side, regardless of the existing layout's structure.
   */
  splitRoot(direction: Direction, panels: string[], opts?: { id?: string }): DockLeaf | null {
    if (panels.length === 0) return null;
    const newLeaf = new DockLeaf(opts?.id ?? this.#newLeafId(), panels);
    this.#insertAtRoot(newLeaf, direction);
    this.#normalize();
    this.activeLeafId = newLeaf.id;
    return newLeaf;
  }

  /** Take a panel out of a leaf and split at the container edge with it. */
  splitRootWithPanel(
    panelId: string,
    fromLeafId: string,
    direction: Direction,
  ): DockLeaf | null {
    const from = this.findLeaf(fromLeafId);
    if (!from || !from.panels.includes(panelId)) return null;
    // Sole panel of the only docked leaf: any root split recreates the
    // same layout — no-op.
    if (from === this.root && from.panels.length === 1) return null;

    this.#detachPanel(from, panelId);
    const newLeaf = this.splitRoot(direction, [panelId]);
    if (from.panels.length === 0) {
      this.removeLeaf(from.id);
    }
    return newLeaf;
  }

  /** Remove a leaf — docked (tree collapses) or floating (window closes). */
  removeLeaf(id: string): void {
    const floating = this.floatingOf(id);
    if (floating) {
      this.floating.splice(this.floating.indexOf(floating), 1);
      this.#normalize();
      return;
    }

    const leaf = this.#treeLeaf(id);
    if (!leaf) return;

    if (this.root === leaf) {
      this.root = null;
    } else {
      const parent = this.#findParent(leaf);
      if (!parent) return;
      parent.children.splice(parent.children.indexOf(leaf), 1);
    }
    this.#normalize();
  }

  // ─── Floating ─────────────────────────────────────────────────────────

  /**
   * Detach a docked group into a floating window (all its panels).
   * Returns the floating group, or null if the leaf is not docked.
   */
  floatLeaf(leafId: string, rect?: FloatRect): FloatingGroup | null {
    const leaf = this.#treeLeaf(leafId);
    if (!leaf) return null;

    if (this.root === leaf) {
      this.root = null;
    } else {
      const parent = this.#findParent(leaf);
      if (!parent) return null;
      parent.children.splice(parent.children.indexOf(leaf), 1);
    }
    const group = new FloatingGroup(leaf, rect ?? this.#nextFloatRect());
    this.floating.push(group);
    this.#normalize();
    this.activeLeafId = leaf.id;
    return group;
  }

  /**
   * Detach a single panel into its own floating window. When the panel is
   * already the sole occupant of a floating group, the window just moves.
   */
  floatPanel(panelId: string, fromLeafId: string, rect?: FloatRect): FloatingGroup | null {
    const from = this.findLeaf(fromLeafId);
    if (!from || !from.panels.includes(panelId)) return null;

    const existing = this.floatingOf(fromLeafId);
    if (existing && from.panels.length === 1) {
      if (rect) existing.rect = { ...rect };
      this.bringToFront(fromLeafId);
      return existing;
    }

    this.#detachPanel(from, panelId);
    const group = new FloatingGroup(
      new DockLeaf(this.#newLeafId(), [panelId]),
      rect ?? this.#nextFloatRect(),
    );
    this.floating.push(group);
    if (from.panels.length === 0) {
      this.removeLeaf(from.id);
    }
    this.activeLeafId = group.leaf.id;
    return group;
  }

  /** Re-dock a floating group at a container edge. */
  unfloatLeaf(leafId: string, direction: Direction = "right"): DockLeaf | null {
    const group = this.floatingOf(leafId);
    if (!group) return null;
    this.floating.splice(this.floating.indexOf(group), 1);
    this.#insertAtRoot(group.leaf, direction);
    this.#normalize();
    this.activeLeafId = group.leaf.id;
    return group.leaf;
  }

  /** Raise a floating window to the top of the stack. */
  bringToFront(leafId: string): void {
    const group = this.floatingOf(leafId);
    if (!group) return;
    const idx = this.floating.indexOf(group);
    if (idx !== this.floating.length - 1) {
      this.floating.splice(idx, 1);
      this.floating.push(group);
    }
    this.activeLeafId = leafId;
  }

  // ─── Maximize ─────────────────────────────────────────────────────────

  maximizeLeaf(id: string): void {
    if (this.#treeLeaf(id)) this.maximizedLeafId = id;
  }

  restore(): void {
    this.maximizedLeafId = null;
  }

  toggleMaximize(id: string): void {
    if (this.maximizedLeafId === id) this.restore();
    else this.maximizeLeaf(id);
  }

  // ─── Serialization ────────────────────────────────────────────────────

  serialize(): SerializedDock {
    const serLeaf = (leaf: DockLeaf): SerializedLeaf => ({
      type: "leaf",
      id: leaf.id,
      fraction: leaf.fraction,
      panels: [...leaf.panels],
      active: leaf.activePanel,
    });
    const ser = (node: DockNode): SerializedNode =>
      node.kind === "leaf"
        ? serLeaf(node)
        : {
            type: "branch",
            fraction: node.fraction,
            orientation: node.orientation,
            children: node.children.map(ser),
          };
    return {
      v: 1,
      maximized: this.maximizedLeafId,
      active: this.activeLeafId,
      floating: this.floating.map((f) => ({
        rect: { ...f.rect },
        leaf: serLeaf(f.leaf),
      })),
      root: this.root ? ser(this.root) : null,
    };
  }

  /**
   * Restore a layout persisted by DockLayout's `persistKey`, falling back
   * to the given spec when nothing (or something malformed) is stored.
   */
  static fromStorage(key: string, fallback?: SerializedNode | null): DockModel {
    try {
      const raw = globalThis.localStorage?.getItem(key);
      if (raw) return DockModel.deserialize(JSON.parse(raw));
    } catch {
      /* fall through to the fallback layout */
    }
    return new DockModel(fallback ?? null);
  }

  /** Rebuild from serialized state. Throws on malformed input. */
  static deserialize(data: unknown): DockModel {
    if (typeof data !== "object" || data === null || (data as SerializedDock).v !== 1) {
      throw new Error("DockModel.deserialize: unsupported format");
    }
    const doc = data as SerializedDock;
    const model = new DockModel(doc.root ?? null);

    if (Array.isArray(doc.floating)) {
      const seen = new Set(model.leaves().map((l) => l.id));
      for (const entry of doc.floating) {
        if (entry?.leaf?.type !== "leaf") {
          throw new Error("DockModel.deserialize: floating entry is not a leaf");
        }
        const leaf = model.#buildNode(entry.leaf, seen);
        if (leaf && leaf.kind === "leaf") {
          const r = entry.rect;
          const rect =
            r && [r.left, r.top, r.width, r.height].every((v) => typeof v === "number")
              ? r
              : undefined;
          model.floating.push(new FloatingGroup(leaf, rect));
        }
      }
      model.#normalize();
    }

    if (doc.maximized && model.#treeLeaf(doc.maximized)) {
      model.maximizedLeafId = doc.maximized;
    }
    if (doc.active && model.findLeaf(doc.active)) {
      model.activeLeafId = doc.active;
    }
    return model;
  }

  // ─── Internals ────────────────────────────────────────────────────────

  /** Cascaded default placement for new floating windows. */
  #nextFloatRect(): FloatRect {
    const offset = 48 + (this.floating.length % 6) * 28;
    return { left: offset, top: offset, width: 360, height: 260 };
  }

  #newLeafId(): string {
    let id: string;
    do {
      id = `leaf-${++this.#idCounter}`;
    } while (this.findLeaf(id) !== null);
    return id;
  }

  #findParent(target: DockNode): DockBranch | null {
    const walk = (node: DockNode | null): DockBranch | null => {
      if (!node || node.kind === "leaf") return null;
      for (const child of node.children) {
        if (child === target) return node;
        const found = walk(child);
        if (found) return found;
      }
      return null;
    };
    return walk(this.root);
  }

  /** Remove a panel from a leaf, repairing activePanel. Does not collapse. */
  #detachPanel(leaf: DockLeaf, panelId: string): void {
    const idx = leaf.panels.indexOf(panelId);
    if (idx === -1) return;
    leaf.panels.splice(idx, 1);
    if (leaf.activePanel === panelId) {
      leaf.activePanel = leaf.panels[Math.min(idx, leaf.panels.length - 1)] ?? "";
    }
  }

  #insertSplit(target: DockLeaf, direction: Direction, newLeaf: DockLeaf): void {
    const orientation = directionOrientation(direction);
    const before = direction === "left" || direction === "up";
    const parent = this.#findParent(target);

    if (parent && parent.orientation === orientation) {
      // Same-axis: the new leaf becomes a sibling, splitting the target's share.
      const idx = parent.children.indexOf(target);
      newLeaf.fraction = target.fraction / 2;
      target.fraction = target.fraction / 2;
      parent.children.splice(before ? idx : idx + 1, 0, newLeaf);
    } else {
      // Orthogonal (or root leaf): wrap the target in a new branch.
      const branch = new DockBranch(orientation, [], target.fraction);
      target.fraction = 0.5;
      newLeaf.fraction = 0.5;
      branch.children = before ? [newLeaf, target] : [target, newLeaf];
      if (parent) {
        parent.children[parent.children.indexOf(target)] = branch;
      } else {
        this.root = branch;
      }
    }
  }

  /**
   * Insert a leaf at the container edge. Joins the root branch as a
   * sibling when orientations match (taking a 1/(n+1) share); otherwise
   * wraps the whole layout, giving the new leaf a quarter of the axis.
   */
  #insertAtRoot(leaf: DockLeaf, direction: Direction): void {
    const orientation = directionOrientation(direction);
    const before = direction === "left" || direction === "up";

    if (this.root === null) {
      leaf.fraction = 1;
      this.root = leaf;
      return;
    }

    if (this.root.kind === "branch" && this.root.orientation === orientation) {
      const n = this.root.children.length;
      leaf.fraction = 1 / n; // ≈ 1/(n+1) share after renormalization
      this.root.children.splice(before ? 0 : n, 0, leaf);
      return;
    }

    const branch = new DockBranch(orientation, [], 1);
    leaf.fraction = 0.25;
    this.root.fraction = 0.75;
    branch.children = before ? [leaf, this.root] : [this.root, leaf];
    this.root = branch;
  }

  /** Restore all tree + floating invariants after a structural mutation. */
  #normalize(): void {
    this.root = this.#normalizeNode(this.root);
    if (this.root) this.#renormalizeFractions(this.root);

    // Floating: repair active tabs, drop emptied windows.
    for (const group of this.floating) {
      const leaf = group.leaf;
      if (leaf.panels.length > 0 && !leaf.panels.includes(leaf.activePanel)) {
        leaf.activePanel = leaf.panels[0];
      }
    }
    this.floating = this.floating.filter((f) => f.leaf.panels.length > 0);

    if (this.maximizedLeafId && !this.#treeLeaf(this.maximizedLeafId)) {
      this.maximizedLeafId = null;
    }
    if (this.activeLeafId && !this.findLeaf(this.activeLeafId)) {
      this.activeLeafId =
        this.leaves()[0]?.id ??
        this.floating[this.floating.length - 1]?.leaf.id ??
        null;
    }
  }

  #normalizeNode(node: DockNode | null): DockNode | null {
    if (!node) return null;
    if (node.kind === "leaf") {
      if (node.panels.length === 0) return null;
      if (!node.panels.includes(node.activePanel)) {
        node.activePanel = node.panels[0];
      }
      return node;
    }

    const kids: DockNode[] = [];
    for (const child of node.children) {
      const normalized = this.#normalizeNode(child);
      if (!normalized) continue;
      if (normalized.kind === "branch" && normalized.orientation === node.orientation) {
        // Merge same-orientation child branch: splice grandchildren in,
        // scaling their fractions into the child's share.
        const sum = normalized.children.reduce((s, g) => s + g.fraction, 0) || 1;
        for (const grandchild of normalized.children) {
          grandchild.fraction = normalized.fraction * (grandchild.fraction / sum);
          kids.push(grandchild);
        }
      } else {
        kids.push(normalized);
      }
    }

    if (kids.length === 0) return null;
    if (kids.length === 1) {
      kids[0].fraction = node.fraction;
      return kids[0];
    }
    node.children = kids;
    return node;
  }

  #renormalizeFractions(node: DockNode): void {
    if (node.kind === "leaf") return;
    const sum = node.children.reduce((s, c) => s + c.fraction, 0);
    for (const child of node.children) {
      child.fraction = sum > 0 ? child.fraction / sum : 1 / node.children.length;
      this.#renormalizeFractions(child);
    }
  }

  #buildNode(spec: SerializedNode, seenIds: Set<string>): DockNode | null {
    if (spec.type === "leaf") {
      if (!Array.isArray(spec.panels)) {
        throw new Error("DockModel: leaf spec missing panels array");
      }
      if (spec.id !== undefined && seenIds.has(spec.id)) {
        throw new Error(`DockModel: duplicate leaf id "${spec.id}"`);
      }
      let id = spec.id;
      if (id === undefined) {
        do {
          id = `leaf-${++this.#idCounter}`;
        } while (seenIds.has(id));
      }
      seenIds.add(id);
      return new DockLeaf(id, spec.panels, spec.active, spec.fraction ?? 1);
    }
    if (spec.type === "branch") {
      if (!Array.isArray(spec.children)) {
        throw new Error("DockModel: branch spec missing children array");
      }
      const children = spec.children
        .map((c) => this.#buildNode(c, seenIds))
        .filter((c): c is DockNode => c !== null);
      return new DockBranch(spec.orientation, children, spec.fraction ?? 1);
    }
    throw new Error("DockModel: unknown node spec type");
  }
}

// ─── Branch-flow arrows ─────────────────────────────────────────────────────
// Pure geometry for the disassembly gutter: for each visible branch/call
// whose target is also visible, an arrow from the source row to the target
// row, with non-overlapping arrows nested into lanes (interval-graph
// coloring). The panel maps row indices to measured pixel positions and
// draws the SVG.

import type { DisasmLine } from "../types";
import { refTarget } from "./hardware.js";

export interface BranchArrow {
  fromIdx: number;
  toIdx: number;
  /** Nesting lane (0 = innermost/closest to the code). */
  lane: number;
  dir: "up" | "down" | "self";
}

/**
 * Arrows for branches whose target is an instruction start within `lines`.
 * Lanes are assigned shortest-span-first so tightly-nested loops sit inside
 * their enclosing branches.
 */
export function computeArrows(lines: DisasmLine[]): BranchArrow[] {
  const addrToIdx = new Map<number, number>();
  lines.forEach((l, i) => addrToIdx.set(l.addr, i));

  const raw: Omit<BranchArrow, "lane">[] = [];
  lines.forEach((line, fromIdx) => {
    const target = refTarget(line);
    if (target === undefined) return;
    const toIdx = addrToIdx.get(target);
    if (toIdx === undefined) return; // target off-window
    raw.push({
      fromIdx,
      toIdx,
      dir: toIdx === fromIdx ? "self" : toIdx < fromIdx ? "up" : "down",
    });
  });

  // Assign lanes: shortest span first, lowest free lane among overlaps.
  raw.sort((a, b) => span(a) - span(b));
  const assigned: BranchArrow[] = [];
  for (const arrow of raw) {
    const used = new Set(
      assigned.filter((o) => overlaps(arrow, o)).map((o) => o.lane),
    );
    let lane = 0;
    while (used.has(lane)) lane++;
    assigned.push({ ...arrow, lane });
  }
  return assigned;
}

/** Max lane index in use (for sizing the gutter), or -1 if no arrows. */
export function maxLane(arrows: BranchArrow[]): number {
  return arrows.reduce((m, a) => Math.max(m, a.lane), -1);
}

function span(a: { fromIdx: number; toIdx: number }): number {
  return Math.abs(a.toIdx - a.fromIdx);
}

/** Two arrows overlap if their inclusive row spans intersect. */
function overlaps(
  a: { fromIdx: number; toIdx: number },
  b: { fromIdx: number; toIdx: number },
): boolean {
  const [a0, a1] = [Math.min(a.fromIdx, a.toIdx), Math.max(a.fromIdx, a.toIdx)];
  const [b0, b1] = [Math.min(b.fromIdx, b.toIdx), Math.max(b.fromIdx, b.toIdx)];
  return a0 <= b1 && b0 <= a1;
}

/**
 * Addresses that are in-window branch/call targets landing on an
 * instruction start — the code locations worth a `loc_` label.
 */
export function collectTargets(lines: DisasmLine[]): Set<number> {
  const starts = new Set(lines.map((l) => l.addr));
  const targets = new Set<number>();
  for (const line of lines) {
    const t = refTarget(line);
    if (t !== undefined && starts.has(t)) targets.add(t);
  }
  return targets;
}

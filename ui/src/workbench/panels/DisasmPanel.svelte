<script lang="ts">
  /**
   * Disassembly panel (gui_spec §3) — live instruction stream from the
   * sm83-isa decoder via the backend's `disassemble` export.
   *
   * Columns: breakpoint gutter · branch-arrow lane · address · raw bytes ·
   * mnemonic · ; comment. Label rows (`VBlank:`, `loc_XXXX:`) head their
   * targets. Follows PC (with context above) while running/stepping;
   * scrolling / the address box / a jump switch to a free view (⌖ PC
   * re-follows). Click a JP/JR/CALL/RST target to navigate; Back / Alt+←
   * returns. Right-click for per-instruction actions incl. Run-to-here.
   * Gutter dots toggle persisted breakpoints — not yet enforced.
   */
  import { ContextMenu, type ContextMenuItem } from "@gestalt/phi";
  import type { DisasmLine } from "../../types";
  import { hex16, hex8, type EmuController } from "../emu.svelte.js";
  import { windowAround, windowFrom, prevInstr } from "../disasm.js";
  import { commentFor, refTarget, operandRef, symbolFor } from "../hardware.js";
  import { computeArrows, collectTargets, type BranchArrow } from "../arrows.js";

  let { emu }: { emu: EmuController } = $props();

  const BEFORE = 8;
  const AFTER = 30;

  // Arrow gutter geometry.
  const LANE_W = 7;
  const MAX_LANES = 4;
  const ZONE_W = 30; // px reserved for the arrow lanes (fixed → stable columns)

  let lines = $state<DisasmLine[]>([]);
  let follow = $state(true);
  let topAddr = $state(0);
  let addrInput = $state("");
  let addrInputEl = $state<HTMLInputElement>();
  let history = $state<number[]>([]);
  let operandValues = $state(new Map<number, number>());
  let rowsEl = $state<HTMLDivElement>();
  let rowCenters = $state(new Map<number, number>());

  const disasmFn = (a: number, c: number) => emu.disasm(a, c);

  async function refresh() {
    if (!emu.romLoaded || !emu.canDisasm) return;
    let next: DisasmLine[];
    if (follow) {
      const pc = emu.cpu?.pc ?? 0;
      next = (await windowAround(disasmFn, pc, BEFORE, AFTER)).lines;
    } else {
      next = await windowFrom(disasmFn, topAddr, BEFORE + AFTER + 1);
    }
    lines = next;
    // Live values for the memory operands in view.
    const refs = new Set<number>();
    for (const l of next) {
      const r = operandRef(l);
      if (r !== undefined) refs.add(r);
    }
    const vals = new Map<number, number>();
    for (const a of refs) {
      const [v] = await emu.read(a, 1);
      if (v !== undefined) vals.set(a, v);
    }
    operandValues = vals;
  }

  // Heartbeat: follow PC (~10 Hz while running, immediate on step/pause).
  $effect(() => {
    const _ = emu.frameCount;
    void follow;
    void topAddr;
    if (!emu.romLoaded || !emu.canDisasm) return;
    if (emu.running && _ % 6 !== 0) return;
    void refresh();
  });

  // ─── Derived annotations ───────────────────────────────────────────────

  const arrows = $derived(computeArrows(lines));
  const targets = $derived(collectTargets(lines));

  function labelFor(addr: number): string | undefined {
    const sym = symbolFor(addr);
    if (sym) return sym;
    if (targets.has(addr)) return `loc_${hex16(addr)}`;
    return undefined;
  }

  function comment(row: DisasmLine): string | undefined {
    const base = commentFor(row);
    const ref = operandRef(row);
    if (ref !== undefined) {
      const v = operandValues.get(ref);
      const val = v !== undefined ? ` = $${hex8(v)}` : "";
      return `${base ?? `($${hex16(ref)})`}${val}`;
    }
    return base;
  }

  // ─── Branch-arrow SVG paths (measured row positions) ───────────────────

  // Measure each rendered instruction row's vertical center after layout.
  $effect(() => {
    void lines;
    void arrows;
    if (!rowsEl) return;
    const top = rowsEl.getBoundingClientRect().top;
    const centers = new Map<number, number>();
    for (const el of rowsEl.querySelectorAll<HTMLElement>(".disasm-row")) {
      const addr = Number(el.dataset.addr);
      const r = el.getBoundingClientRect();
      centers.set(addr, r.top - top + r.height / 2);
    }
    rowCenters = centers;
  });

  interface ArrowPath {
    line: string;
    head: string;
    active: boolean;
  }

  const arrowPaths = $derived.by((): ArrowPath[] => {
    const pc = emu.cpu?.pc;
    const out: ArrowPath[] = [];
    for (const a of arrows) {
      const fromAddr = lines[a.fromIdx]?.addr;
      const toAddr = lines[a.toIdx]?.addr;
      const yFrom = fromAddr !== undefined ? rowCenters.get(fromAddr) : undefined;
      const yTo = toAddr !== undefined ? rowCenters.get(toAddr) : undefined;
      if (yFrom === undefined || yTo === undefined) continue;
      const lane = Math.min(a.lane, MAX_LANES - 1);
      const x = ZONE_W - (lane + 1) * LANE_W;
      const right = ZONE_W - 1;
      const [sy, ty] = a.dir === "self" ? [yFrom - 4, yTo + 4] : [yFrom, yTo];
      out.push({
        line: `M ${right} ${sy} H ${x} V ${ty} H ${right}`,
        head: `M ${right} ${ty} l -4 -3 M ${right} ${ty} l -4 3`,
        active: fromAddr === pc || toAddr === pc,
      });
    }
    return out;
  });

  // ─── Navigation ────────────────────────────────────────────────────────

  function goTo(addr: number, pushHistory = true) {
    hidePreview();
    if (pushHistory) history.push(follow ? (emu.cpu?.pc ?? 0) : topAddr);
    follow = false;
    topAddr = addr & 0xffff;
    void refresh();
  }

  function back() {
    const prev = history.pop();
    if (prev === undefined) return;
    follow = false;
    topAddr = prev & 0xffff;
    void refresh();
  }

  function refollow() {
    follow = true;
    addrInput = "";
    void refresh();
  }

  function onAddrSubmit(e: Event) {
    e.preventDefault();
    const parsed = parseInt(addrInput.replace(/^0x/i, "").replace(/^\$/, ""), 16);
    if (!Number.isNaN(parsed)) goTo(parsed);
  }

  // ─── Scroll (free mode) ────────────────────────────────────────────────

  async function scrollBy(instrs: number) {
    if (follow) {
      follow = false;
      topAddr = lines[0]?.addr ?? emu.cpu?.pc ?? 0;
    }
    if (instrs > 0) {
      for (let i = 0; i < instrs; i++) {
        const first = await disasmFn(topAddr, 1);
        topAddr = (topAddr + (first[0]?.len ?? 1)) & 0xffff;
      }
    } else {
      for (let i = 0; i < -instrs; i++) topAddr = await prevInstr(disasmFn, topAddr);
    }
    void refresh();
  }

  function onWheel(e: WheelEvent) {
    e.preventDefault();
    hidePreview();
    scrollBy(e.deltaY > 0 ? 3 : -3);
  }

  function onKeydown(e: KeyboardEvent) {
    let handled = true;
    if (e.key === "ArrowDown") scrollBy(1);
    else if (e.key === "ArrowUp") scrollBy(-1);
    else if (e.key === "PageDown") scrollBy(AFTER);
    else if (e.key === "PageUp") scrollBy(-(BEFORE + AFTER));
    else if (e.altKey && e.key === "ArrowLeft") back();
    else if (e.ctrlKey && e.key.toLowerCase() === "g") addrInputEl?.focus();
    else handled = false;
    if (handled) {
      e.preventDefault();
      e.stopPropagation();
    }
  }

  // ─── Context menu ──────────────────────────────────────────────────────

  let menu = $state<{ x: number; y: number; line: DisasmLine } | null>(null);

  function openMenu(e: MouseEvent, line: DisasmLine) {
    e.preventDefault();
    hidePreview();
    menu = { x: e.clientX, y: e.clientY, line };
  }

  const menuItems = $derived.by((): ContextMenuItem[] => {
    if (!menu) return [];
    const line = menu.line;
    const target = refTarget(line);
    const isBp = emu.breakpoints.has(line.addr);
    const items: ContextMenuItem[] = [];
    if (emu.canStepInstruction) {
      items.push({ id: "runto", label: `Run to $${hex16(line.addr)}` });
    }
    if (target !== undefined) {
      const sym = symbolFor(target);
      items.push({ id: "goto", label: `Go to ${sym ?? "$" + hex16(target)}` });
    }
    const memRef = operandRef(line);
    if (memRef !== undefined) {
      items.push({ id: "mem", label: `Show $${hex16(memRef)} in Memory` });
    }
    items.push({ id: "bp", label: isBp ? "Remove breakpoint" : "Set breakpoint", separator: true });
    items.push({ id: "view", label: "Set view here" });
    items.push({ id: "copy-text", label: "Copy instruction", separator: true });
    items.push({ id: "copy-bytes", label: "Copy bytes" });
    return items;
  });

  function onMenuAction(id: string) {
    const line = menu?.line;
    menu = null;
    if (!line) return;
    switch (id) {
      case "runto":
        void emu.runTo(line.addr);
        break;
      case "goto": {
        const t = refTarget(line);
        if (t !== undefined) goTo(t);
        break;
      }
      case "mem": {
        const r = operandRef(line);
        if (r !== undefined) emu.requestMemoryView(r);
        break;
      }
      case "bp":
        emu.toggleBreakpoint(line.addr);
        break;
      case "view":
        goTo(line.addr);
        break;
      case "copy-text":
        void navigator.clipboard?.writeText(line.text);
        break;
      case "copy-bytes":
        void navigator.clipboard?.writeText(line.bytes);
        break;
    }
  }

  function onTargetClick(line: DisasmLine) {
    const t = refTarget(line);
    if (t !== undefined) goTo(t);
  }

  /** Split JP/JR/CALL text so the trailing $XXXX target renders as a link. */
  function linkPart(line: DisasmLine): { head: string; token: string } | null {
    if (!/^(JP|JR|CALL)\b/.test(line.text)) return null;
    const m = line.text.match(/^(.*?)(\$[0-9A-Fa-f]{4})\s*$/);
    return m ? { head: m[1], token: m[2] } : null;
  }

  /** True for illegal-opcode / raw-data lines (formatted `DB $xx`). */
  function isData(line: DisasmLine): boolean {
    return line.text.startsWith("DB ");
  }

  // ─── Jump-target hover preview ─────────────────────────────────────────
  // Peek at a branch destination without navigating. Deliberately reserved:
  // a 400ms hover delay (no flicker), stale-guarded async, pointer-events
  // none (a peek, not a surface), clamped to the viewport, and dismissed on
  // any scroll / navigation / menu. Shows up to 3 instructions at the target.

  const PREVIEW_DELAY = 400;
  let preview = $state<{ x: number; y: number; label?: string; lines: DisasmLine[] } | null>(null);
  let previewTimer: ReturnType<typeof setTimeout> | undefined;
  let previewSeq = 0;

  function hidePreview() {
    clearTimeout(previewTimer);
    previewSeq++;
    preview = null;
  }

  function onTargetEnter(e: MouseEvent, line: DisasmLine) {
    if (menu) return; // don't compete with an open context menu
    const target = refTarget(line);
    if (target === undefined) return;
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    clearTimeout(previewTimer);
    const seq = ++previewSeq;
    previewTimer = setTimeout(async () => {
      const peek = await emu.disasm(target, 3);
      if (seq !== previewSeq || peek.length === 0) return; // stale / empty
      const w = 232;
      const h = 12 + (symbolFor(target) ? 16 : 0) + peek.length * 17;
      let x = rect.right + 10;
      let y = rect.top - 4;
      if (x + w > window.innerWidth - 6) x = rect.left - w - 10;
      if (y + h > window.innerHeight - 6) y = window.innerHeight - h - 6;
      preview = { x, y: Math.max(6, y), label: symbolFor(target), lines: peek };
    }, PREVIEW_DELAY);
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="disasm-panel" tabindex="-1" onkeydown={onKeydown}>
  {#if emu.canDisasm}
    <div class="disasm-controls">
      <button class="disasm-btn" title="Back (Alt+←)" disabled={history.length === 0} onclick={back}>←</button>
      <form class="disasm-addr" onsubmit={onAddrSubmit}>
        <span class="disasm-addr-prefix">$</span>
        <input
          class="disasm-addr-input"
          bind:this={addrInputEl}
          bind:value={addrInput}
          placeholder={emu.cpu ? hex16(emu.cpu.pc) : "0000"}
          spellcheck="false"
          aria-label="Go to address (Ctrl+G)"
        />
      </form>
      <button class="disasm-btn follow" class:active={follow} title={follow ? "Following PC" : "Follow PC"} onclick={refollow}>⌖ PC</button>
    </div>

    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="disasm-rows" bind:this={rowsEl} onwheel={onWheel}>
      {#if lines.length > 0}
        <svg class="arrow-svg" style="left:16px;width:{ZONE_W}px" aria-hidden="true">
          {#each arrowPaths as p (p.line)}
            <path class="arrow-line" class:active={p.active} d={p.line} />
            <path class="arrow-head" class:active={p.active} d={p.head} />
          {/each}
        </svg>

        {#each lines as row (row.addr)}
          {@const label = labelFor(row.addr)}
          {#if label}
            <div class="disasm-label">{label}:</div>
          {/if}
          {@const isPc = emu.cpu?.pc === row.addr}
          {@const isBp = emu.breakpoints.has(row.addr)}
          {@const link = linkPart(row)}
          {@const cmt = comment(row)}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div class="disasm-row" class:current={isPc} class:data={isData(row)} data-addr={row.addr} oncontextmenu={(e) => openMenu(e, row)}>
            <button
              class="disasm-gutter"
              class:bp={isBp}
              title={isBp ? "Breakpoint (not yet enforced)" : isPc ? "Current instruction" : "Set breakpoint"}
              aria-label="Toggle breakpoint at {hex16(row.addr)}"
              onclick={() => emu.toggleBreakpoint(row.addr)}
            >
              {#if isBp}<span class="bp-dot set"></span>
              {:else if isPc}<span class="pc-caret">▶</span>
              {:else}<span class="bp-dot hint"></span>{/if}
            </button>
            <span class="arrow-zone" style="width:{ZONE_W}px"></span>
            <span class="disasm-addr-col">{hex16(row.addr)}</span>
            <span class="disasm-raw">{row.bytes}</span>
            {#if link}
              <span class="disasm-mnemonic"
                >{link.head}<button
                  class="target"
                  onclick={() => onTargetClick(row)}
                  onmouseenter={(e) => onTargetEnter(e, row)}
                  onmouseleave={hidePreview}
                >{link.token}</button></span>
            {:else}
              <span class="disasm-mnemonic">{row.text}</span>
            {/if}
            {#if cmt}<span class="disasm-comment">; {cmt}</span>{/if}
          </div>
        {/each}
      {:else}
        <div class="empty-hint">Load a ROM to disassemble</div>
      {/if}
    </div>

    <div class="disasm-footer">breakpoints persist but are not yet enforced</div>
  {:else}
    <div class="empty-hint">
      Disassembly needs a backend with the sm83-isa export (rebuild the WASM package).
    </div>
  {/if}

  {#if menu}
    <ContextMenu x={menu.x} y={menu.y} items={menuItems} onaction={onMenuAction} onclose={() => (menu = null)} />
  {/if}

  {#if preview}
    <div class="disasm-preview" style="left:{preview.x}px;top:{preview.y}px">
      {#if preview.label}<div class="preview-label">{preview.label}:</div>{/if}
      {#each preview.lines as l (l.addr)}
        <div class="preview-line"><span class="preview-addr">{hex16(l.addr)}</span> {l.text}</div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .disasm-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    outline: none;
  }

  .disasm-controls {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 8px;
    border-bottom: 1px solid var(--stroke-lo);
  }

  .disasm-addr {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 0 6px;
    background: var(--fill-lo);
    border: 1px solid var(--stroke-mid);
    border-radius: var(--radius-sm);
  }

  .disasm-addr-prefix { font-family: var(--font-mono); font-size: 11px; color: var(--text-faint); }

  .disasm-addr-input {
    width: 44px;
    padding: 3px 0;
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-hi);
    background: none;
    border: none;
    outline: none;
    text-transform: uppercase;
  }

  .disasm-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    min-width: 22px;
    height: 22px;
    padding: 0 6px;
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-subtle);
    background: none;
    border: 1px solid var(--stroke-mid);
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: color 0.1s ease, border-color 0.1s ease;
  }

  .disasm-btn:hover:not(:disabled) { color: var(--text-mid); }
  .disasm-btn:disabled { opacity: 0.35; cursor: default; }
  .disasm-btn.follow { font-size: 10px; margin-left: auto; }
  .disasm-btn.active {
    color: var(--interactive);
    border-color: var(--interactive-ring);
    background: var(--interactive-fill);
  }

  .disasm-rows {
    position: relative;
    flex: 1;
    min-height: 0;
    overflow: hidden;
    padding: 4px 0;
    font-family: var(--font-mono);
    font-size: 11px;
    line-height: 1.7;
  }

  .arrow-svg {
    position: absolute;
    top: 0;
    height: 100%;
    pointer-events: none;
    overflow: visible;
    z-index: 1;
  }

  /* Arrows are structural context: dim/neutral by default, only the branch
     touching the PC lights up in the execution accent. */
  .arrow-line, .arrow-head {
    fill: none;
    stroke: var(--text-faint);
    stroke-width: 1;
    stroke-linejoin: round;
    stroke-linecap: round;
    opacity: 0.4;
  }

  .arrow-line.active, .arrow-head.active {
    stroke: var(--interactive);
    opacity: 1;
  }

  /* Label rows are block headers: bright + bold, but NOT the execution
     accent (green is reserved for the PC). Extra top margin separates
     blocks. */
  .disasm-label {
    margin-top: 6px;
    padding: 3px 8px 2px;
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-mid);
    font-weight: 600;
    /* An extremely faint band (~2% white) — the label sits on a whisper of
       a surface, no boxed-bar hairline edge. */
    background: oklch(1 0 0 / 2%);
  }

  .disasm-row {
    position: relative;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 8px 0 0;
    white-space: nowrap;
    z-index: 2;
  }

  .disasm-row.current {
    background: var(--interactive-fill);
    box-shadow: inset 2px 0 0 var(--interactive);
  }

  .disasm-gutter {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 18px;
    padding: 0;
    border: none;
    background: none;
    cursor: pointer;
    flex-shrink: 0;
    z-index: 3;
  }

  .arrow-zone { flex-shrink: 0; } /* reserves the arrow lane width */

  .bp-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    border: 1px solid var(--text-faint);
    background: transparent;
    transition: opacity 0.1s ease;
  }

  /* Empty gutter by default; a faint "click to break" hint on row hover. */
  .bp-dot.hint { opacity: 0; }
  .disasm-row:hover .bp-dot.hint { opacity: 0.5; }
  .disasm-gutter:hover .bp-dot.hint {
    opacity: 0.9;
    border-color: var(--color-destructive, #e5534b);
  }
  /* A set breakpoint is always solid. */
  .bp-dot.set {
    background: var(--color-destructive, #e5534b);
    border-color: var(--color-destructive, #e5534b);
  }

  .pc-caret {
    color: var(--interactive);
    font-size: 9px;
    line-height: 1;
    pointer-events: none;
  }

  /* Read hierarchy: mnemonic brightest (what you read), address secondary,
     raw bytes dimmest (reference data). */
  .disasm-addr-col { color: var(--text-subtle); flex-shrink: 0; }
  .disasm-raw {
    color: var(--text-faint);
    width: 58px;
    flex-shrink: 0;
    overflow: hidden;
    opacity: 0.8;
  }
  .disasm-mnemonic { color: var(--text-hi); flex-shrink: 0; }

  /* Jump targets read as navigable code locations (neutral + dotted
     underline), not execution — distinct from the green PC. */
  .target {
    padding: 0;
    border: none;
    background: none;
    font: inherit;
    color: var(--text-hi);
    cursor: pointer;
    text-decoration: underline dotted;
    text-decoration-color: var(--text-faint);
    text-underline-offset: 2px;
    transition: text-decoration-color 0.1s ease;
  }
  .target:hover { text-decoration-color: var(--text-hi); }

  /* Comments sit in a soft column right after the mnemonic (not floated to
     the clipping panel edge). */
  .disasm-comment { color: var(--text-faint); }

  /* Data rows (illegal opcodes / raw bytes disassembled as `DB`) recede so
     code stands out — reinforces the read hierarchy. */
  .disasm-row.data .disasm-mnemonic {
    color: var(--text-faint);
    font-style: italic;
  }
  .disasm-row.data .disasm-raw { opacity: 0.55; }

  /* Jump-target hover preview: a small, non-interactive peek. */
  .disasm-preview {
    position: fixed;
    z-index: 200;
    min-width: 150px;
    max-width: 232px;
    padding: 5px 8px;
    background: var(--surface-5, oklch(0.22 0.015 250));
    border: 1px solid var(--stroke-mid);
    border-radius: var(--radius-sm);
    box-shadow: 0 6px 24px oklch(0 0 0 / 45%);
    font-family: var(--font-mono);
    font-size: 11px;
    line-height: 1.55;
    pointer-events: none;
  }

  .preview-label {
    color: var(--text-hi);
    font-weight: 600;
    padding-bottom: 2px;
  }

  .preview-line { color: var(--text-mid); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .preview-addr { color: var(--text-subtle); }

  .disasm-footer {
    padding: 4px 8px;
    font-family: var(--font-sans);
    font-size: 10px;
    color: var(--text-faint);
    border-top: 1px solid var(--stroke-lo);
    flex-shrink: 0;
  }

  .empty-hint {
    padding: 16px 8px;
    font-family: var(--font-sans);
    font-size: 11px;
    color: var(--text-faint);
    text-align: center;
  }
</style>

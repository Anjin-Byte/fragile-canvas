<script lang="ts">
  /**
   * Editor panel — write SM83 assembly and run it on the emulator.
   *
   * A plain `wrap="off"` <textarea> is the text model (native undo/selection/
   * IME); two scroll-synced layers sit around it. Because there's no soft wrap
   * and the line-height is fixed, source line L is always visual row L, so every
   * marker is pure arithmetic (`PAD + (L-1)*LH`) — no per-line measurement.
   *
   *   ┌ gutter ┬──────── text-wrap ────────┐
   *   │ nº ● │ overlay bands (PC / errors)  │  ← translateY(-scrollTop)
   *   │      │ <textarea> (transparent bg)  │  ← the only scroller
   *
   * Live diagnostics (debounced) come from `emu.assembleCurrent`; Run is
   * `emu.runCurrent` (⌘/Ctrl+Enter). Gutter clicks toggle source-line
   * breakpoints, unioned with the disassembly gutter via `enforcedBreakpoints`.
   */
  import { onMount, onDestroy } from "svelte";
  import { Play, Zap, Hammer, AlignJustify, ArrowDownToLine, Redo2, Eye, Crosshair } from "lucide-svelte";
  import { ContextMenu, type ContextMenuItem } from "@gestalt/phi";
  import type { StopReason } from "../../types";
  import type { EmuController } from "../emu.svelte.js";
  import { tokenize, isInstructionLine } from "../highlight.js";
  import { listingByLine, formatAddr, formatBytes } from "../listing.js";
  import { bytesCycles, formatCycles, sumCycles } from "../cycles.js";
  import { inlayHints, formatHintValue } from "../inlay.js";
  import { formatSource } from "../format.js";

  let { emu }: { emu: EmuController } = $props();

  // Fixed metrics — kept in sync with the CSS below so line L ≡ row L.
  const LH = 19; // line height, px
  const PAD = 6; // textarea top padding, px

  let taEl = $state<HTMLTextAreaElement>();
  let scrollTop = $state(0);
  let scrollLeft = $state(0);

  // PC-follow: when on, the view auto-scrolls to keep the running line visible.
  // A manual scroll turns it off (so you can read elsewhere while it runs); the
  // ⌖ button re-follows. `#autoScrolling` marks scrolls WE cause so the onScroll
  // handler doesn't mistake them for the user reaching for the scrollbar.
  let followPc = $state(true);
  let autoScrolling = false;
  let selStart = $state(0);
  let selEnd = $state(0);

  // ─── View options (persisted) ──────────────────────────────────────────
  // Toggling these must keep the layout tidy: the gutter width is DERIVED from
  // the visible columns, so the code reflows (flexbox) when any are hidden.
  const VIEW_KEY = "fc-editor-view";
  interface ViewOpts {
    lineNumbers: boolean;
    addresses: boolean;
    highlight: boolean;
    currentLine: boolean;
    cycles: boolean;
    inlay: boolean;
    bytes: boolean;
  }
  const DEFAULT_VIEW: ViewOpts = {
    lineNumbers: true,
    addresses: true,
    highlight: true,
    currentLine: true,
    cycles: true,
    inlay: true,
    bytes: false,
  };
  const VIEW_ITEMS: { key: keyof ViewOpts; label: string }[] = [
    { key: "lineNumbers", label: "Line numbers" },
    { key: "addresses", label: "Addresses" },
    { key: "highlight", label: "Syntax highlighting" },
    { key: "currentLine", label: "Current line" },
    { key: "cycles", label: "Cycle counts" },
    { key: "inlay", label: "Inlay values" },
    { key: "bytes", label: "Assembled bytes" },
  ];

  let view = $state<ViewOpts>(loadView());
  let viewMenuOpen = $state(false);
  let viewEl = $state<HTMLDivElement>();

  function loadView(): ViewOpts {
    try {
      const raw = localStorage.getItem(VIEW_KEY);
      return raw ? { ...DEFAULT_VIEW, ...(JSON.parse(raw) as Partial<ViewOpts>) } : { ...DEFAULT_VIEW };
    } catch {
      return { ...DEFAULT_VIEW };
    }
  }
  $effect(() => {
    const snapshot = JSON.stringify(view); // reads every field → save on any change
    try {
      localStorage.setItem(VIEW_KEY, snapshot);
    } catch {
      /* best-effort */
    }
  });
  // Close the view menu on an outside click.
  $effect(() => {
    if (!viewMenuOpen) return;
    const close = (e: PointerEvent) => {
      if (viewEl && !viewEl.contains(e.target as Node)) viewMenuOpen = false;
    };
    window.addEventListener("pointerdown", close, true);
    return () => window.removeEventListener("pointerdown", close, true);
  });

  /** Gutter width adapts to the visible columns (the breakpoint dot always
   *  stays); the text-wrap is `flex: 1`, so the code reflows when it changes. */
  const gutterWidth = $derived(
    20 + (view.lineNumbers ? 24 : 0) + (view.addresses ? 56 : 0),
  );

  const STOP_LABEL: Record<StopReason, string> = {
    halt: "Halted",
    breakpoint: "Hit breakpoint",
    leftRange: "Left code range",
    selfLoop: "Reached self-loop",
    budget: "Budget exhausted",
  };

  // ─── Derived view state ────────────────────────────────────────────────
  const lines = $derived(emu.source.split("\n"));
  const asm = $derived(emu.asmResult);
  const diags = $derived(asm?.diagnostics ?? []);
  /** Line-tagged diagnostics (0 = whole-program → problems list only). */
  const lineDiags = $derived(diags.filter((d) => d.line > 0));
  /** Lines that actually emitted bytes — a breakpoint anywhere else won't fire. */
  const emittingLines = $derived(new Set((asm?.sourceMap ?? []).map((s) => s.line)));
  const pcLine = $derived(emu.currentSourceLine);
  const lastRun = $derived(emu.lastRun);
  const errorCount = $derived(diags.length);
  /** The line the caret is on (for the current-line highlight). */
  const caretLine = $derived(emu.source.slice(0, selStart).split("\n").length);

  // ─── Feature data (pure modules) ───────────────────────────────────────
  /** Per-line assembled address + bytes (live listing gutter). */
  const listing = $derived(listingByLine(asm));
  /** Highlighted tokens per line (syntax highlighting mirror). */
  const hlLines = $derived(lines.map((l) => tokenize(l)));
  /** Resolved symbol values referenced on each line (inlay hints, in the rail). */
  const inlayByLine = $derived.by(() => {
    const m = new Map<number, string>();
    for (const h of inlayHints(emu.source, asm?.symbols ?? {})) {
      const v = formatHintValue(h.value);
      m.set(h.line, m.has(h.line) ? `${m.get(h.line)} ${v}` : v);
    }
    return m;
  });
  /** Per-line cycle label ("8", "12/8"), only for instruction lines. */
  const cyclesByLine = $derived.by(() => {
    const m = new Map<number, string>();
    for (const [line, l] of listing) {
      if (l.bytes.length > 0 && isInstructionLine(lines[line - 1] ?? "")) {
        m.set(line, formatCycles(bytesCycles(l.bytes)));
      }
    }
    return m;
  });

  const statusText = $derived.by(() => {
    if (errorCount > 0) return `${errorCount} problem${errorCount === 1 ? "" : "s"}`;
    if (lastRun) return `${STOP_LABEL[lastRun.reason]} · ${lastRun.steps} instr`;
    return "";
  });

  /** T-cycle sum over a multi-line selection (shown in the status). */
  const selectionCycles = $derived.by(() => {
    if (selEnd <= selStart) return null;
    const startLine = emu.source.slice(0, selStart).split("\n").length;
    const endLine = emu.source.slice(0, selEnd).split("\n").length;
    if (endLine === startLine) return null;
    const costs = [];
    for (let ln = startLine; ln <= endLine; ln++) {
      const l = listing.get(ln);
      if (l && l.bytes.length > 0 && isInstructionLine(lines[ln - 1] ?? "")) costs.push(bytesCycles(l.bytes));
    }
    if (costs.length === 0) return null;
    const t = sumCycles(costs);
    return `Σ ${formatCycles(t)}t · ${t.taken / 4}M`;
  });

  /** A set line breakpoint that maps to no emitted byte (blank/comment/EQU)
   *  never fires — dim it so that isn't a mystery (only once we've assembled). */
  function bpIsLive(line: number): boolean {
    return !asm || emittingLines.has(line);
  }

  // ─── Live assemble (debounced) ─────────────────────────────────────────
  let assembleTimer: ReturnType<typeof setTimeout> | undefined;

  function scheduleAssemble() {
    clearTimeout(assembleTimer);
    if (!emu.canAssemble) return;
    assembleTimer = setTimeout(() => void emu.assembleCurrent(), 300);
  }

  onMount(() => {
    if (emu.canAssemble) void emu.assembleCurrent();
  });
  onDestroy(() => clearTimeout(assembleTimer));

  // ─── Scroll sync + PC auto-scroll ──────────────────────────────────────
  function onScroll(e: Event) {
    const el = e.currentTarget as HTMLTextAreaElement;
    // A vertical scroll we didn't initiate (wheel / drag / keyboard) means the
    // user wants to look elsewhere — stop chasing the PC until they re-follow.
    if (followPc && !autoScrolling && Math.abs(el.scrollTop - scrollTop) > 0.5) {
      followPc = false;
    }
    scrollTop = el.scrollTop;
    scrollLeft = el.scrollLeft;
  }

  function onSelect() {
    if (!taEl) return;
    selStart = taEl.selectionStart;
    selEnd = taEl.selectionEnd;
  }

  // ─── Format + source-level stepping ────────────────────────────────────
  function formatDoc() {
    emu.source = formatSource(emu.source);
    scheduleAssemble();
    taEl?.focus();
  }

  /** Center the textarea on a given line (marking it as our own scroll). */
  function scrollToLine(line: number) {
    if (!taEl) return;
    autoScrolling = true;
    taEl.scrollTop = Math.max(0, (line - 1) * LH - taEl.clientHeight / 2);
    scrollTop = taEl.scrollTop;
    requestAnimationFrame(() => (autoScrolling = false));
  }

  // Keep the current PC line in view after a run/step — but only while follow
  // is on. A manual scroll clears `followPc`, so you can watch other parts of
  // the program during execution without being pulled back to the PC.
  $effect(() => {
    const l = pcLine;
    if (!followPc || l == null || !taEl) return;
    const y = (l - 1) * LH;
    const top = taEl.scrollTop;
    const h = taEl.clientHeight;
    if (y < top || y + LH > top + h) scrollToLine(l);
  });

  /** ⌖ button: off → on (jump to PC and resume following); on → off (unlock). */
  function toggleFollowPc() {
    if (followPc) {
      followPc = false;
      return;
    }
    followPc = true;
    if (pcLine != null) scrollToLine(pcLine);
  }

  // ─── Keyboard: ⌘↵ = Launch (live), ⌘⇧↵ = Run to stop (fast) ─────────────
  function onKeydown(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
      e.preventDefault();
      if (e.shiftKey) void emu.runCurrent();
      else void emu.launchCurrent();
    }
  }

  // ─── Gutter / problems interactions ────────────────────────────────────
  function toggleBp(line: number) {
    emu.toggleLineBreakpoint(line);
  }

  function lineStartIndex(text: string, line: number): number {
    let idx = 0;
    const parts = text.split("\n");
    for (let i = 0; i < line - 1 && i < parts.length; i++) idx += parts[i].length + 1;
    return idx;
  }

  function gotoLine(line: number) {
    if (!taEl || line < 1) return;
    const start = lineStartIndex(emu.source, line);
    const end = start + (emu.source.split("\n")[line - 1]?.length ?? 0);
    taEl.focus();
    taEl.setSelectionRange(start, end);
    const y = (line - 1) * LH;
    if (y < taEl.scrollTop || y + LH > taEl.scrollTop + taEl.clientHeight) {
      taEl.scrollTop = Math.max(0, y - taEl.clientHeight / 2);
    }
  }

  // ─── Reveal-in-source (from disasm/memory): scroll + select the line ───────
  let lastSourceSeq = 0;
  $effect(() => {
    const r = emu.sourceRequest;
    if (r.seq === lastSourceSeq) return;
    lastSourceSeq = r.seq;
    gotoLine(r.line);
  });

  // ─── Line context menu (right-click a line) ────────────────────────────────
  let bodyEl = $state<HTMLDivElement>();
  let menu = $state<{ x: number; y: number; line: number } | null>(null);

  /** The source line under a click, from its Y and the fixed line geometry. */
  function lineAtEvent(e: MouseEvent): number {
    const rect = bodyEl!.getBoundingClientRect();
    const y = e.clientY - rect.top;
    const line = Math.floor((y - PAD + scrollTop) / LH) + 1;
    return Math.max(1, Math.min(lines.length, line));
  }

  function openMenu(e: MouseEvent) {
    if (!bodyEl) return;
    e.preventDefault();
    menu = { x: e.clientX, y: e.clientY, line: lineAtEvent(e) };
  }

  const menuItems = $derived.by((): ContextMenuItem[] => {
    if (!menu) return [];
    const line = menu.line;
    const addr = emu.addrForLine(line);
    const canRunTo = addr != null && emu.romLoaded && emu.canStepInstruction;
    const items: ContextMenuItem[] = [];
    if (canRunTo) items.push({ id: "runto", label: `Run to line ${line}` });
    items.push({
      id: "bp",
      label: emu.hasBreakpointAtLine(line) ? "Remove breakpoint" : "Set breakpoint",
      separator: canRunTo,
    });
    if (addr != null) {
      items.push({ id: "reveal-disasm", label: "Reveal in disassembly", separator: true });
      items.push({ id: "reveal-mem", label: "Reveal in memory" });
    }
    items.push({ id: "copy", label: "Copy line", separator: true });
    return items;
  });

  function onMenuAction(id: string) {
    const line = menu?.line;
    menu = null;
    if (line == null) return;
    switch (id) {
      case "runto":
        void emu.runToLine(line);
        break;
      case "bp":
        emu.toggleBreakpointAtLine(line);
        break;
      case "reveal-disasm":
        emu.revealInDisasm({ kind: "line", line });
        break;
      case "reveal-mem": {
        const a = emu.addrForLine(line);
        if (a != null) emu.requestMemoryView(a);
        break;
      }
      case "copy":
        void navigator.clipboard?.writeText(emu.source.split("\n")[line - 1] ?? "");
        break;
    }
  }
</script>

<div class="editor-panel">
  {#if emu.canAssemble}
    <div class="ed-controls">
      <button class="ed-btn launch" title="Launch — run live, like a ROM (⌘↵)" onclick={() => void emu.launchCurrent()}>
        <Play size={12} /> Launch
      </button>
      <button class="ed-btn" title="Run to stop — fast, to HALT / breakpoint (⌘⇧↵)" onclick={() => void emu.runCurrent()}>
        <Zap size={12} /> Run
      </button>
      <button class="ed-btn" title="Assemble — check for errors" onclick={() => void emu.assembleCurrent()}>
        <Hammer size={12} /> Check
      </button>

      <div class="ed-divider"></div>

      <button class="ed-btn ed-icon" title="Format — align label · mnemonic · operands · comment" aria-label="Format" onclick={formatDoc}>
        <AlignJustify size={13} />
      </button>
      <div class="ed-view" bind:this={viewEl}>
        <button
          class="ed-btn ed-icon"
          class:active={viewMenuOpen}
          title="View options"
          aria-label="View options"
          aria-haspopup="true"
          aria-expanded={viewMenuOpen}
          onclick={() => (viewMenuOpen = !viewMenuOpen)}
        >
          <Eye size={13} />
        </button>
        {#if viewMenuOpen}
          <div class="ed-view-menu" role="menu">
            {#each VIEW_ITEMS as item (item.key)}
              <button
                class="ed-view-row"
                role="menuitemcheckbox"
                aria-checked={view[item.key]}
                onclick={() => (view[item.key] = !view[item.key])}
              >
                <span class="ed-check" class:on={view[item.key]}>✓</span>
                {item.label}
              </button>
            {/each}
          </div>
        {/if}
      </div>
      {#if emu.romLoaded}
        <button
          class="ed-btn ed-icon"
          class:active={followPc}
          title={followPc ? "Following the running line — click to unlock the view" : "Follow the running line"}
          aria-label="Follow program counter"
          aria-pressed={followPc}
          onclick={toggleFollowPc}
        >
          <Crosshair size={13} />
        </button>
      {/if}
      {#if emu.romLoaded && !emu.running && emu.canStepInstruction}
        <button class="ed-btn ed-icon" title="Step into — advance one source line" aria-label="Step into" onclick={() => void emu.stepSourceLine()}>
          <ArrowDownToLine size={13} />
        </button>
        <button class="ed-btn ed-icon" title="Step over — advance a line, skipping CALLs" aria-label="Step over" onclick={() => void emu.stepOverLine()}>
          <Redo2 size={13} />
        </button>
      {/if}
      <span class="ed-status" class:err={errorCount > 0}>
        {#if selectionCycles}<span class="ed-sel-cycles">{selectionCycles}</span>{/if}
        {statusText}
      </span>
    </div>

    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="ed-body"
      style="--ed-lh:{LH}px;--ed-pad:{PAD}px"
      bind:this={bodyEl}
      oncontextmenu={openMenu}
    >
      <div class="ed-gutter" style="width:{gutterWidth}px">
        <div class="ed-gutter-inner" style="transform:translateY({-scrollTop}px)">
          {#each lines as _, i (i)}
            {@const n = i + 1}
            {@const isBp = emu.hasBreakpointAtLine(n)}
            {@const li = listing.get(n)}
            <button
              class="ed-gutter-cell"
              class:bp={isBp}
              class:pc={pcLine === n}
              class:dead={isBp && !bpIsLive(n)}
              title={isBp
                ? bpIsLive(n)
                  ? `Breakpoint on line ${n}`
                  : `Breakpoint on line ${n} — no code here, won't fire`
                : `Set breakpoint on line ${n}`}
              aria-label="Toggle breakpoint on line {n}"
              onclick={() => toggleBp(n)}
            >
              <span class="ed-bp-dot"></span>
              {#if view.lineNumbers}<span class="ed-lineno">{n}</span>{/if}
              {#if view.addresses}<span class="ed-addr">{li ? formatAddr(li.addr) : ""}</span>{/if}
            </button>
          {/each}
        </div>
      </div>

      <div class="ed-text-wrap">
        <div class="ed-overlay">
          <div class="ed-overlay-inner" style="transform:translateY({-scrollTop}px)">
            {#if view.currentLine && caretLine !== pcLine}
              <div class="ed-band current" style="top:{PAD + (caretLine - 1) * LH}px;height:{LH}px"></div>
            {/if}
            {#if pcLine != null}
              <div class="ed-band pc" style="top:{PAD + (pcLine - 1) * LH}px;height:{LH}px"></div>
            {/if}
            {#each lineDiags as d (d.line + ":" + d.msg)}
              <div class="ed-band diag" style="top:{PAD + (d.line - 1) * LH}px;height:{LH}px"></div>
            {/each}
          </div>
        </div>

        <!-- Syntax-highlight mirror: colored text behind the transparent textarea. -->
        {#if view.highlight}
          <div class="ed-highlight" aria-hidden="true">
            <div class="ed-hl-inner" style="transform:translate({-scrollLeft}px,{-scrollTop}px)">
              {#each hlLines as toks, i (i)}
                <div class="ed-hl-line">{#each toks as tk, j (j)}<span class="hl-{tk.kind}">{tk.text}</span>{/each}</div>
              {/each}
            </div>
          </div>
        {/if}

        <!-- Right rail: resolved values · bytes · cycles, per emitting line. -->
        {#if view.inlay || view.bytes || view.cycles}
          <div class="ed-rail" aria-hidden="true">
            <div class="ed-rail-inner" style="transform:translateY({-scrollTop}px)">
            {#each lines as _, i (i)}
              {@const n = i + 1}
              {@const li = listing.get(n)}
              {#if li && li.bytes.length > 0}
                <div class="ed-rail-row" style="top:{PAD + (n - 1) * LH}px">
                  {#if view.inlay && inlayByLine.has(n)}<span class="ed-rail-inlay">{inlayByLine.get(n)}</span>{/if}
                  {#if view.bytes}<span class="ed-rail-bytes">{formatBytes(li.bytes, 3)}</span>{/if}
                  {#if view.cycles}<span class="ed-rail-cyc">{cyclesByLine.has(n) ? cyclesByLine.get(n) + "t" : ""}</span>{/if}
                </div>
              {/if}
            {/each}
            </div>
          </div>
        {/if}

        <textarea
          class="ed-text"
          class:plain={!view.highlight}
          bind:this={taEl}
          bind:value={emu.source}
          oninput={scheduleAssemble}
          onscroll={onScroll}
          onkeydown={onKeydown}
          onselect={onSelect}
          onclick={onSelect}
          onkeyup={onSelect}
          wrap="off"
          spellcheck="false"
          autocapitalize="off"
          autocomplete="off"
          autocorrect="off"
          aria-label="Assembly source"
        ></textarea>
      </div>
    </div>

    {#if diags.length > 0}
      <div class="ed-problems">
        {#each diags as d (d.line + ":" + d.msg)}
          <button class="ed-problem" onclick={() => gotoLine(d.line)}>
            <span class="ed-problem-line">{d.line > 0 ? `L${d.line}` : "—"}</span>
            <span class="ed-problem-msg">{d.msg}</span>
          </button>
        {/each}
      </div>
    {/if}
  {:else}
    <div class="empty-hint">
      Writing assembly needs a backend with the sm83-isa export (the web build has it).
    </div>
  {/if}

  {#if menu}
    <ContextMenu x={menu.x} y={menu.y} items={menuItems} onaction={onMenuAction} onclose={() => (menu = null)} />
  {/if}
</div>

<style>
  .editor-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .ed-controls {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 8px;
    border-bottom: 1px solid var(--stroke-lo);
  }

  .ed-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 22px;
    padding: 0 9px;
    font-family: var(--font-sans);
    font-size: 11px;
    font-weight: 500;
    color: var(--text-subtle);
    background: none;
    border: 1px solid var(--stroke-mid);
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: color 0.1s ease, border-color 0.1s ease, background 0.1s ease;
  }

  .ed-btn:hover {
    color: var(--text-hi);
  }

  .ed-btn.launch {
    color: var(--interactive);
    border-color: var(--interactive-ring);
  }

  .ed-btn.launch:hover {
    background: var(--interactive-fill);
  }

  .ed-status {
    margin-left: auto;
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-faint);
  }

  .ed-status.err {
    color: var(--color-destructive, #e5534b);
  }

  .ed-icon {
    padding: 0 6px;
  }

  .ed-icon.active {
    color: var(--interactive);
    border-color: var(--interactive-ring);
    background: var(--interactive-fill);
  }

  /* Separates the build actions (Launch/Run/Check) from the editor tools. */
  .ed-divider {
    width: 1px;
    height: 16px;
    background: var(--stroke-mid);
    margin: 0 3px;
    flex-shrink: 0;
  }

  /* View-options popover. */
  .ed-view {
    position: relative;
    display: inline-flex;
  }

  .ed-view-menu {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    min-width: 184px;
    padding: 4px;
    background: var(--surface-3);
    border: 1px solid var(--stroke-mid);
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-overlay);
    z-index: 200;
  }

  .ed-view-row {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 5px 8px;
    font-family: var(--font-sans);
    font-size: 11px;
    color: var(--text-mid);
    background: none;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    text-align: left;
  }

  .ed-view-row:hover {
    background: var(--fill-mid);
    color: var(--text-hi);
  }

  .ed-check {
    width: 12px;
    flex-shrink: 0;
    font-size: 11px;
    color: var(--interactive);
    opacity: 0;
  }

  .ed-check.on {
    opacity: 1;
  }

  .ed-sel-cycles {
    color: var(--interactive);
    margin-right: 6px;
  }

  /* ── Body: gutter | (overlay + textarea) ── */
  .ed-body {
    position: relative;
    display: flex;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }

  .ed-gutter {
    position: relative;
    overflow: hidden;
    background: var(--fill-lo);
    border-right: 1px solid var(--stroke-lo);
    flex-shrink: 0;
    /* width is set inline from `gutterWidth` so the code reflows on toggle */
  }

  .ed-gutter-inner {
    position: absolute;
    inset: 0;
    padding-top: var(--ed-pad);
  }

  .ed-gutter-cell {
    display: flex;
    align-items: center;
    gap: 4px;
    width: 100%;
    height: var(--ed-lh);
    padding: 0 6px 0 5px;
    background: none;
    border: none;
    cursor: pointer;
  }

  .ed-lineno {
    width: 20px;
    text-align: right;
    font-family: var(--font-mono);
    font-size: 11px;
    line-height: 1;
    color: var(--text-faint);
    opacity: 0.45; /* recede — the address is the useful gutter datum */
    transition: color 0.1s ease, opacity 0.1s ease;
  }

  .ed-gutter-cell.pc .ed-lineno,
  .ed-gutter-cell:hover .ed-lineno {
    opacity: 0.9;
  }

  .ed-gutter-cell.pc .ed-lineno {
    color: var(--interactive);
    font-weight: 600;
  }

  /* Live listing: the assembled address of each emitting line — a faint
     blue-grey (ties to the "address" hue), distinct from the dim line number. */
  .ed-addr {
    margin-left: auto;
    font-family: var(--font-mono);
    font-size: 10px;
    line-height: 1;
    color: oklch(0.6 0.035 232);
    opacity: 0.75;
  }

  .ed-bp-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    border: 1px solid var(--text-faint);
    background: transparent;
    flex-shrink: 0;
    opacity: 0;
    transition: opacity 0.1s ease;
  }

  .ed-gutter-cell:hover .ed-bp-dot {
    opacity: 0.5;
    border-color: var(--color-destructive, #e5534b);
  }

  .ed-gutter-cell.bp .ed-bp-dot {
    opacity: 1;
    background: var(--color-destructive, #e5534b);
    border-color: var(--color-destructive, #e5534b);
  }

  /* A breakpoint on a non-emitting line can't fire — hollow it out. */
  .ed-gutter-cell.dead .ed-bp-dot {
    background: transparent;
    opacity: 0.6;
  }

  .ed-text-wrap {
    position: relative;
    flex: 1;
    min-width: 0;
    overflow: hidden;
  }

  .ed-overlay {
    position: absolute;
    inset: 0;
    overflow: hidden;
    pointer-events: none;
    z-index: 0;
  }

  /* ── Syntax-highlight mirror (behind the transparent textarea) ── */
  .ed-highlight {
    position: absolute;
    inset: 0;
    overflow: hidden;
    pointer-events: none;
    z-index: 1;
  }

  .ed-hl-inner {
    position: absolute;
    top: 0;
    left: 0;
    padding: var(--ed-pad) 8px;
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: var(--ed-lh);
    tab-size: 4;
    white-space: pre;
  }

  .ed-hl-line {
    height: var(--ed-lh);
    white-space: pre;
  }

  /* Harmonized palette — even lightness (~0.76), moderate chroma; comments
     desaturated to recede, registers a distinct blue so operands read apart. */
  .hl-comment { color: oklch(0.52 0.02 250); font-style: italic; }
  .hl-string { color: oklch(0.77 0.1 155); }
  .hl-label { color: var(--text-hi); font-weight: 600; }
  .hl-directive { color: oklch(0.74 0.11 300); }
  .hl-mnemonic { color: var(--interactive); }
  .hl-register { color: oklch(0.76 0.13 232); }
  .hl-number { color: oklch(0.8 0.1 68); }
  .hl-ident { color: var(--text-mid); }
  .hl-punct { color: var(--text-subtle); }

  /* ── Right rail: resolved values · bytes · cycles ── */
  .ed-rail {
    position: absolute;
    inset: 0;
    overflow: hidden;
    pointer-events: none;
    z-index: 1;
  }

  .ed-rail-inner {
    position: absolute;
    top: 0;
    right: 8px;
    left: 0;
  }

  /* Fixed-width, right-anchored cells → the columns line up vertically into a
     clean stripe instead of a ragged right edge. */
  .ed-rail-row {
    position: absolute;
    right: 0;
    height: var(--ed-lh);
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 12px;
    font-family: var(--font-mono);
    font-size: 10px;
    white-space: nowrap;
  }

  .ed-rail-inlay {
    text-align: right;
    color: oklch(0.72 0.13 300);
    opacity: 0.85;
  }

  .ed-rail-bytes {
    width: 62px;
    text-align: right;
    color: var(--text-faint);
    opacity: 0.6;
  }

  .ed-rail-cyc {
    width: 38px;
    text-align: right;
    color: oklch(0.76 0.13 232);
    opacity: 0.9;
  }

  .ed-overlay-inner {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
  }

  .ed-band {
    position: absolute;
    left: 0;
    right: 0;
  }

  .ed-band.pc {
    background: var(--interactive-fill);
    box-shadow: inset 2px 0 0 var(--interactive);
  }

  .ed-band.diag {
    background: oklch(0.63 0.21 25 / 12%);
    box-shadow: inset 0 -2px 0 var(--color-destructive, #e5534b);
  }

  /* Subtle highlight on the caret's line (orientation). */
  .ed-band.current {
    background: oklch(1 0 0 / 3.5%);
  }

  .ed-text {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    margin: 0;
    padding: var(--ed-pad) 8px;
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: var(--ed-lh);
    tab-size: 4;
    color: transparent; /* the highlight mirror renders the visible text */
    background: transparent;
    caret-color: var(--interactive);
    border: none;
    outline: none;
    resize: none;
    white-space: pre;
    overflow: auto;
    z-index: 2;
  }

  .ed-text::selection {
    background: var(--interactive-fill);
  }

  /* Highlighting off → the textarea shows its own (opaque) text. */
  .ed-text.plain {
    color: var(--text-hi);
  }

  /* ── Problems list ── */
  .ed-problems {
    max-height: 33%;
    overflow-y: auto;
    border-top: 1px solid var(--stroke-lo);
    flex-shrink: 0;
  }

  .ed-problem {
    display: flex;
    gap: 8px;
    width: 100%;
    padding: 3px 8px;
    background: none;
    border: none;
    border-bottom: 1px solid var(--stroke-lo);
    cursor: pointer;
    text-align: left;
    transition: background 0.1s ease;
  }

  .ed-problem:hover {
    background: var(--fill-mid);
  }

  .ed-problem-line {
    flex-shrink: 0;
    width: 34px;
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-destructive, #e5534b);
  }

  .ed-problem-msg {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-mid);
  }

  .empty-hint {
    padding: 16px 8px;
    font-family: var(--font-sans);
    font-size: 11px;
    color: var(--text-faint);
    text-align: center;
  }
</style>

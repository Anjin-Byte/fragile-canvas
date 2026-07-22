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
  import { Play, Zap, Hammer } from "lucide-svelte";
  import { ContextMenu, type ContextMenuItem } from "@gestalt/phi";
  import type { StopReason } from "../../types";
  import type { EmuController } from "../emu.svelte.js";

  let { emu }: { emu: EmuController } = $props();

  // Fixed metrics — kept in sync with the CSS below so line L ≡ row L.
  const LH = 19; // line height, px
  const PAD = 6; // textarea top padding, px

  let taEl = $state<HTMLTextAreaElement>();
  let scrollTop = $state(0);

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
  const diagLineSet = $derived(new Set(lineDiags.map((d) => d.line)));
  /** Lines that actually emitted bytes — a breakpoint anywhere else won't fire. */
  const emittingLines = $derived(new Set((asm?.sourceMap ?? []).map((s) => s.line)));
  const pcLine = $derived(emu.currentSourceLine);
  const lastRun = $derived(emu.lastRun);
  const errorCount = $derived(diags.length);

  const statusText = $derived.by(() => {
    if (errorCount > 0) return `${errorCount} problem${errorCount === 1 ? "" : "s"}`;
    if (lastRun) return `${STOP_LABEL[lastRun.reason]} · ${lastRun.steps} instr`;
    return "";
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
    scrollTop = (e.currentTarget as HTMLTextAreaElement).scrollTop;
  }

  // Keep the current PC line in view after a run/step.
  $effect(() => {
    const l = pcLine;
    if (l == null || !taEl) return;
    const y = (l - 1) * LH;
    const top = taEl.scrollTop;
    const h = taEl.clientHeight;
    if (y < top || y + LH > top + h) {
      taEl.scrollTop = Math.max(0, y - h / 2);
    }
  });

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
      <span class="ed-status" class:err={errorCount > 0}>{statusText}</span>
    </div>

    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="ed-body"
      style="--ed-lh:{LH}px;--ed-pad:{PAD}px"
      bind:this={bodyEl}
      oncontextmenu={openMenu}
    >
      <div class="ed-gutter">
        <div class="ed-gutter-inner" style="transform:translateY({-scrollTop}px)">
          {#each lines as _, i (i)}
            {@const n = i + 1}
            {@const isBp = emu.hasBreakpointAtLine(n)}
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
              <span class="ed-lineno">{n}</span>
            </button>
          {/each}
        </div>
      </div>

      <div class="ed-text-wrap">
        <div class="ed-overlay">
          <div class="ed-overlay-inner" style="transform:translateY({-scrollTop}px)">
            {#if pcLine != null}
              <div class="ed-band pc" style="top:{PAD + (pcLine - 1) * LH}px;height:{LH}px"></div>
            {/if}
            {#each lineDiags as d (d.line + ":" + d.msg)}
              <div class="ed-band diag" style="top:{PAD + (d.line - 1) * LH}px;height:{LH}px"></div>
            {/each}
          </div>
        </div>
        <textarea
          class="ed-text"
          bind:this={taEl}
          bind:value={emu.source}
          oninput={scheduleAssemble}
          onscroll={onScroll}
          onkeydown={onKeydown}
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
    width: 46px;
    overflow: hidden;
    background: var(--fill-lo);
    border-right: 1px solid var(--stroke-lo);
    flex-shrink: 0;
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
    margin-left: auto;
    font-family: var(--font-mono);
    font-size: 11px;
    line-height: 1;
    color: var(--text-faint);
    transition: color 0.1s ease;
  }

  .ed-gutter-cell.pc .ed-lineno {
    color: var(--interactive);
    font-weight: 600;
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
    z-index: 1;
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
    color: var(--text-hi);
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

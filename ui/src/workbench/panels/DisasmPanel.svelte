<script lang="ts">
  /**
   * Disassembly panel (gui_spec §3) — live instruction stream from the
   * sm83-isa decoder via the backend's `disassemble` export.
   *
   * Columns: breakpoint gutter · address · raw bytes · mnemonic · ; comment.
   * Follows PC (with context above it) while running/stepping; scrolling,
   * the address box, or a jump switches to a free view (⌖ PC re-follows).
   * Click a JP/JR/CALL/RST target to navigate; Back / Alt+← returns.
   * Right-click for per-instruction actions. Gutter dots toggle persisted
   * breakpoints — not yet enforced (the break engine is a follow-up).
   */
  import { ContextMenu, type ContextMenuItem } from "@gestalt/phi";
  import type { DisasmLine } from "../../types";
  import { hex16, type EmuController } from "../emu.svelte.js";
  import { windowAround, windowFrom, prevInstr } from "../disasm.js";
  import { commentFor, refTarget, symbolFor } from "../hardware.js";

  let { emu }: { emu: EmuController } = $props();

  const BEFORE = 8; // context rows above PC in follow mode
  const AFTER = 30; // rows below

  let lines = $state<DisasmLine[]>([]);
  let follow = $state(true);
  let topAddr = $state(0); // anchor in free mode
  let addrInput = $state("");
  let addrInputEl = $state<HTMLInputElement>();
  let history = $state<number[]>([]); // jump-back stack (anchor addrs)

  const disasmFn = (a: number, c: number) => emu.disasm(a, c);

  async function refresh() {
    if (!emu.romLoaded || !emu.canDisasm) return;
    if (follow) {
      const pc = emu.cpu?.pc ?? 0;
      const w = await windowAround(disasmFn, pc, BEFORE, AFTER);
      lines = w.lines;
    } else {
      lines = await windowFrom(disasmFn, topAddr, BEFORE + AFTER + 1);
    }
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

  // ─── Navigation ────────────────────────────────────────────────────────

  function goTo(addr: number, pushHistory = true) {
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
      // Leaving follow: anchor on the current top so the view doesn't jump.
      follow = false;
      topAddr = lines[0]?.addr ?? emu.cpu?.pc ?? 0;
    }
    if (instrs > 0) {
      for (let i = 0; i < instrs && lines.length > 0; i++) {
        const first = await disasmFn(topAddr, 1);
        topAddr = (topAddr + (first[0]?.len ?? 1)) & 0xffff;
      }
    } else {
      for (let i = 0; i < -instrs; i++) {
        topAddr = await prevInstr(disasmFn, topAddr);
      }
    }
    void refresh();
  }

  function onWheel(e: WheelEvent) {
    e.preventDefault();
    scrollBy(e.deltaY > 0 ? 3 : -3);
  }

  function onKeydown(e: KeyboardEvent) {
    // Handled keys are consumed here (stopPropagation) so they don't also
    // reach the workbench's window-level joypad/transport handler.
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
    menu = { x: e.clientX, y: e.clientY, line };
  }

  const menuItems = $derived.by((): ContextMenuItem[] => {
    if (!menu) return [];
    const line = menu.line;
    const target = refTarget(line);
    const isBp = emu.breakpoints.has(line.addr);
    const items: ContextMenuItem[] = [];
    if (target !== undefined) {
      const sym = symbolFor(target);
      items.push({ id: "goto", label: `Go to ${sym ?? "$" + hex16(target)}` });
    }
    items.push({ id: "bp", label: isBp ? "Remove breakpoint" : "Set breakpoint" });
    items.push({ id: "view", label: "Set view here", separator: true });
    items.push({ id: "copy-text", label: "Copy instruction", separator: true });
    items.push({ id: "copy-bytes", label: "Copy bytes" });
    return items;
  });

  function onMenuAction(id: string) {
    const line = menu?.line;
    menu = null;
    if (!line) return;
    switch (id) {
      case "goto": {
        const t = refTarget(line);
        if (t !== undefined) goTo(t);
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

  function comment(line: DisasmLine): string | undefined {
    return commentFor(line);
  }

  /**
   * Split a JP/JR/CALL line into a head plus its trailing absolute target
   * ($XXXX, exactly 4 hex) so the target can render as a clickable link
   * verbatim. RST (2-hex $XX) and non-branches return null (plain text +
   * the comment column carries the symbol).
   */
  function linkPart(line: DisasmLine): { head: string; token: string; target: number } | null {
    if (!/^(JP|JR|CALL)\b/.test(line.text)) return null;
    const m = line.text.match(/^(.*?)(\$[0-9A-Fa-f]{4})\s*$/);
    if (!m) return null;
    return { head: m[1], token: m[2], target: parseInt(m[2].slice(1), 16) };
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="disasm-panel" tabindex="-1" onkeydown={onKeydown}>
  {#if emu.canDisasm}
    <div class="disasm-controls">
      <button
        class="disasm-btn"
        title="Back (Alt+←)"
        disabled={history.length === 0}
        onclick={back}
      >←</button>
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
      <button
        class="disasm-btn follow"
        class:active={follow}
        title={follow ? "Following PC" : "Follow PC"}
        onclick={refollow}
      >⌖ PC</button>
    </div>

    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="disasm-rows" onwheel={onWheel}>
      {#if lines.length > 0}
        {#each lines as row (row.addr)}
          {@const isPc = emu.cpu?.pc === row.addr}
          {@const isBp = emu.breakpoints.has(row.addr)}
          {@const link = linkPart(row)}
          {@const cmt = comment(row)}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="disasm-row"
            class:current={isPc}
            oncontextmenu={(e) => openMenu(e, row)}
          >
            <button
              class="disasm-gutter"
              class:bp={isBp}
              title={isBp ? "Breakpoint (not yet enforced)" : "Set breakpoint"}
              aria-label="Toggle breakpoint at {hex16(row.addr)}"
              onclick={() => emu.toggleBreakpoint(row.addr)}
            ><span class="bp-dot"></span></button>
            <span class="disasm-addr-col">{hex16(row.addr)}</span>
            <span class="disasm-raw">{row.bytes}</span>
            {#if link}
              <span class="disasm-mnemonic"
                >{link.head}<button class="target" onclick={() => onTargetClick(row)}
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

    <div class="disasm-footer">
      breakpoints persist but are not yet enforced
    </div>
  {:else}
    <div class="empty-hint">
      Disassembly needs a backend with the sm83-isa export
      (rebuild the WASM package).
    </div>
  {/if}

  {#if menu}
    <ContextMenu
      x={menu.x}
      y={menu.y}
      items={menuItems}
      onaction={onMenuAction}
      onclose={() => (menu = null)}
    />
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

  .disasm-addr-prefix {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-faint);
  }

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

  .disasm-btn:hover:not(:disabled) {
    color: var(--text-mid);
  }

  .disasm-btn:disabled {
    opacity: 0.35;
    cursor: default;
  }

  .disasm-btn.follow {
    font-size: 10px;
    margin-left: auto;
  }

  .disasm-btn.active {
    color: var(--interactive);
    border-color: var(--interactive-ring);
    background: var(--interactive-fill);
  }

  .disasm-rows {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    padding: 4px 0;
    font-family: var(--font-mono);
    font-size: 11px;
    line-height: 1.7;
  }

  .disasm-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 8px 0 0;
    white-space: nowrap;
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
  }

  .bp-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    border: 1px solid var(--text-faint);
    background: transparent;
    transition: background 0.1s ease, border-color 0.1s ease;
  }

  .disasm-gutter:hover .bp-dot {
    border-color: var(--color-destructive, #e5534b);
  }

  /* Dimmed fill signals "planned / not yet enforced". */
  .disasm-gutter.bp .bp-dot {
    background: color-mix(in oklab, var(--color-destructive, #e5534b) 55%, transparent);
    border-color: var(--color-destructive, #e5534b);
  }

  .disasm-addr-col {
    color: var(--text-subtle);
    flex-shrink: 0;
  }

  .disasm-raw {
    color: var(--text-faint);
    width: 62px;
    flex-shrink: 0;
    overflow: hidden;
  }

  .disasm-mnemonic {
    color: var(--text-mid);
  }

  .target {
    padding: 0;
    border: none;
    background: none;
    font: inherit;
    color: var(--interactive);
    cursor: pointer;
    text-decoration: underline;
    text-decoration-color: transparent;
    transition: text-decoration-color 0.1s ease;
  }

  .target:hover {
    text-decoration-color: var(--interactive);
  }

  .disasm-comment {
    color: var(--text-faint);
    margin-left: auto;
    padding-left: 12px;
  }

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

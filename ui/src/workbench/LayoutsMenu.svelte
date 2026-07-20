<script lang="ts">
  /**
   * Panel-layout menu: apply a built-in preset, load/delete a saved config,
   * save the current arrangement under a name, or export/import as JSON.
   *
   * All operations are thin glue over the dock's own serialization
   * (see layouts.ts). Applying anything replaces the workbench model, which
   * DockLayout then auto-persists as the live layout via its persistKey.
   */
  import { DockModel, type SerializedDock, type SerializedNode } from "@gestalt/phi";
  import { LayoutGrid, ChevronDown, Save, Download, Upload, Trash2 } from "lucide-svelte";
  import {
    BUILTIN_PRESETS,
    listSaved,
    getSaved,
    saveLayout,
    deleteLayout,
    reconcile,
  } from "./layouts.js";

  let {
    model,
    panelIds,
    onapply,
  }: {
    model: DockModel;
    panelIds: string[];
    onapply: (model: DockModel) => void;
  } = $props();

  let open = $state(false);
  let saved = $state<string[]>([]);
  let saveName = $state("");
  let menuEl = $state<HTMLDivElement>();
  let fileInput = $state<HTMLInputElement>();

  const known = $derived(new Set(panelIds));

  function refresh() {
    saved = listSaved();
  }

  function toggle() {
    open = !open;
    if (open) refresh();
  }

  function applyPreset(spec: SerializedNode) {
    onapply(new DockModel(JSON.parse(JSON.stringify(spec))));
    open = false;
  }

  function applyDock(dock: SerializedDock) {
    onapply(DockModel.deserialize(reconcile(dock, known)));
    open = false;
  }

  function loadSaved(name: string) {
    const dock = getSaved(name);
    if (dock) applyDock(dock);
  }

  function remove(name: string, e: MouseEvent) {
    e.stopPropagation();
    deleteLayout(name);
    refresh();
  }

  function save() {
    const name = saveName.trim();
    if (!name) return;
    saveLayout(name, model.serialize());
    saveName = "";
    refresh();
  }

  function exportCurrent() {
    const blob = new Blob([JSON.stringify(model.serialize(), null, 2)], {
      type: "application/json",
    });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "fc-layout.json";
    a.click();
    URL.revokeObjectURL(url);
    open = false;
  }

  async function onImport() {
    const file = fileInput?.files?.[0];
    if (fileInput) fileInput.value = "";
    if (!file) return;
    try {
      applyDock(JSON.parse(await file.text()));
    } catch {
      /* invalid or incompatible file — ignore */
    }
  }

  // Close on click-outside.
  $effect(() => {
    if (!open) return;
    const close = (e: PointerEvent) => {
      if (menuEl && !menuEl.contains(e.target as Node)) open = false;
    };
    window.addEventListener("pointerdown", close, true);
    return () => window.removeEventListener("pointerdown", close, true);
  });
</script>

<div class="layouts-menu" bind:this={menuEl}>
  <button class="lm-trigger" title="Panel layouts" aria-expanded={open} onclick={toggle}>
    <LayoutGrid size={13} /> Layouts <ChevronDown size={11} />
  </button>

  {#if open}
    <div class="lm-list">
      <div class="lm-section">Presets</div>
      {#each BUILTIN_PRESETS as p (p.name)}
        <button class="lm-item" onclick={() => applyPreset(p.spec)}>{p.name}</button>
      {/each}

      <div class="lm-divider"></div>
      <div class="lm-section">Saved</div>
      {#if saved.length === 0}
        <div class="lm-empty">No saved layouts</div>
      {:else}
        {#each saved as name (name)}
          <div class="lm-saved">
            <button class="lm-saved-load" onclick={() => loadSaved(name)}>{name}</button>
            <button class="lm-del" title="Delete {name}" onclick={(e) => remove(name, e)}>
              <Trash2 size={12} />
            </button>
          </div>
        {/each}
      {/if}

      <div class="lm-divider"></div>
      <form class="lm-save" onsubmit={(e) => { e.preventDefault(); save(); }}>
        <input class="lm-save-input" placeholder="Save current as…" spellcheck="false" bind:value={saveName} />
        <button class="lm-save-btn" type="submit" title="Save" disabled={!saveName.trim()}>
          <Save size={12} />
        </button>
      </form>

      <div class="lm-actions">
        <button class="lm-action" onclick={exportCurrent}><Download size={12} /> Export</button>
        <button class="lm-action" onclick={() => fileInput?.click()}><Upload size={12} /> Import</button>
      </div>
      <input
        bind:this={fileInput}
        type="file"
        accept="application/json,.json"
        class="lm-hidden"
        onchange={onImport}
      />
    </div>
  {/if}
</div>

<style>
  .layouts-menu {
    position: relative;
  }

  /* Matches the toolbar button look (Toolbar's scoped .tb-btn can't reach
     across component boundaries). */
  .lm-trigger {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 24px;
    padding: 0 9px;
    font-family: var(--font-sans);
    font-size: 11px;
    font-weight: 500;
    color: var(--text-subtle);
    background: none;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: color 0.1s ease, background 0.1s ease;
  }

  .lm-trigger:hover {
    color: var(--text-hi);
    background: var(--fill-mid);
  }

  .lm-list {
    position: absolute;
    top: calc(100% + 4px);
    right: 0;
    min-width: 190px;
    padding: 4px;
    background: var(--surface-3);
    border: 1px solid var(--stroke-mid);
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-overlay);
    z-index: 200;
  }

  .lm-section {
    padding: 4px 8px 2px;
    font-family: var(--font-sans);
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-faint);
  }

  .lm-item {
    display: block;
    width: 100%;
    padding: 5px 8px;
    border: none;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--text-mid);
    font-family: var(--font-sans);
    font-size: 12px;
    text-align: left;
    cursor: pointer;
    transition: background 0.1s ease, color 0.1s ease;
  }

  .lm-item:hover {
    background: var(--fill-mid);
    color: var(--text-hi);
  }

  .lm-empty {
    padding: 4px 8px 6px;
    font-family: var(--font-sans);
    font-size: 11px;
    color: var(--text-faint);
  }

  .lm-saved {
    display: flex;
    align-items: center;
  }

  .lm-saved-load {
    flex: 1;
    min-width: 0;
    padding: 5px 8px;
    border: none;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--text-mid);
    font-family: var(--font-sans);
    font-size: 12px;
    text-align: left;
    cursor: pointer;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    transition: background 0.1s ease, color 0.1s ease;
  }

  .lm-saved-load:hover {
    background: var(--fill-mid);
    color: var(--text-hi);
  }

  .lm-del {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    padding: 0;
    border: none;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--text-faint);
    cursor: pointer;
    flex-shrink: 0;
  }

  .lm-del:hover {
    color: var(--color-destructive, #e5534b);
    background: var(--fill-mid);
  }

  .lm-divider {
    height: 1px;
    margin: 4px 2px;
    background: var(--stroke-lo);
  }

  .lm-save {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 2px;
  }

  .lm-save-input {
    flex: 1;
    min-width: 0;
    padding: 4px 6px;
    font-family: var(--font-sans);
    font-size: 12px;
    color: var(--text-hi);
    background: var(--fill-lo);
    border: 1px solid var(--stroke-mid);
    border-radius: var(--radius-sm);
    outline: none;
  }

  .lm-save-input:focus {
    border-color: var(--interactive);
  }

  .lm-save-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    padding: 0;
    border: 1px solid var(--stroke-mid);
    border-radius: var(--radius-sm);
    background: var(--fill-lo);
    color: var(--text-mid);
    cursor: pointer;
    flex-shrink: 0;
  }

  .lm-save-btn:hover:not(:disabled) {
    color: var(--text-hi);
    border-color: var(--interactive);
  }

  .lm-save-btn:disabled {
    opacity: 0.35;
    cursor: default;
  }

  .lm-actions {
    display: flex;
    gap: 4px;
    padding: 2px;
  }

  .lm-action {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    flex: 1;
    padding: 5px 8px;
    border: none;
    border-radius: var(--radius-sm);
    background: var(--fill-lo);
    color: var(--text-subtle);
    font-family: var(--font-sans);
    font-size: 11px;
    cursor: pointer;
    transition: background 0.1s ease, color 0.1s ease;
  }

  .lm-action:hover {
    background: var(--fill-mid);
    color: var(--text-hi);
  }

  .lm-hidden {
    position: absolute;
    opacity: 0;
    width: 0;
    height: 0;
    pointer-events: none;
  }
</style>

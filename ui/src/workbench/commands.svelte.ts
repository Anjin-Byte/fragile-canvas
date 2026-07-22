// ─── Command registry ───────────────────────────────────────────────────────
// The single source of truth for "everything the app can do". The MenuBar
// renders it now; a Cmd+K palette and a keymap can reuse the same list later.
//
// Reactive state lives in thunks (`checked()`, `value()`, `enabled()`): the
// MenuBar evaluates them during render, so reading $state inside — e.g.
// `getModel().findPanel(id)` or `settings.masterVolume` — makes the menu
// update automatically, the same way panels do. Commands are bound to live
// objects via `createCommands`; note `model` is reassigned on layout-load, so
// it is read through `getModel()` and never captured.

import type { PanelDef, DockModel } from "@gestalt/phi";
import type { EmuController } from "./emu.svelte.js";
import type { Settings } from "./settings.svelte.js";
import { SPEED_PRESETS } from "./settings.svelte.js";

/** Display a speed multiplier compactly ("½×", "1×", …). */
export function speedLabel(n: number): string {
  return n === 0.5 ? "½×" : `${n}×`;
}

/** Menu groups, in bar order. Only groups with commands render. */
export type CommandGroup = "File" | "View" | "Machine" | "Build" | "Audio" | "Display" | "Help";
export const GROUP_ORDER: CommandGroup[] = [
  "File",
  "View",
  "Machine",
  "Build",
  "Audio",
  "Display",
  "Help",
];

interface Base {
  id: string;
  label: string;
  group: CommandGroup;
  enabled?: () => boolean;
}

export type Command =
  | (Base & { kind: "action"; run: () => void; shortcut?: string })
  | (Base & { kind: "toggle"; checked: () => boolean; run: () => void })
  | (Base & {
      kind: "choice";
      options: { value: string; label: string }[];
      value: () => string;
      select: (v: string) => void;
    })
  | (Base & {
      kind: "value";
      min: number;
      max: number;
      step: number;
      value: () => number;
      set: (v: number) => void;
      format?: (v: number) => string;
    })
  | (Base & { kind: "submenu"; items: () => Command[] });

export interface CommandCtx {
  emu: EmuController;
  /** Read the live dock model (reassigned on layout-load — never capture). */
  getModel: () => DockModel;
  setModel: (model: DockModel) => void;
  settings: Settings;
  panelDefs: PanelDef[];
}

export function createCommands(ctx: CommandCtx): Command[] {
  const { emu, getModel, settings, panelDefs } = ctx;
  const commands: Command[] = [];

  // ── View: one toggle per panel (reopen a closed tab) ──
  for (const def of panelDefs) {
    const closable = def.closable !== false;
    commands.push({
      kind: "toggle",
      id: `view.${def.id}`,
      label: def.title ?? def.id,
      group: "View",
      checked: () => getModel().findPanel(def.id) !== null,
      // Permanent panels (closable:false, e.g. Screen) stay on.
      enabled: () => closable,
      run: () => {
        const leaf = getModel().findPanel(def.id);
        if (leaf) getModel().closePanel(leaf.id, def.id);
        else getModel().openPanel(def.id);
      },
    });
  }

  // ── Machine: transport + speed + boot (registry entries for the palette) ──
  commands.push({
    kind: "toggle",
    id: "machine.run",
    label: "Run / Pause",
    group: "Machine",
    // Free-run only — an assembled snippet (code mode) runs via Build ▸ Run.
    enabled: () => emu.canFreeRun,
    checked: () => emu.running,
    run: () => emu.toggle(),
  });
  commands.push({
    kind: "action",
    id: "machine.stepInstruction",
    label: "Step Instruction",
    group: "Machine",
    shortcut: "F10",
    enabled: () => emu.romLoaded && !emu.running && emu.canStepInstruction,
    run: () => void emu.stepInstruction(),
  });
  commands.push({
    kind: "action",
    id: "machine.stepFrame",
    label: "Step Frame",
    group: "Machine",
    shortcut: "F6",
    enabled: () => emu.romLoaded && !emu.running,
    run: () => void emu.stepFrame(),
  });
  commands.push({
    kind: "action",
    id: "machine.reset",
    label: "Reset",
    group: "Machine",
    enabled: () => emu.romLoaded,
    run: () => void emu.reset(),
  });
  commands.push({
    kind: "choice",
    id: "machine.speed",
    label: "Speed",
    group: "Machine",
    options: SPEED_PRESETS.map((n) => ({ value: String(n), label: speedLabel(n) })),
    value: () => String(settings.speed),
    select: (v) => {
      settings.speed = Number(v);
      settings.save();
    },
  });
  commands.push({
    kind: "toggle",
    id: "machine.turbo",
    label: "Fast-forward",
    group: "Machine",
    enabled: () => emu.romLoaded,
    checked: () => emu.turbo,
    run: () => (emu.turbo = !emu.turbo),
  });
  commands.push({
    kind: "toggle",
    id: "machine.skipBoot",
    label: "Skip boot ROM",
    group: "Machine",
    checked: () => settings.skipBoot,
    run: () => {
      // A boot-mode preference — it selects how the *next* load / reset boots.
      // It deliberately does NOT reboot the running machine (that's Reset's job,
      // which reloads honoring this flag), so Power isn't a second Reset.
      settings.skipBoot = !settings.skipBoot;
      settings.save();
      emu.skipBoot = settings.skipBoot;
    },
  });

  // ── Build: assemble the editor document, then launch/run/check it ──
  // (all need the sm83-isa export via `canAssemble`)
  commands.push({
    kind: "action",
    id: "build.launch",
    label: "Launch (real-time)",
    group: "Build",
    shortcut: "⌘↵",
    enabled: () => emu.canAssemble,
    run: () => void emu.launchCurrent(),
  });
  commands.push({
    kind: "action",
    id: "build.run",
    label: "Run to stop (fast)",
    group: "Build",
    shortcut: "⌘⇧↵",
    enabled: () => emu.canAssemble,
    run: () => void emu.runCurrent(),
  });
  commands.push({
    kind: "action",
    id: "build.assemble",
    label: "Assemble (check)",
    group: "Build",
    enabled: () => emu.canAssemble,
    run: () => void emu.assembleCurrent(),
  });

  // ── Audio: master volume + mute (backends without setMasterVolume dim) ──
  const audioSupported = () => emu.backend.setMasterVolume != null;
  commands.push({
    kind: "value",
    id: "audio.volume",
    label: "Master volume",
    group: "Audio",
    min: 0,
    max: 1,
    step: 0.01,
    enabled: audioSupported,
    value: () => settings.masterVolume,
    set: (v) => {
      settings.masterVolume = v;
      settings.save();
    },
    format: (v) => `${Math.round(v * 100)}%`,
  });
  commands.push({
    kind: "toggle",
    id: "audio.mute",
    label: "Mute",
    group: "Audio",
    enabled: audioSupported,
    checked: () => settings.muted,
    run: () => {
      settings.muted = !settings.muted;
      settings.save();
    },
  });

  // ── Display: screen/render options ──
  commands.push({
    kind: "toggle",
    id: "display.lcdEffect",
    label: "LCD screen effect",
    group: "Display",
    checked: () => settings.lcdEffect,
    run: () => {
      settings.lcdEffect = !settings.lcdEffect;
      settings.save();
    },
  });

  return commands;
}

/** Index commands by id (for future palette / keymap dispatch). */
export function commandsById(commands: Command[]): Map<string, Command> {
  return new Map(commands.map((c) => [c.id, c]));
}

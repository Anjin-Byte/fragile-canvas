// ─── Workbench settings ─────────────────────────────────────────────────────
// App-level "dials" as reactive $state, persisted to localStorage. Commands
// mutate these; appliers ($effects) push them to subsystems (audio gain,
// emulator speed/boot, the screen shader). Keeping them here — separate from
// emulator and dock state — is what lets a menu, a palette, a preferences
// dialog, or a restored config all drive the same value through one path.

const SETTINGS_KEY = "fc-workbench-settings";

/** Emulation speed multipliers offered on the toolbar / in preferences. */
export const SPEED_PRESETS = [0.5, 1, 2, 4] as const;

export class Settings {
  /** Master output gain, 0–1. Defaults low so audio isn't jarring on first load. */
  masterVolume = $state(0.25);
  muted = $state(false);
  /** Emulation speed multiplier (one of SPEED_PRESETS). */
  speed = $state(1);
  /** Skip the boot ROM (jump straight to $0100) on the next load. */
  skipBoot = $state(false);
  /** Render the full LCD grid/grain shader (false = flat palette). */
  lcdEffect = $state(true);

  constructor() {
    try {
      const raw = localStorage.getItem(SETTINGS_KEY);
      if (raw) {
        const s = JSON.parse(raw);
        if (typeof s.masterVolume === "number") {
          this.masterVolume = Math.max(0, Math.min(1, s.masterVolume));
        }
        if (typeof s.muted === "boolean") this.muted = s.muted;
        if (typeof s.speed === "number" && (SPEED_PRESETS as readonly number[]).includes(s.speed)) {
          this.speed = s.speed;
        }
        if (typeof s.skipBoot === "boolean") this.skipBoot = s.skipBoot;
        if (typeof s.lcdEffect === "boolean") this.lcdEffect = s.lcdEffect;
      }
    } catch {
      /* absent/corrupt — defaults */
    }
  }

  /** Persist current values (call after any mutation). */
  save(): void {
    try {
      localStorage.setItem(
        SETTINGS_KEY,
        JSON.stringify({
          masterVolume: this.masterVolume,
          muted: this.muted,
          speed: this.speed,
          skipBoot: this.skipBoot,
          lcdEffect: this.lcdEffect,
        }),
      );
    } catch {
      /* storage unavailable — best-effort */
    }
  }
}

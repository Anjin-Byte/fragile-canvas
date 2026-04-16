# Screen Resize Guide

Reference for implementing dynamic screen sizing without regressing the LCD shader visuals.

---

## Architecture Overview

The rendering pipeline has two layers:

1. **Screen.svelte** (`ui/src/components/Screen.svelte`) — WebGL2 canvas with a fragment shader that turns 160x144 shade indices into a DMG LCD effect (palette, grid, gap blending, overlay).
2. **Emulator.svelte** (`ui/src/Emulator.svelte`) — contains the `.screen-well` container that sizes the canvas via CSS. Currently hardcoded at 480x432px (3x scale).

The shader is already fully resolution-adaptive through a single `u_resolution` uniform. The only work needed for resize support is keeping the canvas backing buffer, viewport, and uniform in sync with the CSS layout.

---

## Resolution-Dependent Points in the Shader

### Safe (auto-adapt via `u_resolution`)

All line numbers below refer to positions within the `FRAG_SRC` shader string in Screen.svelte, not the file itself.

| Shader line | Code | What it does |
|-------------|------|-------------|
| 15-16 | `dotW = u_resolution.x / 160.0` | Computes screen pixels per game pixel. All grid math derives from this. |
| 18-19 | `mod(gl_FragCoord.x, dotW) / dotW` | Maps each fragment to its position within its game-pixel cell. |
| 21-24 | `gapPx = 0.8; gapNormX = gapPx / dotW` | Gap width and anti-alias softness, normalized to dot size. |

These all adapt automatically when `u_resolution` is correct.

### Fixed to game resolution (correct, do not change)

| Shader line | Code | Why it's fixed |
|-------------|------|---------------|
| 35 | `texelSize = vec2(1.0/160.0, 1.0/144.0)` | Samples adjacent *game* pixels for neighbor-aware gap blending. Must stay at game resolution regardless of screen size. |
| 2 | `texture(u_game, v_uv).r` | Game texture sampled via normalized UV with NEAREST filtering. Resolution-independent. |
| 9 | `texture(u_overlay, v_uv).r` | Overlay texture sampled via normalized UV with LINEAR filtering. Stretches to fill any canvas size. |

### Tuning concern at different scales

`gapPx = 0.8` is a constant physical pixel width. Its visual proportion changes with scale:

| Scale | dotW (px) | Gap as % of dot | Visual effect |
|-------|-----------|-----------------|---------------|
| 2x | 2 | 40% | Grid dominates, looks harsh |
| 3x | 3 | 27% | Current look (the reference) |
| 4x | 4 | 20% | Tighter grid, still good |
| 6x | 6 | 13% | Grid very subtle |
| 8x | 8 | 10% | Grid barely visible |

If you want consistent grid appearance across scales, change to proportional:
```glsl
float gapPx = dotW * 0.27; // maintain 27% ratio (matches 3x reference)
```
This is a visual design decision, not a correctness issue. The current constant `0.8` works well within a reasonable range (3x-6x) but degrades at extremes.

---

## What `initGL` Does Today

```js
const dpr = window.devicePixelRatio || 1;
const rect = el.getBoundingClientRect();
el.width  = Math.round(rect.width  * dpr) || SCREEN_W * 3;  // 480 fallback
el.height = Math.round(rect.height * dpr) || SCREEN_H * 3;  // 432 fallback
```

- Runs **once** on mount via `use:initGL`
- Sizes the backing buffer to CSS size x DPR
- Falls back to 480x432 if the element has no layout size yet
- Sets `u_resolution` and `gl.viewport` once, never updates them
- **No ResizeObserver** — if the container changes size after mount, the backing buffer stays stale, `u_resolution` is wrong, and the browser rescales causing moire

---

## Implementation Plan

### Step 1: Add a resize handler to Screen.svelte

Add a `ResizeObserver` that updates the three resolution-dependent pieces:

```js
function handleResize() {
  if (!gl || !canvas) return;

  const dpr = window.devicePixelRatio || 1;
  const rect = canvas.getBoundingClientRect();
  const w = Math.round(rect.width  * dpr);
  const h = Math.round(rect.height * dpr);

  // Skip if unchanged (avoid unnecessary re-init)
  if (canvas.width === w && canvas.height === h) return;

  canvas.width  = w;
  canvas.height = h;
  gl.viewport(0, 0, w, h);
  gl.uniform2f(u_resolution, w, h);
  render();
}
```

Attach via `ResizeObserver` in `initGL`:

```js
const ro = new ResizeObserver(handleResize);
ro.observe(el);
```

Disconnect on destroy.

### Step 2: Handle DPR changes

When the window moves between displays with different pixel ratios, the DPR changes and the backing buffer needs to be resized. The `matchMedia` approach requires re-registering after each change because the query is tied to a specific DPR value:

```js
function watchDPR(callback) {
  const mql = window.matchMedia(
    `(resolution: ${window.devicePixelRatio}dppx)`
  );
  mql.addEventListener("change", () => {
    callback();
    watchDPR(callback); // re-register for the new DPR value
  }, { once: true });
}

watchDPR(handleResize);
```

Modern alternative: `ResizeObserver` with `devicePixelContentBoxSize` reports the size in device pixels directly, eliminating manual DPR math. Check browser support before relying on it:

```js
const ro = new ResizeObserver((entries) => {
  for (const entry of entries) {
    // devicePixelContentBoxSize gives exact device-pixel dimensions
    // — no DPR multiplication needed, no rounding error
    const dpSize = entry.devicePixelContentBoxSize?.[0];
    if (dpSize) {
      resizeCanvas(dpSize.inlineSize, dpSize.blockSize);
    } else {
      // Fallback: manual DPR multiplication
      const dpr = window.devicePixelRatio || 1;
      const rect = entry.contentRect;
      resizeCanvas(
        Math.round(rect.width * dpr),
        Math.round(rect.height * dpr)
      );
    }
  }
});
ro.observe(canvas, { box: "device-pixel-content-box" });
```

This approach handles both container resizes AND DPR changes in a single observer.

### Step 3: Make the container responsive (Emulator.svelte)

Replace the fixed `.screen-well` size with a scale-factor approach. There are two distinct sizes to reason about:

- **CSS size** — what the user sees. Should be an integer multiple of 160x144 for clean layout.
- **Backing buffer** — actual pixel data the shader writes to. Must be CSS size x DPR for `gl_FragCoord` alignment.

```js
// Compute largest integer scale that fits the available space (CSS pixels)
const maxW = containerWidth;
const maxH = containerHeight;
const scale = Math.max(1, Math.min(
  Math.floor(maxW / 160),
  Math.floor(maxH / 144)
));
const wellW = 160 * scale;  // CSS px — set on .screen-well
const wellH = 144 * scale;

// Backing buffer is handled by handleResize / ResizeObserver:
// canvas.width  = wellW * dpr  (e.g., 480 * 2 = 960 on a retina display)
// canvas.height = wellH * dpr
```

Integer CSS scaling ensures each game pixel maps to exactly NxN CSS pixels. On a 2x DPR display at 3x CSS scale, the backing buffer is 960x864 — each game pixel is 6x6 device pixels. The shader handles this correctly because `dotW = 960 / 160 = 6`.

Non-integer CSS scales work (the shader's smoothstep handles sub-pixel boundaries), but integer scales produce the sharpest grid with zero sub-pixel artifacts.

### Step 4: Scale bezel proportionally

Bezel padding, font sizes, border-radius, and LED size should scale with the screen:

```css
/* Example: derive from a CSS custom property set by JS */
.screen-bezel {
  --scale: 3; /* set dynamically */
  border-radius: calc(var(--scale) * 5px);
}

.bezel-top {
  padding: calc(var(--scale) * 5px) calc(var(--scale) * 8px) 0;
}
```

Or use `em`/`rem` units tied to a font-size that scales with the screen.

---

## What Does NOT Need to Change

- The fragment shader source code (fully parametric via `u_resolution`)
- The vertex shader / quad geometry
- Game texture setup (R8, NEAREST, 160x144)
- Overlay texture setup (RGBA, LINEAR, stretched via UV)
- The `blit()` function
- The OKLAB color space conversions
- The palette upload

---

## Critical Invariants

1. **`canvas.width/height` must equal CSS layout size x DPR.** If they diverge, the browser rescales the buffer and `gl_FragCoord` no longer aligns with physical pixels, producing moire in the grid.

2. **`u_resolution` must match `canvas.width/height`.** The shader uses this to compute dot size. A stale value means the grid pattern doesn't match the actual pixel grid.

3. **`gl.viewport` must match `canvas.width/height`.** Otherwise the shader renders to a subset of the buffer.

4. **Game texture stays at 160x144, NEAREST filter.** Never upscale the texture itself. The shader handles all upscaling via the full-screen quad UV mapping. The `NEAREST` filter ensures each shade index is read as a clean integer with no interpolation.

5. **Overlay texture stays at LINEAR filter.** It's a continuous-tone wear texture that should interpolate smoothly at any scale.

---

## Testing Checklist

When implementing resize, verify at each scale factor:

- [ ] Grid lines are crisp with no moire or shimmer
- [ ] Gap blending between different shades shows smooth OKLAB transitions
- [ ] Overlay texture (worn surface variation) is visible but subtle
- [ ] No visual difference vs current 3x output when displayed at 3x
- [ ] Shade 0 (lightest) gaps are distinct from shade 0 pixel interiors
- [ ] No flicker or blank frames during resize transitions
- [ ] Works on high-DPI displays (2x DPR)
- [ ] Works when moving window between displays with different DPR

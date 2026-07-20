<script lang="ts">
  // Pre-baked substrate grain: 8-bit grayscale, computed from the original
  // 16-bit normal map by the exact browser pipeline this component used to
  // run at load time (see tools/lcd-regression/). Byte-identical samples,
  // ~6x smaller file, no main-thread conversion loop, 1/4 the VRAM.
  import substrateGrainUrl from "./substrate_grain.png";

  const SCREEN_W = 160;
  const SCREEN_H = 144;

  // Lospec "DMG-01 Accurate" palette — photo-sampled from real hardware.
  // Hue shifts from warm yellow-green (shade 0) to cool green (shade 2).
  const DMG_PALETTE = [
    [0x9A / 255, 0x9E / 255, 0x3F / 255],  // #9A9E3F shade 0 — lightest
    [0x49 / 255, 0x6B / 255, 0x22 / 255],  // #496B22 shade 1
    [0x0E / 255, 0x45 / 255, 0x0B / 255],  // #0E450B shade 2
    [0x1B / 255, 0x2A / 255, 0x09 / 255],  // #1B2A09 shade 3 — darkest
  ];

  // ── CPU-side OKLAB (same truncated constants as the shader) ──
  // The palette's OKLAB values and the gap-blend bases are constants of
  // the palette, so they're computed once here (in double precision) and
  // uploaded as uniforms instead of being re-derived per fragment.
  function toOklab(rgb: number[]): number[] {
    const [r, g, b] = rgb.map((v) => Math.pow(v, 2.2));
    const l = Math.cbrt(Math.max(0.4122 * r + 0.5363 * g + 0.0514 * b, 0));
    const m = Math.cbrt(Math.max(0.2119 * r + 0.6806 * g + 0.1075 * b, 0));
    const s = Math.cbrt(Math.max(0.0883 * r + 0.2817 * g + 0.63 * b, 0));
    return [
      0.2105 * l + 0.7937 * m - 0.0041 * s,
      1.978 * l - 2.4286 * m + 0.4506 * s,
      0.0259 * l + 0.7827 * m - 0.8086 * s,
    ];
  }

  const PALETTE_LAB = DMG_PALETTE.map(toOklab);
  // Gap color = mix(mix(labSelf, labNeighbor, 0.3), labShade0, 0.35),
  // expanded to labSelf*0.455 + (labNeighbor*0.195 + labShade0*0.35).
  // The parenthesized part depends only on the neighbor's palette index.
  const GAP_BASE = PALETTE_LAB.map((lab) =>
    lab.map((v, i) => v * 0.195 + PALETTE_LAB[0][i] * 0.35),
  );

  // ── Shader sources ──

  const VERT_SRC = `#version 300 es
    in vec2 a_pos;
    in vec2 a_uv;
    out vec2 v_uv;
    void main() {
      v_uv = a_uv;
      gl_Position = vec4(a_pos, 0.0, 1.0);
    }
  `;

  // Fragment shader: DMG LCD with accurate palette and OKLAB gap blending.
  //
  // Colors: Lospec "DMG-01 Accurate" palette (photo-sampled from hardware).
  // Grid: gl_FragCoord-aligned per-edge gaps (no moiré).
  // Blending: gaps between different-shade pixels interpolated in OKLAB
  // for perceptually uniform transitions across the STN hue shift.
  //
  // Performance restructure (output verified byte-equivalent, ≤1 LSB, by
  // tools/lcd-regression/harness.html): all forward sRGB→OKLAB conversions
  // are palette constants and arrive precomputed in u_paletteLab/u_gapBase;
  // the gap-color math runs only for fragments inside a gap. Per-fragment
  // transcendental work drops from ~57 pow() calls to 3 (interior) / 15
  // (gap). The gap formula mix(mix(self, n, .3), s0, .35) is expanded to
  // self*0.455 + (n*0.195 + s0*0.35) with the parenthesized part baked
  // into u_gapBase per palette index.
  const FRAG_SRC = `#version 300 es
    precision highp float;

    in vec2 v_uv;
    out vec4 fragColor;

    uniform sampler2D u_game;
    uniform sampler2D u_overlay;
    uniform vec2 u_resolution;
    uniform vec3 u_palette[4];
    uniform vec3 u_paletteLab[4]; // OKLAB of each palette entry (CPU-computed)
    uniform vec3 u_gapBase[4];    // paletteLab[n]*0.195 + paletteLab[0]*0.35
    uniform float u_gridIntensity; // 0 = flat palette, 1 = full LCD model
    uniform float u_overlayIntensity; // 0 = no overlay, 1 = full scratch effect

    // ── OKLAB → sRGB (the only conversion left per fragment) ──
    vec3 linearToSrgb(vec3 c) { return pow(max(c, vec3(0.0)), vec3(1.0/2.2)); }

    vec3 oklabToLinear(vec3 lab) {
      float l = lab.x + 0.3963*lab.y + 0.2159*lab.z;
      float m = lab.x - 0.1056*lab.y - 0.0639*lab.z;
      float s = lab.x - 0.0894*lab.y - 1.2910*lab.z;
      return vec3(
        +4.0767*l*l*l - 3.3077*m*m*m + 0.2310*s*s*s,
        -1.2684*l*l*l + 2.6097*m*m*m - 0.3413*s*s*s,
        -0.0042*l*l*l - 0.7034*m*m*m + 1.7076*s*s*s
      );
    }

    vec3 fromOklab(vec3 lab) { return linearToSrgb(oklabToLinear(lab)); }

    // Neighbor shade lookup with CLAMP_TO_EDGE semantics. texelFetch keeps
    // the sampling well-defined inside non-uniform control flow.
    int shadeAt(ivec2 p) {
      ivec2 q = clamp(p, ivec2(0), ivec2(${SCREEN_W - 1}, ${SCREEN_H - 1}));
      float s = texelFetch(u_game, q, 0).r;
      return clamp(int(s * 255.0 + 0.5), 0, 3);
    }

    void main() {
      // ── Game data → palette color ──
      float shadeRaw = texture(u_game, v_uv).r;
      int shade = clamp(int(shadeRaw * 255.0 + 0.5), 0, 3);
      vec3 flatColor = u_palette[shade];

      // ── Per-pixel variation: perturb lightness in OKLAB ──
      // Simulates the fine matte grain of the DMG front polarizer.
      // The overlay is a pre-baked scalar grain texture (from the DMG
      // substrate normal map), tiled across the screen at a density that
      // reads as surface texture.
      float grain = texture(u_overlay, v_uv * 12.0).r; // tiled 12x across screen
      vec3 labPixel = u_paletteLab[shade];
      labPixel.x += (grain - 0.5) * u_overlayIntensity;
      vec3 pixelColor = fromOklab(labPixel);

      // ── Grid: aligned to screen pixels (no moiré) ──
      float dotW = u_resolution.x / float(${SCREEN_W});
      float dotH = u_resolution.y / float(${SCREEN_H});

      float cellX = mod(gl_FragCoord.x, dotW) / dotW;
      float cellY = mod(gl_FragCoord.y, dotH) / dotH;

      float gapPx = 0.8;
      float gapNormX = gapPx / max(dotW, 1.0);
      float gapNormY = gapPx / max(dotH, 1.0);
      float softNorm = 0.5 / max(dotW, 1.0);

      // Per-edge gap amounts (0 = in gap, 1 = inside segment)
      float edgeL = smoothstep(0.0, gapNormX + softNorm, cellX);
      float edgeR = smoothstep(0.0, gapNormX + softNorm, 1.0 - cellX);
      float edgeB = smoothstep(0.0, gapNormY + softNorm, cellY);
      float edgeT = smoothstep(0.0, gapNormY + softNorm, 1.0 - cellY);

      float gapL_w = 1.0 - edgeL;  // weight: how much we're in the left gap
      float gapR_w = 1.0 - edgeR;
      float gapT_w = 1.0 - edgeT;
      float gapB_w = 1.0 - edgeB;
      float totalGap = gapL_w + gapR_w + gapT_w + gapB_w;

      vec3 lcdColor;
      if (totalGap < 0.001) {
        // Fully inside segment — no gap influence, skip all gap math.
        lcdColor = pixelColor;
      } else {
        // ── Gap colors: blend self toward neighbor and "reflector" ──
        // Shade 0 acts as the reflector — the base color the LCD shows
        // between segments — so gaps stay visually distinct even between
        // same-shade pixels. Each edge contributes independently from
        // pixelColor to avoid cross-axis contamination.
        ivec2 texel = ivec2(floor(v_uv * vec2(float(${SCREEN_W}), float(${SCREEN_H}))));
        vec3 selfPart = labPixel * 0.455;
        vec3 gapL = fromOklab(selfPart + u_gapBase[shadeAt(texel + ivec2(-1, 0))]);
        vec3 gapR = fromOklab(selfPart + u_gapBase[shadeAt(texel + ivec2( 1, 0))]);
        vec3 gapT = fromOklab(selfPart + u_gapBase[shadeAt(texel + ivec2(0, -1))]);
        vec3 gapB = fromOklab(selfPart + u_gapBase[shadeAt(texel + ivec2(0,  1))]);

        // Weighted average of all gap contributions, blended with center
        vec3 gapBlend = (gapL * gapL_w + gapR * gapR_w + gapT * gapT_w + gapB * gapB_w) / totalGap;
        lcdColor = mix(pixelColor, gapBlend, min(totalGap, 1.0));
      }

      // ── Blend with flat palette mode ──
      vec3 color = mix(flatColor, lcdColor, u_gridIntensity);

      fragColor = vec4(color, 1.0);
    }
  `;

  // ── WebGL state ──
  let canvas: HTMLCanvasElement;
  let gl: WebGL2RenderingContext | null = null;
  let program: WebGLProgram | null = null;
  let gameTexture: WebGLTexture | null = null;
  let overlayTexture: WebGLTexture | null = null;
  let texData: Uint8Array = new Uint8Array(SCREEN_W * SCREEN_H);

  // Uniforms
  let u_game: WebGLUniformLocation | null = null;
  let u_overlay: WebGLUniformLocation | null = null;
  let u_resolution: WebGLUniformLocation | null = null;
  let u_palette: WebGLUniformLocation | null = null;
  let u_gridIntensity: WebGLUniformLocation | null = null;
  let u_overlayIntensity: WebGLUniformLocation | null = null;

  // LCD grid/grain shader on (full DMG look) vs off (flat palette).
  // Seeds the initial uniforms and is toggled live via setLcdEffect().
  let lcdEffectOn = true;

  function compileShader(gl: WebGL2RenderingContext, type: number, source: string): WebGLShader {
    const shader = gl.createShader(type)!;
    gl.shaderSource(shader, source);
    gl.compileShader(shader);
    if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
      console.error('Shader compile error:', gl.getShaderInfoLog(shader));
    }
    return shader;
  }

  function initGL(el: HTMLCanvasElement) {
    canvas = el;

    // Size backing buffer as an integer multiple of 160×144.
    // This preserves the exact game aspect ratio and gives the
    // shader clean integer dot sizes (no sub-pixel distortion).
    const dpr = window.devicePixelRatio || 1;
    const rect = el.getBoundingClientRect();
    const s0 = Math.max(1, Math.min(
      Math.floor(rect.width * dpr / SCREEN_W),
      Math.floor(rect.height * dpr / SCREEN_H)
    ));
    el.width = SCREEN_W * s0;
    el.height = SCREEN_H * s0;

    gl = el.getContext("webgl2", { antialias: false, alpha: false })!;
    if (!gl) {
      console.error("WebGL2 not available");
      return;
    }

    // Compile shaders
    const vert = compileShader(gl, gl.VERTEX_SHADER, VERT_SRC);
    const frag = compileShader(gl, gl.FRAGMENT_SHADER, FRAG_SRC);
    program = gl.createProgram()!;
    gl.attachShader(program, vert);
    gl.attachShader(program, frag);
    gl.linkProgram(program);

    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
      console.error('Program link error:', gl.getProgramInfoLog(program));
      return;
    }

    gl.useProgram(program);

    // Full-screen quad (two triangles)
    const quadVerts = new Float32Array([
      // pos       uv
      -1, -1,    0, 1,   // bottom-left  (uv flipped Y for texture)
       1, -1,    1, 1,   // bottom-right
      -1,  1,    0, 0,   // top-left
       1,  1,    1, 0,   // top-right
    ]);

    const vao = gl.createVertexArray()!;
    gl.bindVertexArray(vao);

    const vbo = gl.createBuffer()!;
    gl.bindBuffer(gl.ARRAY_BUFFER, vbo);
    gl.bufferData(gl.ARRAY_BUFFER, quadVerts, gl.STATIC_DRAW);

    const a_pos = gl.getAttribLocation(program, "a_pos");
    gl.enableVertexAttribArray(a_pos);
    gl.vertexAttribPointer(a_pos, 2, gl.FLOAT, false, 16, 0);

    const a_uv = gl.getAttribLocation(program, "a_uv");
    gl.enableVertexAttribArray(a_uv);
    gl.vertexAttribPointer(a_uv, 2, gl.FLOAT, false, 16, 8);

    // Game texture — single-channel (R8), nearest filtering
    gameTexture = gl.createTexture()!;
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, gameTexture);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);

    // Upload initial dark frame
    texData.fill(3); // shade 3 = darkest
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.R8, SCREEN_W, SCREEN_H, 0, gl.RED, gl.UNSIGNED_BYTE, texData);

    // Overlay texture — pre-baked substrate grain, single channel (R8).
    overlayTexture = gl.createTexture()!;
    gl.activeTexture(gl.TEXTURE1);
    gl.bindTexture(gl.TEXTURE_2D, overlayTexture);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.REPEAT);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.REPEAT);

    // 1x1 placeholder until the image loads (0 = same pre-load look as the
    // old black RGBA placeholder's red channel).
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.R8, 1, 1, 0, gl.RED, gl.UNSIGNED_BYTE, new Uint8Array([0]));

    const overlayImg = new Image();
    overlayImg.onload = () => {
      if (!gl || !overlayTexture) return;
      gl.activeTexture(gl.TEXTURE1);
      gl.bindTexture(gl.TEXTURE_2D, overlayTexture);
      // Upload decoded bytes as-is: no browser colorspace transform, and
      // the grayscale PNG's gray value lands in the red channel.
      gl.pixelStorei(gl.UNPACK_COLORSPACE_CONVERSION_WEBGL, gl.NONE);
      gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.R8, gl.RED, gl.UNSIGNED_BYTE, overlayImg);
      render();
    };
    overlayImg.src = substrateGrainUrl;

    // Get uniform locations
    u_game = gl.getUniformLocation(program, "u_game");
    u_overlay = gl.getUniformLocation(program, "u_overlay");
    u_resolution = gl.getUniformLocation(program, "u_resolution");
    u_palette = gl.getUniformLocation(program, "u_palette");
    u_gridIntensity = gl.getUniformLocation(program, "u_gridIntensity");
    u_overlayIntensity = gl.getUniformLocation(program, "u_overlayIntensity");

    // Set static uniforms
    gl.uniform1i(u_game, 0);
    gl.uniform1i(u_overlay, 1);
    gl.uniform2f(u_resolution, el.width, el.height);
    gl.uniform1f(u_gridIntensity, lcdEffectOn ? 1.0 : 0.0); // full LCD grid
    gl.uniform1f(u_overlayIntensity, lcdEffectOn ? 0.03 : 0.0); // subtle worn surface

    // Upload palette (sRGB, OKLAB, and precomputed gap bases)
    const paletteFlat = new Float32Array(12);
    const paletteLabFlat = new Float32Array(12);
    const gapBaseFlat = new Float32Array(12);
    for (let i = 0; i < 4; i++) {
      for (let c = 0; c < 3; c++) {
        paletteFlat[i * 3 + c] = DMG_PALETTE[i][c];
        paletteLabFlat[i * 3 + c] = PALETTE_LAB[i][c];
        gapBaseFlat[i * 3 + c] = GAP_BASE[i][c];
      }
    }
    gl.uniform3fv(u_palette, paletteFlat);
    gl.uniform3fv(gl.getUniformLocation(program, "u_paletteLab"), paletteLabFlat);
    gl.uniform3fv(gl.getUniformLocation(program, "u_gapBase"), gapBaseFlat);

    // Initial render
    gl.viewport(0, 0, el.width, el.height);
    render();

    // ── Resize handling ──
    // ResizeObserver keeps the backing buffer in sync with CSS layout.
    // DPR watcher handles moving between displays with different pixel ratios.

    function resize() {
      if (!gl || !canvas) return;
      const dpr = window.devicePixelRatio || 1;
      const rect = canvas.getBoundingClientRect();
      // Integer multiple of 160×144 — preserves exact game aspect ratio
      // and gives the shader clean integer dot sizes.
      const s = Math.max(1, Math.min(
        Math.floor(rect.width * dpr / SCREEN_W),
        Math.floor(rect.height * dpr / SCREEN_H)
      ));
      const w = SCREEN_W * s;
      const h = SCREEN_H * s;
      if (canvas.width === w && canvas.height === h) return;
      canvas.width = w;
      canvas.height = h;
      gl.viewport(0, 0, w, h);
      gl.uniform2f(u_resolution, w, h);
      render();
    }

    const ro = new ResizeObserver(resize);
    ro.observe(el);

    // DPR changes (e.g., window dragged to a different monitor).
    // Must re-register after each change since the query is tied to a specific value.
    let dprMql: MediaQueryList | null = null;
    function watchDPR() {
      dprMql = window.matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`);
      dprMql.addEventListener("change", () => { resize(); watchDPR(); }, { once: true });
    }
    watchDPR();

    return {
      destroy() {
        ro.disconnect();
        // Clean up DPR listener — remove from current mql
        dprMql?.removeEventListener("change", resize);
        // Release GL resources; the context itself is freed eagerly
        // rather than waiting on canvas GC.
        if (gl) {
          if (program) gl.deleteProgram(program);
          if (gameTexture) gl.deleteTexture(gameTexture);
          if (overlayTexture) gl.deleteTexture(overlayTexture);
          gl.getExtension("WEBGL_lose_context")?.loseContext();
        }
        gl = null;
        program = null;
        gameTexture = null;
        overlayTexture = null;
      },
    };
  }

  function render() {
    if (!gl || !program) return;
    gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
  }

  export function blit(shades: Uint8Array) {
    if (!gl || !gameTexture) return;

    // Upload shade indices as R8 texture
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, gameTexture);
    gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, SCREEN_W, SCREEN_H, gl.RED, gl.UNSIGNED_BYTE, shades);

    render();
  }

  /** Toggle the LCD grid/grain shader. Off = flat DMG palette (u_gridIntensity
   *  and u_overlayIntensity → 0). Safe to call before the GL context exists;
   *  the value is re-applied when it initializes. */
  export function setLcdEffect(on: boolean) {
    lcdEffectOn = on;
    if (!gl || !program || !u_gridIntensity || !u_overlayIntensity) return;
    gl.useProgram(program);
    gl.uniform1f(u_gridIntensity, on ? 1.0 : 0.0);
    gl.uniform1f(u_overlayIntensity, on ? 0.03 : 0.0);
    render();
  }
</script>

<canvas
  use:initGL
  class="screen"
></canvas>

<style>
  .screen {
    width: 100%;
    height: 100%;
    display: block;
    border-radius: var(--radius-sm);
  }
</style>

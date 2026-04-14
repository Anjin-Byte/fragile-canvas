<script lang="ts">
  import wireframeUrl from "./wireframe.jpg";

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
  const FRAG_SRC = `#version 300 es
    precision highp float;

    in vec2 v_uv;
    out vec4 fragColor;

    uniform sampler2D u_game;
    uniform sampler2D u_overlay;
    uniform vec2 u_resolution;
    uniform vec3 u_palette[4];
    uniform float u_gridIntensity; // 0 = flat palette, 1 = full LCD model
    uniform float u_overlayIntensity; // 0 = no overlay, 1 = full scratch effect

    // ── OKLAB color space conversion ──
    // Perceptually uniform blending for gap colors between shades
    // that shift in hue (yellow-green → cool green on real STN LCD).

    vec3 srgbToLinear(vec3 c) { return pow(c, vec3(2.2)); }
    vec3 linearToSrgb(vec3 c) { return pow(max(c, vec3(0.0)), vec3(1.0/2.2)); }

    vec3 linearToOklab(vec3 c) {
      float l = pow(max(0.4122*c.r + 0.5363*c.g + 0.0514*c.b, 0.0), 1.0/3.0);
      float m = pow(max(0.2119*c.r + 0.6806*c.g + 0.1075*c.b, 0.0), 1.0/3.0);
      float s = pow(max(0.0883*c.r + 0.2817*c.g + 0.6300*c.b, 0.0), 1.0/3.0);
      return vec3(
        0.2105*l + 0.7937*m - 0.0041*s,
        1.9780*l - 2.4286*m + 0.4506*s,
        0.0259*l + 0.7827*m - 0.8086*s
      );
    }

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

    vec3 toOklab(vec3 srgb) { return linearToOklab(srgbToLinear(srgb)); }
    vec3 fromOklab(vec3 lab) { return linearToSrgb(oklabToLinear(lab)); }

    void main() {
      // ── Game data → palette color ──
      float shadeRaw = texture(u_game, v_uv).r;
      int shade = clamp(int(shadeRaw * 255.0 + 0.5), 0, 3);
      vec3 pixelColor = u_palette[shade];

      // ── Per-pixel variation: perturb lightness in OKLAB ──
      // Simulates manufacturing variation in LCD cell response.
      // Sampled per game-pixel so each dot has a consistent offset.
      float variation = texture(u_overlay, v_uv).r - 0.5; // centered around 0
      vec3 labPixel = toOklab(pixelColor);
      labPixel.x += variation * u_overlayIntensity;
      pixelColor = fromOklab(labPixel);

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

      float inSegment = edgeL * edgeR * edgeT * edgeB;

      // ── Per-edge neighbor colors (OKLAB blending) ──
      vec2 texelSize = vec2(1.0 / float(${SCREEN_W}), 1.0 / float(${SCREEN_H}));

      float sL = texture(u_game, v_uv + vec2(-texelSize.x, 0.0)).r;
      float sR = texture(u_game, v_uv + vec2( texelSize.x, 0.0)).r;
      float sT = texture(u_game, v_uv + vec2(0.0, -texelSize.y)).r;
      float sB = texture(u_game, v_uv + vec2(0.0,  texelSize.y)).r;

      // Look up each neighbor's palette color
      vec3 cL = u_palette[clamp(int(sL * 255.0 + 0.5), 0, 3)];
      vec3 cR = u_palette[clamp(int(sR * 255.0 + 0.5), 0, 3)];
      vec3 cT = u_palette[clamp(int(sT * 255.0 + 0.5), 0, 3)];
      vec3 cB = u_palette[clamp(int(sB * 255.0 + 0.5), 0, 3)];

      // Gap color: blend the average of (self + neighbor) toward shade 0
      // in OKLAB. Shade 0 acts as the "reflector" — the base color the
      // LCD shows between segments. This ensures gaps are always visually
      // distinct from segment interiors, even between same-shade pixels.
      vec3 labShade0 = toOklab(u_palette[0]);
      vec3 labSelf = toOklab(pixelColor);
      float neighborAmt = 0.3;  // how much the neighbor influences the gap
      float liftAmt = 0.35;     // how much the gap lifts toward shade 0

      vec3 gapL = fromOklab(mix(mix(labSelf, toOklab(cL), neighborAmt), labShade0, liftAmt));
      vec3 gapR = fromOklab(mix(mix(labSelf, toOklab(cR), neighborAmt), labShade0, liftAmt));
      vec3 gapT = fromOklab(mix(mix(labSelf, toOklab(cT), neighborAmt), labShade0, liftAmt));
      vec3 gapB = fromOklab(mix(mix(labSelf, toOklab(cB), neighborAmt), labShade0, liftAmt));

      // ── Composite: per-edge gap blending (parallel, not sequential) ──
      // Each edge contributes independently from pixelColor to avoid
      // cross-axis contamination where left/right mixes would bleed
      // into top/bottom results.
      float gapL_w = 1.0 - edgeL;  // weight: how much we're in the left gap
      float gapR_w = 1.0 - edgeR;
      float gapT_w = 1.0 - edgeT;
      float gapB_w = 1.0 - edgeB;
      float totalGap = gapL_w + gapR_w + gapT_w + gapB_w;

      // Weighted average of all gap contributions, blended with pixel center
      vec3 lcdColor;
      if (totalGap < 0.001) {
        // Fully inside segment — no gap influence
        lcdColor = pixelColor;
      } else {
        vec3 gapBlend = (gapL * gapL_w + gapR * gapR_w + gapT * gapT_w + gapB * gapB_w) / totalGap;
        lcdColor = mix(pixelColor, gapBlend, min(totalGap, 1.0));
      }

      // ── Blend with flat palette mode ──
      vec3 flatColor = u_palette[shade];
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

    // Match canvas to actual display pixels — critical for
    // FRAGCOORD-based grid to align without moiré.
    const dpr = window.devicePixelRatio || 1;
    const rect = el.getBoundingClientRect();
    el.width = Math.round(rect.width * dpr) || SCREEN_W * 3;
    el.height = Math.round(rect.height * dpr) || SCREEN_H * 3;

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

    // Overlay texture — worn surface scratches
    overlayTexture = gl.createTexture()!;
    gl.activeTexture(gl.TEXTURE1);
    gl.bindTexture(gl.TEXTURE_2D, overlayTexture);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);

    // Upload a 1x1 black placeholder until the image loads
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, 1, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array([0, 0, 0, 255]));

    const overlayImg = new Image();
    overlayImg.onload = () => {
      if (!gl || !overlayTexture) return;

      // Pre-blur by downscaling then upscaling on a temporary canvas
      const scale = 0.125/4; // 1/8 resolution — controls blur amount
      const small = document.createElement("canvas");
      small.width = Math.max(1, Math.round(overlayImg.width * scale));
      small.height = Math.max(1, Math.round(overlayImg.height * scale));
      const sCtx = small.getContext("2d")!;
      sCtx.drawImage(overlayImg, 0, 0, small.width, small.height);

      const tmp = document.createElement("canvas");
      tmp.width = overlayImg.width;
      tmp.height = overlayImg.height;
      const ctx = tmp.getContext("2d")!;
      ctx.imageSmoothingEnabled = true;
      ctx.imageSmoothingQuality = "high";
      ctx.drawImage(small, 0, 0, tmp.width, tmp.height);

      gl.activeTexture(gl.TEXTURE1);
      gl.bindTexture(gl.TEXTURE_2D, overlayTexture);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, tmp);
      render();
    };
    overlayImg.src = wireframeUrl;

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
    gl.uniform1f(u_gridIntensity, 1.0); // full LCD grid
    gl.uniform1f(u_overlayIntensity, 0.03); // subtle worn surface

    // Upload palette
    const paletteFlat = new Float32Array(12);
    for (let i = 0; i < 4; i++) {
      paletteFlat[i * 3 + 0] = DMG_PALETTE[i][0];
      paletteFlat[i * 3 + 1] = DMG_PALETTE[i][1];
      paletteFlat[i * 3 + 2] = DMG_PALETTE[i][2];
    }
    gl.uniform3fv(u_palette, paletteFlat);

    // Initial render
    gl.viewport(0, 0, el.width, el.height);
    render();
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

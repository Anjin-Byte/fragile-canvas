// Encode the browser-baked substrate grain (extracted from a harness
// #compute --dump-dom capture) as an 8-bit grayscale PNG.
//
//   node encode_png.mjs <harness_compute.html> <out.png>
//
// The grain bytes come from Chrome itself (the exact original
// Image→canvas→magnitude pipeline), so the PNG's sample values are
// byte-identical to what the old runtime code computed. Zero-dependency
// PNG writer: IHDR + IDAT(zlib, filter 0) + IEND.

import { readFileSync, writeFileSync } from "node:fs";
import { deflateSync } from "node:zlib";

const [, , inPath, outPath] = process.argv;
if (!inPath || !outPath) {
  console.error("usage: node encode_png.mjs <harness_compute.html> <out.png>");
  process.exit(1);
}

// ─── Extract WxH:base64 from <pre id="grain"> ───────────────────────────
const html = readFileSync(inPath, "utf8");
const marker = '<pre id="grain"';
const start = html.indexOf(marker);
if (start === -1) throw new Error("no grain payload in dump");
const open = html.indexOf(">", start) + 1;
const close = html.indexOf("</pre>", open);
const payload = html.slice(open, close).trim();
const colon = payload.indexOf(":");
const [w, h] = payload.slice(0, colon).split("x").map(Number);
const bytes = Buffer.from(payload.slice(colon + 1), "base64");
if (bytes.length !== w * h) {
  throw new Error(`payload size mismatch: ${bytes.length} != ${w}x${h}`);
}
console.log(`grain: ${w}x${h}, ${bytes.length} bytes`);

// ─── PNG writer ─────────────────────────────────────────────────────────
const CRC_TABLE = new Int32Array(256);
for (let n = 0; n < 256; n++) {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  CRC_TABLE[n] = c;
}
function crc32(buf) {
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i++) c = CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}
function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
}

const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(w, 0);
ihdr.writeUInt32BE(h, 4);
ihdr[8] = 8;  // bit depth
ihdr[9] = 0;  // color type: grayscale
ihdr[10] = 0; // compression
ihdr[11] = 0; // filter method
ihdr[12] = 0; // interlace

// Scanlines: filter byte 0 + row. (Filter "up" would compress noise no
// better — grain is spatially uncorrelated.)
const raw = Buffer.alloc((w + 1) * h);
for (let y = 0; y < h; y++) {
  raw[y * (w + 1)] = 0;
  bytes.copy(raw, y * (w + 1) + 1, y * w, (y + 1) * w);
}
const idat = deflateSync(raw, { level: 9 });

const png = Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk("IHDR", ihdr),
  chunk("IDAT", idat),
  chunk("IEND", Buffer.alloc(0)),
]);
writeFileSync(outPath, png);
console.log(`wrote ${outPath}: ${(png.length / 1024 / 1024).toFixed(1)} MB`);

// Histogram sanity: grain should be broad, not clipped to a few values.
const hist = new Array(8).fill(0);
for (const b of bytes) hist[b >> 5]++;
console.log("value histogram (32-wide bins):", hist.map((v) => (v / bytes.length * 100).toFixed(1) + "%").join(" "));

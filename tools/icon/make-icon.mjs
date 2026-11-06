// Icona provvisoria (in attesa del Figma): microfono bianco su quadrato arrotondato.
// node tools/icon/make-icon.mjs && npx tauri icon tools/icon/icon.png
import { writeFileSync } from 'node:fs';
import { deflateSync } from 'node:zlib';

const N = 1024;
const px = Buffer.alloc(N * N * 4);
const sdRoundRect = (x, y, cx, cy, hw, hh, r) => {
  const qx = Math.abs(x - cx) - hw + r, qy = Math.abs(y - cy) - hh + r;
  return Math.hypot(Math.max(qx, 0), Math.max(qy, 0)) + Math.min(Math.max(qx, qy), 0) - r;
};
const cov = (d) => Math.max(0, Math.min(1, 0.5 - d));
for (let y = 0; y < N; y++) for (let x = 0; x < N; x++) {
  const bg = cov(sdRoundRect(x, y, 512, 512, 480, 480, 200));
  // capsula del microfono
  const cap = cov(sdRoundRect(x, y, 512, 400, 120, 220, 120));
  // arco del supporto: anello di raggio 210 sotto il centro della capsula, solo metà inferiore
  const ring = Math.abs(Math.hypot(x - 512, y - 430) - 210) - 26;
  const arc = y > 430 ? cov(ring) : 0;
  const stem = cov(sdRoundRect(x, y, 512, 730, 26, 70, 10));
  const base = cov(sdRoundRect(x, y, 512, 810, 150, 26, 26));
  const fg = Math.max(cap, arc, stem, base);
  const i = (y * N + x) * 4;
  const r = 79 + (255 - 79) * fg, g = 70 + (255 - 70) * fg, b = 229 + (255 - 229) * fg;
  px[i] = r; px[i + 1] = g; px[i + 2] = b; px[i + 3] = Math.round(255 * bg);
}
const raw = Buffer.alloc((N * 4 + 1) * N);
for (let y = 0; y < N; y++) { raw[y * (N * 4 + 1)] = 0; px.copy(raw, y * (N * 4 + 1) + 1, y * N * 4, (y + 1) * N * 4); }
const crcTable = Array.from({ length: 256 }, (_, n) => { let c = n; for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1; return c >>> 0; });
const crc = (buf) => { let c = 0xffffffff; for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8); return (c ^ 0xffffffff) >>> 0; };
const chunk = (type, data) => { const len = Buffer.alloc(4); len.writeUInt32BE(data.length); const td = Buffer.concat([Buffer.from(type), data]); const c = Buffer.alloc(4); c.writeUInt32BE(crc(td)); return Buffer.concat([len, td, c]); };
const ihdr = Buffer.alloc(13); ihdr.writeUInt32BE(N, 0); ihdr.writeUInt32BE(N, 4); ihdr[8] = 8; ihdr[9] = 6;
const png = Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk('IHDR', ihdr), chunk('IDAT', deflateSync(raw, { level: 9 })), chunk('IEND', Buffer.alloc(0))]);
writeFileSync(new URL('./icon.png', import.meta.url), png);
console.log('icon.png', png.length, 'byte');

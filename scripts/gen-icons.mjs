import { deflateSync, crc32 } from 'node:zlib';
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const out = join(dirname(fileURLToPath(import.meta.url)), '..', 'src-tauri', 'icons');
mkdirSync(out, { recursive: true });

const BARS = [0.28, 0.5, 0.82, 0.58, 0.36];
const ACCENT = [56, 189, 248];
const BG = [11, 13, 16];

function inRoundRect(x, y, w, h, r) {
  const cx = Math.min(Math.max(x, r), w - r);
  const cy = Math.min(Math.max(y, r), h - r);
  return (x - cx) ** 2 + (y - cy) ** 2 <= r * r;
}

function render(size, { background }) {
  const px = Buffer.alloc(size * size * 4);
  const SS = 4;
  const bw = size * 0.085;
  const gap = size * 0.065;
  const total = BARS.length * bw + (BARS.length - 1) * gap;
  const x0 = (size - total) / 2;
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      let r = 0, g = 0, b = 0, a = 0;
      for (let sy = 0; sy < SS; sy++) {
        for (let sx = 0; sx < SS; sx++) {
          const fx = x + (sx + 0.5) / SS;
          const fy = y + (sy + 0.5) / SS;
          let col = null;
          if (background && inRoundRect(fx, fy, size, size, size * 0.22)) {
            const t = fy / size;
            col = [BG[0] + 12 * t, BG[1] + 16 * t, BG[2] + 22 * t, 1];
          }
          for (let i = 0; i < BARS.length; i++) {
            const bx = x0 + i * (bw + gap);
            const bh = size * 0.62 * BARS[i];
            const by = (size - bh) / 2;
            if (fx >= bx && fx <= bx + bw && fy >= by && fy <= by + bh && inRoundRect(fx - bx, fy - by, bw, bh, bw / 2)) {
              col = [...ACCENT, 1];
            }
          }
          if (col) { r += col[0]; g += col[1]; b += col[2]; a += col[3]; }
        }
      }
      const n = SS * SS;
      const o = (y * size + x) * 4;
      if (a > 0) {
        px[o] = Math.round(r / a); px[o + 1] = Math.round(g / a); px[o + 2] = Math.round(b / a);
      }
      px[o + 3] = Math.round((a / n) * 255);
    }
  }
  return px;
}

function chunk(type, data) {
  const len = Buffer.alloc(4); len.writeUInt32BE(data.length);
  const td = Buffer.concat([Buffer.from(type), data]);
  const crc = Buffer.alloc(4); crc.writeUInt32BE(crc32(td) >>> 0);
  return Buffer.concat([len, td, crc]);
}

function png(size, rgba) {
  const raw = Buffer.alloc((size * 4 + 1) * size);
  for (let y = 0; y < size; y++) {
    raw[y * (size * 4 + 1)] = 0;
    rgba.copy(raw, y * (size * 4 + 1) + 1, y * size * 4, (y + 1) * size * 4);
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0); ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8; ihdr[9] = 6;
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', ihdr), chunk('IDAT', deflateSync(raw, { level: 9 })), chunk('IEND', Buffer.alloc(0)),
  ]);
}

function ico(images) {
  const head = Buffer.alloc(6); head.writeUInt16LE(1, 2); head.writeUInt16LE(images.length, 4);
  let offset = 6 + images.length * 16;
  const dirs = [], datas = [];
  for (const { size, data } of images) {
    const d = Buffer.alloc(16);
    d[0] = size >= 256 ? 0 : size; d[1] = size >= 256 ? 0 : size;
    d.writeUInt16LE(1, 4); d.writeUInt16LE(32, 6);
    d.writeUInt32LE(data.length, 8); d.writeUInt32LE(offset, 12);
    offset += data.length; dirs.push(d); datas.push(data);
  }
  return Buffer.concat([head, ...dirs, ...datas]);
}

const write = (name, buf) => { writeFileSync(join(out, name), buf); console.log('✓', name); };

write('32x32.png', png(32, render(32, { background: true })));
write('128x128.png', png(128, render(128, { background: true })));
write('128x128@2x.png', png(256, render(256, { background: true })));
write('icon.png', png(512, render(512, { background: true })));
write('tray.png', png(32, render(32, { background: false })));
write('icon.ico', ico([16, 32, 48, 64, 128, 256].map((s) => ({ size: s, data: png(s, render(s, { background: true })) }))));

function icns(entries) {
  const parts = entries.map(([type, size]) => {
    const data = png(size, render(size, { background: true }));
    const head = Buffer.alloc(8);
    head.write(type, 0, 'ascii');
    head.writeUInt32BE(data.length + 8, 4);
    return Buffer.concat([head, data]);
  });
  const head = Buffer.alloc(8);
  head.write('icns', 0, 'ascii');
  head.writeUInt32BE(8 + parts.reduce((n, b) => n + b.length, 0), 4);
  return Buffer.concat([head, ...parts]);
}

write('icon.icns', icns([['ic11', 32], ['ic12', 64], ['ic07', 128], ['ic13', 256], ['ic08', 256], ['ic09', 512], ['ic14', 1024], ['ic10', 1024]]));

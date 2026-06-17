// Generates compas app icons from the "Needle & Rose" brand mark (see the design
// handoff). Rasterizes an inline SVG with sharp to PNGs (256/128/32) and packs a
// PNG-embedded ICO for Windows. Run: `npm run icons` (from frontend/).
import sharp from "sharp";
import { writeFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const OUT = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "src-tauri", "icons");
mkdirSync(OUT, { recursive: true });

// The mark, centered on a dark rounded tile with a radial magenta glow.
function svg(size, { minorTicks = true } = {}) {
  const ringW = size <= 32 ? 8 : 3;
  const tickW = size <= 32 ? 7 : 4.5;
  const minor =
    minorTicks && size > 32
      ? `<g stroke="rgba(255,255,255,.22)" stroke-width="2" stroke-linecap="round">
           <line x1="98" y1="22" x2="92" y2="28"/><line x1="98" y1="98" x2="92" y2="92"/>
           <line x1="22" y1="98" x2="28" y2="92"/><line x1="22" y1="22" x2="28" y2="28"/></g>`
      : "";
  return Buffer.from(`<svg xmlns="http://www.w3.org/2000/svg" width="${size}" height="${size}" viewBox="0 0 256 256">
    <defs><radialGradient id="g" cx="50%" cy="32%" r="62%">
      <stop offset="0" stop-color="#241526"/><stop offset="1" stop-color="#0c0b10"/>
    </radialGradient></defs>
    <rect x="0" y="0" width="256" height="256" rx="${Math.round(size * 0.235 * (256 / size))}" fill="url(#g)"/>
    <g transform="translate(38 38) scale(1.5)">
      <circle cx="60" cy="60" r="54" fill="none" stroke="rgba(255,255,255,.20)" stroke-width="${ringW}"/>
      <g stroke="#ff2e7e" stroke-width="${tickW}" stroke-linecap="round">
        <line x1="60" y1="6" x2="60" y2="18"/><line x1="114" y1="60" x2="102" y2="60"/>
        <line x1="60" y1="114" x2="60" y2="102"/><line x1="6" y1="60" x2="18" y2="60"/>
      </g>
      ${minor}
      <polygon points="60,19 71,62 60,70 49,62" fill="#ff2e7e"/>
      <polygon points="60,101 49,58 60,50 71,58" fill="#3a3a45"/>
      <circle cx="60" cy="60" r="8" fill="#0a0a0c" stroke="#ff2e7e" stroke-width="3.5"/>
    </g>
  </svg>`);
}

async function png(size, opts) {
  return sharp(svg(size, opts)).resize(size, size).png().toBuffer();
}

function ico(pngBuf) {
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(1, 4);
  const entry = Buffer.alloc(16);
  entry[0] = 0; // width 256
  entry[1] = 0; // height 256
  entry.writeUInt16LE(1, 4);
  entry.writeUInt16LE(32, 6);
  entry.writeUInt32LE(pngBuf.length, 8);
  entry.writeUInt32LE(22, 12);
  return Buffer.concat([header, entry, pngBuf]);
}

const p256 = await png(256);
writeFileSync(join(OUT, "icon.png"), p256);
writeFileSync(join(OUT, "128x128.png"), await png(128));
writeFileSync(join(OUT, "32x32.png"), await png(32, { minorTicks: false }));
writeFileSync(join(OUT, "icon.ico"), ico(p256));
console.log(`icons written to ${OUT}`);

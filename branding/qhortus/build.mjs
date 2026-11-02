// Regenerates every file in this directory from the geometry below.
// Run from any directory whose node_modules holds @resvg/resvg-js:
//   npm i --no-save @resvg/resvg-js@2.6.2 && node path/to/branding/qhortus/build.mjs
import { writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const OUT = dirname(fileURLToPath(import.meta.url));
const { Resvg } = createRequire(join(process.cwd(), "noop.js"))("@resvg/resvg-js");

const COLOR = { ink: "#17322A", brass: "#B0843A", stone: "#DCE5D8", brassOnDark: "#D2A658" };

// Every length derives from R and φ — see the construction sheet in README.md.
const PHI = (1 + Math.sqrt(5)) / 2;
const R = 80, C = 100;
const w = R / PHI, h = R, r = w / 2;
const t = R / PHI ** 3, gap = t / PHI ** 2, over = R / PHI ** 2;
const ax0 = C - r, ax1 = C + r, ay1 = C + h / 2, acy = C - h / 2 + r;
const d = Math.SQRT1_2;
const sx = ax1 - t * d, sy = ay1 - t * d;
const b = (sx - C + sy - C) * d, c = (sx - C) ** 2 + (sy - C) ** 2 - R * R;
const L = -b + Math.sqrt(b * b - c) + over;
const ex = sx + L * d, ey = sy + L * d;
const f = (n) => +n.toFixed(2);

const disc =
  `M${C},${C - R}A${R},${R} 0 1 0 ${C},${C + R}A${R},${R} 0 1 0 ${C},${C - R}Z` +
  `M${f(ax0)},${f(ay1)}V${f(acy)}A${f(r)},${f(r)} 0 0 1 ${f(ax1)},${f(acy)}V${f(ay1)}Z`;

// The tail is cut out of the disc by a mask, not by painting the background
// colour over it — so the gap stays transparent on any surface.
const markBody = (fg, accent) => `<defs><mask id="q" maskUnits="userSpaceOnUse" x="0" y="0" width="200" height="200">
<rect width="200" height="200" fill="#fff"/>
<line x1="${f(sx - gap * d)}" y1="${f(sy - gap * d)}" x2="${f(ex + 20)}" y2="${f(ey + 20)}" stroke="#000" stroke-width="${f(t + 2 * gap)}"/>
</mask></defs>
<path d="${disc}" fill="${fg}" fill-rule="evenodd" mask="url(#q)"/>
<line x1="${f(sx)}" y1="${f(sy)}" x2="${f(ex)}" y2="${f(ey)}" stroke="${accent}" stroke-width="${f(t)}"/>`;

const svg = (body) => `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 200">\n${body}\n</svg>\n`;

// Tile: the uploaded logo/favicon. One image serves both site themes, so the
// mark sits on its own ink square instead of relying on the page background.
const S = 0.78; // mark bbox ≈ x 20–177, y 20–192; centred in the tile at this scale
const tile = `<rect width="200" height="200" rx="44" fill="${COLOR.ink}"/>
<g transform="translate(${f(100 - 98.4 * S)} ${f(100 - 106 * S)}) scale(${S})">
${markBody(COLOR.stone, COLOR.brassOnDark)}
</g>`;

const files = {
  "qhortus-mark.svg": svg(markBody(COLOR.ink, COLOR.brass)),
  "qhortus-mark-reversed.svg": svg(markBody(COLOR.stone, COLOR.brassOnDark)),
  "qhortus-tile.svg": svg(tile),
};
for (const [name, body] of Object.entries(files)) writeFileSync(join(OUT, name), body);

const png = (src, size) =>
  new Resvg(src, { fitTo: { mode: "width", value: size } }).render().asPng();

writeFileSync(join(OUT, "logo-512.png"), png(files["qhortus-tile.svg"], 512));

// ICO with PNG-encoded entries (valid since Vista; every current browser reads it).
const sizes = [16, 32, 48];
const images = sizes.map((s) => png(files["qhortus-tile.svg"], s));
const header = Buffer.alloc(6 + 16 * sizes.length);
header.writeUInt16LE(0, 0);
header.writeUInt16LE(1, 2);
header.writeUInt16LE(sizes.length, 4);
let offset = header.length;
sizes.forEach((s, i) => {
  const e = 6 + 16 * i;
  header.writeUInt8(s, e);
  header.writeUInt8(s, e + 1);
  header.writeUInt16LE(1, e + 4);
  header.writeUInt16LE(32, e + 6);
  header.writeUInt32LE(images[i].length, e + 8);
  header.writeUInt32LE(offset, e + 12);
  offset += images[i].length;
});
writeFileSync(join(OUT, "favicon.ico"), Buffer.concat([header, ...images]));

console.log(`wrote ${Object.keys(files).length + 2} files to ${OUT}`);

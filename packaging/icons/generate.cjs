// Renders the icon files of git-bull from `packaging/git-bull.svg`.
//
// The generated files are committed, so building or packaging git-bull needs
// none of this; run it only when the logo changes:
//
//   npm install --no-save @resvg/resvg-js@2.6.2
//   node packaging/icons/generate.cjs
//
// It writes:
//   packaging/icons/git-bull-<size>.png   Linux icon theme sizes
//   packaging/icons/git-bull.ico          Windows
//   packaging/icons/git-bull.icns         macOS, with the margin of its icon grid
//   crates/gitbull-app/assets/icon.png    the window icon, embedded in the binary

const fs = require("node:fs");
const path = require("node:path");
const { Resvg } = require("@resvg/resvg-js");

const root = path.resolve(__dirname, "..", "..");
const source = fs.readFileSync(path.join(root, "packaging", "git-bull.svg"), "utf8");

// The art fills the canvas, as Linux and Windows expect. macOS draws no frame
// around an icon, so its art stays inside 80% of the canvas, as in Apple's
// icon grid; the extra viewBox margin does that.
const MAC_MARGIN = 128;
const macSource = source.replace(
  'viewBox="0 0 1024 1024"',
  `viewBox="${-MAC_MARGIN} ${-MAC_MARGIN} ${1024 + 2 * MAC_MARGIN} ${1024 + 2 * MAC_MARGIN}"`,
);
if (macSource === source) throw new Error("the SVG has no 1024 viewBox to pad");

function png(svg, size) {
  return new Resvg(svg, { fitTo: { mode: "width", value: size } }).render().asPng();
}

function write(file, bytes) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, bytes);
  console.log(`${path.relative(root, file)} (${bytes.length} bytes)`);
}

// Linux: one PNG per icon theme size.
const icons = path.join(root, "packaging", "icons");
const linuxSizes = [16, 24, 32, 48, 64, 128, 256, 512];
for (const size of linuxSizes) write(path.join(icons, `git-bull-${size}.png`), png(source, size));

// Windows: an ICO holding PNG images, which Windows Vista and later read.
const icoSizes = [16, 24, 32, 48, 64, 128, 256];
const images = icoSizes.map((size) => png(source, size));
const header = Buffer.alloc(6);
header.writeUInt16LE(1, 2); // type: icon
header.writeUInt16LE(images.length, 4);
const directory = Buffer.alloc(16 * images.length);
let offset = header.length + directory.length;
images.forEach((image, i) => {
  const size = icoSizes[i];
  const entry = directory.subarray(16 * i, 16 * i + 16);
  entry.writeUInt8(size === 256 ? 0 : size, 0); // width; 0 means 256
  entry.writeUInt8(size === 256 ? 0 : size, 1); // height
  entry.writeUInt16LE(1, 4); // colour planes
  entry.writeUInt16LE(32, 6); // bits per pixel
  entry.writeUInt32LE(image.length, 8);
  entry.writeUInt32LE(offset, 12);
  offset += image.length;
});
write(path.join(icons, "git-bull.ico"), Buffer.concat([header, directory, ...images]));

// macOS: an ICNS of PNG images; the types name the pixel size.
const icnsTypes = [
  ["icp4", 16],
  ["icp5", 32],
  ["ic11", 32], // 16 pt at 2x
  ["ic12", 64], // 32 pt at 2x
  ["ic07", 128],
  ["ic13", 256], // 128 pt at 2x
  ["ic08", 256],
  ["ic14", 512], // 256 pt at 2x
  ["ic09", 512],
  ["ic10", 1024], // 512 pt at 2x
];
const rendered = new Map();
const chunks = icnsTypes.map(([type, size]) => {
  if (!rendered.has(size)) rendered.set(size, png(macSource, size));
  const data = rendered.get(size);
  const chunkHeader = Buffer.alloc(8);
  chunkHeader.write(type, 0, "ascii");
  chunkHeader.writeUInt32BE(8 + data.length, 4);
  return Buffer.concat([chunkHeader, data]);
});
const body = Buffer.concat(chunks);
const icnsHeader = Buffer.alloc(8);
icnsHeader.write("icns", 0, "ascii");
icnsHeader.writeUInt32BE(8 + body.length, 4);
write(path.join(icons, "git-bull.icns"), Buffer.concat([icnsHeader, body]));

// The window icon, for the title bar and taskbar of X11 and Windows.
write(path.join(root, "crates", "gitbull-app", "assets", "icon.png"), png(source, 256));

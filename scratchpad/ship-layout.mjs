// One frame of the designer's draw buffer, dumped as SVG.
//
// The same trick as `scratchpad/layout.rs` does for the room, and for the same
// reason: a bunk half inside the heads, a ghost drawn at the wrong scale, a
// use-spot marker on the wrong side of a turned engine are all obvious here
// and invisible in every assertion. `rsvg-convert` turns the output into a
// PNG. After any change to crates/ship/src/paint.rs this is worth thirty
// seconds.
//
//   node scratchpad/ship-layout.mjs empty   > /tmp/ship.svg
//   node scratchpad/ship-layout.mjs ship    > /tmp/ship.svg
//   node scratchpad/ship-layout.mjs ghost   > /tmp/ship.svg
//   node scratchpad/ship-layout.mjs spots   > /tmp/ship.svg
//
// It writes the SVG to stdout and a line about what it caught to stderr, so a
// redirect gives a clean file.

import { bootWasmPage } from "./stub.mjs";

const WHEN = process.argv[2] ?? "ship";

const FLOOR = 0;
const WALL = 1;
const ENGINE = 3;
const BUNK = 4;
const COLD_STORE = 5;
const WORKTOP = 6;
const HOB = 7;
const DISHWASHER = 8;
const TABLE = 9;
const CHAIR = 10;
const TOILET = 11;
const BASIN = 12;
const HYDRO_BAY = 13;
const BROOM_LOCKER = 14;

const page = await bootWasmPage({
  html: "web/ship.html",
  host: "web/ship.js",
  wasm: "web/ship.wasm",
  search: "?stock=1&area=20&players=1&slot=0",
});
const { byId, root, wasm } = page;
const canvas = byId.get("stage");
const tile = wasm.ship_tile();

function at(tx, ty) {
  const s = wasm.ship_view_scale();
  return {
    pointerId: 1,
    clientX: (tx * tile + tile / 2) * s + wasm.ship_view_x(),
    clientY: (ty * tile + tile / 2) * s + wasm.ship_view_y(),
  };
}

const pick = (kind) => root.querySelector(`[data-part="${kind}"]`).dispatch("click");

function drag(x0, y0, x1, y1, button = 0) {
  canvas.dispatch("pointerdown", { button, ...at(x0, y0) });
  canvas.dispatch("pointermove", { button, ...at(x1, y1) });
  canvas.dispatch("pointerup", { button, ...at(x1, y1) });
}

function put(kind, tx, ty) {
  pick(kind);
  drag(tx, ty, tx, ty);
}

function buildShip() {
  pick(FLOOR);
  drag(2, 2, 17, 17);
  pick(WALL);
  drag(1, 1, 18, 1);
  drag(1, 18, 18, 18);
  drag(1, 1, 1, 18);
  drag(18, 1, 18, 18);
  put(COLD_STORE, 3, 3);
  put(WORKTOP, 5, 3);
  put(HOB, 8, 3);
  put(DISHWASHER, 10, 3);
  put(TOILET, 12, 3);
  put(BASIN, 14, 3);
  put(BROOM_LOCKER, 16, 3);
  put(TABLE, 4, 6);
  put(HYDRO_BAY, 10, 10);
  put(ENGINE, 7, 13);
  put(BUNK, 14, 8);
  put(CHAIR, 4, 7);
}

let caught = "";
if (WHEN === "empty") {
  caught = "a bare build area — the grid, its edge, and nothing on it";
} else if (WHEN === "ghost") {
  // A turned engine hanging over the edge of the deck: the ghost should be
  // red, and the use spot should be north of it rather than west.
  pick(FLOOR);
  drag(2, 2, 10, 10);
  pick(ENGINE);
  page.fire("keydown", { key: "r" });
  canvas.dispatch("pointermove", at(9, 6));
  caught = "a turned engine half off the deck — the ghost is refused";
} else if (WHEN === "spots") {
  // Resting on a placed part: it is ringed, and its use spots are marked.
  pick(FLOOR);
  drag(2, 2, 12, 12);
  put(ENGINE, 6, 5);
  canvas.dispatch("pointermove", at(7, 6));
  caught = "the pointer on an engine — ringed, with the tile it is used from";
} else {
  buildShip();
  canvas.dispatch("pointermove", at(3, 3));
  caught = "the whole reference ship, with the pointer on the cold store";
}

wasm.ship_render();

const stride = wasm.ship_stride();
const shapes = new Float32Array(wasm.memory.buffer, wasm.ship_draw_ptr(), wasm.ship_draw_len());

const scale = wasm.ship_view_scale();
const ox = wasm.ship_view_x();
const oy = wasm.ship_view_y();
const W = canvas.clientWidth;
const H = canvas.clientHeight;

const out = [
  `<svg xmlns="http://www.w3.org/2000/svg" width="${W}" height="${H}" viewBox="0 0 ${W} ${H}">`,
  `<rect width="${W}" height="${H}" fill="#0c1210"/>`,
  `<g transform="translate(${ox.toFixed(3)} ${oy.toFixed(3)}) scale(${scale.toFixed(5)})">`,
];

for (let i = 0; i < shapes.length; i += stride) {
  const [kind, x, y, w, h, rot, radius, line] = shapes.slice(i, i + 8);
  const r = Math.round(shapes[i + 8] * 255);
  const g = Math.round(shapes[i + 9] * 255);
  const b = Math.round(shapes[i + 10] * 255);
  const a = shapes[i + 11];
  const paint =
    line > 0
      ? `fill="none" stroke="rgb(${r},${g},${b})" stroke-opacity="${a}" stroke-width="${line}"`
      : `fill="rgb(${r},${g},${b})" fill-opacity="${a}"`;
  const spin = rot === 0 ? "" : ` transform="rotate(${((rot * 180) / Math.PI).toFixed(3)} ${x} ${y})"`;
  if (kind === 1) {
    out.push(
      `<ellipse cx="${x}" cy="${y}" rx="${Math.abs(w) / 2}" ry="${Math.abs(h) / 2}" ${paint}${spin}/>`,
    );
  } else {
    out.push(
      `<rect x="${x - w / 2}" y="${y - h / 2}" width="${Math.abs(w)}" height="${Math.abs(h)}"` +
        (radius > 0 ? ` rx="${radius}"` : "") +
        ` ${paint}${spin}/>`,
    );
  }
}

out.push("</g></svg>");
process.stdout.write(out.join("\n") + "\n");
process.stderr.write(
  `${WHEN}: ${caught}\n` +
    `  ${shapes.length / stride} shapes, ${wasm.ship_part_total()} parts, ` +
    `scale ${scale.toFixed(3)}\n`,
);

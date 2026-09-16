// One frame of the World tab's draw buffer, dumped as SVG.
//
// The same trick as `scratchpad/ship-layout.mjs`, for the same reason: a
// galaxy drawn mirrored, a halo ten times too big, a station square sitting
// under its planet instead of on it, a marker at the wrong star are all
// obvious here and invisible in every assertion. `rsvg-convert` turns the
// output into a PNG.
//
//   node scratchpad/lobby-layout.mjs galaxy  > /tmp/lobby.svg
//   node scratchpad/lobby-layout.mjs zoomed  > /tmp/lobby.svg
//   node scratchpad/lobby-layout.mjs system  > /tmp/lobby.svg
//
// `galaxy` is the reference galaxy at the fit with a star hovered, a start
// picked and a ping going; `zoomed` is the same wheeled in on the start;
// `system` is the side panel's diagram for the start's system, with the
// host's labels drawn on as text.
//
// It writes the SVG to stdout and a line about what it caught to stderr, so
// a redirect gives a clean file.

import { readFileSync } from "node:fs";

import { bootWasmPage } from "./stub.mjs";

const WHEN = process.argv[2] ?? "galaxy";

const fixtureSource = readFileSync("crates/worldgen/src/fixture.rs", "utf8");
const REFERENCE_SEED = BigInt(/REFERENCE_SEED: u64 = ([0-9a-fx_]+);/.exec(fixtureSource)[1].replace(/_/g, ""));

const page = await bootWasmPage({
  html: "web/builder.html",
  host: "web/builder.js",
  wasm: "web/lobby.wasm",
});
const { byId, click, dispatch, wasm, step, drawn } = page;

click("play");
byId.get("setup-tool").querySelectorAll(".tool-tabs .tab")[1].dispatch("click");
step(1);
byId.get("seed").value = REFERENCE_SEED.toString();
dispatch("seed-form", "submit");

const canvas = byId.get("galaxy");
const systemCanvas = byId.get("system-canvas");

// A star with a station, to pick, and another to ping.
const lit = [];
for (let star = 0; star < wasm.lobby_star_count() && lit.length < 2; star++) {
  if (wasm.lobby_star_has_station(star) === 1) lit.push(star);
}
const [start, other] = lit;
const on = (star) => ({ clientX: wasm.lobby_star_screen_x(star), clientY: wasm.lobby_star_screen_y(star), pointerId: 1, button: 0 });

canvas.dispatch("pointerdown", on(start));
canvas.dispatch("pointerup", on(start));
byId.get("stations").querySelector("[data-pick]").dispatch("click");
page.win.bimsDeliver("suggestion", { from: "Ada", star: other, station: 0 });
step(20);
canvas.dispatch("pointermove", on(other));

let caught = "";
let W;
let H;
if (WHEN === "zoomed") {
  for (let i = 0; i < 6; i++) canvas.dispatch("wheel", { ...on(start), deltaY: -600 });
  canvas.dispatch("pointermove", on(start));
  step(1);
  wasm.lobby_render();
  W = canvas.clientWidth;
  H = canvas.clientHeight;
  caught = "the reference galaxy wheeled in on the start, which is hovered";
} else if (WHEN === "system") {
  // The stub gives every canvas the same size, and the labels were placed
  // for that size by the frame that painted them, so the dump is at it too.
  W = systemCanvas.clientWidth;
  H = systemCanvas.clientHeight;
  // Open the star again so the panel is repainted on the next frame — the
  // labels are drawn only when it is — with `drawn` emptied just before.
  drawn.length = 0;
  canvas.dispatch("pointerdown", on(start));
  canvas.dispatch("pointerup", on(start));
  step(1);
  wasm.lobby_render_system(W, H);
  caught = `the system of star ${start} — the start ringed, the host's labels on`;
} else {
  step(1);
  wasm.lobby_render();
  W = canvas.clientWidth;
  H = canvas.clientHeight;
  caught = `the reference galaxy at the fit — the start marked at star ${start}, a ping at ${other}, which is hovered`;
}

const stride = wasm.lobby_stride();
const shapes = new Float32Array(wasm.memory.buffer, wasm.lobby_draw_ptr(), wasm.lobby_draw_len());

const out = [
  `<svg xmlns="http://www.w3.org/2000/svg" width="${W}" height="${H}" viewBox="0 0 ${W} ${H}">`,
  `<rect width="${W}" height="${H}" fill="#0c1210"/>`,
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

// The host's labels, as it drew them onto the system canvas.
if (WHEN === "system") {
  for (const d of drawn) {
    out.push(
      `<text x="${d.x}" y="${d.y}" fill="${d.fill}" font-family="sans-serif" font-size="11" dominant-baseline="middle">${d.text}</text>`,
    );
  }
}

out.push("</svg>");
process.stdout.write(out.join("\n") + "\n");
process.stderr.write(
  `${WHEN}: ${caught}\n  ${shapes.length / stride} shapes; hovered ${byId.get("hover-said").textContent || "nothing"}; ` +
    `start ${byId.get("spawn-said").textContent}\n`,
);

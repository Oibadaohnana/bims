// One frame of the designer's draw buffer, dumped as SVG.
//
// The same trick as `scratchpad/layout.rs` does for the room, and for the same
// reason: a bunk half inside the heads, a ghost drawn at the wrong scale, a
// use-spot marker on the wrong side of a turned engine are all obvious here
// and invisible in every assertion. `rsvg-convert` turns the output into a
// PNG. After any change to crates/ship/src/paint.rs this is worth thirty
// seconds.
//
//   node scratchpad/ship-layout.mjs empty    > /tmp/ship.svg
//   node scratchpad/ship-layout.mjs ship     > /tmp/ship.svg
//   node scratchpad/ship-layout.mjs ghost    > /tmp/ship.svg
//   node scratchpad/ship-layout.mjs spots    > /tmp/ship.svg
//   node scratchpad/ship-layout.mjs exposure > /tmp/ship.svg
//   node scratchpad/ship-layout.mjs game     > /tmp/ship.svg
//   node scratchpad/ship-layout.mjs map      > /tmp/ship.svg
//   node scratchpad/ship-layout.mjs given    > /tmp/ship.svg
//   node scratchpad/ship-layout.mjs headup   > /tmp/ship.svg
//   node scratchpad/ship-layout.mjs mapup    > /tmp/ship.svg
//   node scratchpad/ship-layout.mjs turn     > /tmp/ship.svg
//   node scratchpad/ship-layout.mjs burn     > /tmp/ship.svg
//   node scratchpad/ship-layout.mjs docked   > /tmp/ship.svg
//   node scratchpad/ship-layout.mjs residents > /tmp/ship.svg
//   node scratchpad/ship-layout.mjs approach > /tmp/ship.svg
//   node scratchpad/ship-layout.mjs crossing > /tmp/ship.svg
//   node scratchpad/ship-layout.mjs simulation > /tmp/ship.svg
//   node scratchpad/ship-layout.mjs station  > /tmp/ship.svg
//
// `game` and `map` are the game, and they are the ones worth the thirty
// seconds: the ship is drawn **turned** and the sky behind it is not, and
// there is no assertion anywhere that can tell you a hull has come out
// mirrored or a starfield has come out rotating with it. `game` catches the
// ship at a heading of about a fifth of a turn for exactly that reason.
// `headup` and `mapup` are the same two moments with the view head up:
// the hull square to the window and the sky and the map turned round it —
// and a corner of the window the turned starfield no longer reaches is the
// thing to look for there. `turn` and `burn` catch the exhaust: the ship
// mid-align with the thrusters puffing, and under the engines with the
// plume out of the stern. A puff going *into* the hull, or a flame over the
// deck, is what to look for. `docked` is the ship at its berth, zoomed out
// so the whole station is in the picture beside it — the two airlocks
// should meet, both open, and the hull should be clear of the station's.
// `residents` stands the ship fifty tiles off the nearest station somebody
// lives on and lets it run an hour: the station's room is open, the people
// on it should be on its deck, and its fixtures should be the room's
// pictures rather than coloured blocks. `approach` is the ship six hundred
// tiles short of a station, zoomed right out: the station should be a plate
// with its icon on it, getting nearer, and nothing inside it drawn.
// `crossing` sends the crew member onto the station's deck and catches them
// partway: docked, the two are one room, and the Bim should be in the tube
// between the hulls or just through it, on the station's side.
// `simulation` and `station` are `ship.html?mode=1` as `nix run
// .#simulation` opens it, with no ship built by hand at all: the playtest
// ship docked at its spawn, head up, an hour in, with the station's rooms
// beside it — the chamfered bow, the bulkheads with their doorways, the
// helm, the shelves and the shower are all drawn here and nowhere else.
// `simulation` is close in on the ship; `station` is zoomed out over the
// station, with its residents about their rooms.
//
// It writes the SVG to stdout and a line about what it caught to stderr, so a
// redirect gives a clean file.

import { buildFlyer, buildShip, designTools } from "./flyer.mjs";
import { simulationSpawn } from "./spawn.mjs";
import { bootWasmPage } from "./stub.mjs";

const WHEN = process.argv[2] ?? "ship";

/** The few part kinds this file places by hand, beyond what `flyer.mjs`
 * builds: the pieces the `ghost`, `spots` and `exposure` moments want. */
const FLOOR = 0;
const WALL = 1;
const ENGINE = 3;
const HOB = 7;
const BUNK = 4;
const OUTSIDE_WALL = 16;

// A design page wants a spawn in its query or it shows the error screen; the
// simulation's is the one a harness can get without a lobby.
const spawn = await simulationSpawn();

const page = await bootWasmPage({
  html: "web/ship.html",
  host: "web/ship.js",
  wasm: "web/ship.wasm",
  // The game needs a ship a trip can be planned for, and one of those costs a
  // good deal more than a ship you can merely live on.
  // An empty grid, because every moment below builds its own ship — except
  // `given`, which is what a player actually opens on: the playtest ship in
  // the middle of the lobby's forty-tile grid.
  search:
    WHEN === "given"
      ? `?money=100000&area=40&players=1&slot=0&preset=1${spawn.query}`
      : WHEN === "simulation" || WHEN === "station"
        ? "?mode=1"
        : `?money=200000&area=20&players=1&slot=0&preset=0${spawn.query}`,
});
const { byId, root, wasm } = page;
const designCanvas = byId.get("stage");
const gameCanvas = byId.get("game-stage");
let canvas = designCanvas;
const tools = designTools(page);
const { at, pick, drag, put } = tools;

let caught = "";
if (WHEN === "given") {
  canvas.dispatch("pointermove", at(16, 17));
  caught = "the playtest ship as the designer opens on it, with the pointer on the hob";
} else if (WHEN === "simulation" || WHEN === "station") {
  canvas = gameCanvas;
  wasm.ship_set_head_up(1);
  wasm.ship_cmd_speed(0, 4);
  for (let i = 0; i < 60 * 60; i++) page.step(1);
  const middle = { x: gameCanvas.clientWidth / 2, y: gameCanvas.clientHeight / 2 };
  const station = wasm.ship_docked_at() - 1;
  const dx = wasm.ship_station_x(station) - wasm.ship_world_x();
  const dy = wasm.ship_station_y(station) - wasm.ship_world_y();
  if (WHEN === "station") {
    wasm.ship_zoom(middle.x, middle.y, 0.3);
    wasm.ship_pan(-dx * wasm.ship_view_scale(), dy * wasm.ship_view_scale());
  } else {
    wasm.ship_zoom(middle.x, middle.y, 0.8);
  }
  page.step(1);
  caught =
    WHEN === "station"
      ? `station ${station} an hour after the world opened, its rooms and its residents, with the ship at its port`
      : "the playtest ship docked at its spawn an hour in, head up, close enough to read the fittings";
} else if (WHEN === "empty") {
  caught = "a bare build area — the grid, its edge, and nothing on it";
} else if (WHEN === "ghost") {
  // A turned engine hanging over the edge of the deck: the ghost should be
  // red, and the use spot should be north of it rather than west.
  //
  // Bare frame beyond the deck on purpose: it has to read as *something
  // built* rather than as empty build area, and this is the only view that
  // shows the two side by side. Plating lays frame and deck together, so the
  // bare part is plated and then peeled back to its frame.
  pick(FLOOR);
  drag(2, 2, 14, 14);
  drag(11, 2, 14, 14, 2);
  drag(2, 11, 10, 14, 2);
  pick(ENGINE);
  page.fire("keydown", { key: "r" });
  canvas.dispatch("pointermove", at(9, 6));
  caught =
    "a turned engine half off the deck — the ghost is refused, with bare " +
    "frame beyond the plating";
} else if (WHEN === "spots") {
  // Resting on a placed part: it is ringed, and its use spots are marked.
  pick(FLOOR);
  drag(2, 2, 12, 12);
  put(ENGINE, 6, 5);
  canvas.dispatch("pointermove", at(7, 6));
  caught = "the pointer on an engine — ringed, with the tile it is used from";
} else if (WHEN === "exposure") {
  // A room with a hole in its hull. The tint over every tile the outside can
  // see into is the one piece of this page nothing else can show: it is not
  // tied to the issue list, so there is no row to point at and no assertion
  // that can tell you it looks right.
  pick(FLOOR);
  drag(3, 3, 14, 14);
  pick(OUTSIDE_WALL);
  drag(3, 3, 14, 3);
  drag(3, 14, 14, 14);
  drag(3, 3, 3, 14);
  drag(14, 3, 14, 14);
  // Two holes: one in the wall, and an internal wall that is not hull, so
  // the far room stays lit through it.
  drag(8, 3, 8, 3, 2);
  pick(WALL);
  drag(4, 9, 13, 9);
  put(HOB, 6, 6);
  put(BUNK, 11, 11);
  caught =
    "a hull with a hole in it — every tile the outside can see into, tinted, " +
    "including through the internal wall";
} else if (["game", "map", "headup", "mapup", "turn", "burn", "docked", "residents", "approach", "crossing", "undocking", "docking"].includes(WHEN)) {
  // A ship a trip can actually be planned for: everything `buildShip` puts
  // down, plus what flying wants — thrusters, an airlock, an array, a tank
  // and something to burn.
  buildFlyer(tools);
  byId.get("accept").dispatch("click");
  page.step(1);
  canvas = gameCanvas;

  const headUp = WHEN === "headup" || WHEN === "mapup";
  wasm.ship_set_head_up(headUp ? 1 : 0);
  if (WHEN === "map" || WHEN === "mapup") {
    wasm.ship_set_view_mode(1);
    // Under way for the same reason `game` is: a map at a heading of nothing
    // says nothing about which way it turns.
    if (headUp) {
      wasm.ship_man_helm_for_probe(0);
      wasm.ship_cmd_confirm_point(0, wasm.ship_world_x() + 400000, wasm.ship_world_y() + 90000);
      wasm.ship_cmd_speed(0, 4);
      for (let i = 0; i < 700; i++) page.step(1);
    }
    caught = headUp
      ? "the system map turned round the ship, with the marker straight up"
      : "the system map — the star, what has been found, the ring the scanner " +
        "reaches to, and the ship pointing where it is pointing";
  } else if (WHEN === "docked") {
    wasm.ship_set_head_up(1);
    const middle = { x: gameCanvas.clientWidth / 2, y: gameCanvas.clientHeight / 2 };
    wasm.ship_zoom(middle.x, middle.y, 0.22);
    // The station is to one side of the ship; pan so both are in the frame.
    const station = wasm.ship_docked_at() - 1;
    const dx = wasm.ship_station_x(station) - wasm.ship_world_x();
    const dy = wasm.ship_station_y(station) - wasm.ship_world_y();
    wasm.ship_pan(-dx * wasm.ship_view_scale() * 0.5, dy * wasm.ship_view_scale() * 0.5);
    page.step(1);
    caught = `the ship docked at station ${station}, airlock to airlock, with the whole station beside it`;
  } else if (WHEN === "undocking" || WHEN === "docking") {
    // Halfway through the push-off, or halfway through coming alongside:
    // the ship a little off the station's door, and for the docking, still
    // turning onto the berth's heading. Both are read off the clock, so
    // the moment is a number of steps.
    const station = wasm.ship_docked_at() - 1;
    wasm.ship_man_helm_for_probe(0);
    // Not far, for the docking: it has to come back inside the budget.
    const reach = WHEN === "undocking" ? 400000 : 30000;
    wasm.ship_cmd_confirm_point(0, wasm.ship_world_x() + reach, wasm.ship_world_y() + reach / 4);
    let budget = 40000;
    while (budget-- > 0 && wasm.ship_world_state() !== 4) page.step(1);
    if (WHEN === "undocking") {
      for (let i = 0; i < 90; i++) page.step(1);
    } else {
      // Out, round, and back to the door it left; catch it coming alongside.
      wasm.ship_cmd_speed(0, 4);
      while (budget-- > 0 && wasm.ship_world_state() !== 1) page.step(1);
      wasm.ship_man_helm_for_probe(0);
      wasm.ship_cmd_confirm_node(0, 1, station);
      while (budget-- > 0 && wasm.ship_world_state() !== 5) page.step(1);
      wasm.ship_cmd_speed(0, 1);
      for (let i = 0; i < 150; i++) page.step(1);
    }
    const middle = { x: gameCanvas.clientWidth / 2, y: gameCanvas.clientHeight / 2 };
    wasm.ship_zoom(middle.x, middle.y, 0.12);
    const dx = wasm.ship_station_x(station) - wasm.ship_world_x();
    const dy = wasm.ship_station_y(station) - wasm.ship_world_y();
    wasm.ship_pan(-dx * wasm.ship_view_scale() * 0.5, dy * wasm.ship_view_scale() * 0.5);
    page.step(1);
    caught =
      WHEN === "undocking"
        ? `the ship halfway through pushing off station ${station}'s door, state ${wasm.ship_world_state()}`
        : `the ship halfway through coming alongside station ${station}, state ${wasm.ship_world_state()}, heading ${((wasm.ship_world_heading() * 180) / Math.PI).toFixed(0)}°`;
  } else if (WHEN === "approach") {
    const station = wasm.ship_docked_at() - 1;
    const away = wasm.ship_station_radius(station) + 600 * wasm.ship_tile();
    wasm.ship_put_for_probe(wasm.ship_station_x(station) + away, wasm.ship_station_y(station));
    page.step(1);
    const middle = { x: gameCanvas.clientWidth / 2, y: gameCanvas.clientHeight / 2 };
    wasm.ship_zoom(middle.x, middle.y, 0.02);
    wasm.ship_pan(away * wasm.ship_view_scale() * 0.5, 0);
    page.step(1);
    caught = `the ship six hundred tiles off station ${station}, which is a plate with its icon on it`;
  } else if (WHEN === "crossing") {
    wasm.ship_set_head_up(1);
    page.step(1);
    // The station's deck, to starboard of the crew member, off the hull.
    const s = wasm.ship_view_scale();
    const p = {
      x: wasm.ship_view_x() + wasm.ship_crew_x(0) * s,
      y: wasm.ship_view_y() + wasm.ship_crew_y(0) * s,
    };
    let over = null;
    for (let d = 60; d < 2000 && !over; d += 20) {
      const q = { x: p.x + d * s, y: p.y };
      gameCanvas.dispatch("pointermove", { pointerId: 3, clientX: q.x, clientY: q.y });
      const spot = wasm.bims_spot_at(wasm.ship_room_x(q.x, q.y), wasm.ship_room_y(q.x, q.y));
      if (wasm.ship_game_tile_inside() === 0 && spot === 1) over = { x: q.x + 8 * wasm.ship_tile() * s, y: q.y };
    }
    if (!over) throw new Error("no station deck to starboard");
    gameCanvas.dispatch("pointerdown", { pointerId: 3, button: 2, clientX: over.x, clientY: over.y });
    // Until the Bim is past the ship's skin: the airlock is at the hull's
    // edge, and a few tiles beyond it is the tube.
    const skin = wasm.ship_room_x(over.x, over.y) - 7 * wasm.ship_tile();
    let budget = 20000;
    while (budget-- > 0 && wasm.bims_bim_x(0) < skin) page.step(1);
    const middle = { x: gameCanvas.clientWidth / 2, y: gameCanvas.clientHeight / 2 };
    wasm.ship_zoom(middle.x, middle.y, 0.6);
    wasm.ship_pan(-(wasm.ship_crew_x(0)) * wasm.ship_view_scale() * 0.9, -(wasm.ship_crew_y(0)) * wasm.ship_view_scale() * 0.9);
    page.step(1);
    caught = "the crew member crossing from the ship to the station through the mated airlocks";
  } else if (WHEN === "residents") {
    let lived = -1;
    for (let id = 0; id < 8 && lived < 0; id++) {
      if (wasm.ship_station_residents(id) > 0) lived = id;
    }
    if (lived < 0) throw new Error("nobody lives on any station of the spawn system");
    const radius = wasm.ship_station_radius(lived);
    wasm.ship_put_for_probe(wasm.ship_station_x(lived) + radius + 20 * wasm.ship_tile(), wasm.ship_station_y(lived));
    wasm.ship_cmd_speed(0, 4);
    for (let i = 0; i < 60 * 60; i++) page.step(1);
    const middle = { x: gameCanvas.clientWidth / 2, y: gameCanvas.clientHeight / 2 };
    wasm.ship_zoom(middle.x, middle.y, 0.3);
    const dx = wasm.ship_station_x(lived) - wasm.ship_world_x();
    const dy = wasm.ship_station_y(lived) - wasm.ship_world_y();
    wasm.ship_pan(-dx * wasm.ship_view_scale() * 0.8, dy * wasm.ship_view_scale() * 0.8);
    page.step(1);
    caught = `the ship beside station ${lived}, with ${wasm.ship_resident_count()} residents up and about after an hour`;
  } else if (WHEN === "turn" || WHEN === "burn") {
    // Head up, so the exhaust is read against a hull that is square: a puff
    // from the wrong nozzle is a puff into the ship, and that is easier to
    // see when the ship is not also turned.
    wasm.ship_set_head_up(1);
    wasm.ship_man_helm_for_probe(0);
    wasm.ship_cmd_confirm_point(0, wasm.ship_world_x() + 400000, wasm.ship_world_y() + 90000);
    wasm.ship_cmd_speed(0, 4);
    // Phase codes are `flight::Phase`: 0 aligning, 1 burning.
    const want = WHEN === "turn" ? 0 : 1;
    // A little way into the phase rather than its first frame, so the turn
    // has a rate to show and the burn a flame that has flickered.
    let budget = 20000;
    while (budget-- > 0 && wasm.ship_trip_phase() !== want) page.step(1);
    for (let i = 0; i < 90; i++) page.step(1);
    caught =
      WHEN === "turn"
        ? "the ship turning to its bearing, thrusters puffing on the corners that turn it that way"
        : "the ship under its engines, with the plume out of the stern and nothing over the deck";
  } else {
    // Turned, on purpose. A hull drawn mirrored, or a starfield that turns
    // with the ship, is obvious here and invisible in every assertion.
    wasm.ship_man_helm_for_probe(0);
    wasm.ship_cmd_confirm_point(0, wasm.ship_world_x() + 400000, wasm.ship_world_y() + 90000);
    // At the top speed, because a ship turning at a tenth of a degree a
    // second would need ten real minutes to reach an interesting attitude.
    wasm.ship_cmd_speed(0, 4);
    for (let i = 0; i < 700; i++) page.step(1);
    caught =
      `the ship under way at a heading of ${((wasm.ship_world_heading() * 180) / Math.PI).toFixed(0)}°, ` +
      (headUp ? "held square to the window, with the sky turned round it" : "with the sky behind it square to the window");
  }
} else {
  buildShip(tools);
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
  `<g transform="translate(${ox.toFixed(3)} ${oy.toFixed(3)}) scale(${scale.toPrecision(9)})">`,
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
  } else if (kind === 2) {
    // The bottom-left half of the box, as the host draws it.
    const points = [
      [x - w / 2, y - h / 2],
      [x - w / 2, y + h / 2],
      [x + w / 2, y + h / 2],
    ]
      .map(([px, py]) => `${px},${py}`)
      .join(" ");
    out.push(`<polygon points="${points}" ${paint}${spin}/>`);
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
    `${wasm.ship_exposed_count()} exposed, scale ${scale.toPrecision(4)}\n`,
);

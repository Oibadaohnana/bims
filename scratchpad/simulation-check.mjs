// Does `nix run .#simulation` open on a ship that can fly?
//
// `ship.html?mode=1` is meant to skip the design phase and put the playtest
// ship at the simulation's dock with its purse and a full tank — and then
// to be a game: a trip confirmed from the map has to be flown to the end at
// the top speed. This boots that page against the real ship.wasm and does
// exactly that, and it reads the numbers it compares against out of the Rust
// sources rather than carrying copies: `SIMULATION_MONEY` from
// crates/world/src/data.rs and `PLAYTEST_HASH` from
// crates/shipdesign/src/fixture.rs.

import { readFileSync } from "node:fs";

import { bootWasmPage } from "./stub.mjs";

const fails = [];
function check(what, ok, extra = "") {
  if (ok) console.log(`  ok   ${what}`);
  else {
    console.log(`  FAIL ${what}${extra ? ` — ${extra}` : ""}`);
    fails.push(what);
  }
}

const literal = (text) => BigInt(text.replace(/_/g, ""));
const whole = (hi, lo) => (BigInt(hi >>> 0) << 32n) | BigInt(lo >>> 0);

const dataSource = readFileSync("crates/world/src/data.rs", "utf8");
const SIMULATION_MONEY = literal(/SIMULATION_MONEY: Money = ([0-9_]+);/.exec(dataSource)[1]);
const fixtureSource = readFileSync("crates/shipdesign/src/fixture.rs", "utf8");
const PLAYTEST_HASH = literal(/PLAYTEST_HASH: u64 = ([0-9a-fx_]+);/.exec(fixtureSource)[1]);
const PLAYTEST_PARTS = Number(/PLAYTEST_PARTS: u32 = (\d+);/.exec(fixtureSource)[1]);

/** `economy::Storage::FuelTank`. */
const FUEL_CLASS = 1;

const page = await bootWasmPage({
  html: "web/ship.html",
  host: "web/ship.js",
  wasm: "web/ship.wasm",
  search: "?mode=1",
});
const { byId, root, wasm, step } = page;

/** Step until `check` holds, or give up. */
function until(check, what, limit = 40000) {
  for (let i = 0; i < limit; i++) {
    if (check()) return i;
    step(1);
  }
  throw new Error(`never ${what} in ${limit} frames`);
}

const showing = () =>
  root
    .querySelectorAll("[data-screen]")
    .filter((s) => !s.hidden)
    .map((s) => s.dataset.screen)
    .join();

console.log("the simulation");
check("no error box", !byId.get("error").textContent, byId.get("error").textContent);
check("it opens on the game, with no design phase", showing() === "game", showing());
check("the world is open", wasm.ship_world_ready() === 1);
check("for one player", wasm.ship_players() === 1 && wasm.ship_local_slot() === 0);
check(
  "docked at the simulation's dock",
  wasm.ship_world_state() === 0 &&
    wasm.ship_docked_at() === wasm.ship_simulation_station() + 1 &&
    wasm.ship_world_star() === wasm.ship_simulation_star(),
  `state ${wasm.ship_world_state()}, dock ${wasm.ship_docked_at()}, star ${wasm.ship_world_star()}`,
);
check(
  "on the playtest ship",
  whole(wasm.ship_hash_hi(), wasm.ship_hash_lo()) === PLAYTEST_HASH && wasm.ship_part_total() === PLAYTEST_PARTS,
  `${whole(wasm.ship_hash_hi(), wasm.ship_hash_lo()).toString(16)}, ${wasm.ship_part_total()} parts`,
);
check(
  "with the simulation's purse",
  whole(wasm.ship_remaining_hi(), wasm.ship_remaining_lo()) === SIMULATION_MONEY,
  String(whole(wasm.ship_remaining_hi(), wasm.ship_remaining_lo())),
);
check(
  "and a full tank",
  wasm.ship_fuel_aboard() > 0 && wasm.ship_fuel_aboard() === wasm.ship_storage_capacity(FUEL_CLASS),
  `${wasm.ship_fuel_aboard()} of ${wasm.ship_storage_capacity(FUEL_CLASS)}`,
);
check("the station panel is up, because it is docked", byId.get("game-trade").hidden === false);

// And somebody is aboard: the simulation's one player, at the bunk, named.
step(1);
check("the Bim is aboard from the first step", wasm.ship_crew_count() === 1, String(wasm.ship_crew_count()));
check(
  "standing somewhere on the ship",
  Number.isFinite(wasm.ship_crew_x(0)) && Math.abs(wasm.ship_crew_x(0)) < 20 * wasm.ship_tile(),
  `${wasm.ship_crew_x(0)}, ${wasm.ship_crew_y(0)}`,
);
check("and named over their head", page.drawn.some((d) => d.text === "James"), page.drawn.map((d) => d.text).join(","));

// --- the deck ---------------------------------------------------------------
//
// The crew aboard are the room's Bims, and the room's own page is here with
// them: `web/crew.js`, the same tray, side and menus, wired to the same
// `bims_*` exports — which `ship.wasm` carries and `bims::host_aboard` points
// at the room the world is stepping. What this page adds is the pointer:
// a canvas point read back through the ship's camera into the room's units.

console.log("\nthe deck");
const deckCanvas = byId.get("game-stage");

/** Where a Bim is on the canvas, the way `paintCrewNames` works it out. */
const bimPixel = (who) => ({
  x: wasm.ship_view_x() + wasm.ship_crew_x(who) * wasm.ship_view_scale(),
  y: wasm.ship_view_y() + wasm.ship_crew_y(who) * wasm.ship_view_scale(),
});
const pointer = (p, more = {}) => ({ pointerId: 3, button: 0, clientX: p.x, clientY: p.y, ...more });

// The coordinate bridge, first: the pixel under James reads back as the room
// coordinates the room says he is at.
{
  const p = bimPixel(0);
  const back = { x: wasm.ship_room_x(p.x, p.y), y: wasm.ship_room_y(p.x, p.y) };
  check(
    "a Bim's pixel reads back as the Bim's room position",
    Math.hypot(back.x - wasm.bims_bim_x(0), back.y - wasm.bims_bim_y(0)) < 1,
    `${back.x}, ${back.y} against ${wasm.bims_bim_x(0)}, ${wasm.bims_bim_y(0)}`,
  );
}

const sheets = () => byId.get("side").querySelectorAll(".crew").filter((c) => !c.hidden).length;
check("the player's sheet is up to begin with, as in the room", sheets() === 1 && wasm.bims_is_selected(0) === 1);

// Escape is the settings sheet — every key explained — and Escape again
// puts it away. It does not touch the selection.
const settings = byId.get("settings");
check("the settings sheet starts hidden", settings.hidden === true);
page.fire("keydown", { key: "Escape" });
step(1);
check("Escape opens the settings sheet", settings.hidden === false);
check("and leaves the selection alone", wasm.bims_is_selected(0) === 1 && sheets() === 1);
// The stub keeps no markup text, so the sheet's words are read off the file.
// Every key the page handles has to be on it as a <kbd>: a key added to the
// handler and not to the sheet is a key nobody can find.
{
  const html = readFileSync("web/ship.html", "utf8");
  const sheet = html.slice(html.indexOf('id="settings"'), html.indexOf("</div>\n    </div>", html.indexOf('id="settings"')));
  const source = readFileSync("web/ship.js", "utf8");
  const handled = new Set();
  for (const m of source.matchAll(/key === "([a-z0-9]+)"/g)) handled.add(m[1]);
  for (const m of source.matchAll(/"([a-z]+)"\.includes\(key\)/g)) for (const k of m[1]) handled.add(k);
  const label = (k) => (k === "escape" ? "Esc" : k.toUpperCase());
  const missing = [...handled].filter((k) => !sheet.includes(`<kbd>${label(k)}</kbd>`));
  check("the sheet names every key the page handles", handled.size >= 6 && missing.length === 0, `handled ${[...handled].join(" ")}; missing ${missing.join(" ")}`);
}
page.fire("keydown", { key: "1" });
check("keys do nothing under the sheet", settings.hidden === false);
page.fire("keydown", { key: "Escape" });
step(1);
check("Escape again closes it", settings.hidden === true);
byId.get("settings-close").dispatch("click");
page.fire("keydown", { key: "Escape" });
byId.get("settings-close").dispatch("click");
check("so does its button", settings.hidden === true);

/** Clear the selection, the room's way, for the checks below that want to
 * start from nobody selected. */
const deselect = () => {
  wasm.bims_clear_selection();
  step(1);
};
deselect();
check("nobody is selected to begin the click checks", wasm.bims_selected_count() === 0 && sheets() === 0, `${sheets()} sheets`);
page.fire("keydown", { key: "1" });
step(1);
check("1 selects the player", wasm.bims_is_selected(0) === 1 && sheets() === 1);

// A click on the Bim selects.
deselect();
deckCanvas.dispatch("pointerdown", pointer(bimPixel(0)));
deckCanvas.dispatch("pointerup", pointer(bimPixel(0)));
step(1);
check("clicking the Bim selects it", wasm.bims_is_selected(0) === 1 && sheets() === 1);

// A marquee dragged round the Bim selects it too — the box is handed to the
// room in its own units, so it is the room's test that decides.
deselect();
{
  const p = bimPixel(0);
  const s = wasm.ship_view_scale();
  deckCanvas.dispatch("pointerdown", pointer({ x: p.x - 60 * s, y: p.y - 60 * s }));
  deckCanvas.dispatch("pointermove", pointer({ x: p.x + 10 * s, y: p.y + 10 * s }));
  step(1);
  check("a marquee in progress selects nothing yet", wasm.bims_selected_count() === 0);
  deckCanvas.dispatch("pointerup", pointer({ x: p.x + 60 * s, y: p.y + 60 * s }));
  step(1);
  check("a marquee round the Bim selects it", wasm.bims_is_selected(0) === 1, String(wasm.bims_selected_count()));
}

// r recruits, and the badge in the bar says so; r again lets go.
const badge = byId.get("recruited");
check("the recruited badge starts hidden", badge.hidden === true);
page.fire("keydown", { key: "r" });
check("r recruits", wasm.bims_is_recruited() === 1 && badge.hidden === false);
page.fire("keydown", { key: "r" });
check("r again lets go", wasm.bims_is_recruited() === 0 && badge.hidden === true);

// A right-click on open deck is an order. Found by asking the room what is
// under each pixel, in the room's units, a body's width or so from the Bim.
{
  const p = bimPixel(0);
  const s = wasm.ship_view_scale();
  let target = null;
  for (let d = 60; d < 400 && !target; d += 20) {
    for (const [dx, dy] of [[d, 0], [-d, 0], [0, d], [0, -d]]) {
      const q = { x: p.x + dx * s, y: p.y + dy * s };
      const spot = wasm.bims_spot_at(wasm.ship_room_x(q.x, q.y), wasm.ship_room_y(q.x, q.y));
      if (spot === 1 && wasm.bims_hit_at(wasm.ship_room_x(q.x, q.y), wasm.ship_room_y(q.x, q.y)) === 0) {
        target = q;
        break;
      }
    }
  }
  check("there is open deck within reach of the Bim", target !== null);

  // The readout, first: it says what is under the pointer in the ship view,
  // and it is the design's word for a part and the room's for a fixture.
  // Open deck reads as deck plating, with the tile after it; a scan across
  // the hull finds a fixture the room names — the hob or the cold store —
  // and the readout says so in the room's words. Off the hull it is empty.
  const what = byId.get("game-what");
  deckCanvas.dispatch("pointermove", pointer(target));
  step(1);
  check(
    "the readout names the deck under the pointer",
    /^Deck plating · -?\d+, -?\d+$/.test(what.textContent),
    JSON.stringify(what.textContent),
  );
  // Ten tiles either way: the bunk is to starboard and the galley to
  // port, at opposite sides of the main deck.
  let fixture = null;
  outer: for (let dy = -20; dy <= 20; dy++) {
    for (let dx = -20; dx <= 20; dx++) {
      const q = { x: p.x + dx * 26 * s, y: p.y + dy * 26 * s };
      const spot = wasm.bims_spot_at(wasm.ship_room_x(q.x, q.y), wasm.ship_room_y(q.x, q.y));
      if (spot === 5 || spot === 6) {
        fixture = { q, spot };
        break outer;
      }
    }
  }
  check("the galley is somewhere under the glass", fixture !== null);
  if (fixture) {
    deckCanvas.dispatch("pointermove", pointer(fixture.q));
    step(1);
    const name = fixture.spot === 5 ? "Cold store" : "Hob";
    check(
      "and the room's name wins for a fixture it knows",
      what.textContent.startsWith(name),
      JSON.stringify(what.textContent),
    );
  }
  deckCanvas.dispatch("pointerleave", {});
  step(1);
  check("off the canvas it says nothing", what.textContent === "", JSON.stringify(what.textContent));
  const before = { x: wasm.bims_bim_x(0), y: wasm.bims_bim_y(0) };
  deckCanvas.dispatch("pointerdown", pointer(target, { button: 2 }));
  step(1);
  const order = page.lastCall("bims_order_move");
  check("a right-click on the deck is an order", order !== undefined, "bims_order_move was never called");
  // Run a while at 1x: the world steps sixty times a real second, and a
  // Bim's walk is the room's own.
  for (let i = 0; i < 240; i++) step(1);
  const after = { x: wasm.bims_bim_x(0), y: wasm.bims_bim_y(0) };
  check(
    "and the Bim sets off",
    Math.hypot(after.x - before.x, after.y - before.y) > 10,
    `moved ${Math.hypot(after.x - before.x, after.y - before.y).toFixed(1)}`,
  );
}

// Docked, the station is in the ship's room: a right-click on the station's
// deck is an order like any other, and the Bim walks through the airlocks
// to it. The station's deck is found by scanning to starboard from the
// Bim — the ship docks with its airlock to the station, and the readout
// says when the pointer has left the hull.
console.log("\nthrough the airlock");
{
  check("the world opened docked", wasm.ship_world_state() === 0);
  check("with the station's residents in the room", wasm.bims_crew() > wasm.ship_crew_count(), `${wasm.bims_crew()} in the room, ${wasm.ship_crew_count()} crew`);
  const p = bimPixel(0);
  const s = wasm.ship_view_scale();
  let over = null;
  for (let d = 60; d < 2000 && !over; d += 20) {
    const q = { x: p.x + d * s, y: p.y };
    deckCanvas.dispatch("pointermove", pointer(q));
    const spot = wasm.bims_spot_at(wasm.ship_room_x(q.x, q.y), wasm.ship_room_y(q.x, q.y));
    if (wasm.ship_game_tile_inside() === 0 && spot === 1) over = q;
  }
  // Well onto the station rather than the first tile of the passage, so that
  // "arrived" cannot be satisfied from the ship's own airlock.
  if (over) {
    const further = { x: over.x + 6 * wasm.ship_tile() * s, y: over.y };
    deckCanvas.dispatch("pointermove", pointer(further));
    const spot = wasm.bims_spot_at(wasm.ship_room_x(further.x, further.y), wasm.ship_room_y(further.x, further.y));
    if (spot === 1) over = further;
  }
  check("there is station deck to starboard, off the hull", over !== null);
  if (over) {
    deckCanvas.dispatch("pointerdown", pointer(over, { button: 2 }));
    step(1);
    const target = { x: wasm.ship_room_x(over.x, over.y), y: wasm.ship_room_y(over.x, over.y) };
    let arrived = false;
    wasm.ship_cmd_speed(0, 4);
    for (let i = 0; i < 6000 && !arrived; i++) {
      step(1);
      const at = { x: wasm.bims_bim_x(0), y: wasm.bims_bim_y(0) };
      if (Math.hypot(at.x - target.x, at.y - target.y) < 2 * wasm.ship_tile()) arrived = true;
    }
    check("and the Bim walks through the airlock onto the station", arrived);
    const q = bimPixel(0);
    deckCanvas.dispatch("pointermove", pointer(q));
    check("standing off the ship's hull", wasm.ship_game_tile_inside() === 0);
    wasm.ship_cmd_speed(0, 1);
  }
}

// The ship's doors are powered: a right-click on one opens a menu with the
// bathroom door's four words on it, the readout names it with its state,
// and "Lock" walks James to the panel — after which the door reads locked
// and the menu offers "Unlock". The door is found by asking the room what
// is under each tile of the ship, in the room's units.
console.log("\nthe ship's doors");
{
  wasm.ship_cmd_speed(0, 1);
  const tile = wasm.ship_tile();
  // The pixel of a given door, or of any door when `which` is null, found
  // by asking the room what is under each tile round James — the camera
  // keeps him in the frame, so a pixel is only good until he walks.
  const findDoor = (which) => {
    const s = wasm.ship_view_scale();
    const p = bimPixel(0);
    for (let dy = -20; dy <= 20; dy++) {
      for (let dx = -20; dx <= 20; dx++) {
        const q = { x: p.x + dx * tile * s, y: p.y + dy * tile * s };
        const rx = wasm.ship_room_x(q.x, q.y);
        const ry = wasm.ship_room_y(q.x, q.y);
        if (wasm.bims_spot_at(rx, ry) !== 17) continue;
        const at = wasm.bims_ship_door_at(rx, ry) - 1;
        if (which === null || at === which) return { q, rx, ry, at };
      }
    }
    return null;
  };
  let door = findDoor(null);
  check("there is a door somewhere under the glass", door !== null);
  if (door) {
    const which = door.at;
    check("and the room knows which one it is", which >= 0, String(which));
    const what = byId.get("game-what");
    deckCanvas.dispatch("pointermove", pointer(door.q));
    step(1);
    check("the readout names it with its state", /^Door · (shut|open|held open|locked) · -?\d+, -?\d+$/.test(what.textContent), JSON.stringify(what.textContent));
    check("it is a thing a click acts on", wasm.bims_hit_at(door.rx, door.ry) === 9, String(wasm.bims_hit_at(door.rx, door.ry)));
    deckCanvas.dispatch("contextmenu", pointer(door.q, { button: 2 }));
    const menu = byId.get("menu");
    const labels = () => menu.querySelectorAll("button").map((b) => b.querySelector("span").textContent);
    check("a right-click opens its menu", menu.hidden === false && labels().length === 2, labels().join(" / "));
    check("with hold open and lock on it", labels()[0] === "Hold door open" && labels()[1] === "Lock", labels().join(" / "));
    menu.querySelectorAll("button")[1].dispatch("click");
    check("the order is an errand, not a switch", wasm.bims_ship_door_is_locked(which) === 0);
    for (let i = 0; i < 60 * 60 * 5 && wasm.bims_ship_door_is_locked(which) === 0; i++) step(1);
    check("and once James has walked to the panel the door is locked", wasm.bims_ship_door_is_locked(which) === 1);
    // James has walked and the camera with him: find the same door again.
    door = findDoor(which);
    check("the door is still under the glass", door !== null);
    if (door) {
      deckCanvas.dispatch("pointermove", pointer(door.q));
      step(1);
      check("which the readout says", /^Door · locked/.test(what.textContent), JSON.stringify(what.textContent));
      deckCanvas.dispatch("contextmenu", pointer(door.q, { button: 2 }));
      check("and the menu now offers Unlock", labels()[1] === "Unlock" && menu.querySelectorAll("button")[0].disabled === true, labels().join(" / "));
      menu.querySelectorAll("button")[1].dispatch("click");
      for (let i = 0; i < 60 * 60 * 5 && wasm.bims_ship_door_is_locked(which) === 1; i++) step(1);
      check("unlocking is the same walk back", wasm.bims_ship_door_is_locked(which) === 0);
      deckCanvas.dispatch("pointerleave", {});
      step(1);
    }
  }
}

// The shower is a fixture with a menu, like the pan: a right-click on it
// offers "Take a shower", and taking it is an errand — James walks there
// and his activity reads as the shower.
console.log("\nthe shower");
{
  const tile = wasm.ship_tile();
  const s = wasm.ship_view_scale();
  const p = bimPixel(0);
  let shower = null;
  for (let dy = -20; dy <= 20 && !shower; dy++) {
    for (let dx = -20; dx <= 20; dx++) {
      const q = { x: p.x + dx * tile * s, y: p.y + dy * tile * s };
      const rx = wasm.ship_room_x(q.x, q.y);
      const ry = wasm.ship_room_y(q.x, q.y);
      if (wasm.bims_spot_at(rx, ry) === 21) {
        shower = { q, rx, ry };
        break;
      }
    }
  }
  check("there is a shower somewhere under the glass", shower !== null);
  if (shower) {
    const what = byId.get("game-what");
    deckCanvas.dispatch("pointermove", pointer(shower.q));
    step(1);
    check("the readout names it", /^Shower/.test(what.textContent), JSON.stringify(what.textContent));
    check("it is a thing a click acts on", wasm.bims_hit_at(shower.rx, shower.ry) === 10, String(wasm.bims_hit_at(shower.rx, shower.ry)));
    deckCanvas.dispatch("contextmenu", pointer(shower.q, { button: 2 }));
    const menu = byId.get("menu");
    const labels = () => menu.querySelectorAll("button").map((b) => b.querySelector("span").textContent);
    check("a right-click offers a shower", menu.hidden === false && labels()[0] === "Take a shower", labels().join(" / "));
    check("which can be taken", menu.querySelectorAll("button")[0].disabled === false);
    menu.querySelectorAll("button")[0].dispatch("click");
    let showering = false;
    for (let i = 0; i < 60 * 60 * 5 && !showering; i++) {
      step(1);
      showering = wasm.bims_activity(0) === 17;
    }
    check("and James goes and takes one", showering, String(wasm.bims_activity(0)));
    deckCanvas.dispatch("pointerleave", {});
    step(1);
  }
}

// The tray is the room's: three tabs, a timetable a slot an hour, the work
// list a row a job, and the triggers under the strip.
const trayTabs = byId.get("tray").querySelectorAll("#tray-tabs .tab").map((t) => t.dataset.tab);
check("the tray has the room's three tabs", trayTabs.join() === "schedule,work,management", trayTabs.join());
check(
  "the timetable has a slot an hour",
  byId.get("hours").querySelectorAll("li").length === wasm.bims_schedule_hours(),
  String(byId.get("hours").querySelectorAll("li").length),
);
check(
  "the work list has a row a job",
  byId.get("work").querySelectorAll("tbody tr").length === wasm.bims_work_count(),
  String(byId.get("work").querySelectorAll("tbody tr").length),
);
check("the triggers are under the strip", byId.get("thresholds").querySelectorAll(".trigger").length > 0);
check("the management row has the autonomy switch", byId.get("autonomy").checked === (wasm.bims_is_autonomous() === 1));

// --- what is aboard -----------------------------------------------------------
//
// The items panel on the left: a row a resource, grouped by hold, each with
// an icon and the count. The food rows read the room's cold store rather
// than the manifest, because that is what the crew can eat; everything else
// is the hold. The icons are CSS rules in ship.html, one a resource, and a
// resource without one is a bare slot — so the file is read for them.
console.log("\nwhat is aboard");
{
  const items = byId.get("items");
  const every = items.querySelectorAll(".item");
  // The rows for what is bought, and the rows for what is made aboard —
  // the stew the crew cook for the shelf — which the manifest has never
  // heard of and the room counts.
  const rows = every.filter((r) => r.dataset.resource !== undefined);
  const made = every.filter((r) => r.dataset.made !== undefined);
  const resources = wasm.ship_resource_count();
  check("a row a resource", rows.length === resources, `${rows.length} against ${resources}`);
  check("and a row for the stew made aboard", made.length === 1 && made[0].dataset.made === "stew", made.map((r) => r.dataset.made).join());
  const stewAt = every.indexOf(made[0]);
  const COLD_STORE_CLASS = 2;
  check(
    "which sits in the cold store's group, after the food",
    stewAt > 0 &&
      !made[0].className.includes("first-of-hold") &&
      wasm.ship_storage_of(Number(every[stewAt - 1].dataset.resource)) === COLD_STORE_CLASS &&
      (stewAt === every.length - 1 || every[stewAt + 1].className.includes("first-of-hold")),
    `${made[0].className} at ${stewAt} of ${every.length}`,
  );
  check(
    "and counts what the room says",
    made[0].querySelector(".count").textContent === String(wasm.bims_store_stew()),
    made[0].querySelector(".count").textContent,
  );
  const classes = rows.map((r) => wasm.ship_storage_of(Number(r.dataset.resource)));
  check(
    "grouped by hold, shelves first",
    classes.every((c, i) => i === 0 || c >= classes[i - 1]) && classes[0] === 0,
    classes.join(),
  );
  const firsts = every.filter((r) => r.className.includes("first-of-hold")).length;
  check("a rule between one hold and the next", firsts === wasm.ship_storage_count(), String(firsts));
  check(
    "every row has an icon slot",
    rows.every((r) => r.querySelectorAll(".icon").length === 1 && r.querySelector(".icon").dataset.resource === r.dataset.resource) &&
      made.every((r) => r.querySelector(".icon").dataset.made === r.dataset.made),
  );
  const page = readFileSync("web/ship.html", "utf8");
  const missingIcon = [];
  for (let id = 0; id < resources; id++) {
    if (!page.includes(`#items .icon[data-resource="${id}"]`)) missingIcon.push(id);
  }
  for (const r of made) {
    if (!page.includes(`#items .icon[data-made="${r.dataset.made}"]`)) missingIcon.push(r.dataset.made);
  }
  check("and ship.html has an icon for each", missingIcon.length === 0, `no icon for ${missingIcon.join()}`);
  check("the ? is there to explain them", items.querySelectorAll(".what").length === 1);

  const VEGETABLE = 4;
  const TOFU = 5;
  const expected = (id) =>
    id === VEGETABLE ? wasm.bims_store_veg() : id === TOFU ? wasm.bims_store_tofu() : wasm.ship_cargo(id);
  const counted = () => rows.map((r) => r.querySelector(".count").textContent);
  check(
    "each count is what is aboard",
    rows.every((r) => r.querySelector(".count").textContent === String(expected(Number(r.dataset.resource)))),
    counted().join(),
  );
  check(
    "a row with something in it is lit",
    rows.every((r) => r.className.includes("held") === (expected(Number(r.dataset.resource)) > 0)),
    rows.map((r) => r.className).join(";"),
  );
  // The food rows follow the room's cold store, whatever the manifest says
  // — a buy at the dock goes on the manifest, and whether the room sees it
  // straight away or at the next dock, the row says what the room says.
  const vegRow = rows.find((r) => Number(r.dataset.resource) === VEGETABLE);
  const buyVeg = byId.get("game-goods").querySelector(`[data-resource="${VEGETABLE}"] [data-buy="1"]`);
  const vegOnManifest = wasm.ship_cargo(VEGETABLE);
  buyVeg.dispatch("click");
  step(1);
  check("a vegetable bought goes on the manifest", wasm.ship_cargo(VEGETABLE) === vegOnManifest + 1);
  check(
    "and the vegetable row reads the room's cold store",
    vegRow.querySelector(".count").textContent === String(wasm.bims_store_veg()),
    `${vegRow.querySelector(".count").textContent} against ${wasm.bims_store_veg()} in the room, ${wasm.ship_cargo(VEGETABLE)} on the manifest`,
  );
  byId.get("game-goods").querySelector(`[data-resource="${VEGETABLE}"] [data-sell="1"]`).dispatch("click");
  step(1);

  // A buy at the dock shows up on the next frame. The playtest ship's
  // shelves are full, so a unit of metal goes first to make the room.
  const ORE = 0;
  const METAL = 1;
  const oreRow = rows.find((r) => Number(r.dataset.resource) === ORE);
  const oreBefore = wasm.ship_cargo(ORE);
  const metalBefore = wasm.ship_cargo(METAL);
  byId.get("game-goods").querySelector(`[data-resource="${METAL}"] [data-sell="1"]`).dispatch("click");
  step(1);
  check("a unit of metal sold makes room on the shelves", wasm.ship_cargo(METAL) === metalBefore - 1);
  const buyOne = byId.get("game-goods").querySelector(`[data-resource="${ORE}"] [data-buy="1"]`);
  check("the ore buy button is live at the dock", buyOne !== null && buyOne.disabled === false);
  buyOne.dispatch("click");
  step(1);
  check("buying ore takes one aboard", wasm.ship_cargo(ORE) === oreBefore + 1, String(wasm.ship_cargo(ORE)));
  check(
    "and the ore row says so",
    oreRow.querySelector(".count").textContent === String(oreBefore + 1) && oreRow.className.includes("held"),
    `${oreRow.querySelector(".count").textContent} (${oreRow.className})`,
  );
  const sellOne = byId.get("game-goods").querySelector(`[data-resource="${ORE}"] [data-sell="1"]`);
  sellOne.dispatch("click");
  step(1);
  check("selling it back clears the row", oreRow.querySelector(".count").textContent === String(oreBefore));
  byId.get("game-goods").querySelector(`[data-resource="${METAL}"] [data-buy="1"]`).dispatch("click");
  step(1);
  check("and the metal is back where it was", wasm.ship_cargo(METAL) === metalBefore);

  // --- keep so many made ----------------------------------------------------
  //
  // A row for something the benches make carries a target, stepped with two
  // buttons; a row for something they do not — ore — carries none. The
  // target goes through `net` like a deal, and the benches work to it: the
  // playtest ship has ore, a smelter and the power, so a target for metal
  // one above what is aboard has a Bim at the smelter, and the log says
  // what was made.
  const metalRow = rows.find((r) => Number(r.dataset.resource) === METAL);
  check("a made resource's row has a keep control", metalRow.querySelector(".keep") !== null);
  check("and a raw one's does not", oreRow.querySelector(".keep") === null);
  check(
    "which says what making one takes",
    /ore/.test(metalRow.querySelector(".keep").title) && /smelter/i.test(metalRow.querySelector(".keep").title),
    metalRow.querySelector(".keep").title,
  );
  check("the target starts at nothing", wasm.ship_craft_target(METAL) === 0 && metalRow.querySelector(".target").textContent === "0");
  const more = metalRow.querySelector('[data-keep="more"]');
  more.dispatch("click");
  step(1);
  check("one click asks for a few more", wasm.ship_craft_target(METAL) > 0, String(wasm.ship_craft_target(METAL)));
  check("and the row shows it", metalRow.querySelector(".target").textContent === String(wasm.ship_craft_target(METAL)));
  metalRow.querySelector('[data-keep="less"]').dispatch("click");
  step(1);
  check("and one click back takes it off again", wasm.ship_craft_target(METAL) === 0);
  // Now one metal more than is aboard, and wait for it: half an hour at the
  // bench plus the walk, so a couple of game hours at most.
  const metalNow = wasm.ship_cargo(METAL);
  const oreNow = wasm.ship_cargo(ORE);
  wasm.ship_cmd_keep(0, METAL, metalNow + 1);
  step(1);
  check("a target one above what is aboard is an order", wasm.ship_craft_target(METAL) === metalNow + 1);
  let smelted = false;
  const logText = () => byId.get("log").textContent;
  for (let i = 0; i < 60 * 60 * 3 && !smelted; i++) {
    step(1);
    smelted = wasm.ship_cargo(METAL) === metalNow + 1;
  }
  check("and a Bim smelts a metal out of two ore", smelted && wasm.ship_cargo(ORE) === oreNow - 2, `${wasm.ship_cargo(METAL)} metal, ${wasm.ship_cargo(ORE)} ore`);
  check("which the log says", /Made 1 metal/.test(logText()), logText().slice(-120));
  wasm.ship_cmd_keep(0, METAL, 0);
  step(1);
}

// A world of the query's choosing, when it says: the seed, the type, the
// star and the station all go through, and a bad one is no world at all.
const chosen = await bootWasmPage({
  html: "web/ship.html",
  host: "web/ship.js",
  wasm: "web/ship.wasm",
  search: `?mode=1&seedHi=0&seedLo=5&galaxy=3&star=${wasm.ship_simulation_star()}&station=${wasm.ship_simulation_station()}`,
});
// The same ids in a different galaxy are very likely no station at all, and
// then the simulation has nowhere to open; what must not happen is a world
// somewhere else.
check(
  "a query's spawn is honoured or refused, never replaced",
  chosen.wasm.ship_world_ready() === 0 ||
    (chosen.wasm.ship_world_star() === wasm.ship_simulation_star() &&
      chosen.wasm.ship_docked_at() === wasm.ship_simulation_station() + 1),
  `ready ${chosen.wasm.ship_world_ready()}, star ${chosen.wasm.ship_world_star()}`,
);

// `random=1` — `nix run .#test` — is the simulation somewhere else: the page
// rolls a seed and a pick, wasm turns the pick into a dock somebody lives
// on. With the seed and the roll on the query it is the same somewhere
// twice, which is the only way a harness can look at it; and with a roll of
// nought it is the simulation's own dock, so the two agree about what a
// dock is.
console.log("\nsomewhere at random");
{
  const at = (roll) =>
    bootWasmPage({
      html: "web/ship.html",
      host: "web/ship.js",
      wasm: "web/ship.wasm",
      search: `?mode=1&random=1&seedHi=0&seedLo=5&roll=${roll}`,
    });
  const once = await at(7);
  check("it opens docked", once.wasm.ship_world_ready() === 1 && once.wasm.ship_world_state() === 0);
  check("at a station somebody lives on", once.wasm.ship_station_residents(once.wasm.ship_docked_at() - 1) > 0);
  check("with them in the room", once.wasm.bims_crew() > once.wasm.ship_crew_count());
  const again = await at(7);
  check(
    "the same roll is the same place",
    again.wasm.ship_world_star() === once.wasm.ship_world_star() && again.wasm.ship_docked_at() === once.wasm.ship_docked_at(),
  );
  const elsewhere = await at(3);
  check(
    "a different roll is somewhere else",
    elsewhere.wasm.ship_world_star() !== once.wasm.ship_world_star() || elsewhere.wasm.ship_docked_at() !== once.wasm.ship_docked_at(),
    `both at star ${once.wasm.ship_world_star()}, dock ${once.wasm.ship_docked_at()}`,
  );
  check("no error box", !once.byId.get("error").textContent, once.byId.get("error").textContent);
}

// --- a trip, at the top speed ------------------------------------------------

console.log("\na trip");
page.fire("keydown", { key: "m" });
check("M opens the map", wasm.ship_view_mode() === 1);
const gameCanvas = byId.get("game-stage");
const middle = { x: gameCanvas.clientWidth / 2, y: gameCanvas.clientHeight / 2 };
wasm.ship_zoom(middle.x, middle.y, 100000);
const zoomedTo = wasm.ship_view_scale();
// The map keeps its zoom between visits: a map zoomed in on the dock is
// still zoomed in on the dock when it is opened again.
page.fire("keydown", { key: "m" });
page.fire("keydown", { key: "m" });
check("the map is back", wasm.ship_view_mode() === 1);
check("at the zoom it was left at", wasm.ship_view_scale() === zoomedTo, `${wasm.ship_view_scale()} against ${zoomedTo}`);

// The ship is flown from the helm, and James is not there yet: the walk
// is the button's, and the map aims at nothing until he arrives.
const takeHelm = byId.get("take-helm");
const aimedAt = { clientX: middle.x + 60, clientY: middle.y - 25, pointerId: 1 };
gameCanvas.dispatch("pointerdown", { button: 0, ...aimedAt });
step(1);
check("nothing is aimed at before James is at the helm", wasm.ship_preview_state() === 0, String(wasm.ship_preview_state()));
check("the Take the helm button is live", takeHelm.disabled === false);
takeHelm.dispatch("click");
until(() => wasm.ship_at_helm(0) === 1, "James reached the helm");
step(1);
check("the helm is his now", byId.get("helm-watch").textContent.includes("James"), byId.get("helm-watch").textContent);
gameCanvas.dispatch("pointerdown", { button: 0, ...aimedAt });
step(1);
check("clicking the map quotes a trip", wasm.ship_preview_state() === 1, String(wasm.ship_preview_state()));
check("that the tank can pay for", wasm.ship_preview_fuel() > 0 && wasm.ship_preview_fuel() <= wasm.ship_fuel_aboard(), `${wasm.ship_preview_fuel()} of ${wasm.ship_fuel_aboard()}`);

const confirm = byId.get("confirm");
check("Confirm is live", confirm.disabled === false);
confirm.dispatch("click");
step(2);
// From a berth: everybody to their own side of the airlock, the push-off,
// and then the trip. The station's people are all ashore already, so the
// first of those is over in a step.
check("Confirm starts the departure", [3, 4].includes(wasm.ship_world_state()), String(wasm.ship_world_state()));
until(() => wasm.ship_world_state() === 2, "the ship set off");
check("Confirm sets the ship going", wasm.ship_world_state() === 2, String(wasm.ship_world_state()));

// 24x is the last speed button. Read off the page rather than written down:
// the buttons are built from what wasm says the speeds are.
const buttons = root.querySelectorAll("[data-speed]");
const top = buttons[buttons.length - 1];
check("the top speed is 24x", top.textContent === "24×", top.textContent);
top.dispatch("click");
step(2);
check("and it is what the world is running at", wasm.ship_speed_multiplier(wasm.ship_effective_speed()) === 24, String(wasm.ship_speed_multiplier(wasm.ship_effective_speed())));

let frames = 0;
while (wasm.ship_world_state() === 2 && frames < 40000) {
  step(1);
  frames++;
}
check("the trip completes at 24x", wasm.ship_world_state() === 1, `state ${wasm.ship_world_state()} after ${frames} frames`);
check("holding at a point in space", wasm.ship_docked_at() === 0);
check("having burnt some fuel", wasm.ship_fuel_aboard() < wasm.ship_storage_capacity(FUEL_CLASS), String(wasm.ship_fuel_aboard()));
check("still no error box", !byId.get("error").textContent, byId.get("error").textContent);

// --- a station, off the map ----------------------------------------------------
//
// The system opens charted — every planet and station the lobby's chart
// showed is on the map from the first step — so a station is something to
// click on, and a click on one is an order to go there. Aimed by clicking
// its icon, not by naming it: that is the path a player has.

console.log("\nordered to a station");
check("the whole system is on the map", wasm.ship_map_count() > 2, String(wasm.ship_map_count()));
const stations = [];
for (let i = 0; i < wasm.ship_map_count(); i++) {
  if (wasm.ship_map_kind(i) === 1) stations.push(i);
}
check("and more than one station in it", stations.length >= 2, `${stations.length} stations`);
// The nearest station that is not where the ship is holding.
const here = { x: wasm.ship_world_x(), y: wasm.ship_world_y() };
const far = (i) => Math.hypot(wasm.ship_map_x(i) - here.x, wasm.ship_map_y(i) - here.y);
const goal = stations.filter((i) => far(i) > 1000).sort((a, b) => far(a) - far(b))[0];
check("one of them is somewhere else", goal !== undefined);
const s = wasm.ship_view_scale();
const iconAt = {
  clientX: wasm.ship_view_x() + (wasm.ship_map_x(goal) - here.x) * s,
  clientY: wasm.ship_view_y() - (wasm.ship_map_y(goal) - here.y) * s,
  pointerId: 1,
  button: 0,
};
gameCanvas.dispatch("pointerdown", iconAt);
step(1);
check("clicking its icon picks it", wasm.ship_map_pick(iconAt.clientX, iconAt.clientY, 14) === goal + 1);
check("and quotes a trip that docks", wasm.ship_preview_state() === 1 && wasm.ship_preview_docks() === 1, `state ${wasm.ship_preview_state()} docks ${wasm.ship_preview_docks()}`);
check("the helm names a station", /station|orbital|refinery|outpost|derelict|relay/i.test(byId.get("plan").textContent), byId.get("plan").textContent);
confirm.dispatch("click");
step(2);
check("Confirm sends the ship", wasm.ship_world_state() === 2, String(wasm.ship_world_state()));
let flew = 0;
while (wasm.ship_world_state() === 2 && flew < 60000) {
  step(1);
  flew++;
}
// The trip ends short of the berth and the ship comes alongside on its
// own: a slide onto the berth over a few minutes, never a jump.
check("it comes alongside", wasm.ship_world_state() === 5, String(wasm.ship_world_state()));
check("which the readout says", byId.get("trip-phase").textContent === "Docking", byId.get("trip-phase").textContent);
let jumped = 0;
let was = { x: wasm.ship_world_x(), y: wasm.ship_world_y() };
while (wasm.ship_world_state() === 5 && flew < 60000) {
  step(1);
  flew++;
  const now = { x: wasm.ship_world_x(), y: wasm.ship_world_y() };
  jumped = Math.max(jumped, Math.hypot(now.x - was.x, now.y - was.y));
  was = now;
}
check("without jumping", jumped < 24 * 200, `${jumped} in one frame`);
const wanted = wasm.ship_map_id(goal);
check("and it docks there", wasm.ship_world_state() === 0 && wasm.ship_docked_at() === wanted + 1, `state ${wasm.ship_world_state()}, dock ${wasm.ship_docked_at()} after ${flew} frames`);
check("with the station panel back up", byId.get("game-trade").hidden === false);
check("still no error box", !byId.get("error").textContent, byId.get("error").textContent);

console.log(fails.length ? `\n${fails.length} FAILED` : "\nall passed");
process.exit(fails.length ? 1 : 0);

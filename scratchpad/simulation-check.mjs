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

// --- a trip, at the top speed ------------------------------------------------

console.log("\na trip");
page.fire("keydown", { key: "m" });
check("M opens the map", wasm.ship_view_mode() === 1);
const gameCanvas = byId.get("game-stage");
const middle = { x: gameCanvas.clientWidth / 2, y: gameCanvas.clientHeight / 2 };
wasm.ship_zoom(middle.x, middle.y, 100000);
const aimedAt = { clientX: middle.x + 60, clientY: middle.y - 25, pointerId: 1 };
gameCanvas.dispatch("pointerdown", { button: 0, ...aimedAt });
step(1);
check("clicking the map quotes a trip", wasm.ship_preview_state() === 1, String(wasm.ship_preview_state()));
check("that the tank can pay for", wasm.ship_preview_fuel() > 0 && wasm.ship_preview_fuel() <= wasm.ship_fuel_aboard(), `${wasm.ship_preview_fuel()} of ${wasm.ship_fuel_aboard()}`);

const confirm = byId.get("confirm");
check("Confirm is live", confirm.disabled === false);
confirm.dispatch("click");
step(2);
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
const wanted = wasm.ship_map_id(goal);
check("and it docks there", wasm.ship_world_state() === 0 && wasm.ship_docked_at() === wanted + 1, `state ${wasm.ship_world_state()}, dock ${wasm.ship_docked_at()} after ${flew} frames`);
check("with the station panel back up", byId.get("game-trade").hidden === false);
check("still no error box", !byId.get("error").textContent, byId.get("error").textContent);

console.log(fails.length ? `\n${fails.length} FAILED` : "\nall passed");
process.exit(fails.length ? 1 : 0);

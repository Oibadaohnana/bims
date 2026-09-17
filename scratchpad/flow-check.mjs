// The whole game, in the order a player meets it: the start menu, the World
// tab, a station picked, Start, the designer opened with what Start wrote, a
// ship built and accepted, and the world open — **docked at the station that
// was picked**, not at wherever the world would have started on its own.
//
// Two pages, two wasms, one query string between them. `builder-check.mjs`
// walks the first page and `ship-check.mjs` the second; what neither can see
// is the seam — that the numbers one writes are the numbers the other reads,
// and that the world honours them. This is that check, and nothing else.

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

const showing = (page) =>
  page.root
    .querySelectorAll("[data-screen]")
    .filter((s) => !s.hidden)
    .map((s) => s.dataset.screen)
    .join();

// A known galaxy, so the station picked is one the wasm on the other page
// generates the same way: the seed the worldgen fixture pins.
const fixtureSource = readFileSync("crates/worldgen/src/fixture.rs", "utf8");
const REFERENCE_SEED = BigInt(/REFERENCE_SEED: u64 = ([0-9a-fx_]+);/.exec(fixtureSource)[1].replace(/_/g, ""));

// --- the lobby ------------------------------------------------------------

const lobby = await bootWasmPage({
  html: "web/builder.html",
  host: "web/builder.js",
  wasm: "web/lobby.wasm",
});
console.log("the builder");
check("the builder booted", !lobby.byId.get("error").textContent, lobby.byId.get("error").textContent);

lobby.click("play");
lobby.byId.get("setup-tool").querySelectorAll(".tool-tabs .tab")[1].dispatch("click");
lobby.step(1);
lobby.byId.get("seed").value = REFERENCE_SEED.toString();
lobby.dispatch("seed-form", "submit");

// Pick a station the way a player would: click a star that has one, then
// its first station in the panel. Zoomed in on it first so the click is
// unambiguous in a dense core.
let lit = 0;
while (lobby.wasm.lobby_star_has_station(lit) === 0) lit++;
const canvas = lobby.byId.get("galaxy");
const at = () => ({
  clientX: lobby.wasm.lobby_star_screen_x(lit),
  clientY: lobby.wasm.lobby_star_screen_y(lit),
  pointerId: 1,
  button: 0,
});
canvas.dispatch("wheel", { ...at(), deltaY: -4000 });
canvas.dispatch("pointerdown", at());
canvas.dispatch("pointerup", at());
check("a star with a station is open", lobby.wasm.lobby_inspected() === lit, String(lobby.wasm.lobby_inspected()));
const rows = lobby.byId.get("stations").querySelectorAll("[data-station]");
check("and its stations are listed", rows.length > 0);
rows[0].querySelector("[data-pick]").dispatch("click");
const [star, station] = lobby.lastCall("lobby_set_spawn");
check("a station is the pending start", star === lit && station === 0, `${star}, ${station}`);
const startSaid = lobby.byId.get("spawn-said").textContent;

lobby.root.querySelector("[data-screen=setup] [data-start]").dispatch("click");
check("Start goes to the handover", showing(lobby) === "build", showing(lobby));
const went = lobby.visited[lobby.visited.length - 1] ?? "";
check("and navigates to the designer", went.startsWith("ship.html?"), went);
const search = went.slice(went.indexOf("?"));
const query = new Map(search.slice(1).split("&").map((pair) => pair.split("=")));
check("with the star and the station in the query", query.get("star") === String(star) && query.get("station") === String(station), search);

// --- the designer, opened with exactly what Start wrote --------------------

const ship = await bootWasmPage({
  html: "web/ship.html",
  host: "web/ship.js",
  wasm: "web/ship.wasm",
  search,
});
console.log("\nthe designer, with the lobby's query");
check("the designer booted", !ship.byId.get("error").textContent, ship.byId.get("error").textContent);
check("on the design phase, not the error screen", showing(ship) === "design", showing(ship));
check("the spawn was accepted", ship.wasm.ship_spawn_ok() === 1);
check("for the lobby's crew", ship.wasm.ship_players() === 1, String(ship.wasm.ship_players()));

// The designer opens on the playtest ship, given: a ship the player can
// accept as it stands, or change first. Here it is accepted as it stands,
// which is the shortest path a player has to the world.
const PLAYTEST_PARTS = Number(
  /PLAYTEST_PARTS: u32 = (\d+);/.exec(readFileSync("crates/shipdesign/src/fixture.rs", "utf8"))[1],
);
check("the designer opens on a ship already laid out", ship.wasm.ship_part_total() === PLAYTEST_PARTS, String(ship.wasm.ship_part_total()));
check("that is whole and flyable", ship.wasm.ship_has_errors() === 0 && ship.wasm.ship_issue_count() === 0, `${ship.wasm.ship_issue_count()} issues`);
const pool = ship.wasm.ship_pool_hi() * 2 ** 32 + ship.wasm.ship_pool_lo();
const left = ship.wasm.ship_remaining_hi() * 2 ** 32 + ship.wasm.ship_remaining_lo();
check("and cost the crew nothing", left === pool && pool > 0, `${left} of ${pool}`);

const accept = ship.byId.get("accept");
check("Accept is on offer", accept.disabled === false);
accept.dispatch("click");
ship.step(1);

// --- the world, where the lobby said ---------------------------------------

console.log("\nthe world the Accept opened");
check("the game took over", showing(ship) === "game", showing(ship));
check("the world is open", ship.wasm.ship_world_ready() === 1);
check("in the star the lobby picked", ship.wasm.ship_world_star() === star, `${ship.wasm.ship_world_star()} vs ${star}`);
check(
  "docked at the station the lobby picked",
  ship.wasm.ship_world_state() === 0 && ship.wasm.ship_docked_at() === station + 1,
  `state ${ship.wasm.ship_world_state()}, dock ${ship.wasm.ship_docked_at()}`,
);
check(
  "in the galaxy the lobby picked",
  (BigInt(query.get("seedHi")) << 32n) + BigInt(query.get("seedLo")) === REFERENCE_SEED && query.get("galaxy") === "0",
  search,
);
check("and the lobby had named that very place", startSaid.includes(" at "), startSaid);
ship.step(1);
check("with the player aboard", ship.wasm.ship_crew_count() === 1, String(ship.wasm.ship_crew_count()));
check("named", ship.drawn.some((d) => d.text === "James"), ship.drawn.map((d) => d.text).join(","));
check("still no error box", !ship.byId.get("error").textContent, ship.byId.get("error").textContent);

console.log(fails.length ? `\n${fails.length} FAILED` : "\nall passed");
process.exit(fails.length ? 1 : 0);

// A station to start at, for a harness that has no lobby to pick one.
//
// `web/ship.html` refuses to open a design phase without a spawn in its
// query — a star and a station, which the lobby's World tab writes — and it
// never falls back to one of its own. A harness that wants the design phase
// therefore has to bring a spawn, and the only honest place to get one is
// the wasm: `ship_simulation_star`/`_station` are the simulation's dock for
// the seed the page opened with, exported for exactly this. Reading them
// means booting a page once, which is what this does.

import { bootWasmPage } from "./stub.mjs";

/** The simulation's spawn for the default seed, and the query fragment that
 * hands it to a design page: `&star=N&station=M`. */
export async function simulationSpawn() {
  const page = await bootWasmPage({
    html: "web/ship.html",
    host: "web/ship.js",
    wasm: "web/ship.wasm",
    search: "",
  });
  const star = page.wasm.ship_simulation_star();
  const station = page.wasm.ship_simulation_station();
  return { star, station, query: `&star=${star}&station=${station}` };
}

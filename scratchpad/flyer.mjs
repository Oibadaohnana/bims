// A ship a trip can be planned for, built through the designer's own page.
//
// Shared by the harnesses that need one but are not about building it —
// `ship-layout.mjs` wants a ship to look at, `flow-check.mjs` wants one to
// accept — so it is written once here rather than once in each. It is the
// same ship `ship-check.mjs` builds step by step with a check after every
// part; that file keeps its own copy because the checks *are* the point.
//
// It is built through the palette and the canvas, with the same pointer
// events a player sends, and bought through the trade panel, because a ship
// written straight into wasm would be a ship the page never had to admit.

/** `PartKind` discriminants, so the ship below reads as a ship. */
const FLOOR = 0;
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
const OUTSIDE_WALL = 16;
const HELM = 17;
const POWER_CONDUIT = 19;
const FUEL_TANK = 21;
const AIRLOCK = 23;
const SENSOR_ARRAY = 24;
const THRUSTER = 27;

/** `physics::ResourceId`. */
const FUEL = 2;
const VEGETABLE = 4;
const TOFU = 5;

/** The pointer helpers a design page wants: a tile as a pointer event, a
 * palette pick, a drag, a placement, a purchase. `page` is what
 * `bootWasmPage` handed back for web/ship.html. */
export function designTools(page) {
  const { byId, root, wasm } = page;
  const canvas = byId.get("stage");
  const tile = wasm.ship_tile();

  /** The middle of a tile, in canvas pixels, through the camera the page is
   * actually using rather than a scale written down here. */
  function at(tx, ty) {
    const s = wasm.ship_view_scale();
    return {
      pointerId: 1,
      clientX: (tx * tile + tile / 2) * s + wasm.ship_view_x(),
      clientY: (ty * tile + tile / 2) * s + wasm.ship_view_y(),
    };
  }

  function pick(kind) {
    const button = root.querySelector(`[data-part="${kind}"]`);
    if (!button) throw new Error(`no palette button for part ${kind}`);
    button.dispatch("click");
  }

  /** Press, move, release — which is a click when the two tiles are one. */
  function drag(x0, y0, x1, y1, button = 0) {
    canvas.dispatch("pointerdown", { button, ...at(x0, y0) });
    canvas.dispatch("pointermove", { button, ...at(x1, y1) });
    canvas.dispatch("pointerup", { button, ...at(x1, y1) });
  }

  function put(kind, tx, ty) {
    pick(kind);
    drag(tx, ty, tx, ty);
  }

  /** Press a buy or sell button on the design phase's trade panel. */
  function deal(resource, step, buying) {
    const key = buying ? "buy" : "sell";
    const button = root.querySelector(`[data-resource="${resource}"] [data-${key}="${step}"]`);
    if (!button) throw new Error(`no ${key} ${step} button for resource ${resource}`);
    button.dispatch("click");
  }

  return { canvas, tile, at, pick, drag, put, deal };
}

/** A whole ship you can live on, on a twenty-tile grid: frame, deck, hull,
 * galley, heads, a table and a chair, a bunk, a helm, a bay, a locker, an
 * engine. Nothing a trip wants yet — see `buildFlyer`. */
export function buildShip(tools) {
  const { pick, drag, put } = tools;
  // Deck over the lot, one tile wider than the room, because the hull
  // stands on it. Plating lays its own frame.
  pick(FLOOR);
  drag(1, 1, 18, 18);
  pick(OUTSIDE_WALL);
  drag(1, 1, 18, 1);
  drag(1, 18, 18, 18);
  drag(1, 1, 1, 18);
  drag(18, 1, 18, 18);
  // A run of conduit down the middle, to see it drawn under what stands on
  // it rather than instead of it.
  pick(POWER_CONDUIT);
  drag(9, 4, 9, 16);
  put(HELM, 6, 6);
  put(COLD_STORE, 3, 3);
  put(WORKTOP, 5, 3);
  put(HOB, 8, 3);
  put(DISHWASHER, 10, 3);
  put(TOILET, 12, 3);
  put(BASIN, 14, 3);
  put(BROOM_LOCKER, 16, 3);
  put(TABLE, 4, 6);
  put(HYDRO_BAY, 3, 10);
  // The engine in the stern with its bell over the edge: an engine fires
  // aft and has to fire into space, so two tiles of stern plating come off
  // (the deck under them stays) and the engine stands there.
  drag(7, 18, 8, 18, 2);
  put(ENGINE, 7, 16);
  put(BUNK, 14, 8);
  put(CHAIR, 4, 7);
}

/** `buildShip` plus what flying wants — thrusters, an airlock, an array, a
 * tank and something to burn — so that the ship Accept hands over is one a
 * trip can be planned for. */
export function buildFlyer(tools) {
  const { drag, put, deal } = tools;
  buildShip(tools);
  for (const [x, y, kind] of [
    [9, 1, THRUSTER],
    [9, 18, THRUSTER],
    [1, 9, THRUSTER],
    [18, 9, THRUSTER],
    [5, 1, SENSOR_ARRAY],
  ]) {
    // Peel the wall off; the frame under it stays and the part stands on it.
    drag(x, y, x, y, 2);
    put(kind, x, y);
  }
  // The airlock goes in the skin — a ship docks by an airlock that opens
  // onto space, and one on the deck is a door to nowhere. Peel two tiles
  // of plating; the deck under them stays, and the airlock stands on it.
  drag(18, 11, 18, 12, 2);
  put(AIRLOCK, 18, 11);
  put(FUEL_TANK, 2, 12);
  deal(VEGETABLE, 10, true);
  deal(TOFU, 10, true);
  deal(FUEL, 100, true);
  deal(FUEL, 100, true);
}

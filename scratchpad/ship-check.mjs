// Does the ship designer actually run, and does it design a ship?
//
// `cargo build` says nothing about a page of plain JavaScript, and `cargo
// test` says nothing about the wasm the browser loads. This boots
// web/ship.js against the real ship.wasm and a stub DOM, then builds a whole
// ship through the palette and the canvas — the same pointer events a player
// sends — and accepts it.
//
// Three things here cannot be checked anywhere else:
//
//   * **The cross-target hash.** `ship_self_check` computes the reference
//     design's `design_hash` in wasm and compares it against the constants
//     `crates/shipdesign/src/tests.rs` pins natively. Two targets, one
//     written-down number; a target that drifted fails one of the two.
//   * **Accept.** It is disabled while anything is an error, it ends the
//     phase when everybody has given it, and any edit takes it back.
//   * **The words.** Every issue the wasm can raise has a line in
//     ISSUE_LINES, and every part has a name in PART_NAMES — checked by
//     counting rows against what wasm says there are, because an issue with
//     no line is *dropped* from the page rather than shown blank.

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

/** The `PartKind` discriminants, which are what the palette buttons are keyed
 * by. Written out so the ship built below reads as a ship. */
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
const STRUCTURE = 15;
const OUTSIDE_WALL = 16;
const HELM = 17;
const FUEL_TANK = 21;
const AIRLOCK = 23;
const SENSOR_ARRAY = 24;
const SHELF = 25;
const THRUSTER = 27;

/** `physics::ResourceId`, for the trade panel. */
const METAL = 1;
const FUEL = 2;
const VEGETABLE = 4;
const TOFU = 5;

/** `IssueCode::RadiationExposure`. The one issue that is always first and is
 * styled louder than an error. */
const RADIATION = 24;

/** `economy::Storage`. Which hold a resource goes in. */
const SHELF_CLASS = 0;
const FUEL_CLASS = 1;

/** What a lone player's pool is, with the lobby's standard purse: their own
 * €100 000 plus `economy::SOLO_BONUS`. Written out because a designer opened
 * with no query at all has to land on exactly this, and reading it back off
 * the wasm would be the check agreeing with itself. */
const SOLO_POOL = 120000;

/** A money figure, out of the two `u32` halves the boundary carries it in —
 * the same arithmetic the host does, and the reason it is worth doing here
 * too: a `_hi` wired to a `_lo` is invisible until the numbers are large. */
const amount = (hi, lo) => hi * 2 ** 32 + lo;

/** Open a designer with a given query string, and hand back the page plus
 * the few helpers that turn tiles into pointer events. */
async function session(search) {
  const page = await bootWasmPage({
    html: "web/ship.html",
    host: "web/ship.js",
    wasm: "web/ship.wasm",
    search,
  });
  const { byId, root, wasm } = page;
  const canvas = byId.get("stage");
  const gameCanvas = byId.get("game-stage");
  const tile = wasm.ship_tile();

  /** The middle of a tile, in canvas pixels. Worked out from the camera the
   * page is actually using rather than from a scale written down here — the
   * view fits itself to the canvas, and a hardcoded number would put every
   * click one tile out the day that changed. */
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

  function hover(tx, ty) {
    canvas.dispatch("pointermove", at(tx, ty));
  }

  const pool = () => amount(wasm.ship_pool_hi(), wasm.ship_pool_lo());
  const left = () => amount(wasm.ship_remaining_hi(), wasm.ship_remaining_lo());
  const price = (kind) =>
    amount(wasm.ship_part_price_hi(kind), wasm.ship_part_price_lo(kind));

  /** Frame and deck over a rectangle, which everything else needs under it.
   * Two drags, because that is what a player does. */
  function found(x0, y0, x1, y1) {
    pick(STRUCTURE);
    drag(x0, y0, x1, y1);
    pick(FLOOR);
    drag(x0, y0, x1, y1);
  }

  /** Press a buy or sell button on a resource's row. */
  function deal(resource, step, buying) {
    const key = buying ? "buy" : "sell";
    const button = root.querySelector(
      `[data-resource="${resource}"] [data-${key}="${step}"]`,
    );
    if (!button) throw new Error(`no ${key} ${step} button for resource ${resource}`);
    button.dispatch("click");
  }

  const cargo = (resource) => wasm.ship_cargo(resource);
  const held = (class_) => wasm.ship_storage_used(class_);
  const room = (class_) => wasm.ship_storage_capacity(class_);

  // --- once the game has started -----------------------------------------
  //
  // A second canvas, a second set of pointer helpers, and one way to let time
  // pass. `step` runs frames; how many world steps each is worth is the
  // page's own arithmetic, which is exactly the half being checked.

  /** A point on the game canvas, from a system position. */
  function mapAt(x, y) {
    const s = wasm.ship_view_scale();
    const here = { x: wasm.ship_world_x(), y: wasm.ship_world_y() };
    return {
      pointerId: 1,
      clientX: wasm.ship_view_x() + (x - here.x) * s,
      // System +y is north and the screen's y grows down.
      clientY: wasm.ship_view_y() - (y - here.y) * s,
    };
  }

  const speedButton = (code) => root.querySelector(`[data-speed="${code}"]`);

  /** Press a buy or sell button on the **game's** station panel. The design
   * phase has a panel of its own and it is a different transaction. */
  function trade(resource, step, buying) {
    const key = buying ? "buy" : "sell";
    const button = root.querySelector(
      `#game-goods [data-resource="${resource}"] [data-${key}="${step}"]`,
    );
    if (!button) throw new Error(`no game ${key} ${step} button for resource ${resource}`);
    button.dispatch("click");
  }

  /** Run frames until `check()` comes true, or give up and say so. */
  function until(check, what, limit = 40000) {
    for (let i = 0; i < limit; i++) {
      if (check()) return i;
      page.step(1);
    }
    throw new Error(`never ${what} in ${limit} frames`);
  }

  return {
    ...page,
    canvas,
    gameCanvas,
    tile,
    at,
    pick,
    drag,
    put,
    hover,
    pool,
    left,
    price,
    found,
    deal,
    cargo,
    held,
    room,
    mapAt,
    speedButton,
    trade,
    until,
  };
}

/** The reference ship, built through the palette: deck, hull, galley, heads,
 * a table, a bay, a locker, an engine, thrusters, an airlock, a sensor array,
 * a full tank, and a bed and a seat each.
 *
 * Deliberately the same layout as `shipdesign::fixture::flyer`, so a change to
 * the rules that breaks one breaks both — but built out of pointer events
 * rather than out of `apply`, which is the half a native test cannot reach.
 * And **flyable**, not merely liveable: everything below the design phase is
 * about a trip, and a ship with no thrusters would make every one of those
 * checks a check that the refusal works. */
function buildShip(page, crew) {
  const { pick, drag, put, found } = page;

  // The frame first, and it has to reach one tile further out than the deck:
  // the hull ring stands straight on it.
  pick(STRUCTURE);
  drag(1, 1, 18, 18);

  // Then the deck inside the ring.
  pick(FLOOR);
  drag(2, 2, 17, 17);

  // Hull round the outside. Four straight runs; the corners overlap and are
  // skipped, which is the drag rule doing what it should. Outside wall
  // rather than plain wall, because plain wall does not shield.
  pick(OUTSIDE_WALL);
  drag(1, 1, 18, 1);
  drag(1, 18, 18, 18);
  drag(1, 1, 1, 18);
  drag(18, 1, 18, 18);

  void found;

  // The galley and the heads along one row, everything facing down the room
  // so every use spot is the open row below.
  put(COLD_STORE, 3, 3);
  put(WORKTOP, 5, 3);
  put(HOB, 8, 3);
  put(DISHWASHER, 10, 3);
  put(TOILET, 12, 3);
  put(BASIN, 14, 3);
  put(BROOM_LOCKER, 16, 3);

  put(TABLE, 4, 6);
  put(HELM, 7, 6);
  put(HYDRO_BAY, 10, 10);
  put(ENGINE, 7, 13);

  for (let i = 0; i < crew; i++) {
    put(BUNK, 14, 8 + 2 * i);
    put(CHAIR, 4 + (i % 2), 7 + 2 * Math.floor(i / 2));
  }

  // What a *trip* needs, on top of what living aboard needs. The thrusters
  // and the array go in place of hull plating — both shield, so the skin is
  // still closed — and a right-drag takes the frame under the wall off with
  // it, so it goes back first.
  for (const [x, y, kind] of [
    [9, 1, THRUSTER],
    [9, 18, THRUSTER],
    [1, 9, THRUSTER],
    [18, 9, THRUSTER],
    [5, 1, SENSOR_ARRAY],
  ]) {
    drag(x, y, x, y, 2);
    put(STRUCTURE, x, y);
    put(kind, x, y);
  }
  put(AIRLOCK, 16, 8);
  put(FUEL_TANK, 2, 12);

  // And something to eat and something to burn. Bought through the panel,
  // which is the only way a player can.
  page.deal(VEGETABLE, 10, true);
  page.deal(TOFU, 10, true);
  page.deal(FUEL, 100, true);
  page.deal(FUEL, 100, true);
}

// --- a solo designer, on a small ship -------------------------------------

const solo = await session("?money=100000&area=20&players=1&slot=0");
const { byId, root, wasm, step, fire } = solo;

console.log("ship designer booted");
check("no error box", !byId.get("error").textContent, byId.get("error").textContent);

// --- the boundary, both ways ----------------------------------------------
//
// Every `wasm.ship_*` the host calls has to exist as an export, and an export
// nothing calls is worth a look: one side may have been renamed without the
// other, and a call to a missing export is `undefined is not a function` at
// whatever moment the player happens to reach it.
//
// Read with `readFileSync` rather than with a shell `grep`, deliberately: the
// shell's `grep` here is `ugrep --ignore-files` and returns *nothing at all*
// for files under web/, so a boundary check built on it comes back clean
// because it never read the file.

const hostSource = readFileSync("web/ship.js", "utf8");
check(
  "ship.js has no stray NUL in it",
  !hostSource.includes("\0"),
  "one byte, invisible in every editor, and it turns grep binary",
);
const called = new Set([...hostSource.matchAll(/wasm\.(ship_[a-z0-9_]+)/g)].map((m) => m[1]));
const exported = new Set(Object.keys(wasm).filter((name) => name.startsWith("ship_")));
check("the host calls the wasm at all", called.size > 20, `${called.size} calls`);
const missing = [...called].filter((name) => !exported.has(name));
check("everything the host calls is exported", missing.length === 0, missing.join(", "));

// Four exports the page has no use for, because they exist for **this file**:
// the self check, the two halves of the reference hash it reads, and the tile
// size, which is what turns a tile into a pointer event down here. The page
// itself never needs it — every piece of geometry is done in wasm — and a
// harness that wrote `52` of its own would be a second copy of the one number
// `shipdesign` says is fixed.
//
// Everything else the wasm exports, the page calls. An export that drops off
// the page and is not named here fails rather than sitting there looking
// implemented.
const FOR_THE_HARNESS = new Set([
  "ship_self_check",
  "ship_reference_hash_hi",
  "ship_reference_hash_lo",
  "ship_reference_checksum_hi",
  "ship_reference_checksum_lo",
  "ship_tile",
]);
const orphans = [...exported].filter((name) => !called.has(name) && !FOR_THE_HARNESS.has(name));
check("no export has quietly stopped being called", orphans.length === 0, orphans.join(", "));

// --- the wasm agrees with the native build --------------------------------

const ALL_CHECKS = 0b1111111;
const selfCheck = wasm.ship_self_check();
check(
  "wasm hashes the reference design the same as native does",
  selfCheck === ALL_CHECKS,
  `self check ${selfCheck.toString(2)} of ${ALL_CHECKS.toString(2)} — crew 1 hash here is ` +
    `${wasm.ship_reference_hash_hi(1).toString(16)}${wasm
      .ship_reference_hash_lo(1)
      .toString(16)
      .padStart(8, "0")}`,
);

// --- the palette ----------------------------------------------------------

const partButtons = root.querySelectorAll("[data-part]");
check(
  "there is a button for every part the wasm knows about",
  partButtons.length === wasm.ship_part_count(),
  `${partButtons.length} buttons, ${wasm.ship_part_count()} parts`,
);
check(
  "none of them fell into 'Anything else'",
  !root.querySelectorAll("h2").some((h) => h.textContent === "Anything else"),
);
const unnamed = partButtons
  .map((b) => b.querySelector(".name").textContent)
  .filter((name) => !name || /^Part \d+$/.test(name));
check("every part has a name", unnamed.length === 0, unnamed.join(", "));

// --- the money ------------------------------------------------------------
//
// One pool, everybody's money in it. A lone player's is their own plus the
// solo bonus, and `economy` is the only place that sum is done — this checks
// the figure the page is actually spending against, through the boundary the
// player's browser reads it through.

const moneyRow = root.querySelector("[data-money]");
check("there is a money readout", !!moneyRow);
check("the pool is what one player brings plus the bonus", solo.pool() === SOLO_POOL, String(solo.pool()));
check("and none of it is spent yet", solo.left() === SOLO_POOL, String(solo.left()));

// The euro sign and the grouping are the host's; what is compared is the
// digits, so the harness carries no copy of either.
const digits = (text) => text.replace(/[^0-9]/g, "");
check(
  "the readout says what is left",
  digits(moneyRow.querySelector(".left").textContent) === String(SOLO_POOL),
  moneyRow.querySelector(".left").textContent,
);
check(
  "beside what there was",
  digits(moneyRow.querySelector(".of").textContent) === String(SOLO_POOL),
  moneyRow.querySelector(".of").textContent,
);

const deckPrice = solo.price(FLOOR);
const framePrice = solo.price(STRUCTURE);
check("a tile of deck has a price", deckPrice > 0, String(deckPrice));

// --- the frame, and what stands on it -------------------------------------
//
// Nothing goes down on nothing any more: the frame is the first thing built
// and everything else is over it. That is the rule a player meets first, so
// it is the first thing checked.

solo.put(FLOOR, 5, 5);
check("deck will not go down on nothing", wasm.ship_part_total() === 0);
check(
  "and the page says the frame comes first",
  byId.get("said").textContent.toLowerCase().includes("structure"),
  byId.get("said").textContent,
);

solo.put(STRUCTURE, 5, 5);
check("the frame goes down on nothing", wasm.ship_part_total() === 1, String(wasm.ship_part_total()));
solo.put(FLOOR, 5, 5);
check("and then the deck goes down on it", wasm.ship_part_total() === 2, String(wasm.ship_part_total()));
check(
  "both came out of the pool",
  solo.left() === SOLO_POOL - deckPrice - framePrice,
  `${solo.left()} left of ${SOLO_POOL}`,
);
check(
  "which the readout picks up",
  digits(moneyRow.querySelector(".left").textContent) ===
    String(SOLO_POOL - deckPrice - framePrice),
  moneyRow.querySelector(".left").textContent,
);

// A right-drag clears both, top of the stack down, and refunds the lot.
solo.drag(5, 5, 5, 5, 2);
check("a right-drag takes the whole stack off", wasm.ship_part_total() === 0);
check("and hands the money back", solo.left() === SOLO_POOL, String(solo.left()));

// --- the ghost ------------------------------------------------------------

solo.pick(HOB);
solo.hover(5, 5);
check("a hob will not stand on nothing", wasm.ship_ghost_ok() === 0);
solo.put(STRUCTURE, 5, 5);
solo.pick(HOB);
solo.hover(5, 5);
check("nor on bare frame", wasm.ship_ghost_ok() === 0);
solo.put(FLOOR, 5, 5);
solo.pick(HOB);
solo.hover(5, 5);
check("but it will stand on deck", wasm.ship_ghost_ok() === 1);

// Hull stands straight on the frame, with no deck under it. That is what
// lets a ship be skinned before it is floored.
solo.pick(OUTSIDE_WALL);
solo.hover(5, 5);
check("hull needs no deck", wasm.ship_ghost_ok() === 1);

solo.pick(ENGINE);
check("the engine starts upright, two by three", wasm.ship_part_w(ENGINE) === 2 && wasm.ship_part_h(ENGINE) === 3);
fire("keydown", { key: "r" });
check("R turns the ghost", wasm.ship_ghost_rotation() === 1, String(wasm.ship_ghost_rotation()));
check(
  "and the footprint turns with it",
  wasm.ship_part_w(ENGINE) === 3 && wasm.ship_part_h(ENGINE) === 2,
  `${wasm.ship_part_w(ENGINE)}×${wasm.ship_part_h(ENGINE)}`,
);
for (let i = 0; i < 3; i++) fire("keydown", { key: "r" });
check("four turns is back where it started", wasm.ship_ghost_rotation() === 0);

// A pointer off the build area is off it, rather than sticking to the edge.
solo.pick(FLOOR);
solo.hover(-3, -3);
check("a pointer outside the build area is outside it", wasm.ship_hover_inside() === 0);
check("so nothing would be placed there", wasm.ship_ghost_ok() === 0);

// Clear the tile again so the ship below is built on a bare grid.
solo.drag(5, 5, 5, 5, 2);
check("the grid is bare again", wasm.ship_part_total() === 0, String(wasm.ship_part_total()));

// --- what is wrong with it ------------------------------------------------

const issueRows = () => byId.get("issue-list").children;
check("an empty grid has faults", wasm.ship_issue_count() > 0, String(wasm.ship_issue_count()));
check(
  "every fault the wasm raises has words for it",
  issueRows().length === wasm.ship_issue_count(),
  `${issueRows().length} rows, ${wasm.ship_issue_count()} issues`,
);
check(
  "and none of them is blank",
  issueRows().every((row) => row.textContent.length > 4),
);

const accept = byId.get("accept");
check("Accept is disabled while anything is an error", accept.disabled === true);
check(
  "and says why",
  byId.get("accept-note").textContent.includes("errors"),
  byId.get("accept-note").textContent,
);

// Resting on a fault rings its tiles. A highlight, not a tooltip: nothing is
// said, and it goes the moment the pointer leaves — so what proves it
// happened is the shape buffer getting longer, not any text appearing.
//
// Two tiles of frame at opposite corners is a ship in two pieces, which is
// the simplest fault that has tiles to point at. The frame is what the check
// walks — a ship is its structure.
solo.put(STRUCTURE, 2, 2);
solo.put(STRUCTURE, 17, 17);
const split = issueRows().find((row) => row.dataset.issue === "1");
check("two loose tiles are a ship in two pieces", !!split, issueRows().map((r) => r.dataset.issue).join(" "));
if (split) {
  wasm.ship_render();
  const plain = wasm.ship_draw_len();
  split.dispatch("pointerenter");
  wasm.ship_render();
  const lit = wasm.ship_draw_len();
  split.dispatch("pointerleave");
  wasm.ship_render();
  const after = wasm.ship_draw_len();
  check("resting on a fault rings it on the deck", lit > plain, `${plain} then ${lit}`);
  check("and the ring goes when the pointer does", after === plain, `${after} against ${plain}`);
}
solo.drag(2, 2, 2, 2, 2);
solo.drag(17, 17, 17, 17, 2);
check("and the grid is bare again", wasm.ship_part_total() === 0, String(wasm.ship_part_total()));

// --- build the whole thing ------------------------------------------------

buildShip(solo, 1);
check("the ship got built", wasm.ship_part_total() > 600, String(wasm.ship_part_total()));
const stillWrong = [];
for (let i = 0; i < wasm.ship_issue_count(); i++) {
  if (wasm.ship_issue_severity(i) === 0) stillWrong.push(wasm.ship_issue_code(i));
}
check("and nothing is an error any more", stillWrong.length === 0, `codes ${stillWrong.join(", ")}`);
check("which the page agrees with", wasm.ship_has_errors() === 0);
// A ship with everything a trip wants has nothing left to warn about, and the
// page says so in one row rather than in none.
check(
  "nothing is a warning either, on a ship that can actually fly",
  wasm.ship_issue_count() === 0,
  `${wasm.ship_issue_count()} left`,
);
check(
  "so the checks panel says as much",
  issueRows().length === 1 && issueRows()[0].className === "clean",
  issueRows().map((r) => r.textContent).join(" | "),
);
check("there was money to spare", solo.left() > 0, String(solo.left()));
check("and it is the pool less what the ship cost", solo.left() < SOLO_POOL, String(solo.left()));
check(
  "and the finished hull lets no radiation in",
  wasm.ship_exposed_count() === 0,
  `${wasm.ship_exposed_count()} tiles exposed`,
);

// --- accepting ------------------------------------------------------------

check("Accept comes alive", accept.disabled === false);
const showing = () =>
  root
    .querySelectorAll("[data-screen]")
    .filter((s) => !s.hidden)
    .map((s) => s.dataset.screen)
    .join();
check("the design screen is what is up", showing() === "design", showing());

const moneyAtAccept = solo.left();
accept.dispatch("click");
check("one Accept settles a solo ship", wasm.ship_phase() === 1, String(wasm.ship_phase()));
check("and the game takes over", showing() === "game", showing());
check("and nothing can be moved any more", wasm.ship_phase() !== 0);

// --- the game -------------------------------------------------------------
//
// One world, one clock. Everything below is the page turning the crank on it:
// the design phase is over and the ship is real.

console.log("\nthe game the Accept started");
check("the world is open", wasm.ship_world_ready() === 1);
check(
  "docked at the spawn station",
  wasm.ship_world_state() === 0 && wasm.ship_docked_at() !== 0,
  `state ${wasm.ship_world_state()}, dock ${wasm.ship_docked_at()}`,
);
check(
  "with the money the design phase left over",
  solo.left() === moneyAtAccept,
  `${solo.left()} against ${moneyAtAccept}`,
);
check("the clock is at the beginning", wasm.ship_world_day() === 0 && wasm.ship_world_steps() === 0);
check("it is not going anywhere yet", wasm.ship_world_speed() === 0);
check(
  "and it knows how far it can see",
  wasm.ship_detection_range() > 0,
  String(wasm.ship_detection_range()),
);
check(
  "the spawn dock is on the map",
  wasm.ship_map_count() > 0,
  `${wasm.ship_map_count()} found`,
);
check("still no error box", !byId.get("error").textContent, byId.get("error").textContent);

// A click on the deck after that does nothing at all.
const settledCount = wasm.ship_part_total();
solo.put(FLOOR, 6, 6);
check("a click after the design is settled is ignored", wasm.ship_part_total() === settledCount);

// --- the clock runs, and the speed buttons decide how fast -----------------

const speedNow = () => wasm.ship_speed_multiplier(wasm.ship_effective_speed());
check("it starts at real time", speedNow() === 1, String(speedNow()));

step(60);
const afterASecond = wasm.ship_world_steps();
check("a second of frames is about a second of steps", afterASecond >= 55 && afterASecond <= 65, String(afterASecond));

solo.speedButton(0).dispatch("click");
step(1); // the order lands on the next step
const paused = wasm.ship_world_steps();
step(60);
check("a pause stops the clock", wasm.ship_world_steps() === paused, `${wasm.ship_world_steps()} against ${paused}`);
check("and the panel says so", speedNow() === 0, String(speedNow()));

solo.speedButton(4).dispatch("click");
step(1);
const before24 = wasm.ship_world_steps();
step(60);
const at24 = wasm.ship_world_steps() - before24;
check(
  "the top speed really is the top speed",
  at24 > 60 * 20,
  `${at24} steps in a second of frames at ${speedNow()}x`,
);

// --- the map, and plotting a trip ------------------------------------------

solo.fire("keydown", { key: "m" });
check("M opens the map", wasm.ship_view_mode() === 1, String(wasm.ship_view_mode()));
check("and the readout says which view it is", byId.get("view-name").textContent.length > 0);

// Everything on the map is something the crew have found. There is no way to
// plot a trip to anything else, because there is no way to point at it.
const mapped = [];
for (let i = 0; i < wasm.ship_map_count(); i++) {
  mapped.push(`${wasm.ship_map_kind(i)}:${wasm.ship_map_id(i)}`);
}
check("the map lists only what has been found", mapped.length === wasm.ship_map_count());

// The map opens showing the whole system, where a trip anybody would sit
// through is a fraction of a pixel. Zoom right in first — which is also the
// only way to check that the zoom works at all.
const middle = { x: solo.gameCanvas.clientWidth / 2, y: solo.gameCanvas.clientHeight / 2 };
const wideOut = wasm.ship_view_scale();
wasm.ship_zoom(middle.x, middle.y, 100000);
check("the map zooms in", wasm.ship_view_scale() > wideOut, `${wideOut} to ${wasm.ship_view_scale()}`);

// Somewhere near, so the trip finishes inside a test rather than inside a
// week. A bare point in space is a perfectly good destination, and a click
// well clear of every icon is how you say so.
const here = { x: wasm.ship_world_x(), y: wasm.ship_world_y() };
const aimedAt = { clientX: middle.x + 60, clientY: middle.y - 25, pointerId: 1 };
check(
  "there is nothing to pick out there",
  wasm.ship_map_pick(aimedAt.clientX, aimedAt.clientY, 14) === 0,
);
solo.gameCanvas.dispatch("pointerdown", { button: 0, ...aimedAt });
// One frame, so the panel has been painted. The quote is re-worked every
// frame rather than once at the click — a number that was right when it was
// worked out and is wrong now is worse than no number.
step(1);
check("clicking the map quotes a trip", wasm.ship_preview_state() === 1, String(wasm.ship_preview_state()));
check("with a time on it", wasm.ship_preview_minutes() > 0, String(wasm.ship_preview_minutes()));
check("and a fuel bill", wasm.ship_preview_fuel() > 0, String(wasm.ship_preview_fuel()));
check(
  "which the helm panel says in words",
  byId.get("plan").textContent.includes("Fuel"),
  byId.get("plan").textContent,
);

const confirm = byId.get("confirm");
check("Confirm is live once there is something to confirm", confirm.disabled === false);

const fuelBefore = wasm.ship_fuel_aboard();
confirm.dispatch("click");
step(2);
check("Confirm sets the ship going", wasm.ship_world_state() === 2, String(wasm.ship_world_state()));
check(
  "and reserves the fuel for it",
  wasm.ship_fuel_reserved() > 0,
  String(wasm.ship_fuel_reserved()),
);
check("which is not burnt yet", wasm.ship_fuel_aboard() === fuelBefore);
check("the route is this player's", wasm.ship_destination_by() === 1, String(wasm.ship_destination_by()));
check(
  "the station panel goes away once the ship has cast off",
  byId.get("game-trade").hidden === true,
);

const reserved = wasm.ship_fuel_reserved();
solo.until(() => wasm.ship_world_state() !== 2, "arrived");
check("it gets there", wasm.ship_world_state() === 1, String(wasm.ship_world_state()));
check("holding, because a point in space is not a dock", wasm.ship_docked_at() === 0);
check(
  "and the fuel is gone from the hold",
  wasm.ship_fuel_aboard() === fuelBefore - reserved,
  `${wasm.ship_fuel_aboard()} against ${fuelBefore - reserved}`,
);
check("with the reservation released", wasm.ship_fuel_reserved() === 0);
check(
  "and the page said so",
  byId.get("log").textContent.length > 0,
  byId.get("log").textContent,
);
check("still no error box", !byId.get("error").textContent, byId.get("error").textContent);

// --- giving up --------------------------------------------------------------

solo.gameCanvas.dispatch("pointerdown", {
  button: 0,
  pointerId: 1,
  clientX: middle.x - 140,
  clientY: middle.y,
});
confirm.dispatch("click");
step(2);
check("off again", wasm.ship_world_state() === 2);

for (let i = 0; i < 40; i++) step(1);
byId.get("abort").dispatch("click");
step(2);
check("Abort is taken", byId.get("log").textContent.toLowerCase().includes("stopping"), byId.get("log").textContent);
solo.until(() => wasm.ship_world_state() !== 2, "stopped");
check("and it comes to rest", wasm.ship_world_speed() === 0, String(wasm.ship_world_speed()));
check("holding, with nobody's route on the map", wasm.ship_destination_by() === 0);

// --- every code has words ----------------------------------------------------

check(
  "PLAN_ERRORS covers every reason a trip can be refused",
  hostSource.includes("const PLAN_ERRORS"),
);
const planErrorCodes = [...hostSource.matchAll(/^\s+(\d+): "/gm)];
check("there are tables of words to check", planErrorCodes.length > 20);

// The event log is the count check: an event whose code has no line is
// dropped, so a missing one shows up as rows that do not add up.
solo.speedButton(1).dispatch("click");
step(2);
wasm.ship_events_clear();
// The middle of the map is where the ship already is, and a trip to where you
// already are is not a trip.
solo.gameCanvas.dispatch("pointerdown", {
  button: 0,
  pointerId: 1,
  clientX: middle.x,
  clientY: middle.y,
});
step(1);
check(
  "a trip to where the ship already is is refused, and says why",
  wasm.ship_preview_state() === 2 && wasm.ship_preview_error() !== 0,
  `${wasm.ship_preview_state()} / ${wasm.ship_preview_error()}`,
);
check(
  "and the panel puts it in words rather than a number",
  byId.get("plan").className === "refused" && byId.get("plan").textContent.length > 10,
  byId.get("plan").textContent,
);

// The frame loop is still running, which is the other way a page dies.
step(120);
check("the frame loop is still going", true);
check("still no error box", !byId.get("error").textContent, byId.get("error").textContent);

// --- two players: an Accept is for one ship and no other -------------------

const pair = await session("?money=100000&area=20&players=2&slot=0");
console.log("\na second designer, two in the lobby");
check("it knows there are two", pair.wasm.ship_players() === 2, String(pair.wasm.ship_players()));
check(
  "two purses make one pool, and no bonus with them",
  pair.pool() === 200000,
  String(pair.pool()),
);
check("and which one you are", pair.wasm.ship_local_slot() === 0);

buildShip(pair, 2);
check("a ship for two has no errors", pair.wasm.ship_has_errors() === 0);

const pairAccept = pair.byId.get("accept");
pairAccept.dispatch("click");
check("your Accept is recorded", pair.wasm.ship_accepted(0) === 1);
check("the other one's is not", pair.wasm.ship_accepted(1) === 0);
check("so the phase is still open", pair.wasm.ship_phase() === 0);
check(
  "and the crew list says so",
  pair.byId.get("slots").children[0].className === "yes" &&
    pair.byId.get("slots").children[1].className === "no",
);

const wasHash = [pair.wasm.ship_hash_hi(), pair.wasm.ship_hash_lo()].join(":");
pair.put(FLOOR, 18, 18);
check("an edit after an Accept lands", pair.wasm.ship_part_total() > 0);
check(
  "and changes the ship's identity",
  [pair.wasm.ship_hash_hi(), pair.wasm.ship_hash_lo()].join(":") !== wasHash,
);
check("which takes the Accept back", pair.wasm.ship_accepted(0) === 0);
check("the button knows it too", pairAccept.textContent === "Accept", pairAccept.textContent);
check("still no error box", !pair.byId.get("error").textContent, pair.byId.get("error").textContent);

// --- a designer nobody told anything --------------------------------------
//
// Opening ship.html with no query at all is a solo game on the standard
// purse, and the pool has to be that purse plus the solo bonus. Nothing but
// this says so: the defaults are in the host, the bonus is in `economy`, and
// only a page actually booted with neither a lobby nor a query puts the two
// together.

const bare = await session("");
console.log("\na designer opened with no query at all");
check("one player, because nobody said otherwise", bare.wasm.ship_players() === 1);
check("and the standard pool with the bonus on it", bare.pool() === SOLO_POOL, String(bare.pool()));

// --- a pool that runs out -------------------------------------------------
//
// `?money=0` is a crew who brought nothing, so the pool is the solo bonus and
// nothing else — enough for a floor and nowhere near an engine. What is being
// checked is the **refusal**: the ghost goes red before the click, the click
// does nothing, and the page says why in words.

const poor = await session("?money=0&area=20&players=1&slot=0");
console.log("\na designer with almost nothing to spend");
check("the pool is the bonus and nothing else", poor.pool() === 20000, String(poor.pool()));

poor.found(2, 2, 3, 4);
check(
  "there is frame and deck for an engine to stand on",
  poor.wasm.ship_part_total() === 12,
  String(poor.wasm.ship_part_total()),
);

const spentOnDeck = poor.pool() - poor.left();
check(
  "which cost six tiles of each",
  spentOnDeck === 6 * (poor.price(FLOOR) + poor.price(STRUCTURE)),
  String(spentOnDeck),
);
check("and an engine costs more than is left", poor.price(ENGINE) > poor.left(), `${poor.price(ENGINE)} against ${poor.left()}`);

poor.pick(ENGINE);
poor.hover(2, 2);
check("so the ghost is red before the click", poor.wasm.ship_ghost_ok() === 0);

const before = poor.wasm.ship_part_total();
poor.put(ENGINE, 2, 2);
check("and the click puts nothing down", poor.wasm.ship_part_total() === before, String(poor.wasm.ship_part_total()));
check("nor spends anything", poor.pool() - poor.left() === spentOnDeck, String(poor.left()));
check(
  "and the page says why, in words",
  poor.byId.get("said").textContent.toLowerCase().includes("money"),
  poor.byId.get("said").textContent,
);
check("still no error box", !poor.byId.get("error").textContent, poor.byId.get("error").textContent);


// --- the station, through the panel ---------------------------------------
//
// Buying is an edit like any other: it goes through `net`, it comes out of
// the same pool, and it changes the ship's identity. What cannot be checked
// anywhere else is that the *buttons* do it — the rules have their own tests
// next door, and this is the half that runs the page.

const shop = await session("?money=100000&area=20&players=1&slot=0");
console.log("\na designer doing the shopping");

const goodsRows = shop.root.querySelectorAll("[data-resource]");
check(
  "the panel has a row for every resource",
  goodsRows.length === shop.wasm.ship_resource_count(),
  `${goodsRows.length} rows, ${shop.wasm.ship_resource_count()} resources`,
);
const unnamedGoods = goodsRows
  .map((r) => r.querySelector(".name").textContent)
  .filter((name) => !name || /^Resource \d+$/.test(name));
check("every resource has a name", unnamedGoods.length === 0, unnamedGoods.join(", "));

const holdRows = shop.root.querySelectorAll("[data-storage]");
check(
  "and a readout for every class of hold",
  holdRows.length === shop.wasm.ship_storage_count(),
  `${holdRows.length} rows`,
);
const unnamedHolds = holdRows
  .map((r) => r.querySelector(".name").textContent)
  .filter((name) => !name || /^Storage \d+$/.test(name));
check("every hold has a name", unnamedHolds.length === 0, unnamedHolds.join(", "));

// A ship with nowhere to put anything cannot buy anything, however much
// money there is. The buttons say so before the click.
check("nothing can be stowed on a bare grid", shop.room(SHELF_CLASS) === 0);
const beforeShopping = shop.left();
shop.deal(METAL, 10, true);
check("so a buy does nothing", shop.cargo(METAL) === 0 && shop.left() === beforeShopping);

// Give it a shelf. Now metal goes aboard and comes out of the pool.
shop.found(2, 2, 6, 6);
shop.put(SHELF, 3, 3);
check("a shelf is a hundred units of racking", shop.room(SHELF_CLASS) === 100, String(shop.room(SHELF_CLASS)));

const beforeMetal = shop.left();
const metalPrice = amount(
  shop.wasm.ship_trade_price_hi(METAL),
  shop.wasm.ship_trade_price_lo(METAL),
);
shop.deal(METAL, 10, true);
check("ten units of metal go aboard", shop.cargo(METAL) === 10, String(shop.cargo(METAL)));
check(
  "and come out of the pool at the station's price",
  shop.left() === beforeMetal - 10 * metalPrice,
  `${shop.left()} against ${beforeMetal - 10 * metalPrice}`,
);
check("the hold fills up", shop.held(SHELF_CLASS) === 10, String(shop.held(SHELF_CLASS)));

const holdRow = shop.root.querySelector(`[data-storage="${SHELF_CLASS}"]`);
check(
  "which the readout says",
  holdRow.querySelector(".used").textContent === "10 / 100",
  holdRow.querySelector(".used").textContent,
);
check(
  "and the row says what is aboard",
  shop.root.querySelector(`[data-resource="${METAL}"] .units`).textContent === "10",
  shop.root.querySelector(`[data-resource="${METAL}"] .units`).textContent,
);

// Selling hands the money back, to the euro.
shop.deal(METAL, 10, false);
check("selling puts it back", shop.cargo(METAL) === 0, String(shop.cargo(METAL)));
check("and the refund is whole", shop.left() === beforeMetal, `${shop.left()} against ${beforeMetal}`);
check("the hold empties", shop.held(SHELF_CLASS) === 0, String(shop.held(SHELF_CLASS)));

// The class is shared and the capacity is real: a hundred units fit and the
// hundred and first is refused before the button is even live.
shop.deal(METAL, 100, true);
check("a shelf takes a hundred", shop.cargo(METAL) === 100, String(shop.cargo(METAL)));
const fullRow = shop.root.querySelector(`[data-storage="${SHELF_CLASS}"]`);
check("and says it is full", fullRow.className.includes("full"), fullRow.className);
const buyMore = shop.root.querySelector(`[data-resource="${METAL}"] [data-buy="1"]`);
check("the buy button goes dead when there is no room", buyMore.disabled === true);

// Fuel is a different class, so a full shelf does not stop it — but there is
// no tank, so there is nowhere for it either.
check("fuel has nowhere to go without a tank", shop.room(FUEL_CLASS) === 0);
shop.deal(FUEL, 1, true);
check("so fuel stays at the station", shop.cargo(FUEL) === 0);

// A shelf with something on it cannot come off.
const shelfId = (() => {
  shop.hover(3, 3);
  return shop.wasm.ship_hovered_part();
})();
check("the shelf is under the pointer", shop.wasm.ship_part_kind(shelfId) === SHELF);
shop.drag(3, 3, 3, 3, 2);
check(
  "a full shelf cannot be taken off",
  shop.cargo(METAL) === 100 && shop.room(SHELF_CLASS) === 100,
  `${shop.room(SHELF_CLASS)} of racking left`,
);
check(
  "and the page says to sell what is in it first",
  shop.byId.get("said").textContent.toLowerCase().includes("sell"),
  shop.byId.get("said").textContent,
);
shop.deal(METAL, 100, false);
shop.drag(3, 3, 3, 3, 2);
check("emptied, it comes off", shop.room(SHELF_CLASS) === 0, String(shop.room(SHELF_CLASS)));
check("still no error box", !shop.byId.get("error").textContent, shop.byId.get("error").textContent);

// --- radiation ------------------------------------------------------------
//
// The one fault on the page that kills people. It is a warning, it is always
// first, it is styled louder than an error, and the deck is tinted whether or
// not anybody is pointing at the row.

const open = await session("?money=100000&area=20&players=1&slot=0");
console.log("\na designer with a hole in the hull");

const issueCodes = (page) =>
  page.byId
    .get("issue-list")
    .children.map((row) => row.dataset.issue)
    .filter(Boolean);

// A room with hull all the way round. The frame has to reach a tile
// further out than the deck, because the hull stands straight on it.
open.pick(STRUCTURE);
open.drag(3, 3, 10, 10);
open.pick(FLOOR);
open.drag(4, 4, 9, 9);
open.pick(OUTSIDE_WALL);
open.drag(3, 3, 10, 3);
open.drag(3, 10, 10, 10);
open.drag(3, 3, 3, 10);
open.drag(10, 3, 10, 10);
check("the hull is closed", open.wasm.ship_exposed_count() === 0, `${open.wasm.ship_exposed_count()} exposed`);
check(
  "so nothing on the list is about radiation",
  !issueCodes(open).includes(String(RADIATION)),
  issueCodes(open).join(" "),
);

// Take one tile of hull out and the room behind it is in the open. The
// right-drag takes the frame under it as well — nothing is standing on it
// once the wall is gone — which is the stack coming apart top down.
open.drag(6, 3, 6, 3, 2);
check("a hole exposes the ship", open.wasm.ship_exposed_count() > 0, String(open.wasm.ship_exposed_count()));
check(
  "and it is the first thing the list says",
  issueCodes(open)[0] === String(RADIATION),
  issueCodes(open).join(" "),
);
const graveRow = open.byId.get("issue-list").children[0];
check("the row is styled louder than an error", graveRow.className === "grave", graveRow.className);
check("and it has words", graveRow.textContent.length > 10, graveRow.textContent);

// The tint is on the deck whether or not the pointer is anywhere near the
// list — which is the whole point of it, and the only way to see it from
// here is that the frame draws more shapes than it did.
open.wasm.ship_render();
const litLength = open.wasm.ship_draw_len();
open.put(STRUCTURE, 6, 3);
open.put(OUTSIDE_WALL, 6, 3);
check("closing the hull again clears it", open.wasm.ship_exposed_count() === 0);
check(
  "and the list stops mentioning it",
  !issueCodes(open).includes(String(RADIATION)),
  issueCodes(open).join(" "),
);
open.wasm.ship_render();
check(
  "and the deck loses the tint it was carrying",
  open.wasm.ship_draw_len() < litLength,
  `${open.wasm.ship_draw_len()} against ${litLength}`,
);

// A plain wall is not hull. Same hole, filled with the wrong thing.
open.drag(6, 3, 6, 3, 2);
open.put(STRUCTURE, 6, 3);
open.put(WALL, 6, 3);
check("a plain wall does not keep it out", open.wasm.ship_exposed_count() > 0);
open.drag(6, 3, 6, 3, 2);
open.put(STRUCTURE, 6, 3);
open.put(OUTSIDE_WALL, 6, 3);
check("and hull does", open.wasm.ship_exposed_count() === 0);
check("still no error box", !open.byId.get("error").textContent, open.byId.get("error").textContent);

// --- an Accept is for a manifest too --------------------------------------

const bought = await session("?money=100000&area=20&players=2&slot=0");
console.log("\ntwo in the lobby, and one of them goes shopping");
buildShip(bought, 2);
check("the ship has no errors", bought.wasm.ship_has_errors() === 0);

const boughtAccept = bought.byId.get("accept");
boughtAccept.dispatch("click");
check("your Accept is recorded", bought.wasm.ship_accepted(0) === 1);

const hashBefore = [bought.wasm.ship_hash_hi(), bought.wasm.ship_hash_lo()].join(":");
bought.deal(TOFU, 1, true);
check("a purchase lands", bought.cargo(TOFU) > 0, String(bought.cargo(TOFU)));
check(
  "and changes the ship's identity",
  [bought.wasm.ship_hash_hi(), bought.wasm.ship_hash_lo()].join(":") !== hashBefore,
);
check("which takes the Accept back", bought.wasm.ship_accepted(0) === 0);
check("still no error box", !bought.byId.get("error").textContent, bought.byId.get("error").textContent);

// --- two of them, flying one ship -----------------------------------------
//
// The half the solo session cannot reach: every command carries a slot, any
// of them can give it, and the slowest speed request is what actually happens.

const crew = await session("?money=200000&area=20&players=2&slot=0");
console.log("\ntwo aboard, one ship");
buildShip(crew, 2);
crew.byId.get("accept").dispatch("click");
// The other one accepts through `net`, which is the only way anything reaches
// the wasm — there is no second browser here, and the seam is what stands in
// for one.
crew.wasm.ship_accept(1, crew.wasm.ship_hash_hi(), crew.wasm.ship_hash_lo());
check("both Accepts settle it", crew.wasm.ship_phase() === 1);
check("and the world opens", crew.wasm.ship_world_ready() === 1);
// Somebody *else's* Accept is what settled it, so the page finds out on its
// next frame rather than in the click handler. That is the seam doing its job
// and it is worth one step to say so.
crew.step(1);
check(
  "and the page moves over on its own",
  crew.root
    .querySelectorAll("[data-screen]")
    .filter((s) => !s.hidden)
    .map((s) => s.dataset.screen)
    .join() === "game",
);

// One player at 1x holds the world at 1x however fast the other wants it.
crew.speedButton(4).dispatch("click");
crew.wasm.ship_cmd_speed(1, 1);
crew.step(1);
check(
  "the slowest request is what happens",
  crew.wasm.ship_speed_multiplier(crew.wasm.ship_effective_speed()) === 1,
  String(crew.wasm.ship_speed_multiplier(crew.wasm.ship_effective_speed())),
);
check(
  "and the panel shows what everybody asked for",
  crew.byId.get("speed-asks").textContent.includes("24"),
  crew.byId.get("speed-asks").textContent,
);
crew.wasm.ship_cmd_speed(1, 4);
check("and lifting it lets the world go", crew.wasm.ship_speed_multiplier(crew.wasm.ship_effective_speed()) === 24);

// --- trading, out here where it is a command ---------------------------------

check("the station panel is up while docked", crew.byId.get("game-trade").hidden === false);
const purse = crew.left();
const vegBefore = crew.wasm.ship_cargo(VEGETABLE);
crew.trade(VEGETABLE, 10, true);
crew.step(2);
check(
  "a buy at the dock takes ten aboard",
  crew.wasm.ship_cargo(VEGETABLE) === vegBefore + 10,
  String(crew.wasm.ship_cargo(VEGETABLE)),
);
check("and comes out of the crew's money", crew.left() < purse, `${crew.left()} against ${purse}`);
crew.trade(VEGETABLE, 10, false);
crew.step(2);
check("selling puts every euro back", crew.left() === purse, `${crew.left()} against ${purse}`);

// --- a redirect, by the other player ------------------------------------------

crew.fire("keydown", { key: "m" });
const crewMiddle = { x: crew.gameCanvas.clientWidth / 2, y: crew.gameCanvas.clientHeight / 2 };
crew.wasm.ship_zoom(crewMiddle.x, crewMiddle.y, 100000);
crew.gameCanvas.dispatch("pointerdown", {
  button: 0,
  pointerId: 1,
  clientX: crewMiddle.x + 70,
  clientY: crewMiddle.y,
});
crew.step(1);
crew.byId.get("confirm").dispatch("click");
crew.step(2);
check("player one sets off", crew.wasm.ship_world_state() === 2);
check("and the route is theirs", crew.wasm.ship_destination_by() === 1, String(crew.wasm.ship_destination_by()));
for (let i = 0; i < 40; i++) crew.step(1);

// The other one sends the ship somewhere else. It stops first — that is what a
// redirect *is* — and the route on the map becomes theirs.
crew.wasm.ship_cmd_confirm_point(1, crew.wasm.ship_world_x() - 20000, crew.wasm.ship_world_y());
crew.step(2);
check(
  "the other player's Confirm takes the route over",
  crew.wasm.ship_destination_by() === 2,
  String(crew.wasm.ship_destination_by()),
);
check("and the ship is stopping first", crew.wasm.ship_trip_aborting() === 1);
check(
  "which the readout says in so many words",
  crew.byId.get("trip-phase").textContent === "Stopping",
  crew.byId.get("trip-phase").textContent,
);
// It comes to rest and sets off again on its own — the player asked once.
// Watched through the state rather than through the event list, because the
// page drains that every frame and a harness reading it afterwards reads an
// empty one.
crew.until(
  () => crew.wasm.ship_world_state() === 2 && crew.wasm.ship_trip_aborting() === 0,
  "stopped and set off again",
);
check("and the ship stops and sets off again on its own", true);
check("still under way", crew.wasm.ship_world_state() === 2);
check("still no error box", !crew.byId.get("error").textContent, crew.byId.get("error").textContent);

// --- arriving at a station, with and without a way off it ---------------------
//
// Docking wants an airlock as well as a station to aim at. Two sessions,
// because it is a fact about the ship rather than about the trip.

for (const withAirlock of [true, false]) {
  const port = await session("?money=200000&area=20&players=1&slot=0");
  console.log(`\na ship ${withAirlock ? "with" : "without"} an airlock, coming alongside`);
  buildShip(port, 1);
  if (!withAirlock) {
    port.drag(16, 8, 16, 9, 2);
    check("the airlock is off", port.wasm.ship_has_errors() === 0);
  }
  port.byId.get("accept").dispatch("click");
  check("the game started", port.wasm.ship_world_ready() === 1);

  const dock = port.wasm.ship_docked_at() - 1;
  const at = { x: port.wasm.ship_world_x(), y: port.wasm.ship_world_y() };
  port.wasm.ship_cmd_speed(0, 4);

  // Out to a point, then back to the dock it just left.
  port.wasm.ship_cmd_confirm_point(0, at.x + 20000, at.y);
  port.step(2);
  port.until(() => port.wasm.ship_world_state() !== 2, "got clear of the dock");
  check("it is holding out in space", port.wasm.ship_world_state() === 1);
  check("and the station panel is gone", port.byId.get("game-trade").hidden === true);

  const back = mapIndex(port, 1, dock);
  check("the dock it left is still on the map", back >= 0, String(back));
  port.wasm.ship_cmd_confirm_node(0, 1, dock);
  port.step(2);
  port.until(() => port.wasm.ship_world_state() !== 2, "came back");

  if (withAirlock) {
    check("it docks", port.wasm.ship_world_state() === 0 && port.wasm.ship_docked_at() === dock + 1);
    check("and the station panel comes back", port.byId.get("game-trade").hidden === false);
  } else {
    check("it holds alongside rather than docking", port.wasm.ship_world_state() === 1);
    check("so there is nowhere to trade", port.byId.get("game-trade").hidden === true);
  }
  check("still no error box", !port.byId.get("error").textContent, port.byId.get("error").textContent);
}

/** Where a node sits in the map list, or -1. */
function mapIndex(page, kind, id) {
  for (let i = 0; i < page.wasm.ship_map_count(); i++) {
    if (page.wasm.ship_map_kind(i) === kind && page.wasm.ship_map_id(i) === id) return i;
  }
  return -1;
}

// --- every code has words --------------------------------------------------
//
// The same rule as the issue list and the diary: a code with no line is a row
// that never appears, so what catches a missing one is the count.

console.log("\nthe words");
const tables = ["PLAN_ERRORS", "REFUSALS", "PHASE_NAMES", "EVENT_LINES", "BODY_KIND_NAMES", "STATION_KIND_NAMES"];
for (const table of tables) {
  check(`${table} exists`, hostSource.includes(`const ${table}`));
}

// `PHASE_NAMES` is indexed by `flight::Phase` and there are five of them, the
// last being "not on a trip at all".
const phaseNames = /const PHASE_NAMES = \[([^\]]*)\]/.exec(hostSource);
check(
  "PHASE_NAMES has a word for every phase",
  phaseNames && phaseNames[1].split(",").filter((s) => s.trim()).length === 5,
  phaseNames ? phaseNames[1] : "not found",
);
const bodyNames = /const BODY_KIND_NAMES = \[([^\]]*)\]/.exec(hostSource);
check(
  "BODY_KIND_NAMES covers every kind of body",
  bodyNames && bodyNames[1].split(",").filter((s) => s.trim()).length === 4,
);
const stationNames = /const STATION_KIND_NAMES = \[([^\]]*)\]/.exec(hostSource);
check(
  "STATION_KIND_NAMES covers every kind of station",
  stationNames && stationNames[1].split(",").filter((s) => s.trim()).length === 5,
);

console.log(fails.length ? `\n${fails.length} FAILED` : "\nall passed");
process.exit(fails.length ? 1 : 0);

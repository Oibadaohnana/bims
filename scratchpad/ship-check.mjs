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
const SHELF = 25;

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

  return {
    ...page,
    canvas,
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
  };
}

/** The reference ship, built through the palette: deck, hull, galley, heads,
 * a table, a bay, a locker, an engine, and a bed and a seat each.
 *
 * Deliberately the same layout as `shipdesign::fixture::reference`, so a
 * change to the rules that breaks one breaks both — but built out of pointer
 * events rather than out of `apply`, which is the half a native test cannot
 * reach. */
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

  // And something to eat, so the ship is provisioned the way the fixture is.
  // Bought through the panel, which is the only way a player can.
  page.deal(VEGETABLE, 10, true);
  page.deal(TOFU, 10, true);
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
  "ship_tile",
]);
const orphans = [...exported].filter((name) => !called.has(name) && !FOR_THE_HARNESS.has(name));
check("no export has quietly stopped being called", orphans.length === 0, orphans.join(", "));

// --- the wasm agrees with the native build --------------------------------

const ALL_CHECKS = 0b111111;
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
check(
  "the warnings that are left still have words",
  issueRows().length === wasm.ship_issue_count(),
  `${issueRows().length} rows, ${wasm.ship_issue_count()} issues`,
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

accept.dispatch("click");
check("one Accept settles a solo ship", wasm.ship_phase() === 1, String(wasm.ship_phase()));
check("the handoff screen takes over", showing() === "done", showing());
check("and nothing can be moved any more", wasm.ship_phase() !== 0);

const handoff = byId.get("handoff").textContent;
check("the handoff says what the ship weighs", /Ship mass/.test(handoff) && wasm.ship_mass() > 0, handoff);
check(
  "and what is left of the money",
  handoff.includes("Money left") && digits(handoff).includes(String(solo.left())),
  handoff,
);
check(
  "and what is in the hold",
  handoff.includes("Cargo") && handoff.includes("Vegetables") && handoff.includes("Tofu"),
  handoff,
);
check(
  "and that it can push itself forward",
  wasm.ship_acceleration(0) > 0,
  String(wasm.ship_acceleration(0)),
);
check(
  "but not backward, with one engine",
  wasm.ship_acceleration(1) === 0,
  String(wasm.ship_acceleration(1)),
);

// A click on the deck after that does nothing at all.
const settledCount = wasm.ship_part_total();
solo.put(FLOOR, 6, 6);
check("a click after the design is settled is ignored", wasm.ship_part_total() === settledCount);

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

console.log(fails.length ? `\n${fails.length} FAILED` : "\nall passed");
process.exit(fails.length ? 1 : 0);

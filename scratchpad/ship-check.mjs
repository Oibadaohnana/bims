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

/** Metal, in `physics::ResourceId` order. The one resource everything costs. */
const METAL = 1;

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

  return { ...page, canvas, tile, at, pick, drag, put, hover };
}

/** The reference ship, built through the palette: deck, hull, galley, heads,
 * a table, a bay, a locker, an engine, and a bed and a seat each.
 *
 * Deliberately the same layout as `shipdesign::fixture::reference`, so a
 * change to the rules that breaks one breaks both — but built out of pointer
 * events rather than out of `apply`, which is the half a native test cannot
 * reach. */
function buildShip(page, crew) {
  const { pick, drag, put } = page;

  // Deck first, as one rectangle.
  pick(FLOOR);
  drag(2, 2, 17, 17);

  // Hull round the outside. Four straight runs; the corners overlap and are
  // skipped, which is the drag rule doing what it should.
  pick(WALL);
  drag(1, 1, 18, 1);
  drag(1, 18, 18, 18);
  drag(1, 1, 1, 18);
  drag(18, 1, 18, 18);

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
  put(HYDRO_BAY, 10, 10);
  put(ENGINE, 7, 13);

  for (let i = 0; i < crew; i++) {
    put(BUNK, 14, 8 + 2 * i);
    put(CHAIR, 4 + (i % 2), 7 + 2 * Math.floor(i / 2));
  }
}

// --- a solo designer, on a small ship -------------------------------------

const solo = await session("?stock=1&area=20&players=1&slot=0");
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

const ALL_CHECKS = 0b11111;
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

// --- the stockpile --------------------------------------------------------

const storeRows = root.querySelectorAll("[data-resource]");
check(
  "the stores show every resource",
  storeRows.length === wasm.ship_resource_count(),
  `${storeRows.length} rows`,
);
const metalAtStart = wasm.ship_remaining(METAL);
check("and there is metal to build with", metalAtStart > 0, String(metalAtStart));
check(
  "the readout says what is left",
  storeRows[METAL].querySelector(".left").textContent === String(metalAtStart),
  storeRows[METAL].querySelector(".left").textContent,
);

// --- one tile, there and back ---------------------------------------------

solo.put(FLOOR, 5, 5);
check("a click lays a tile of deck", wasm.ship_part_total() === 1, String(wasm.ship_part_total()));
check(
  "and it comes out of the stores",
  wasm.ship_remaining(METAL) === metalAtStart - 1,
  `${wasm.ship_remaining(METAL)} left of ${metalAtStart}`,
);
check(
  "which the readout picks up",
  storeRows[METAL].querySelector(".left").textContent === String(metalAtStart - 1),
  storeRows[METAL].querySelector(".left").textContent,
);

// A right-drag clears, and a removal refunds the whole cost.
solo.drag(5, 5, 5, 5, 2);
check("a right-drag takes it off again", wasm.ship_part_total() === 0);
check(
  "and hands the metal back",
  wasm.ship_remaining(METAL) === metalAtStart,
  String(wasm.ship_remaining(METAL)),
);

// --- the ghost ------------------------------------------------------------

solo.pick(HOB);
solo.hover(5, 5);
check("a hob will not stand on nothing", wasm.ship_ghost_ok() === 0);
solo.put(FLOOR, 5, 5);
solo.pick(HOB);
solo.hover(5, 5);
check("but it will stand on deck", wasm.ship_ghost_ok() === 1);

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

// Clear the one tile again so the ship below is built on a bare grid.
solo.drag(5, 5, 5, 5, 2);
check("the deck is bare again", wasm.ship_part_total() === 0, String(wasm.ship_part_total()));

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
// Two tiles of deck at opposite corners is a ship in two pieces, which is the
// simplest fault that has tiles to point at.
solo.put(FLOOR, 2, 2);
solo.put(FLOOR, 17, 17);
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
check("and the deck is bare again", wasm.ship_part_total() === 0, String(wasm.ship_part_total()));

// --- build the whole thing ------------------------------------------------

buildShip(solo, 1);
check("the ship got built", wasm.ship_part_total() > 300, String(wasm.ship_part_total()));
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
check("there was metal to spare", wasm.ship_remaining(METAL) > 0, String(wasm.ship_remaining(METAL)));

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

const pair = await session("?stock=1&area=20&players=2&slot=0");
console.log("\na second designer, two in the lobby");
check("it knows there are two", pair.wasm.ship_players() === 2, String(pair.wasm.ship_players()));
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

console.log(fails.length ? `\n${fails.length} FAILED` : "\nall passed");
process.exit(fails.length ? 1 : 0);

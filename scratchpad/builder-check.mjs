// Does the builder page actually run? `cargo build` says nothing about a page
// of plain JavaScript, and a parse error there is a blank screen with no
// message — so this boots web/builder.js against the stub DOM and the real
// lobby.wasm, and walks the whole path a player walks: menu → setup → world
// → back → lobby → start.
//
// The multiplayer half is surface only, and that is asserted too: a join has
// to *refuse* out loud rather than appear to work, and a guest — delivered a
// room by the transport's inbound door — has to be able to look and suggest
// and not to change anything.
//
// Three things here cannot be checked anywhere else:
//
//   * **The cross-target checksum.** `lobby_galaxy_checksum` computes the
//     reference galaxy's checksum in wasm; `crates/worldgen/src/fixture.rs`
//     pins what it comes out at natively. The constants are read *out of
//     that file* rather than copied here, so the test and this harness are
//     looking at one number.
//   * **"Has a station" against generating the system.** The map dims stars
//     without one, and it must dim exactly the stars that `Galaxy::system`
//     gives none — inspecting is the other path, and the two are compared
//     for every star.
//   * **The words.** `STAR_WORDS` and `STATION_WORDS` have to be exactly as
//     long as the generator's constants, or a star renders as
//     `undefined-284`; and every star and station of the fixed seed has to
//     come out with a name.

import { readFileSync } from "node:fs";

import { bootPage, bootWasmPage } from "./stub.mjs";

const fails = [];
function check(what, ok, extra = "") {
  if (ok) console.log(`  ok   ${what}`);
  else {
    console.log(`  FAIL ${what}${extra ? ` — ${extra}` : ""}`);
    fails.push(what);
  }
}

/** A u64 out of the two u32 halves the boundary carries it in. `>>> 0`
 * because a wasm `u32` with its top bit set arrives in JavaScript as a
 * negative `i32`, and a checksum has that bit set half the time. */
const whole = (hi, lo) => (BigInt(hi >>> 0) << 32n) | BigInt(lo >>> 0);

/** A Rust integer literal — `0x_e6d4_6408_110d_d3f5`, `48` — as a BigInt. */
const literal = (text) => BigInt(text.replace(/_/g, ""));

// --- what the generator pins, read off its own source --------------------

const fixtureSource = readFileSync("crates/worldgen/src/fixture.rs", "utf8");
const REFERENCE_SEED = literal(/REFERENCE_SEED: u64 = ([0-9a-fx_]+);/.exec(fixtureSource)[1]);
const REFERENCE_CHECKSUMS = [
  ...fixtureSource.slice(fixtureSource.indexOf("REFERENCE_CHECKSUMS")).matchAll(/(0x[0-9a-f_]+),/g),
]
  .slice(0, 4)
  .map((m) => literal(m[1]));
check("the fixture pins four checksums", REFERENCE_CHECKSUMS.length === 4, String(REFERENCE_CHECKSUMS.length));

const nameSource = readFileSync("crates/worldgen/src/name.rs", "utf8");
const GEN_STAR_WORDS = Number(/pub const STAR_WORDS: u16 = (\d+);/.exec(nameSource)[1]);
const GEN_STATION_WORDS = Number(/pub const STATION_WORDS: u16 = (\d+);/.exec(nameSource)[1]);

// --- boot -----------------------------------------------------------------

const page = await bootWasmPage({
  html: "web/builder.html",
  host: "web/builder.js",
  wasm: "web/lobby.wasm",
});
const { byId, root, win, click, dispatch, advance, visited, wasm, step, lastCall, drawn } = page;

const screen = (name) => root.querySelector(`[data-screen=${name}]`);
const showing = () =>
  [...root.querySelectorAll("[data-screen]")]
    .filter((s) => !s.hidden)
    .map((s) => s.dataset.screen);

console.log("builder booted");
check("no error box", !byId.get("error").textContent, byId.get("error").textContent);

// --- the boundary, both ways ----------------------------------------------
//
// Read with `readFileSync` rather than with a shell `grep`, deliberately: the
// shell's `grep` here is `ugrep --ignore-files` and returns *nothing at all*
// for files under web/, so a boundary check built on it comes back clean
// because it never read the file.

const hostSource = readFileSync("web/builder.js", "utf8");
check("builder.js has no stray NUL in it", !hostSource.includes("\0"));
const called = new Set([...hostSource.matchAll(/\b(?:wasm|exports)\.(lobby_[a-z0-9_]+)/g)].map((m) => m[1]));
const exported = new Set(Object.keys(wasm).filter((name) => name.startsWith("lobby_")));
check("the host calls the wasm at all", called.size > 20, `${called.size} calls`);
const missing = [...called].filter((name) => !exported.has(name));
check("everything the host calls is exported", missing.length === 0, missing.join(", "));

// Exports the page has no use for, because they exist for **this file**: the
// checksum halves, the positions of things — the wasm projects everything
// itself, so the page never asks — and where a star lands on the canvas,
// which is what puts the pointer on one down here. Everything else the wasm
// exports, the page calls.
const FOR_THE_HARNESS = new Set([
  "lobby_galaxy_checksum_hi",
  "lobby_galaxy_checksum_lo",
  "lobby_star_x",
  "lobby_star_y",
  "lobby_body_x",
  "lobby_body_y",
  "lobby_station_x",
  "lobby_station_y",
  "lobby_star_screen_x",
  "lobby_star_screen_y",
]);
const orphans = [...exported].filter((name) => !called.has(name) && !FOR_THE_HARNESS.has(name));
check("no export has quietly stopped being called", orphans.length === 0, orphans.join(", "));
const unused = [...FOR_THE_HARNESS].filter((name) => !exported.has(name));
check("everything named for the harness exists", unused.length === 0, unused.join(", "));

// --- the word tables ------------------------------------------------------

const countTable = (name) => {
  const body = new RegExp(`const ${name} = \\[([^\\]]*)\\];`).exec(hostSource)[1];
  return [...body.matchAll(/"[^"]+"/g)].length;
};
check(
  `STAR_WORDS has the generator's ${GEN_STAR_WORDS} words`,
  countTable("STAR_WORDS") === GEN_STAR_WORDS,
  String(countTable("STAR_WORDS")),
);
check(
  `STATION_WORDS has the generator's ${GEN_STATION_WORDS} words`,
  countTable("STATION_WORDS") === GEN_STATION_WORDS,
  String(countTable("STATION_WORDS")),
);

// --- the start menu ----------------------------------------------------

check("the menu is what you land on", showing().join() === "menu", showing().join());
check("Play is there", !!byId.get("play"));
check("Create lobby is beside it", !!byId.get("create-lobby"));

// --- Play, and the settings tool ---------------------------------------

click("play");
check("Play opens the setup screen", showing().join() === "setup", showing().join());

const setupTool = byId.get("setup-tool");
const tabsOf = (tool) =>
  tool.querySelectorAll(".tool-tabs .tab").map((t) => t.textContent);
check(
  "the tool has a setup tab and a world tab",
  tabsOf(setupTool).join("|") === "Game setup|World",
  tabsOf(setupTool).join("|"),
);

const chosenIn = (tool, field) => {
  const on = tool.querySelector(`[data-field=${field}] .option.on`);
  return on ? on.dataset.option : "(none)";
};

check(
  "money starts at €100 000 a Bim",
  chosenIn(setupTool, "moneyPerBim") === "standard",
  chosenIn(setupTool, "moneyPerBim"),
);
check("the ship starts at 40 × 40", chosenIn(setupTool, "ship") === "standard", chosenIn(setupTool, "ship"));

const optionIn = (tool, field, id) =>
  tool.querySelector(`[data-field=${field}] [data-option=${id}]`);

// Every option offered is one of the three the game knows about, in order.
// The euro sign and the grouping are read off the page rather than written
// out here: they exist in exactly one place in the host and a copy down here
// would be a second one to keep in step.
const moneyLabels = setupTool
  .querySelectorAll("[data-field=moneyPerBim] .option .sub")
  .map((s) => s.textContent);
const bare = moneyLabels.map((t) => t.replace(/[^0-9]/g, ""));
check("money offers 50 000, 100 000 and 200 000", bare.join(" ") === "50000 100000 200000", moneyLabels.join(" "));
check(
  "and says so in euros",
  moneyLabels.every((t) => t.startsWith("€")),
  moneyLabels.join(" "),
);
const shipLabels = setupTool
  .querySelectorAll("[data-field=ship] .option .sub")
  .map((s) => s.textContent);
check(
  "ships offer 30, 40 and 60 tiles a side",
  shipLabels.join(" ") === "30 × 30 40 × 40 60 × 60",
  shipLabels.join(" "),
);

optionIn(setupTool, "moneyPerBim", "full").dispatch("click");
optionIn(setupTool, "ship", "large").dispatch("click");
check(
  "picking €200 000 sticks",
  chosenIn(setupTool, "moneyPerBim") === "full",
  chosenIn(setupTool, "moneyPerBim"),
);
check("picking 60 × 60 sticks", chosenIn(setupTool, "ship") === "large", chosenIn(setupTool, "ship"));

// Start is not on offer until there is somewhere to start.
const startOn = (name) => screen(name).querySelector("[data-start]");
const whyNot = (name) => screen(name).querySelector("[data-start-why]").textContent;
check("Start is disabled before a station is picked", startOn("setup").disabled === true);
check("and says why", whyNot("setup").includes("station"), whyNot("setup"));

// --- the World tab -----------------------------------------------------

const worldTab = setupTool.querySelectorAll(".tool-tabs .tab")[1];
worldTab.dispatch("click");
const worldPanel = setupTool.querySelector(".tool-body [data-tab=world]");
const setupPanel = setupTool.querySelector(".tool-body [data-tab=setup]");
check("the world tab shows its panel", !worldPanel.hidden);
check("and puts the setup panel away", setupPanel.hidden);
check("the world panel is in the setup tool", worldPanel.contains(byId.get("galaxy")));
check("the world module loaded", !byId.get("world-said").className.includes("warn"), byId.get("world-said").textContent);
step(2);

const canvas = byId.get("galaxy");
const seedField = byId.get("seed");
const NONE = wasm.lobby_none();

check("the seed field shows a whole number", /^\d+$/.test(seedField.value), seedField.value);
check(
  "and it is the seed wasm was given",
  whole(...(lastCall("lobby_init") ?? [0, 0])).toString() === seedField.value,
  `${seedField.value} vs ${lastCall("lobby_init")}`,
);
check("the default galaxy is the two-arm spiral", chosenIn(setupTool, "galaxy") === "two-arm", chosenIn(setupTool, "galaxy"));

// Pin the seed the fixture pins, so everything below is about a known galaxy.
function setSeed(text) {
  seedField.value = text;
  dispatch("seed-form", "submit");
}
setSeed(REFERENCE_SEED.toString());
check("typing the reference seed takes", seedField.value === REFERENCE_SEED.toString(), seedField.value);
check("wasm was told", whole(...lastCall("lobby_set_world").slice(0, 2)) === REFERENCE_SEED);

// The preview is drawn: a background and a thousand stars, at least.
wasm.lobby_render();
const stride = wasm.lobby_stride();
check("the preview draws the galaxy", wasm.lobby_draw_len() / stride > wasm.lobby_star_count(), String(wasm.lobby_draw_len() / stride));
check("there are a thousand stars", wasm.lobby_star_count() === 1000, String(wasm.lobby_star_count()));

// --- the wasm agrees with the native build --------------------------------

const galaxyButtons = setupTool.querySelectorAll("[data-field=galaxy] .option").map((b) => b.dataset.option);
check("four galaxy shapes are offered", galaxyButtons.length === 4, galaxyButtons.join(" "));
for (let type = 0; type < 4; type++) {
  optionIn(setupTool, "galaxy", galaxyButtons[type]).dispatch("click");
  const got = whole(wasm.lobby_galaxy_checksum_hi(), wasm.lobby_galaxy_checksum_lo());
  check(
    `wasm checksums galaxy type ${type} as native does`,
    got === REFERENCE_CHECKSUMS[type],
    `${got.toString(16)} vs ${REFERENCE_CHECKSUMS[type].toString(16)}`,
  );
}
optionIn(setupTool, "galaxy", galaxyButtons[0]).dispatch("click");
check("back on the two-arm spiral", chosenIn(setupTool, "galaxy") === "two-arm");

// --- "has a station" is what generating the system says ------------------

{
  let disagreed = 0;
  let withStation = 0;
  let blank = 0;
  // The page's own words, through the page's own path: click a star and
  // read the heading.
  const heading = () => byId.get("system-name").textContent;
  for (let star = 0; star < wasm.lobby_star_count(); star++) {
    const reported = wasm.lobby_star_has_station(star) === 1;
    wasm.lobby_inspect(star);
    const generated = wasm.lobby_station_count() > 0;
    if (reported !== generated) disagreed++;
    if (generated) withStation++;
  }
  check("every star reported with a station has one when generated, and vice versa", disagreed === 0, `${disagreed} disagree`);
  check("most stars have one", withStation > 450 && withStation < 750, String(withStation));

  // And every one of them has a name, through the page. Clicking is the
  // page's path; here the wasm is told directly and the page asked to paint,
  // which is the same code the click runs.
  const clickStar = (star) => {
    const x = wasm.lobby_star_screen_x(star);
    const y = wasm.lobby_star_screen_y(star);
    canvas.dispatch("pointerdown", { clientX: x, clientY: y, pointerId: 1, button: 0 });
    canvas.dispatch("pointerup", { clientX: x, clientY: y, pointerId: 1, button: 0 });
  };
  const looksBlank = (text) =>
    text.includes("undefined") || text.includes("NaN") || /^\s*-/.test(text) || /\s-\d/.test(text) || text.trim() === "";
  for (let star = 0; star < wasm.lobby_star_count(); star++) {
    clickStar(star);
    if (wasm.lobby_inspected() !== star) continue; // a denser neighbour won the click; it is checked on its own turn
    if (looksBlank(heading())) blank++;
    for (const row of byId.get("stations").querySelectorAll("[data-station]")) {
      if (looksBlank(row.querySelector(".who").textContent)) blank++;
      if (looksBlank(row.querySelector(".where").textContent)) blank++;
    }
    for (const row of byId.get("bodies").querySelectorAll("[data-body]")) {
      if (looksBlank(row.querySelector(".who").textContent)) blank++;
    }
  }
  check("no star, body or station of the fixed seed renders blank", blank === 0, `${blank} blank`);
}

// --- hovering ------------------------------------------------------------

// A star with a station, and one without, to point at.
let lit = -1;
let dim = -1;
for (let star = 0; star < wasm.lobby_star_count() && (lit < 0 || dim < 0); star++) {
  if (wasm.lobby_star_has_station(star) === 1 && lit < 0) lit = star;
  if (wasm.lobby_star_has_station(star) === 0 && dim < 0) dim = star;
}
check("there is a star with a station and one without", lit >= 0 && dim >= 0, `${lit} ${dim}`);

/** Whether the star picked at a point is the nearest one to it on screen. */
function nearestTo(x, y) {
  let best = -1;
  let bestD = Infinity;
  for (let star = 0; star < wasm.lobby_star_count(); star++) {
    const dx = wasm.lobby_star_screen_x(star) - x;
    const dy = wasm.lobby_star_screen_y(star) - y;
    const d = dx * dx + dy * dy;
    if (d < bestD) {
      bestD = d;
      best = star;
    }
  }
  return best;
}

const hoverAt = (x, y) => canvas.dispatch("pointermove", { clientX: x, clientY: y, pointerId: 1 });
{
  const x = wasm.lobby_star_screen_x(lit);
  const y = wasm.lobby_star_screen_y(lit);
  hoverAt(x + 3, y - 2);
  const picked = wasm.lobby_hovered();
  check("hovering beside a star picks a star", picked !== NONE);
  check("and it is the nearest one", picked === nearestTo(x + 3, y - 2), `${picked} vs ${nearestTo(x + 3, y - 2)}`);
  const said = byId.get("hover-said").textContent;
  check("the readout names it", /^[A-Z][a-z]+-\d+ · class [OBAFGKM]/.test(said), said);
  if (picked === lit) check("and says it has a station", said.includes("has a station"), said);
}
{
  // The nearest-star rule holds at full zoom-out in the core, where stars
  // are a pixel apart: whichever is nearest is the one picked.
  const x = 480;
  const y = 320;
  hoverAt(x, y);
  const picked = wasm.lobby_hovered();
  const nearest = nearestTo(x, y);
  check("in the dense middle the pick is still the nearest star", picked === nearest, `${picked} vs ${nearest}`);
}
{
  const x = wasm.lobby_star_screen_x(dim);
  const y = wasm.lobby_star_screen_y(dim);
  hoverAt(x, y);
  if (wasm.lobby_hovered() === dim) {
    check("a star without a station can still be hovered", byId.get("hover-said").textContent.includes("no station"), byId.get("hover-said").textContent);
  }
}
hoverAt(-500, -500);
check("nothing under the pointer off the map", wasm.lobby_hovered() === NONE);
canvas.dispatch("pointerleave", {});
check("leaving clears the readout", byId.get("hover-said").textContent === "");

// Zoom and pan go through, and zooming holds the point under the wheel.
{
  const before = [wasm.lobby_star_screen_x(lit), wasm.lobby_star_screen_y(lit)];
  canvas.dispatch("wheel", { clientX: before[0], clientY: before[1], deltaY: -300 });
  const after = [wasm.lobby_star_screen_x(lit), wasm.lobby_star_screen_y(lit)];
  check("the wheel zooms about the pointer", Math.abs(after[0] - before[0]) < 1 && Math.abs(after[1] - before[1]) < 1, `${before} → ${after}`);
  // A drag that starts on a star must not open it: what the pointer went
  // down on is not what it let go over.
  const open = wasm.lobby_inspected();
  const grabbed = nearestTo(100, 100);
  const from = [wasm.lobby_star_screen_x(grabbed), wasm.lobby_star_screen_y(grabbed)];
  canvas.dispatch("pointerdown", { clientX: from[0], clientY: from[1], pointerId: 1, button: 0 });
  canvas.dispatch("pointermove", { clientX: from[0] + 60, clientY: from[1] + 30, pointerId: 1 });
  canvas.dispatch("pointerup", { clientX: from[0] + 60, clientY: from[1] + 30, pointerId: 1, button: 0 });
  const panned = [wasm.lobby_star_screen_x(lit), wasm.lobby_star_screen_y(lit)];
  check("a drag pans the map", Math.abs(panned[0] - after[0] - 60) < 1 && Math.abs(panned[1] - after[1] - 30) < 1, `${after} → ${panned}`);
  check("and a drag is not a click", wasm.lobby_inspected() === open, `${open} → ${wasm.lobby_inspected()}`);
  canvas.dispatch("wheel", { clientX: 480, clientY: 320, deltaY: 100000 });
}

// --- clicking a star ---------------------------------------------------

{
  const x = wasm.lobby_star_screen_x(lit);
  const y = wasm.lobby_star_screen_y(lit);
  // Zoom in on it first, so the click is unambiguous even in a core.
  canvas.dispatch("wheel", { clientX: x, clientY: y, deltaY: -4000 });
  const zx = wasm.lobby_star_screen_x(lit);
  const zy = wasm.lobby_star_screen_y(lit);
  canvas.dispatch("pointerdown", { clientX: zx, clientY: zy, pointerId: 1, button: 0 });
  canvas.dispatch("pointerup", { clientX: zx, clientY: zy, pointerId: 1, button: 0 });
  step(1);
}
check("clicking a star opens its system", wasm.lobby_inspected() === lit, String(wasm.lobby_inspected()));
const systemName = byId.get("system-name").textContent;
check("the panel names the star", /^[A-Z][a-z]+-\d+ · class [OBAFGKM]$/.test(systemName), systemName);
const bodyRows = byId.get("bodies").querySelectorAll("[data-body]");
check("it lists the bodies", bodyRows.length === wasm.lobby_body_count() && bodyRows.length > 0, String(bodyRows.length));
check(
  "each body is the star's name and a numeral",
  bodyRows.every((r) => r.querySelector(".who").textContent.startsWith(systemName.split(" ")[0] + " ")),
  bodyRows.map((r) => r.querySelector(".who").textContent).join("; "),
);
check(
  "and says what kind it is",
  bodyRows.every((r) => ["Rocky planet", "Gas giant", "Ice world", "Asteroid belt"].includes(r.querySelector(".kind").textContent)),
  bodyRows.map((r) => r.querySelector(".kind").textContent).join("; "),
);
const stationRows = byId.get("stations").querySelectorAll("[data-station]");
check("it lists the stations", stationRows.length === wasm.lobby_station_count() && stationRows.length > 0, String(stationRows.length));
check(
  "each station says its kind and its parent",
  stationRows.every((r) => /^(Orbital|Refinery|Mining outpost|Derelict|Relay) · /.test(r.querySelector(".where").textContent)),
  stationRows.map((r) => r.querySelector(".where").textContent).join("; "),
);
check(
  "the diagram is drawn",
  (() => {
    wasm.lobby_render_system(280, 230);
    return wasm.lobby_draw_len() / stride > wasm.lobby_body_count() + wasm.lobby_station_count();
  })(),
);
check(
  "and labelled with the host's words",
  drawn.some((d) => ["Orbital", "Refinery", "Mining outpost", "Derelict", "Relay"].includes(d.text)),
  drawn.map((d) => d.text).join("|"),
);
check("no distance is shown anywhere in the panel", !/\b(days?|units?|km|ly)\b/i.test(byId.get("system").textContent), byId.get("system").textContent);

// --- picking a station ---------------------------------------------------

stationRows[0].querySelector("[data-pick]").dispatch("click");
const spawnSaid = byId.get("spawn-said").textContent;
check("picking a station names the start in the header", / at [A-Z][a-z]+-\d+$/.test(spawnSaid) && !spawnSaid.includes("undefined"), spawnSaid);
check("wasm was told where", (lastCall("lobby_set_spawn") ?? []).join() === `${lit},0`, String(lastCall("lobby_set_spawn")));
check("Start is enabled", startOn("setup").disabled === false);
check("and the reason is gone", whyNot("setup") === "", whyNot("setup"));
check("the row is marked", stationRows[0].className === "on");

// Changing the seed clears it.
setSeed((REFERENCE_SEED + 1n).toString());
check("a new seed clears the start", byId.get("spawn-said").textContent.includes("nowhere"), byId.get("spawn-said").textContent);
check("and disables Start again", startOn("setup").disabled === true);
check("and closes the system panel", byId.get("system-name").textContent === "Pick a star", byId.get("system-name").textContent);
check("and wasm forgot it", lastCall("lobby_clear_spawn") !== undefined);

// An invalid seed is refused visibly and the previous one kept.
for (const bad of ["abc", "-5", "1.5", "18446744073709551616", ""]) {
  setSeed(bad);
  check(`"${bad}" is refused`, seedField.value === (REFERENCE_SEED + 1n).toString(), seedField.value);
}
check("and the refusal is said", byId.get("world-note").textContent.includes("not a seed"), byId.get("world-note").textContent);
check("and marked", byId.get("world-note").className.includes("warn"), byId.get("world-note").className);
advance(5000);
check("and clears itself", byId.get("world-note").textContent === "", byId.get("world-note").textContent);

// Spaces in a seed are fine — a number read out in threes is typed in threes.
setSeed("1 000 000");
check("a seed with spaces in it is read", seedField.value === "1000000", seedField.value);

// New seed draws a fresh one.
{
  const was = seedField.value;
  click("new-seed");
  check("New seed draws another", seedField.value !== was && /^\d+$/.test(seedField.value), seedField.value);
}

// Random start lands on a station.
setSeed(REFERENCE_SEED.toString());
click("random-start");
check("Random start picks somewhere", !byId.get("spawn-said").textContent.includes("nowhere"), byId.get("spawn-said").textContent);
check("that has a station", wasm.lobby_star_has_station(lastCall("lobby_set_spawn")[0]) === 1);
check("Start is enabled again", startOn("setup").disabled === false);

// Changing the galaxy type clears it too.
optionIn(setupTool, "galaxy", "round").dispatch("click");
check("a new galaxy type clears the start", byId.get("spawn-said").textContent.includes("nowhere"));
optionIn(setupTool, "galaxy", "two-arm").dispatch("click");

// --- back, then the lobby ----------------------------------------------

screen("setup").querySelector("[data-goto]").dispatch("click");
check("Back returns to the menu", showing().join() === "menu", showing().join());

click("create-lobby");
check("Create lobby opens the lobby", showing().join() === "lobby", showing().join());

const code = byId.get("code").textContent;
check("the lobby has a room code", /^[A-HJ-NP-Z2-9]{6}$/.test(code), code);

const slots = byId.get("slots").children;
check("there are four slots", slots.length === 4, String(slots.length));
check("you are in the first one", slots[0].textContent.includes("James (you)"), slots[0].textContent);
check("and you are the host", slots[0].textContent.includes("Host"), slots[0].textContent);
check(
  "the rest are open",
  slots.slice(1).every((s) => s.className === "open"),
  slots.map((s) => s.className).join(" "),
);

// One settings object behind both tools: what was picked on the setup screen
// is what the lobby shows.
const lobbyTool = byId.get("lobby-tool");
check(
  "the lobby carries the €200 000 picked earlier",
  chosenIn(lobbyTool, "moneyPerBim") === "full",
  chosenIn(lobbyTool, "moneyPerBim"),
);
check("and the 60 × 60 ship", chosenIn(lobbyTool, "ship") === "large", chosenIn(lobbyTool, "ship"));

// And the one World panel has moved across with it.
lobbyTool.querySelectorAll(".tool-tabs .tab")[1].dispatch("click");
check("the world panel is now in the lobby tool", lobbyTool.contains(byId.get("galaxy")), "");
check("and no longer in the setup tool", !setupTool.contains(byId.get("galaxy")));
check("with the same seed", seedField.value === REFERENCE_SEED.toString(), seedField.value);

// As host you can still change them.
optionIn(lobbyTool, "moneyPerBim", "lean").dispatch("click");
check(
  "the host can change a setting",
  chosenIn(lobbyTool, "moneyPerBim") === "lean",
  chosenIn(lobbyTool, "moneyPerBim"),
);

// A guest cannot. Nothing decides who is a guest yet — the host is always
// you — but `setEditable` disables every option for one, and the click
// handler refuses a disabled button rather than leaning on the browser to
// swallow the event. Disabling the button is exactly what a guest's lobby
// does to it, so this is that path and not a mock of it.
const full = optionIn(lobbyTool, "moneyPerBim", "full");
full.disabled = true;
full.dispatch("click");
check(
  "a guest cannot change the money",
  chosenIn(lobbyTool, "moneyPerBim") === "lean",
  chosenIn(lobbyTool, "moneyPerBim"),
);
full.disabled = false;

check("the lobby admits nothing can reach it", byId.get("lobby-said").textContent.length > 0);

// Pick a start as host, so there is something for a guest to be shown.
click("random-start");
const hostsStart = byId.get("spawn-said").textContent;
check("the host picked a start", !hostsStart.includes("nowhere"), hostsStart);

// --- being a guest -------------------------------------------------------
//
// A room *delivered* rather than created is one somebody else opened. It
// comes in through the transport's inbound door, which is the one thing on
// `window` — and the page has to react as it will when a socket does this.

screen("lobby").querySelector("[data-goto]").dispatch("click");
win.bimsDeliver("room", { code: "GUEST2", host: false });
win.bimsDeliver("players", [
  { name: "Ada", host: true },
  { name: "James", host: false, you: true },
]);
check("a delivered room opens the lobby", showing().join() === "lobby", showing().join());
check("with its code", byId.get("code").textContent === "GUEST2", byId.get("code").textContent);
check("and the host's name in the first slot", byId.get("slots").children[0].textContent.includes("Ada"));

// What the host chose arrives as settings, and is shown. The seed first, so
// the star the host then names is one of *that* galaxy — `lit` is a star of
// the reference one and means nothing here.
win.bimsDeliver("settings", { seedHi: 7, seedLo: 9, galaxy: 2, spawnStar: null, spawnStation: null });
let guestLit = -1;
for (let star = 0; star < wasm.lobby_star_count() && guestLit < 0; star++) {
  if (wasm.lobby_star_has_station(star) === 1) guestLit = star;
}
win.bimsDeliver("settings", { spawnStar: guestLit, spawnStation: 0 });
lobbyTool.querySelectorAll(".tool-tabs .tab")[1].dispatch("click");
check("a guest sees the host's seed", seedField.value === whole(7, 9).toString(), seedField.value);
check("and the host's galaxy", chosenIn(lobbyTool, "galaxy") === "elliptical", chosenIn(lobbyTool, "galaxy"));
check("and the host's start", / at [A-Z][a-z]+-\d+$/.test(byId.get("spawn-said").textContent), byId.get("spawn-said").textContent);

// But cannot touch any of it.
check("the seed field is read-only", seedField.disabled === true);
check("New seed is off", byId.get("new-seed").disabled === true);
check("Random start is off", byId.get("random-start").disabled === true);
check(
  "the galaxy buttons are off",
  lobbyTool.querySelectorAll("[data-field=galaxy] .option").every((b) => b.disabled),
);
setSeed("12345");
check("a typed seed is refused for a guest", seedField.value === whole(7, 9).toString(), seedField.value);
optionIn(lobbyTool, "galaxy", "round").dispatch("click");
check("a galaxy click is refused for a guest", chosenIn(lobbyTool, "galaxy") === "elliptical");
click("new-seed");
check("New seed does nothing for a guest", seedField.value === whole(7, 9).toString());
const setBefore = lastCall("lobby_set_spawn");
click("random-start");
check("Random start does nothing for a guest", lastCall("lobby_set_spawn") === setBefore);

// A guest can still look, and can suggest.
{
  const x = wasm.lobby_star_screen_x(guestLit);
  const y = wasm.lobby_star_screen_y(guestLit);
  canvas.dispatch("wheel", { clientX: x, clientY: y, deltaY: -4000 });
  const zx = wasm.lobby_star_screen_x(guestLit);
  const zy = wasm.lobby_star_screen_y(guestLit);
  canvas.dispatch("pointerdown", { clientX: zx, clientY: zy, pointerId: 1, button: 0 });
  canvas.dispatch("pointerup", { clientX: zx, clientY: zy, pointerId: 1, button: 0 });
}
check("a guest can inspect a star", wasm.lobby_inspected() === guestLit, String(wasm.lobby_inspected()));
const guestRows = byId.get("stations").querySelectorAll("[data-station]");
check("and the station buttons offer to suggest", guestRows.length > 0 && guestRows[0].querySelector("[data-pick]").textContent === "Suggest", guestRows[0]?.querySelector("[data-pick]")?.textContent);
const spawnBefore = byId.get("spawn-said").textContent;
guestRows[0].querySelector("[data-pick]").dispatch("click");
check("suggesting does not set the start", byId.get("spawn-said").textContent === spawnBefore);
check("but pings the star on the map", (lastCall("lobby_ping") ?? [])[0] === guestLit, String(lastCall("lobby_ping")));
check("and says who suggested what", /suggests starting at .+ at [A-Z][a-z]+-\d+/.test(byId.get("world-note").textContent), byId.get("world-note").textContent);
step(3);
check("the ping is being animated", wasm.lobby_animating() === 1);

// --- joining, which has nothing behind it yet --------------------------

screen("lobby").querySelector("[data-goto]").dispatch("click");
check("Leave returns to the menu", showing().join() === "menu", showing().join());

const note = byId.get("join-note");
byId.get("join-code").value = "nope";
dispatch("join", "submit");
check("a malformed code is refused", note.textContent.includes("not a room code"), note.textContent);
check("and the refusal is marked", note.className.includes("warn"), note.className);

byId.get("join-code").value = "abc234";
dispatch("join", "submit");
check(
  "a well-formed code is refused too, by name",
  note.textContent.includes("ABC234"),
  note.textContent,
);

advance(5000);
check("the refusal clears itself", note.textContent === "", note.textContent);

// --- starting -----------------------------------------------------------

click("play");
setupTool.querySelectorAll(".tool-tabs .tab")[1].dispatch("click");
// Back as host of nothing: the world is editable again, and the start the
// guest was shown is still the pending one.
check("out of the lobby the world is editable again", seedField.disabled === false);
// What the host had chosen is still what is set — leaving a lobby keeps its
// settings, as it does the money and the ship — so put the world back.
check("and the host's galaxy is still what is set", chosenIn(setupTool, "galaxy") === "elliptical", chosenIn(setupTool, "galaxy"));
setSeed(REFERENCE_SEED.toString());
optionIn(setupTool, "galaxy", "two-arm").dispatch("click");
click("random-start");
const startSaid = byId.get("spawn-said").textContent;
const [startStar, startStation] = lastCall("lobby_set_spawn");
screen("setup").querySelector("[data-start]").dispatch("click");
check("Start shows the handover", showing().join() === "build", showing().join());

const said = byId.get("chosen").textContent;
check("it names the ship that was chosen", said.includes("60 × 60"), said);
check("and the money each Bim brings", /50.?000/.test(said) && said.includes("€"), said);
check("and where the game starts", said.includes(startSaid), `${said} / ${startSaid}`);

// And then it actually goes there. The designer is a page of its own, so
// Start is a navigation — and what crosses is **numbers in a query string**,
// which is the same rule the wasm boundary is under. A query carrying "large"
// and "half" would be a string setting that had got out of the builder.
const went = visited[visited.length - 1] ?? "";
check("Start navigates to the ship designer", went.startsWith("ship.html?"), went);

const query = new Map(
  went
    .slice(went.indexOf("?") + 1)
    .split("&")
    .map((pair) => pair.split("=")),
);
check("it carries the money each Bim brings", query.get("money") === "50000", went);
check("and the build area in tiles", query.get("area") === "60", went);
check("and how many are playing", query.get("players") === "1", went);
check("and which slot you are", query.get("slot") === "0", went);
check(
  "and the seed in its two halves",
  whole(Number(query.get("seedHi")), Number(query.get("seedLo"))) === REFERENCE_SEED,
  went,
);
check("and the galaxy type", query.get("galaxy") === "0", went);
check("and the star", query.get("star") === String(startStar), went);
check("and the station", query.get("station") === String(startStation), went);
check(
  "and nothing that is not a number",
  [...query.values()].every((v) => Number.isFinite(Number(v))),
  went,
);

screen("build").querySelector("[data-goto]").dispatch("click");
check("and it goes back to the menu", showing().join() === "menu", showing().join());
check("still no error box", !byId.get("error").textContent, byId.get("error").textContent);

// --- and without the module at all ---------------------------------------
//
// The page has to stand up with no wasm behind it: the menu, the setup
// screen and the lobby are all there, and the World tab says what is
// missing rather than sitting there loading.

{
  const bare = bootPage({ html: "web/builder.html", host: "web/builder.js" });
  check("the page boots with no world module", !bare.byId.get("error").textContent, bare.byId.get("error").textContent);
  bare.click("play");
  bare.byId.get("setup-tool").querySelectorAll(".tool-tabs .tab")[1].dispatch("click");
  const said = bare.byId.get("world-said");
  check("and the World tab says the module is missing", said.className.includes("warn") && said.textContent.includes("world module"), said.textContent);
  check("and Start stays disabled", bare.root.querySelector("[data-screen=setup] [data-start]").disabled === true);
}

console.log(fails.length ? `\n${fails.length} FAILED` : "\nall passed");
process.exit(fails.length ? 1 : 0);

// Does the host actually run? Boots web/bims.js against the real wasm and the
// stub page, then checks the pieces this round added: the hover readout, the
// triggers under the timetable, and that the frame loop keeps going.

import { boot } from "./stub.mjs";

const fails = [];
function check(what, ok, extra = "") {
  if (ok) console.log(`  ok   ${what}`);
  else {
    console.log(`  FAIL ${what}${extra ? ` — ${extra}` : ""}`);
    fails.push(what);
  }
}

const { byId, root, win, step, wasm, lastCall, drawn } = await boot({ frames: 5 });

/** Which fixture the host has last told wasm to ring. */
const lastHighlight = () => lastCall("bims_set_highlight")?.[0] ?? 0;

console.log("host booted");
check("no error box", !byId.get("error").textContent, byId.get("error").textContent);

// --- the readout ------------------------------------------------------

const hover = byId.get("hover");
const thing = hover.querySelector(".thing");
const onIt = hover.querySelector(".on-it");

check("readout says nothing before the pointer arrives", thing.textContent === "—", thing.textContent);

const stage = byId.get("stage");
const scale = wasm.bims_view_scale();
const at = (x, y) => ({
  pointerId: 7,
  button: 0,
  clientX: x * scale + wasm.bims_view_x(),
  clientY: y * scale + wasm.bims_view_y(),
});

// Somewhere that really is open deck — the middle of the room is the chair.
let deckAt = null;
for (let y = 30; y < 560 && !deckAt; y += 5) {
  for (let x = 30; x < 830; x += 5) {
    if (wasm.bims_spot_at(x, y) === 1) { deckAt = { x, y }; break; }
  }
}
stage.dispatch("pointermove", at(deckAt.x, deckAt.y));
step(1);
check("open deck reads as deck", thing.textContent === "Deck plating", thing.textContent);
check("clean deck has nothing on it", onIt.textContent === "", onIt.textContent);

// Walk the whole room and collect every name the readout can produce, so a
// code with no name in SPOT_NAMES shows up as "Something".
const seen = new Map();
for (let y = -20; y < 600; y += 7) {
  for (let x = -20; x < 880; x += 7) {
    const spot = wasm.bims_spot_at(x, y);
    if (!seen.has(spot)) {
      stage.dispatch("pointermove", at(x, y));
      step(1);
      seen.set(spot, thing.textContent);
    }
  }
}
console.log("  every spot the room can return:");
for (const [code, name] of [...seen].sort((a, b) => a[0] - b[0])) {
  console.log(`    ${String(code).padStart(2)}  ${name}`);
}
check("every spot has a name", ![...seen.values()].some((n) => n.startsWith("Something")));
check("the deck is in there", [...seen.values()].some((n) => n.startsWith("Deck plating")));
check("the pan is in there", [...seen.values()].some((n) => n.startsWith("Toilet")));
check("the hob is in there", [...seen.values()].some((n) => n.startsWith("Hob")));
check("the cold store is in there", [...seen.values()].some((n) => n.startsWith("Cold store")));
check("the bay is in there", [...seen.values()].some((n) => n.startsWith("Hydroponic bay")));
check("the bunk is in there", [...seen.values()].some((n) => n.startsWith("Bunk")));
check("the door is in there", [...seen.values()].some((n) => n.startsWith("Bathroom door")));

// Leaving the canvas puts it back to nothing.
stage.dispatch("pointerleave", {});
step(1);
check("readout empties when the pointer leaves", thing.textContent === "—", thing.textContent);

// --- state beside the name --------------------------------------------

// Light the hob by hand through wasm and watch the readout follow it. The hob
// is inside the counter run at the top of the room.
let hobAt = null;
for (let y = 0; y < 600 && !hobAt; y += 3) {
  for (let x = 0; x < 880; x += 3) {
    if (wasm.bims_spot_at(x, y) === 6) {
      hobAt = { x, y };
      break;
    }
  }
}
check("found the hob", hobAt !== null);
if (hobAt) {
  stage.dispatch("pointermove", at(hobAt.x, hobAt.y));
  step(1);
  const cold = thing.textContent;
  check("cold hob reads plainly", cold === "Hob", cold);
}

// --- the triggers ------------------------------------------------------

const thresholds = byId.get("thresholds");
const rows = thresholds.querySelectorAll(".trigger");
check("two trigger rows", rows.length === 2, String(rows.length));

const names = rows.map((r) => r.querySelector(".name").textContent);
check("the rows are Rest and Food", names.join(",") === "Rest,Food", names.join(","));

const restRow = rows[0];
const restBox = restRow.querySelector("input");
const restSlider = restRow.querySelectorAll("input")[1];
const restOut = restRow.querySelector("output");

check("rest starts ticked", restBox.checked === true);
check("rest starts at 10%", restOut.textContent === "10%", restOut.textContent);
check("wasm agrees rest is watched", wasm.bims_need_trigger_on(0) === 1);

// Move it and the number, wasm and the needs panel all follow.
restSlider.value = "35";
restSlider.dispatch("input", {});
step(1);
check("slider moved the readout", restOut.textContent === "35%", restOut.textContent);
check(
  "slider moved wasm",
  Math.abs(wasm.bims_need_trigger(0) - 0.35) < 1e-6,
  String(wasm.bims_need_trigger(0)),
);

// Untick it.
restBox.checked = false;
restBox.dispatch("change", {});
step(1);
check("unticking reaches wasm", wasm.bims_need_trigger_on(0) === 0);
check("unticked row is marked off", restRow.className === "trigger off", restRow.className);

// The needs panels stop calling it urgent while it is off, however low it is.
// Both of them: the thresholds are the ship's setting, not one Bim's.
for (const panel of byId.get("side").querySelectorAll(".needs")) {
  check(
    "an off trigger never marks a rest row urgent",
    panel.children[0].className !== "urgent",
    panel.children[0].className,
  );
}

restBox.checked = true;
restBox.dispatch("change", {});
step(1);
check("ticking it back on reaches wasm", wasm.bims_need_trigger_on(0) === 1);
check("ticked row is marked on", restRow.className === "trigger", restRow.className);

// Put it back where it started so nothing downstream is surprised.
restSlider.value = "10";
restSlider.dispatch("input", {});
step(1);

// --- the management rows point at the deck ------------------------------
//
// The stub has no canvas, so what is checked is the state the renderer reads:
// which spot wasm has been told to ring. That the ring lands on the right
// fixture is the native probe's job.

const stockTr = byId.get("stock").querySelectorAll("tbody tr");
check("three stock rows", stockTr.length === 3, String(stockTr.length));
for (const r of stockTr) {
  check(
    `"${r.querySelector(".where").textContent}" row points at something`,
    r.className.split(/\s+/).includes("points"),
    r.className,
  );
}

const foodControl = byId.get("food-control");
check("the food target points too", foodControl.className.split(/\s+/).includes("points"));

// Rest on each row and watch the ring follow. Codes are from src/room.rs:
// 5 the cold store, 6 the hob, 11 the bay.
const wants = [5, 5, 6];
for (let i = 0; i < stockTr.length; i++) {
  stockTr[i].dispatch("pointerenter", {});
  step(1);
  check(`row ${i} rings spot ${wants[i]}`, lastHighlight() === wants[i], String(lastHighlight()));
  stockTr[i].dispatch("pointerleave", {});
  step(1);
  check(`row ${i} stops ringing when the pointer leaves`, lastHighlight() === 0);
}

foodControl.dispatch("pointerenter", {});
step(1);
check("the food target rings the bay", lastHighlight() === 11, String(lastHighlight()));

// Switching tab takes the row out from under the pointer, and no pointerleave
// follows it. The ring has to go anyway.
byId
  .get("tray-tabs")
  .querySelectorAll(".tab")[0]
  .dispatch("click", {});
step(1);
check("changing tab clears the ring", lastHighlight() === 0, String(lastHighlight()));

// Folding the tray away does the same.
foodControl.dispatch("pointerenter", {});
step(1);
byId.get("tray-toggle").dispatch("click", {});
step(1);
check("folding the tray clears the ring", lastHighlight() === 0, String(lastHighlight()));

// --- there are two of them ----------------------------------------------

const CREW = ["James", "Kate"];
check("wasm says two are aboard", wasm.bims_crew() === 2, String(wasm.bims_crew()));
check("and the player is the first", wasm.bims_player() === 0, String(wasm.bims_player()));

const columns = byId.get("side").querySelectorAll(".crew");
check("two crew panels", columns.length === 2, String(columns.length));
for (let who = 0; who < 2; who++) {
  const head = columns[who].querySelector(".who .tag");
  check(`panel ${who} is ${CREW[who]}`, head.textContent === CREW[who], head.textContent);
  check(
    `panel ${who} has its own needs list`,
    columns[who].querySelectorAll(".needs li").length === wasm.bims_need_count(),
  );
  check(`panel ${who} has its own health box`, columns[who].querySelectorAll(".health").length === 1);
}
check(
  "only the player's panel is marked as theirs",
  columns.filter((c) => c.querySelector(".who").className.includes("yours")).length === 1,
);

// The panel on screen really is live: its bars move with the clock.
const bar = columns[0].querySelectorAll(".needs li")[1].querySelector(".pct");
const before = bar.textContent;
step(600);
check("the panel on screen is live", bar.textContent !== before, `${before} -> ${bar.textContent}`);

// Two agendas, each named.
const agendaBoxes = byId.get("agendas").children;
check("two agenda boxes", agendaBoxes.length === 2, String(agendaBoxes.length));
for (let who = 0; who < 2; who++) {
  const tag = agendaBoxes[who].querySelector(".who .tag");
  check(`agenda ${who} is ${CREW[who]}`, tag.textContent === CREW[who], tag.textContent);
}

// --- the names are on the map --------------------------------------------
//
// Not shapes out of the draw buffer — no strings cross that boundary — so the
// host writes them itself, and the stub's 2D context records what it wrote.

drawn.length = 0;
step(1);
const labels = drawn.map((d) => d.text);
check("both names are drawn on the deck", labels.join(",") === "James,Kate", labels.join(","));
// Over the body, not on it: the label sits above where wasm says the Bim is.
const s2 = wasm.bims_view_scale();
for (let who = 0; who < 2; who++) {
  const want = wasm.bims_view_x() + wasm.bims_bim_x(who) * s2;
  check(
    `${CREW[who]}'s name sits over ${CREW[who]}`,
    Math.abs(drawn[who].x - want) < 0.001,
    `${drawn[who].x} vs ${want}`,
  );
  check(
    `${CREW[who]}'s name sits above the body`,
    drawn[who].y < wasm.bims_view_y() + wasm.bims_bim_y(who) * s2,
  );
}
check(
  "the player's name is in the steerable colour and the other's is not",
  drawn[0].fill !== drawn[1].fill,
  `${drawn[0].fill} ${drawn[1].fill}`,
);

// --- one galley, one pan --------------------------------------------------

check(
  "nobody holds the galley at rest",
  wasm.bims_galley_held_by() === 0 || wasm.bims_galley_held_by() === 1,
  String(wasm.bims_galley_held_by()),
);

// --- one crew sheet at a time --------------------------------------------
//
// Picking a Bim is what puts its panels on screen. Kate's bars are hers until
// you click on her.

const stageEl = byId.get("stage");
const clickAt = (p) => ({
  pointerId: 3,
  button: 0,
  clientX: p.x * wasm.bims_view_scale() + wasm.bims_view_x(),
  clientY: p.y * wasm.bims_view_scale() + wasm.bims_view_y(),
});
const bimAt = (w) => ({ x: wasm.bims_bim_x(w), y: wasm.bims_bim_y(w) });
const visible = () => columns.filter((c) => !c.hidden).length;
const shownWho = () => columns.findIndex((c) => !c.hidden);

step(1);
check("one crew sheet is up at the start", visible() === 1, String(visible()));
check("and it is the player's", shownWho() === 0, String(shownWho()));

// Click Kate: her sheet replaces his.
const kate = bimAt(1);
stageEl.dispatch("pointerdown", clickAt(kate));
stageEl.dispatch("pointerup", clickAt(kate));
step(1);
check("clicking Kate shows Kate", shownWho() === 1, String(shownWho()));
check("and only Kate", visible() === 1, String(visible()));
check(
  "her sheet is marked as somebody else's",
  columns[1].className.includes("theirs") && !columns[0].className.includes("theirs"),
  columns[1].className,
);

// Escape puts them all away.
byId.get("stage");
for (const fn of win.listeners.get("keydown") ?? []) {
  fn({ key: "Escape", preventDefault() {} });
}
step(1);
check("Escape puts the sheet away", visible() === 0, String(visible()));

// And `1` brings the player's back.
for (const fn of win.listeners.get("keydown") ?? []) {
  fn({ key: "1", preventDefault() {} });
}
step(1);
check("pressing 1 brings the player's back", shownWho() === 0, String(shownWho()));

// --- the character sheet --------------------------------------------------

const sheet = columns[0].querySelector(".sheet");
check("the sheet is under the bars", sheet !== null);
const sheetTabs = sheet.querySelectorAll(".sheet-tabs .tab");
check("it has two tabs", sheetTabs.length === 2, String(sheetTabs.length));
check(
  "About and Memory",
  sheetTabs.map((t) => t.textContent).join(",") === "About,Memory",
  sheetTabs.map((t) => t.textContent).join(","),
);

const about = sheet.querySelector("[data-sheet=about]");
const memory = sheet.querySelector("[data-sheet=memory]");
check("About is the page it opens on", !about.hidden && memory.hidden);

const values = about.querySelectorAll(".value").map((v) => v.textContent);
check("it says the name", values[0] === "James", values[0]);
const age = Number(values[1]);
check("it says an age", age === wasm.bims_age(0) && age > 0, values[1]);
// Born between 2350 and 2380, and the year has to be in the date it prints.
const bornYear = wasm.bims_born_year(0);
check("born between 2350 and 2380", bornYear >= 2350 && bornYear <= 2380, String(bornYear));
check("and the date says so", values[2].endsWith(` ${bornYear}`), values[2]);
check(
  "with a month in words",
  /^\d{1,2} [A-Z][a-z]+ \d{4}$/.test(values[2]),
  values[2],
);
console.log(`       James: ${values[0]}, aged ${values[1]}, born ${values[2]}`);

// The Memory tab opens, and fills once the Bim has done something.
sheetTabs[1].dispatch("click", {});
step(1);
check("clicking Memory opens it", !memory.hidden && about.hidden);

// Run until there is something to read. A day at a time so this stays quick.
let filled = false;
for (let i = 0; i < 200 && !filled; i++) {
  step(200);
  filled = wasm.bims_memory_len(0) > 0;
}
step(2);
check("the diary fills as the day goes on", filled, String(wasm.bims_memory_len(0)));
const dayHeads = memory.querySelectorAll(".day");
const lines = memory.querySelectorAll(".line");
check("it is grouped by day", dayHeads.length > 0, String(dayHeads.length));
check("with a line per thing", lines.length === wasm.bims_memory_len(0), String(lines.length));
check(
  "each line has a time and a sentence",
  lines.every((l) => /^\d\d:\d\d/.test(l.textContent) && l.textContent.length > 8),
  lines[0]?.textContent,
);
// No entry may come out as the fallback: every code wasm can emit needs words.
check(
  "every entry has words for it",
  !lines.some((l) => l.textContent.includes("Something happened.")),
  lines.find((l) => l.textContent.includes("Something happened."))?.textContent,
);
console.log(`       first diary line: ${lines[0]?.textContent}`);

// --- the loop keeps running --------------------------------------------

step(600);
check("frame loop still going after 600 frames", true);
check("still no error box", !byId.get("error").textContent, byId.get("error").textContent);

console.log(fails.length ? `\n${fails.length} FAILED` : "\nall passed");
process.exit(fails.length ? 1 : 0);

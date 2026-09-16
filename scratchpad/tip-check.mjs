// Tooltips are asked for, never stumbled into.
//
// Both halves are asserted here: every affordance — a word underlined with
// class `asks`, a `?` button — actually says something, and the containers
// around them stay silent. A player crossing the needs panel on the way to the
// deck should get nothing.

import { boot } from "./stub.mjs";

const fails = [];
function check(what, ok, extra = "") {
  if (ok) console.log(`  ok   ${what}`);
  else {
    console.log(`  FAIL ${what}${extra ? ` — ${extra}` : ""}`);
    fails.push(what);
  }
}

const { byId, root, step } = await boot({ frames: 3 });
const tip = byId.get("tip");

/** Rest on `el` long enough for it to say its piece, and return what it said. */
function ask(el) {
  el.dispatch("pointerenter", {});
  step(30); // comfortably past TIP_DELAY
  const said = tip.hidden ? "" : tip.textContent;
  el.dispatch("pointerleave", {});
  step(1);
  return said;
}

// --- the affordances talk ---------------------------------------------

const asks = root.querySelectorAll(".asks");
check("there are underlined words to ask", asks.length > 0, String(asks.length));
for (const el of asks) {
  const said = ask(el);
  check(`"${el.textContent}" says something`, said.length > 20, said);
}

const marks = root.querySelectorAll(".what");
check("there are question marks to press", marks.length > 0, String(marks.length));
for (const el of marks) {
  const said = ask(el);
  const label = el.getAttribute("aria-label") ?? "?";
  check(`the "${label}" mark says something`, said.length > 20, said);
  check(`the "${label}" mark is labelled for a reader`, Boolean(el.getAttribute("aria-label")));
}

// The triggers under the timetable got a mark of their own this round.
check(
  "the triggers have a mark",
  byId.get("thresholds").querySelectorAll(".what").length === 1,
  String(byId.get("thresholds").querySelectorAll(".what").length),
);

// --- the containers stay silent ---------------------------------------

for (const id of [
  "agendas",
  "tray",
  "tray-body",
  "hours",
  "thresholds",
  "hover",
  "side",
  "left",
  "stock",
  "brushes",
]) {
  const el = byId.get(id);
  check(`#${id} says nothing`, el !== null && ask(el) === "", `#${id}`);
}

// The per-crew panels are built by the host and have classes rather than ids,
// but the rule is the same for every one of them.
for (const el of root.querySelectorAll(".crew")) {
  check("a crew column says nothing", ask(el) === "");
}
for (const el of root.querySelectorAll(".needs")) {
  check("a needs panel says nothing", ask(el) === "");
}
for (const el of root.querySelectorAll(".health")) {
  check("a health panel says nothing", ask(el) === "");
}
for (const el of root.querySelectorAll(".agenda")) {
  check("an agenda says nothing", ask(el) === "");
}
// A name header is a label, not an affordance.
for (const el of root.querySelectorAll(".who")) {
  check("a name header says nothing", ask(el) === "");
}
// Nor does the character sheet, its tabs, or a line of somebody's diary.
for (const el of root.querySelectorAll(".sheet")) {
  check("a character sheet says nothing", ask(el) === "");
}
for (const el of root.querySelectorAll(".sheet .tab")) {
  check("a sheet tab says nothing", ask(el) === "");
}
for (const el of root.querySelectorAll(".about .row")) {
  check("an About row says nothing", ask(el) === "");
}
for (const el of root.querySelectorAll(".diary")) {
  check("the diary says nothing", ask(el) === "");
}

// Nor do the rows inside them, nor the readout's own lines — a pointer
// crossing the panels on its way to the deck must set nothing off.
for (const row of root.querySelectorAll(".needs li")) {
  check(`a needs row says nothing`, ask(row) === "");
}
for (const line of byId.get("hover").children) {
  check(`a readout line says nothing`, ask(line) === "");
}
for (const row of byId.get("thresholds").querySelectorAll(".trigger")) {
  check(`a trigger row says nothing`, ask(row) === "");
  for (const bit of row.children) {
    check(`a trigger control says nothing`, ask(bit) === "");
  }
}

console.log(fails.length ? `\n${fails.length} FAILED` : "\nall passed");
process.exit(fails.length ? 1 : 0);

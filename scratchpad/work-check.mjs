// The work tab: the rows, the boxes, and the sorting.
//
// Nothing in the build type-checks web/bims.js and nothing else executes the
// panel, so this is the only thing standing between a typo in it and a tab
// that does nothing. It boots the real host against the real wasm, clicks its
// way through the panel and checks both ends agree — a box says 4 and the ship
// thinks 4, not one or the other.
//
// The one thing to be careful of: never compare against a hardcoded copy of a
// label. Job names live in `JOB_NAMES` in the host and nowhere else, so they
// are read off the page and checked for being *sane* rather than for being any
// particular words.

import { boot } from "./stub.mjs";

const fails = [];
function check(what, ok, extra = "") {
  if (ok) console.log(`  ok   ${what}`);
  else {
    console.log(`  FAIL ${what}${extra ? ` — ${extra}` : ""}`);
    fails.push(what);
  }
}

const { byId, root, wasm, lastCall } = await boot({ frames: 3 });

check("no error box", !byId.get("error").textContent, byId.get("error").textContent);

const tabs = root.querySelectorAll("#tray .tab");
const workTab = tabs.find((t) => t.dataset.tab === "work");
const panelOf = (name) =>
  root.querySelectorAll("#tray [data-panel]").find((p) => p.dataset.panel === name);

check("there is a Work tab", !!workTab);
check("with a panel behind it", !!panelOf("work"));
check("which starts hidden", panelOf("work").hidden);

// --- the tab shows its panel and only its panel -------------------------

workTab.dispatch("click", {});
check("clicking it shows the work panel", !panelOf("work").hidden);
check(
  "and hides every other panel",
  root
    .querySelectorAll("#tray [data-panel]")
    .every((p) => (p.dataset.panel === "work") !== p.hidden),
);
check(
  "and the tab is the one marked on",
  tabs.every((t) => (t === workTab) === t.className.includes("on")),
);

// --- a row per job ------------------------------------------------------

const rows = byId.get("work").querySelectorAll("tbody tr");
const count = wasm.bims_work_count();
check(`a row per job (${count})`, rows.length === count, rows.length);

// A job added on the wasm side without a name added to the host renders a
// blank row. That is the whole failure mode this catches, and nothing else
// would: the row is there, the box works, and the player cannot tell what it
// is for.
const names = rows.map((r) => r.querySelectorAll("td")[0].textContent);
check("every job is named", names.every((n) => n.trim().length > 0), JSON.stringify(names));
check("and no two share a name", new Set(names).size === names.length, JSON.stringify(names));
console.log(`       jobs: ${names.join(", ")}`);

// --- they start equal ---------------------------------------------------

const boxOf = (row) => row.querySelector(".pri");
const jobOf = (row) => Number(boxOf(row).dataset.job);
const start = wasm.bims_work_priority(jobOf(rows[0]));
check(
  "every job starts at the same priority",
  rows.every((r) => wasm.bims_work_priority(jobOf(r)) === start),
  start,
);
check(
  "and the boxes say so",
  rows.every((r) => boxOf(r).textContent === String(start)),
  rows.map((r) => boxOf(r).textContent).join(","),
);
check(
  "and are coloured for it",
  rows.every((r) => boxOf(r).className === `pri p${start}`),
  boxOf(rows[0]).className,
);

// --- clicking cycles ----------------------------------------------------

const high = wasm.bims_work_highest();
const low = wasm.bims_work_lowest();
check("the range runs most-important first", high < low, `${high}..${low}`);

const first = rows[0];
const box = boxOf(first);
const job = jobOf(first);

// All the way round and back to where it started, checking the box and the
// ship at every step. Reading the box alone would pass with the click wired
// to nothing but a repaint.
let seen = [];
for (let i = 0; i < low - high + 1; i++) {
  box.dispatch("click", {});
  seen.push(Number(box.textContent));
  check(
    `click ${i + 1}: the ship agrees the box says ${box.textContent}`,
    wasm.bims_work_priority(job) === Number(box.textContent),
    `${wasm.bims_work_priority(job)} vs ${box.textContent}`,
  );
  check(
    `click ${i + 1}: the colour follows the number`,
    box.className === `pri p${box.textContent}`,
    box.className,
  );
}
check(
  "a click is one step less important, and wraps at the bottom",
  seen.join(",") === [4, 5, 1, 2, 3].join(","),
  seen.join(","),
);
check("and it is back where it started", wasm.bims_work_priority(job) === start);
check("the cycling went through wasm", lastCall("bims_cycle_work_priority")?.[0] === job);

// Every box must be its own job. One shared handler wired to the wrong index
// is the obvious way to get this wrong and it looks fine on one row.
for (const row of rows) {
  const mine = jobOf(row);
  const before = rows.map((r) => wasm.bims_work_priority(jobOf(r)));
  boxOf(row).dispatch("click", {});
  const after = rows.map((r) => wasm.bims_work_priority(jobOf(r)));
  const moved = rows.filter((r, i) => before[i] !== after[i]).map(jobOf);
  check(`clicking ${names[mine]} moves only ${names[mine]}`, moved.join() === String(mine), moved.join());
}

// --- sorting ------------------------------------------------------------
//
// The list is now deliberately uneven: every job has been clicked once more
// than the one before it, so a sort has something to do.

const sorts = root.querySelectorAll("#work-sort .sort");
check("there are three ways to sort", sorts.length === 3, sorts.length);

const order = () =>
  byId.get("work").querySelectorAll("tbody tr").map((r) => jobOf(r));
const levels = () => order().map((j) => wasm.bims_work_priority(j));
const labels = () =>
  byId
    .get("work")
    .querySelectorAll("tbody tr")
    .map((r) => r.querySelectorAll("td")[0].textContent.toLowerCase());

const up = sorts.find((s) => s.dataset.sort === "up");
const down = sorts.find((s) => s.dataset.sort === "down");
const alpha = sorts.find((s) => s.dataset.sort === "name");
check("one of each", !!up && !!down && !!alpha);

up.dispatch("click", {});
check(
  "most important first",
  levels().every((p, i, all) => i === 0 || all[i - 1] <= p),
  levels().join(","),
);
check("and the button says it was applied", up.className.includes("on"));

down.dispatch("click", {});
check(
  "least important first, the other way round",
  levels().every((p, i, all) => i === 0 || all[i - 1] >= p),
  levels().join(","),
);
check(
  "and only the pressed button is marked",
  sorts.filter((s) => s.className.includes("on")).length === 1 && down.className.includes("on"),
);

alpha.dispatch("click", {});
check(
  "alphabetically by name",
  labels().every((n, i, all) => i === 0 || all[i - 1] <= n),
  labels().join(" | "),
);

// Sorting must move the rows and not lose any: a `replaceChildren` that drops
// one would read as a job quietly disappearing from the panel.
check("and nothing was lost on the way", order().length === count, order().length);
check("nor duplicated", new Set(order()).size === count);

// --- a click on a box unmarks the sort -----------------------------------
//
// The mark says "the list is in this order". Changing a priority may have
// taken it out of that order, and a mark that lies is worse than none.

boxOf(byId.get("work").querySelectorAll("tbody tr")[0]).dispatch("click", {});
check(
  "changing a priority clears the sort mark",
  sorts.every((s) => !s.className.includes("on")),
  sorts.map((s) => s.className).join(" | "),
);

// --- a row points at the place the job happens ---------------------------
//
// Not a tooltip: nothing pops up and nothing is said. It is the same ring the
// management rows use, and it has to go out again when the pointer leaves.

const lastRing = () => lastCall("bims_set_highlight")?.[0] ?? 0;
for (const row of byId.get("work").querySelectorAll("tbody tr")) {
  check(`the ${names[jobOf(row)]} row is offered as pointing`, row.className.includes("points"));
  row.dispatch("pointerenter", {});
  check(`and rings a fixture`, lastRing() !== 0, lastRing());
  row.dispatch("pointerleave", {});
  check(`and lets go of it again`, lastRing() === 0, lastRing());
}

console.log();
if (fails.length) {
  console.log(`${fails.length} FAILED`);
  process.exit(1);
}
console.log("all passed");
